//! Review R3 of the editor lane, as permanent tests: references held by identity through the
//! write policy, the board and the review queue, across save reads (the board and the queue
//! stay while the live world is replaced), with no per-loop or per-write work.
//!
//! - `race` (major 1): a note or critique made in the same serve window as renames.
//! - `follow` (minor 1): renames, moves, undo, delete then respawn at the same path.
//! - modules' and games' renames (minor 2): recorded by snapshot, no `prev_path`.
//! - `tput` / `window` (major 2): the cost at the limits (8192 notes + 8192 critiques).
//! - identities never reused after an older save is read; old files load and their notes
//!   detach; `world_hash` and journal replays are unchanged.

use super::*;
use crate::board::{Board, NoteQuery, NoteTarget, TaskSpec};
use crate::journal::{Kind, UndoRequest, Writer};
use crate::review::{CritiqueQuery, CritiqueSpec, Target};
use crate::swarm::Swarm;
use crate::world::{EntityKey, Recording, Shape, SpawnSpec, UpdateSpec};
use crate::write_policy::{Live, Rule};

const OWN: Writer<'static> = Writer { name: "own", session: Some("own"), via: None };
const OTHER: Writer<'static> = Writer { name: "other", session: Some("other"), via: None };

/// One change of a batch: `(id, new name, new parent)` (`Some(None)`: the top level).
type Set<'a> = (u64, Option<&'a str>, Option<Option<u64>>);

struct T {
    live: Live,
    sw: Swarm,
}

impl T {
    fn new() -> T {
        T { live: Live::new(World::default()), sw: Swarm::default() }
    }

    fn spawn(&mut self, name: &str, parent: Option<u64>) -> u64 {
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some(name.into());
        s.parent = parent.map(EntityKey::Id);
        self.live.commit(&mut self.sw, 0, OWN, Kind::Spawn, Rule::World, |wd| wd.spawn(s, "own").map(|e| e.id)).unwrap().0
    }

