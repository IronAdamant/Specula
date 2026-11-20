//! What physics tells a game's rules: a touch, and a body that fell out of the world.
//!
//! Until 2026-11-21 a module was told nothing of physics: a bounce had to be guessed from a
//! ball's height, and a ball that rolled off its floor fell for ever with nobody the wiser
//! (found by making a ball on a ramp by hand; the owner: "Can this be looked at and fixed").
//!
//! Both are **events from the engine**, pushed after the tick's physics step and heard by
//! every module at the next tick ([`crate::logic::LogicHost::push_engine_event`]), so they
//! are part of the simulation: the same run tells the same events at the same ticks. Nothing
//! is kept here between ticks.
//!
//! - **`contact`**, for a pair of which at least one entity carries the tag [`CONTACTS_TAG`]
//!   (a game says which of its things it wants to hear of: a pile of crates is not a thousand
//!   events a second). `kind`: `begin`, `end` (two solid things), `enter`, `exit` (a
//!   trigger). `a` and `b` are the entities' paths (`a_id`, `b_id` their ids); when only one
//!   carries the tag it is `a`. `point`, `normal` (from `a` to `b`) and `speed` (m/s, how
//!   fast they closed, at a `begin`). At most [`MAX_CONTACTS_TOLD`] a tick: what is past that
//!   is one last `contact` of `kind` `more` with their `count`.
//! - **`fell`**, when a dynamic body passes [`FELL_BELOW_M`] beyond everything that stands
//!   still or is moved by the rules, in the direction gravity pulls: `entity` (its path),
//!   `id`, `position`. Told once, at the tick it passes; the rules put it back or delete it.
//!   At most [`MAX_FELL_TOLD`] a tick. With no gravity, or nothing that stands still, nothing
//!   can fall out and nothing is told.
//!
//! Neither may crowd out what the game itself says (a run's start, a button): they are
//! pushed only while [`EVENTS_KEPT_FREE`] of the waiting events' room is left.

use specula_physics::{BodyKind, EventKind, Space};
use specula_sdk::Value;

/// The tag of a thing whose touches the rules are told of.
pub const CONTACTS_TAG: &str = "contacts";
/// Contact events told in one tick.
pub const MAX_CONTACTS_TOLD: usize = 32;
/// Fallen bodies told in one tick.
pub const MAX_FELL_TOLD: usize = 8;
/// How far beyond everything it could land on a body is when it has fallen out, metres.
pub const FELL_BELOW_M: f32 = 5.0;
/// Room among the waiting events that physics never takes.
pub const EVENTS_KEPT_FREE: usize = 256;
/// How far up from an entity its groups are looked at for the tag.
pub const MAX_DEPTH_WALKED: usize = crate::world::MAX_DEPTH + 1;
/// The most things `Cx::touching` answers.
pub const MAX_TOUCHING: usize = 64;

/// One body out of the world: its key, and how far beyond the lowest thing it could land on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fallen {
    pub key: u64,
    pub beyond_m: f32,
}