    /// Changes in one commit (one `world_batch`): `(id, new name, new parent)`.
    fn batch(&mut self, sets: &[Set<'_>]) -> Option<u64> {
        let sets: Vec<(u64, UpdateSpec)> = sets
            .iter()
            .map(|(id, name, parent)| {
                (*id, UpdateSpec { name: name.map(str::to_owned), parent: parent.map(|p| p.map(EntityKey::Id)), ..UpdateSpec::default() })
            })
            .collect();
        self.live
            .commit(&mut self.sw, 0, OWN, Kind::Batch, Rule::World, |wd| {
                for (id, set) in sets {
                    wd.update(&EntityKey::Id(id), set, "own")?;
                }
                Ok(())
            })
            .unwrap()
            .1
    }

    fn delete(&mut self, id: u64) {
        self.live.commit(&mut self.sw, 0, OWN, Kind::Delete, Rule::World, |wd| wd.delete(&EntityKey::Id(id), None).map(|_| ())).unwrap();
    }

    fn note(&mut self, id: u64, text: &str) {
        let r = EntityRef::of(&self.live, id).unwrap();
        self.sw.board.add_note(OTHER, 1, NoteTarget::Entity(r), text.into()).unwrap();
    }

    fn critique(&mut self, id: u64, text: &str) {
        let spec = CritiqueSpec {
            text: text.into(),
            severity: "note".into(),
            source: "probe".into(),
            target: Some(Target::Entity(EntityRef::of(&self.live, id).unwrap())),
            ..Default::default()
        };
        self.sw.review.add_critiques(OTHER, crate::authority::Authority::Agent, 1, vec![spec]).unwrap();
    }

    /// The note with `text` as `note_list` shows its entity.
    fn noted(&mut self, text: &str) -> Json {
        let all = self.sw.board.list_notes(&NoteQuery { limit: 200, ..NoteQuery::default() }, &self.live);
        let n = all.get("notes").and_then(Json::as_array).unwrap().iter().find(|n| n.get("text").and_then(Json::as_str) == Some(text)).cloned();
        n.and_then(|n| n.get("entity").cloned()).unwrap_or_else(|| panic!("no note {text}"))
    }

    /// The critique with `text` as `critique_list` shows its entity.
    fn critiqued(&mut self, text: &str) -> Json {
        let all = self.sw.review.list_critiques(&CritiqueQuery { limit: 200, detailed: true, ..CritiqueQuery::default() }, &self.live);
        let c = all.get("critiques").and_then(Json::as_array).unwrap().iter().find(|c| c.get("text").and_then(Json::as_str) == Some(text)).cloned();
        c.and_then(|c| c.get("entity").cloned()).unwrap_or_else(|| panic!("no critique {text}"))
    }

    /// The world's files as a save holds them.
    fn save(&self) -> Vec<(String, String)> {
        crate::persist::render(&self.live)
    }

    /// `save_read`: the board and the queue stay; the world is loaded from `files` and replaces
    /// the live one as the engine's `replace_world` does.
    fn read(&mut self, files: &[(String, String)]) {
        let (world, _) = crate::persist::load_with(|f| Ok(files.iter().find(|(n, _)| n == f).map(|(_, t)| t.clone()).expect("file"))).unwrap();
        self.replace(world);
    }

    fn replace(&mut self, world: World) {
        self.sw.remember_entity_paths(&self.live);
        let may = crate::write_policy::may_replace(&self.sw, OWN, &crate::playtest_engine::Playtests::default()).unwrap();
        self.live.replace(may, &mut self.sw, world, "world:s", 0.0, None);
    }

    fn path(&self, id: u64) -> String {
        self.live.path_of(id)
    }
}

fn on(id: u64, path: &str) -> Json {
    Json::obj([("id", Json::from(id)), ("path", Json::from(path))])
}

fn detached(path: &str) -> Json {
    Json::obj([("path", Json::from(path)), ("detached", Json::from(true))])
}

/// R3 major 1 (`probe_race.py`): agent A's `world_batch [rename /aK->/bK, rename /cK->/aK]` and
/// agent B's `note_add` on `/cK/x` in the same serve window. The note was given the batch's
/// prefix rewrites and landed on the other `x` after a save read (16 of 30 notes, 5 of 30
/// critiques). An identity has no path to rewrite: every order, every reference, the same entity.
#[test]
fn race_a_reference_made_beside_renames_stays_on_its_entity_across_a_save_read() {
    let mut t = T::new();
    let mut pairs = Vec::new();
    for k in 0..30 {
        let a = t.spawn(&format!("a{k}"), None);
        let ax = t.spawn("x", Some(a));
        let c = t.spawn(&format!("c{k}"), None);
        let cx = t.spawn("x", Some(c));
        let renames = [(a, Some(format!("b{k}"))), (c, Some(format!("a{k}")))];
        let rename = |t: &mut T| t.batch(&renames.iter().map(|(id, n)| (*id, n.as_deref(), None)).collect::<Vec<_>>());
        // Before, beside (no loop between) or after the batch: no difference.
        match k % 3 {
            0 => {
                t.note(cx, &format!("race{k}"));
                t.critique(cx, &format!("race{k}"));
                rename(&mut t);
            }
            1 => {
                rename(&mut t);
                t.note(cx, &format!("race{k}"));
                t.critique(cx, &format!("race{k}"));
            }
            _ => {
                t.note(cx, &format!("race{k}"));
                rename(&mut t);
                t.critique(cx, &format!("race{k}"));
            }
        }
        pairs.push((k, cx, ax));
    }
    for &(k, cx, _) in &pairs {
        assert_eq!(t.path(cx), format!("/a{k}/x"));
        assert_eq!(t.noted(&format!("race{k}")), on(cx, &format!("/a{k}/x")));
        assert_eq!(t.critiqued(&format!("race{k}")), on(cx, &format!("/a{k}/x")));
    }
    let files = t.save();
    t.read(&files);
    for &(k, cx, ax) in &pairs {
        assert_eq!(t.noted(&format!("race{k}")), on(cx, &format!("/a{k}/x")), "the note is on the noted entity, never on {ax}");
        assert_eq!(t.critiqued(&format!("race{k}")), on(cx, &format!("/a{k}/x")));
    }
}

/// Review R2's probe 8, kept: a note on /gate, /gate renamed /door, a new /gate, then a save
/// written and read. The note is on /door's entity (and the critique too), never the new /gate.
#[test]
fn probe8_a_note_on_a_renamed_entity_stays_on_it_across_a_save_read() {
    let mut t = T::new();
    let gate = t.spawn("gate", None);
    t.note(gate, "hinges are placeholders");
    t.critique(gate, "the hinge is flat");
    t.batch(&[(gate, Some("door"), None)]);
    let new_gate = t.spawn("gate", None);
    let files = t.save();
    t.read(&files);
    assert_eq!(t.noted("hinges are placeholders"), on(gate, "/door"));
    assert_eq!(t.critiqued("the hinge is flat"), on(gate, "/door"));
    let on_new = t.sw.board.list_notes(&NoteQuery { entity: Some(new_gate), limit: 10, ..NoteQuery::default() }, &t.live);
    assert_eq!(on_new.get("total"), Some(&Json::from(0)), "never on the new /gate");
}

/// R3 minor 1 and `probe_follow.py`'s cases: chains of renames, undo, reparenting, a batch with
/// interleaved renames, delete then respawn at the same path, rename then delete; then a save
/// read. A deleted entity's reference is detached (with its last known path) and never handed to
/// the newcomer at its path, before or after the read.
#[test]
fn follow_references_survive_moves_and_undo_and_deletes_detach_them() {
    let mut t = T::new();
    // A chain of renames on the parent.
    let p = t.spawn("p", None);
    let pc = t.spawn("kid", Some(p));
    t.note(pc, "chain");
    for n in ["q", "r", "s"] {
        t.batch(&[(p, Some(n), None)]);
    }
    assert_eq!(t.noted("chain"), on(pc, "/s/kid"));
    // Undo of a rename.
    let u = t.spawn("u", None);
    let uc = t.spawn("kid", Some(u));
    t.note(uc, "undo");
    t.note(u, "undo-self");
    let seq = t.batch(&[(u, Some("v"), None)]).unwrap();
    assert_eq!((t.noted("undo"), t.noted("undo-self")), (on(uc, "/v/kid"), on(u, "/v")));
    t.live.undo(&mut t.sw, 0, OWN, &UndoRequest { ids: Some(vec![seq]), count: 1, force: false }).unwrap();
    assert_eq!((t.noted("undo"), t.noted("undo-self")), (on(uc, "/u/kid"), on(u, "/u")));
    // Reparenting across subtrees, then to the top level.
    let s1 = t.spawn("s1", None);
    let s2 = t.spawn("s2", None);
    let k = t.spawn("k", Some(s1));
    let kk = t.spawn("kk", Some(k));
    t.note(kk, "reparent");
    t.batch(&[(k, None, Some(Some(s2)))]);
    assert_eq!(t.noted("reparent"), on(kk, "/s2/k/kk"));
    t.batch(&[(k, None, Some(None))]);
    assert_eq!(t.noted("reparent"), on(kk, "/k/kk"));
    let _ = s1;
    // One batch renaming parent, child, parent again.
    let b = t.spawn("bp", None);
    let bc = t.spawn("bc", Some(b));
    t.note(bc, "batch");
    t.batch(&[(b, Some("bp2"), None), (bc, Some("bc2"), None), (b, Some("bp3"), None)]);
    assert_eq!(t.noted("batch"), on(bc, "/bp3/bc2"));
    // Delete, then a newcomer at the same path: detached, never the newcomer's.
    let g = t.spawn("gate", None);
    t.note(g, "gate");
    t.critique(g, "gate");
    t.noted("gate");
    t.delete(g);
    let g2 = t.spawn("gate", None);
    assert_ne!(g2, g);
    assert_eq!((t.noted("gate"), t.critiqued("gate")), (detached("/gate"), detached("/gate")));
    // Rename then delete: detached with the last path it was seen at (it was listed after the
    // rename), never on what takes either path.
    let d = t.spawn("dd", None);
    t.note(d, "dd");
    t.batch(&[(d, Some("ee"), None)]);
    assert_eq!(t.noted("dd"), on(d, "/ee"));
    t.delete(d);
    t.spawn("ee", None);
    t.spawn("dd", None);
    assert_eq!(t.noted("dd"), detached("/ee"));
    // A save read: the attached ones stay on their entities, the detached ones stay detached.
    let files = t.save();
    t.read(&files);
    assert_eq!(t.noted("chain"), on(pc, "/s/kid"));
    assert_eq!(t.noted("undo"), on(uc, "/u/kid"));
    assert_eq!(t.noted("reparent"), on(kk, "/k/kk"));
    assert_eq!(t.noted("batch"), on(bc, "/bp3/bc2"));
    assert_eq!((t.noted("gate"), t.critiqued("gate")), (detached("/gate"), detached("/gate")));
    assert_eq!(t.noted("dd"), detached("/ee"));
    // Filters: by entity (attached only) and by path prefix (where the entity is now).
    let by_entity = t.sw.board.list_notes(&NoteQuery { entity: Some(pc), limit: 10, ..NoteQuery::default() }, &t.live);
    assert_eq!(by_entity.get("total"), Some(&Json::from(1)));
    let under_s = t.sw.board.list_notes(&NoteQuery { path_prefix: Some("/s".into()), limit: 10, ..NoteQuery::default() }, &t.live);
    assert_eq!(under_s.get("total"), Some(&Json::from(1)), "/s/kid is under /s (and /s1, /s2 aren't)");
    let under_gate = t.sw.board.list_notes(&NoteQuery { path_prefix: Some("/gate".into()), limit: 10, ..NoteQuery::default() }, &t.live);
    assert_eq!(under_gate.get("total"), Some(&Json::from(1)), "a detached note filters by its last known path");
}

/// An undone delete brings the same identity back, so its references attach again.
#[test]
fn undoing_a_delete_attaches_its_references_again() {
    let mut t = T::new();
    let g = t.spawn("gate", None);
    t.note(g, "gate");
    let seq = t.live.commit(&mut t.sw, 0, OWN, Kind::Delete, Rule::World, |wd| wd.delete(&EntityKey::Id(g), None).map(|_| ())).unwrap().1.unwrap();
    assert_eq!(t.noted("gate"), detached("/gate"));
    t.live.undo(&mut t.sw, 0, OWN, &UndoRequest { ids: Some(vec![seq]), count: 1, force: false }).unwrap();
    assert_eq!(t.noted("gate"), on(g, "/gate"));
}

/// R3 minor 2: renames made by modules and the hosted game are recorded by snapshot, with no
/// `prev_path`, so descendants' notes didn't follow. Identity doesn't read the journal at all.
#[test]
fn renames_recorded_by_snapshot_carry_every_reference() {
    let mut w = World::default();
    let put = |w: &mut World, name: &str, parent: Option<u64>| {
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some(name.into());
        s.parent = parent.map(EntityKey::Id);
        w.spawn(s, "t").unwrap().id
    };
    let yard = put(&mut w, "yard", None);
    let post = put(&mut w, "post", Some(yard));
    let lamp = put(&mut w, "lamp", Some(post));
    let mut refs = [EntityRef::of(&w, post).unwrap(), EntityRef::of(&w, lamp).unwrap()];
    w.set_recording(Recording::Snapshot);
    w.update(&EntityKey::Id(yard), UpdateSpec { name: Some("court".into()), ..UpdateSpec::default() }, "sepulcra").unwrap();
    let _ = w.take_changes();
    for r in &mut refs {
        assert!(r.attached(&w).is_some());
        r.refresh(&w);
    }
    assert_eq!((refs[0].path.as_str(), refs[1].path.as_str()), ("/court/post", "/court/post/lamp"));
}

/// Requirement 8: an identity is never reused within a lineage, even after reading an older
/// save rolls the id counter back. A note on entity 4 of the newer timeline is never handed to
/// the entity that takes id 4 after the older save is read, whatever is read in whatever order.
#[test]
fn identities_are_never_reused_after_an_older_save_is_read() {
    let mut t = T::new();
    let gate = t.spawn("gate", None);
    let older = t.save();
    let x = t.spawn("x", None);
    t.note(x, "on x");
    t.note(gate, "on gate");
    let newer = t.save();
    t.read(&older);
    let y = t.spawn("x", None);
    assert_eq!(y, x, "the id counter went back: the newcomer has x's id and path");
    assert_eq!(t.noted("on x"), detached("/x"), "but never x's identity");
    assert_eq!(t.noted("on gate"), on(gate, "/gate"), "gate is in both saves: the same entity");
    t.note(y, "on y");
    let older_again = t.save();
    t.read(&newer);
    assert_eq!((t.noted("on x"), t.noted("on y")), (on(x, "/x"), detached("/x")), "x is back; y isn't in the newer save");
    t.read(&older_again);
    assert_eq!((t.noted("on x"), t.noted("on y")), (detached("/x"), on(y, "/x")));
    // Reading the same save twice: entities spawned after each read never share an identity.
    t.read(&older);
    let z1 = t.spawn("z", None);
    t.note(z1, "z1");
    t.read(&older);
    let z2 = t.spawn("z", None);
    assert_eq!(z1, z2);
    assert_eq!(t.noted("z1"), detached("/z"));
    // A writer that replaced the live world (a bug the policy recovers from) forks it too.
    let before = t.live.lineage();
    let (live, sw) = (&mut t.live, &mut t.sw);
    // (Reported as a bug: a debug assertion, caught here.)
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| live.raw_writer_for_test(sw, 0, None, |w| *w = w.clone())));
    assert_ne!(t.live.lineage(), before);
}

/// Old files: a world file of format version 1 (no `born`) loads with fresh identities, a board
/// or review file without them loads its references detached, and the next save writes the
/// world again in version 2 (a kept version 1 file would hand out fresh identities on every
/// load).
#[test]
fn old_files_load_with_fresh_identities_and_their_references_detach() {
    let mut t = T::new();
    let gate = t.spawn("gate", None);
    let yard = t.spawn("yard", None);
    let _post = t.spawn("post", Some(yard));
    t.note(gate, "gate");
    t.critique(gate, "gate");
    // As version 1 wrote them: no `born`, the old format, and references without identities.
    let v1: Vec<(String, String)> = t
        .save()
        .into_iter()
        .map(|(n, text)| {
            let born = |l: &str| l.find(",\"born\":\"").map(|i| format!("{}{}", &l[..i], &l[i + 26..])).unwrap_or_else(|| l.to_owned());
            let text: String = text.lines().map(|l| born(l) + "\n").collect();
            (n, text.replace(crate::persist::FORMAT, crate::persist::FORMAT_V1))
        })
        .collect();
    assert!(v1.iter().all(|(_, t)| !t.contains("\"born\"") && t.contains(crate::persist::FORMAT_V1)), "{v1:?}");
    let board = Board::parse(&t.sw.board.render().replace(&format!(",\"born\":\"{:016x}\"", t.live.lineage()), "")).unwrap();
    let review = crate::review::Review::parse(&t.sw.review.render().replace(&format!(",\"born\":\"{:016x}\"", t.live.lineage()), "")).unwrap();
    let (world, saved) = crate::persist::load_with(|f| Ok(v1.iter().find(|(n, _)| n == f).map(|(_, t)| t.clone()).expect("file"))).unwrap();
    assert!(saved.subtrees.is_empty(), "version 1 subtree files are written again by the next save");
    assert_ne!(world.uid_of(gate).unwrap().lineage, t.live.lineage(), "fresh identities");
    assert_eq!(world.path_of(gate), "/gate");
    t.replace(world);
    t.sw.board = board;
    t.sw.review = review;
    assert_eq!((t.noted("gate"), t.critiqued("gate")), (detached("/gate"), detached("/gate")));
    // Saved again: version 2, with identities.
    let again = t.save();
    assert!(again.iter().all(|(_, text)| text.contains(crate::persist::FORMAT) && !text.contains(crate::persist::FORMAT_V1)));
    assert!(again.iter().any(|(_, text)| text.contains("\"born\":\"")));
}