/// The dynamic bodies that are more than [`FELL_BELOW_M`] beyond every body that is not
/// dynamic, along `gravity`, in key order. With `passing` (the seconds of the step just
/// made): only those that passed that line in this step, so each is found once. The second
/// value is how many there are past `limit`.
pub fn fallen(space: &Space, gravity: [f32; 3], passing: Option<f32>, limit: usize) -> (Vec<Fallen>, usize) {
    let g = specula_core::math::Vec3::new(gravity[0], gravity[1], gravity[2]);
    let pull = g.length();
    if pull.is_nan() || pull < 1e-6 {
        return (Vec::new(), 0);
    }
    let along = |v: specula_core::math::Vec3| (v.x * g.x + v.y * g.y + v.z * g.z) / pull;
    // Candidates first: most ticks have none, and then nothing that stands still is looked at.
    let moving = |b: &specula_physics::BodyView<'_>| b.kind == BodyKind::Dynamic && (passing.is_none() || (!b.sleeping && along(b.velocity) > 0.0));
    if !space.bodies().any(|b| moving(&b)) {
        return (Vec::new(), 0);
    }
    // The furthest, along gravity, that anything to land on reaches: a box's far corner.
    let mut lowest: Option<f32> = None;
    for b in space.bodies().filter(|b| b.kind != BodyKind::Dynamic) {
        let Some(bounds) = b.aabb else { continue };
        let pick = |lo: f32, hi: f32, dir: f32| if dir >= 0.0 { hi * dir } else { lo * dir };
        let reach = (pick(bounds.min.x, bounds.max.x, g.x) + pick(bounds.min.y, bounds.max.y, g.y) + pick(bounds.min.z, bounds.max.z, g.z)) / pull;
        if reach.is_finite() {
            lowest = Some(lowest.map_or(reach, |l: f32| l.max(reach)));
        }
    }
    let Some(line) = lowest.map(|l| l + FELL_BELOW_M) else { return (Vec::new(), 0) };
    let mut out: Vec<Fallen> = Vec::new();
    let mut more = 0;
    for b in space.bodies().filter(moving) {
        let now = along(b.center);
        let was = passing.map(|dt| now - along(b.velocity) * dt);
        if now > line && was.is_none_or(|was| was <= line) {
            if out.len() < limit {
                out.push(Fallen { key: b.key, beyond_m: now - line + FELL_BELOW_M });
            } else {
                more += 1;
            }
        }
    }
    out.sort_by_key(|f| f.key);
    (out, more)
}

/// Whether the touches of entity `id` are told: it carries [`CONTACTS_TAG`], or a group above
/// it does (a game tags its root and hears of everything under it). The walk up is bounded.
pub fn hears_contacts(world: &crate::world::World, id: u64) -> bool {
    let mut at = Some(id);
    for _ in 0..MAX_DEPTH_WALKED {
        let Some(e) = at.and_then(|id| world.get(id)) else { return false };
        if e.tags.contains(CONTACTS_TAG) {
            return true;
        }
        at = e.parent;
    }
    false
}

/// The `contact` events of one step's physics events, for the pairs of which an entity
/// carries [`CONTACTS_TAG`], in the step's own order, at most `limit`; and how many more
/// there were.
pub fn contacts_told(events: &[specula_physics::Event], world: &crate::world::World, limit: usize) -> (Vec<Value>, u64) {
    let v3 = |v: specula_core::math::Vec3| Value::from([v.x, v.y, v.z]);
    let (mut told, mut more) = (Vec::new(), 0u64);
    for e in events {
        let kind = match e.kind {
            EventKind::ContactBegin => "begin",
            EventKind::ContactEnd => "end",
            EventKind::TriggerEnter => "enter",
            EventKind::TriggerExit => "exit",
            EventKind::Ccd => continue,
        };
        // (A ragdoll's parts are not entities of their own: not told.)
        let hears = |id: u64| crate::physics::ragdoll::unkey(id).is_none() && hears_contacts(world, id);
        let (a, b) = (hears(e.a), hears(e.b));
        if !a && !b {
            continue;
        }
        if told.len() >= limit {
            more += 1;
            continue;
        }
        // The one that hears is `a`; the normal runs from `a` to `b`.
        let (first, second, normal) = if a { (e.a, e.b, e.normal) } else { (e.b, e.a, specula_core::math::Vec3::new(-e.normal.x, -e.normal.y, -e.normal.z)) };
        told.push(
            Value::map()
                .with("kind", kind)
                .with("a", world.path_of(first).as_str())
                .with("b", world.path_of(second).as_str())
                .with("a_id", first)
                .with("b_id", second)
                .with("point", v3(e.point))
                .with("normal", v3(normal))
                .with("speed", e.speed),
        );
    }
    (told, more)
}