/// Determinism (requirement 6): the identity is metadata. Worlds with the same content and
/// different lineages hash the same, a save and load keeps the hash, and the journal's replay of
/// a run across a save read reproduces the live world's hash.
#[test]
fn world_hash_and_replays_ignore_identities() {
    let build = || {
        let mut t = T::new();
        let a = t.spawn("a", None);
        let b = t.spawn("b", Some(a));
        t.batch(&[(a, Some("c"), None), (b, None, Some(None))]);
        t
    };
    let (mut one, two) = (build(), build());
    assert_ne!(one.live.lineage(), two.live.lineage());
    assert_eq!(one.live.state_hash(), two.live.state_hash());
    let files = one.save();
    let hash = one.live.state_hash();
    one.read(&files);
    assert_eq!(one.live.state_hash(), hash, "a save and a read keep the hash");
    // Entities from the save (its lineage) and spawned after (a new lineage) side by side.
    let d = one.spawn("d", None);
    one.batch(&[(d, Some("e"), None)]);
    assert_ne!(one.live.uid_of(d).unwrap().lineage, one.live.uid_of(1).unwrap().lineage);
    let replayed = one.sw.journal().replay().expect("replayable");
    assert_eq!(replayed.state_hash(), one.live.state_hash(), "the journal's replay reproduces world_hash");
    assert_eq!(replayed.uid_of(d), one.live.uid_of(d), "and carries the identities along");
}

/// Task-board `entities` (the open R1 item): by identity too, shown attached or detached, saved
/// with identities, and an id with no entity is refused.
#[test]
fn task_entities_are_references_by_identity() {
    let mut t = T::new();
    let gate = t.spawn("gate", None);
    let wall = t.spawn("wall", None);
    let spec = TaskSpec { title: "hinges".into(), entities: EntityRef::of_ids(&t.live, &[gate, wall], "entities").unwrap(), ..TaskSpec::default() };
    let id = t.sw.board.post(OWN, 0, spec).unwrap().id;
    assert_eq!(EntityRef::of_ids(&t.live, &[gate, 99], "entities").unwrap_err().code, "not_found");
    t.batch(&[(gate, Some("door"), None)]);
    t.delete(wall);
    t.spawn("wall", None);
    assert!(t.sw.board.refresh_task_entities(id, &t.live));
    let shown = t.sw.board.get(id).unwrap().entities_json(&t.live);
    assert_eq!(shown, Json::arr([on(gate, "/door"), detached("/wall")]));
    let back = Board::parse(&t.sw.board.render()).unwrap();
    assert_eq!(back.get(id).unwrap().entities_json(&t.live), shown, "saved with identities");
    let files = t.save();
    t.read(&files);
    assert_eq!(t.sw.board.get(id).unwrap().entities_json(&t.live), shown);
}