impl crate::Engine {
    /// Pushes this tick's physics events to the modules (see the module's own words).
    pub(crate) fn tell_modules_of_physics(&mut self) {
        if self.logic.is_empty() {
            return;
        }
        let tick = self.clock.tick();
        let Some((at, report)) = self.physics.last.as_ref() else { return };
        if *at != tick {
            return;
        }
        // The waiting events' room that physics may still take.
        let room = |logic: &crate::logic::LogicHost| crate::logic::MAX_PENDING_EVENTS.saturating_sub(EVENTS_KEPT_FREE).saturating_sub(logic.pending_events());
        let v3 = |v: specula_core::math::Vec3| Value::from([v.x, v.y, v.z]);
        let (told, more) = contacts_told(&report.events, &self.world, MAX_CONTACTS_TOLD.min(room(&self.logic)));
        for data in told {
            self.logic.push_engine_event("contact", data, tick);
        }
        if more > 0 && room(&self.logic) > 0 {
            self.logic.push_engine_event("contact", Value::map().with("kind", "more").with("count", more), tick);
        }
        let dt = 1.0 / self.clock.hz() as f32;
        let (fell, _) = fallen(&self.physics.space, self.world.env().physics.gravity, Some(dt), MAX_FELL_TOLD);
        for f in fell {
            if crate::physics::ragdoll::unkey(f.key).is_some() || room(&self.logic) == 0 {
                continue;
            }
            let Some(center) = self.physics.space.body(f.key).map(|b| b.center) else { continue };
            let data = Value::map().with("entity", self.world.path_of(f.key).as_str()).with("id", f.key).with("position", v3(center));
            self.logic.push_engine_event("fell", data, tick);
        }
    }