fn full_stores(t: &mut T, n: usize) -> Vec<u64> {
    let ids: Vec<u64> = t
        .live
        .commit(&mut t.sw, 0, OWN, Kind::Batch, Rule::World, |wd| {
            (0..n)
                .map(|i| {
                    let mut s = SpawnSpec::new(Shape::Group);
                    s.name = Some(format!("r{i}"));
                    wd.spawn(s, "own").map(|e| e.id)
                })
                .collect::<Result<Vec<u64>, WorldError>>()
        })
        .unwrap()
        .0;
    for (i, &id) in ids.iter().enumerate() {
        let r = EntityRef::of(&t.live, id).unwrap();
        t.sw.board.add_note(OTHER, 1, NoteTarget::Entity(r), format!("n{i}")).unwrap();
    }
    for chunk in ids.chunks(32) {
        let specs = chunk
            .iter()
            .map(|&id| CritiqueSpec {
                text: format!("c{id}"),
                severity: "note".into(),
                source: "probe".into(),
                target: Some(Target::Entity(EntityRef::of(&t.live, id).unwrap())),
                ..Default::default()
            })
            .collect();
        t.sw.review.add_critiques(OTHER, crate::authority::Authority::Agent, 1, specs).unwrap();
    }
    ids
}

/// R3 major 2 (`probe_tput.py`, `probe_window.py`): with 8192 notes and 8192 critiques on
/// entities, every 1000-rename batch cost O(renames x references) on the engine thread (83-108
/// ms after one batch headless; a 131 ms frame in the window). Now a write does no reference
/// work at all (the stores don't move), and what is left runs once: a listing page, and the
/// paths remembered before a replacement.
#[test]
fn tput_writes_cost_no_reference_work_at_the_limits() {
    let n = crate::board::MAX_NOTES;
    assert_eq!(n, crate::review::MAX_CRITIQUES);
    let mut t = T::new();
    let ids = full_stores(&mut t, n);
    assert_eq!(t.sw.board.notes_len(), n);
    let (board, review) = (t.sw.board.revision(), t.sw.review.revision());
    // Five 1000-rename batches (the window probe sent six): the board and the queue never move.
    for round in 0..5 {
        let sets: Vec<(u64, String)> = ids.iter().take(1000).enumerate().map(|(i, &id)| (id, format!("{}{i}", if round % 2 == 0 { "z" } else { "r" }))).collect();
        t.batch(&sets.iter().map(|(id, name)| (*id, Some(name.as_str()), None)).collect::<Vec<_>>());
    }
    assert_eq!((t.sw.board.revision(), t.sw.review.revision()), (board, review), "no reference work per write (nothing to save)");
    // A listing page: only its notes are looked up.
    let started = std::time::Instant::now();
    let page = t.sw.board.list_notes(&NoteQuery { limit: 50, ..NoteQuery::default() }, &t.live);
    let listed = started.elapsed();
    assert_eq!(page.get("total"), Some(&Json::from(n)));
    assert!(t.sw.board.revision() > board, "the page's last known paths were refreshed (the renamed ones)");
    // Before a replacement, once: every reference's last known path, within the engine
    // thread's budget even in a debug build.
    let started = std::time::Instant::now();
    let changed = t.sw.remember_entity_paths(&t.live);
    let remembered = started.elapsed();
    assert_eq!(changed, 2 * 1000 - 51, "the renamed entities' notes (less the listed page and the one past it) and critiques");
    assert!(listed < specula_core::speed_budget(std::time::Duration::from_millis(250)), "a page of notes took {listed:?}");
    assert!(remembered < specula_core::speed_budget(std::time::Duration::from_millis(1500)), "remembering 16384 paths took {remembered:?}");
}