    /// The bodies that are out of the world now (`physics_info`'s `out_of_world`; a headless
    /// run says them): each with its path and how far beyond everything it could land on.
    pub(crate) fn out_of_world_json(&self) -> specula_core::Json {
        use specula_core::Json;
        let (out, more) = fallen(&self.physics.space, self.world.env().physics.gravity, None, MAX_FELL_TOLD);
        let total = out.len() + more;
        Json::obj([
            ("total", Json::from(total as u64)),
            (
                "bodies",
                Json::arr(out.iter().map(|f| {
                    let id = crate::physics::ragdoll::unkey(f.key).map_or(f.key, |(e, _)| e);
                    Json::obj([("id", Json::from(id)), ("path", Json::from(self.world.path_of(id))), ("beyond_m", Json::from((f64::from(f.beyond_m) * 10.0).round() / 10.0))])
                })),
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::{Body, Collider, ColliderShape, Physics};
    use crate::world::World;
    use crate::world::{Recording, Shape as WShape, SpawnSpec};
    use specula_core::math::Vec3;

    const DT: f32 = 1.0 / 60.0;

    struct Thing<'a> {
        shape: WShape,
        name: &'a str,
        at: Vec3,
        scale: Vec3,
        collider: Collider,
        body: Option<Body>,
        tags: &'a [&'a str],
    }

    fn thing(w: &mut World, t: Thing) -> u64 {
        let mut s = SpawnSpec::new(t.shape);
        s.name = Some(t.name.into());
        s.position = t.at;
        s.scale = t.scale;
        s.collider = Some(t.collider);
        s.body = t.body;
        s.tags = t.tags.iter().map(|tag| (*tag).to_owned()).collect();
        w.spawn(s, "t").unwrap().id
    }

    fn floor(w: &mut World) -> u64 {
        thing(w, Thing { shape: WShape::Cube, name: "floor", at: Vec3::new(0.0, -0.1, 0.0), scale: Vec3::new(8.0, 0.2, 8.0), collider: Collider::default(), body: None, tags: &[] })
    }

    fn ball(w: &mut World, name: &str, at: Vec3, tags: &[&str]) -> u64 {
        let sphere = Collider { shape: ColliderShape::Sphere { radius: 0.5 }, ..Collider::default() };
        let body = Some(Body::new(crate::physics::BodyKind::Dynamic, 1.0));
        thing(w, Thing { shape: WShape::Sphere, name, at, scale: Vec3::new(1.0, 1.0, 1.0), collider: sphere, body, tags })
    }

    fn step(w: &mut World, p: &mut Physics, tick: u64) {
        let settings = w.env().physics;
        w.set_recording(Recording::Snapshot);
        p.tick(w, 0, &settings, tick, DT);
        w.set_recording(Recording::Full);
        let _ = w.take_changes();
    }

    /// A ball dropped beside the floor is found once, at the tick it passes five metres beyond
    /// the floor's underside; from then on it is out of the world. One on the floor never is,
    /// and with no gravity nothing falls.
    #[test]
    fn a_body_that_leaves_the_floor_is_found_once_as_it_passes() {
        let mut w = World::default();
        let _ = floor(&mut w);
        let lost = ball(&mut w, "lost", Vec3::new(20.0, 1.0, 0.0), &[]);
        let _safe = ball(&mut w, "safe", Vec3::new(0.0, 1.0, 0.0), &[]);
        let mut p = Physics::default();
        let gravity = w.env().physics.gravity;
        let (mut found_at, mut out_before) = (Vec::new(), 0);
        for tick in 0..240 {
            step(&mut w, &mut p, tick);
            let (passing, more) = fallen(&p.space, gravity, Some(DT), MAX_FELL_TOLD);
            assert_eq!(more, 0);
            if let Some(f) = passing.first() {
                assert!(passing.len() == 1 && f.key == lost, "{passing:?}");
                let y = p.space.body(lost).unwrap().center.y;
                // The floor's underside is at -0.2: the line is at -5.2, and it has just passed it.
                assert!(y < -5.2 && y > -5.7 && f.beyond_m >= FELL_BELOW_M && f.beyond_m < FELL_BELOW_M + 0.5, "passed at y {y}: {f:?}");
                found_at.push(tick);
            }
            if found_at.is_empty() {
                out_before += fallen(&p.space, gravity, None, MAX_FELL_TOLD).0.len();
            }
        }
        assert_eq!(found_at.len(), 1, "told once: {found_at:?}");
        assert_eq!(out_before, 0, "not out of the world before it passed the line");
        let (out, _) = fallen(&p.space, gravity, None, MAX_FELL_TOLD);
        assert!(out.len() == 1 && out[0].key == lost && out[0].beyond_m > FELL_BELOW_M, "{out:?}");
        assert!(fallen(&p.space, [0.0; 3], None, MAX_FELL_TOLD).0.is_empty(), "with no gravity nothing falls out");
        // Past the most told, the rest are counted.
        assert_eq!(fallen(&p.space, gravity, None, 0), (Vec::new(), 1));
    }

    /// A tagged ball landing on the floor is told as a `begin` with the ball first and the normal
    /// from the ball toward the floor; an untagged pair is not told; past the most told, the rest
    /// are counted.
    #[test]
    fn a_tagged_things_touch_is_told_with_it_first() {
        let mut w = World::default();
        let ground = floor(&mut w);
        let heard = ball(&mut w, "heard", Vec3::new(-2.0, 1.0, 0.0), &[CONTACTS_TAG]);
        let _quiet = ball(&mut w, "quiet", Vec3::new(2.0, 1.0, 0.0), &[]);
        let mut p = Physics::default();
        let mut begins = Vec::new();
        let mut all_begins = 0;
        for tick in 0..90 {
            step(&mut w, &mut p, tick);
            let events = &p.last.as_ref().unwrap().1.events;
            all_begins += events.iter().filter(|e| e.kind == EventKind::ContactBegin).count();
            let (told, more) = contacts_told(events, &w, MAX_CONTACTS_TOLD);
            assert_eq!(more, 0);
            // With no room nothing is told, and what there was is counted.
            let (none, uncounted) = contacts_told(events, &w, 0);
            assert!(none.is_empty() && uncounted as usize == told.len());
            begins.extend(told.into_iter().filter(|t| t.get("kind").and_then(Value::as_str) == Some("begin")));
        }
        assert!(all_begins >= 2, "both balls landed: {all_begins}");
        let first = begins.first().expect("the tagged ball's landing is told");
        let word = |k: &str| first.get(k).and_then(Value::as_str).map(str::to_owned);
        assert_eq!((word("a").as_deref(), word("b").as_deref()), (Some("/heard"), Some("/floor")), "{first:?}");
        assert_eq!((first.get("a_id").and_then(Value::as_u64), first.get("b_id").and_then(Value::as_u64)), (Some(heard), Some(ground)));
        let normal = first.get("normal").and_then(Value::as_vec3).expect("a normal");
        assert!(normal.y < -0.9, "from the ball toward the floor: {normal:?}");
        assert!(first.get("speed").and_then(Value::as_f32).is_some_and(|s| s > 1.0), "it was falling: {first:?}");
        assert!(begins.iter().all(|b| b.get("a").and_then(Value::as_str) == Some("/heard")), "only the tagged ball's touches are told: {begins:?}");
    }

    /// A game tags its root group and hears of everything under it: a ball with no tag of its
    /// own, below a tagged group, is told; one beside the group is not.
    #[test]
    fn a_tag_on_a_group_covers_what_is_under_it() {
        let mut w = World::default();
        let _ = floor(&mut w);
        let mut group = SpawnSpec::new(WShape::Group);
        group.name = Some("game".into());
        group.tags = [CONTACTS_TAG.to_owned()].into_iter().collect();
        let game = w.spawn(group, "t").unwrap().id;
        let inside = ball(&mut w, "inside", Vec3::new(-2.0, 1.0, 0.0), &[]);
        w.update(&crate::world::EntityKey::Id(inside), crate::world::UpdateSpec { parent: Some(Some(crate::world::EntityKey::Id(game))), ..Default::default() }, "t").unwrap();
        let outside = ball(&mut w, "outside", Vec3::new(2.0, 1.0, 0.0), &[]);
        assert!(hears_contacts(&w, inside) && hears_contacts(&w, game) && !hears_contacts(&w, outside));
        let mut p = Physics::default();
        let mut begins = Vec::new();
        for tick in 0..90 {
            step(&mut w, &mut p, tick);
            let (told, _) = contacts_told(&p.last.as_ref().unwrap().1.events, &w, MAX_CONTACTS_TOLD);
            begins.extend(told.into_iter().filter(|t| t.get("kind").and_then(Value::as_str) == Some("begin")));
        }
        assert!(!begins.is_empty() && begins.iter().all(|b| b.get("a").and_then(Value::as_str) == Some("/game/inside")), "{begins:?}");
    }

    /// What touches a thing can be asked of anything with a collider, tagged or not, while the
    /// physics is lent for a tick; with no physics there is nothing to ask.
    #[test]
    fn what_touches_a_thing_is_answered_for_any_thing() {
        use crate::logic::character::CharacterMover;
        let mut w = World::default();
        let ground = floor(&mut w);
        let one = ball(&mut w, "one", Vec3::new(-2.0, 0.6, 0.0), &[]);
        let two = ball(&mut w, "two", Vec3::new(2.0, 0.6, 0.0), &[]);
        let high = ball(&mut w, "high", Vec3::new(0.0, 40.0, 0.0), &[]);
        let mut p = Physics::default();
        for tick in 0..60 {
            step(&mut w, &mut p, tick);
        }
        let mut mover = crate::physics::mover::PhysicsMover::default();
        assert_eq!(mover.touching(one), None, "no physics is lent");
        mover.physics = Some(p);
        assert_eq!(mover.touching(one), Some(vec![ground]));
        assert_eq!(mover.touching(ground), Some(vec![one, two]), "in increasing order");
        assert_eq!(mover.touching(high), Some(Vec::new()), "still in the air");
    }
}