/// Probatio's survivors on the board: paging past a cursor with a filter, and what refreshing
/// reports (only references that moved; path and task notes, unknown tasks and unmoved
/// entities report nothing and leave the board unsaved).
#[test]
fn listings_page_by_cursor_and_refreshes_report_only_moves() {
    let mut t = T::new();
    let a = t.spawn("a", None);
    let b = t.spawn("b", None);
    for i in 0..5 {
        t.note(a, &format!("a{i}"));
        t.note(b, &format!("b{i}"));
    }
    t.sw.board.add_note(OTHER, 1, NoteTarget::Path("/a".into()), "on the path".into()).unwrap();
    let page = |t: &mut T, after: Option<u64>| t.sw.board.list_notes(&NoteQuery { entity: Some(a), after, limit: 2, ..NoteQuery::default() }, &t.live);
    let first = page(&mut t, None);
    let ids: Vec<u64> = first.get("notes").and_then(Json::as_array).unwrap().iter().filter_map(|n| n.get("id").and_then(Json::as_u64)).collect();
    assert_eq!((ids.len(), first.get("total").and_then(Json::as_u64)), (2, Some(5)));
    let next = first.get("next_cursor").and_then(Json::as_u64).expect("more follow");
    let second = page(&mut t, Some(next));
    let more: Vec<u64> = second.get("notes").and_then(Json::as_array).unwrap().iter().filter_map(|n| n.get("id").and_then(Json::as_u64)).collect();
    assert_eq!(more.len(), 2);
    assert!(more.iter().all(|id| *id > next && !ids.contains(id)), "{ids:?} then {more:?}");
    // Refreshing reports moves only.
    let task = t.sw.board.post(OWN, 0, TaskSpec { title: "t".into(), entities: EntityRef::of_ids(&t.live, &[a], "entities").unwrap(), ..TaskSpec::default() }).unwrap().id;
    let r = t.sw.board.revision();
    assert!(!t.sw.board.refresh_task_entities(task, &t.live), "nothing moved");
    assert!(!t.sw.board.refresh_task_entities(task + 100, &t.live), "no such task");
    assert_eq!(t.sw.remember_entity_paths(&t.live), 0, "nothing moved: path notes and current paths report nothing");
    assert_eq!(t.sw.board.revision(), r, "and the board stays saved");
    t.batch(&[(a, Some("c"), None)]);
    assert_eq!(t.sw.remember_entity_paths(&t.live), 6, "the five notes on a and the task");
    assert!(t.sw.board.revision() > r);
}
