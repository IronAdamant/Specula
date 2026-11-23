//! Predictions: what a scenario will do, by running it on a copy, bounded; and the model's own
//! numbers for how fast heat crosses one gap, which the tests hold a run to.

use crate::error::ElementaError;
use crate::sim::{absorb, heat_rate, lean, ready, relax, Act, Elementa, EventKind, Node, Phase, MAX_ACTS_PER_TICK};
use specula_core::json::Json;
use specula_core::math::Vec3;

/// The longest scenario: ten minutes at 60 ticks a second.
pub const MAX_CHECK_TICKS: u32 = 36_000;
/// Ids of burnt nodes a prediction names; the rest are counted.
pub const MAX_NAMED: usize = 64;
/// The most steps [`Elementa::steps_to_ignite`] follows before it says "never".
pub const MAX_IGNITE_STEPS: u32 = 100_000;

/// What to try: acts at its start, the wind and ambient heat to hold, how long, and a ceiling on
/// the work (a run that would pass it stops and says so).
#[derive(Clone, Debug, PartialEq)]
pub struct Scenario {
    pub acts: Vec<Act>,
    pub wind: Option<Vec3>,
    pub ambient: Option<f32>,
    pub ticks: u32,
    pub max_ops: u64,
}

/// What the scenario came to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prediction {
    pub ticks: u32,
    pub ignited: u64,
    pub burnt: u64,
    pub extinguished: u64,
    pub frozen: u64,
    pub thawed: u64,
    /// Up to [`MAX_NAMED`] ids of nodes that burnt out, in the order they did.
    pub burnt_ids: Vec<u64>,
    pub burning_at_end: u32,
    pub peak_burning: u32,
    /// Ticks from the start to the first and last ignition.
    pub first_ignition: Option<u32>,
    pub last_ignition: Option<u32>,
    /// The tick (from the start) at which nothing was burning any more, if the fire went out.
    pub out_at: Option<u32>,
    /// Node updates plus work, in all.
    pub ops: u64,
    /// The most any one step cost.
    pub worst_step_ops: u64,
    pub worst_step_work: u32,
    /// Steps cut short by the work limit.
    pub deferred_steps: u32,
    /// The most nodes that waited for the burning limit in one step.
    pub most_waiting: u32,
    /// The ceiling on ops said before the run: the steps times the worst case of one.
    pub bound_ops: u64,
    /// The run stopped at `max_ops` before its ticks.
    pub cut: bool,
    /// The state's hash at the end, to compare with a real run.
    pub hash: u64,
}

impl Prediction {
    pub fn to_json(&self) -> Json {
        let opt = |v: Option<u32>| v.map_or(Json::Null, Json::from);
        Json::obj([
            ("ticks", Json::from(self.ticks)),
            ("ignited", Json::from(self.ignited)),
            ("burnt", Json::from(self.burnt)),
            ("extinguished", Json::from(self.extinguished)),
            ("frozen", Json::from(self.frozen)),
            ("thawed", Json::from(self.thawed)),
            ("burnt_ids", Json::arr(self.burnt_ids.iter().map(|&i| Json::from(i)))),
            ("burning_at_end", Json::from(self.burning_at_end)),
            ("peak_burning", Json::from(self.peak_burning)),
            ("first_ignition", opt(self.first_ignition)),
            ("last_ignition", opt(self.last_ignition)),
            ("out_at", opt(self.out_at)),
            ("ops", Json::from(self.ops)),
            ("worst_step_ops", Json::from(self.worst_step_ops)),
            ("worst_step_work", Json::from(self.worst_step_work)),
            ("deferred_steps", Json::from(self.deferred_steps)),
            ("most_waiting", Json::from(self.most_waiting)),
            ("bound_ops", Json::from(self.bound_ops)),
            ("cut", Json::from(self.cut)),
            ("hash", Json::from(format!("{:016x}", self.hash))),
        ])
    }
}

impl Elementa {
    /// The most ops a scenario of `ticks` ticks with `acts` acts can take: every spread step's
    /// worst case (every node updated, the whole work budget) and every act's (the node count).
    pub fn check_bound(&self, ticks: u32, acts: usize) -> u64 {
        let steps = u64::from(ticks) / u64::from(self.every);
        let n = self.nodes.len() as u64;
        steps * (n + u64::from(self.config.limits.work)) + acts as u64 * n.max(1)
    }

    /// Runs the scenario on a copy of this state (this one is not changed) and says what came of
    /// it. Bounded: at most [`MAX_CHECK_TICKS`] ticks and `max_ops` ops; meant for a worker
    /// thread (it never reads the world). Events already waiting here are not counted.
    pub fn check(&self, s: &Scenario) -> Result<Prediction, ElementaError> {
        if s.ticks == 0 || s.ticks > MAX_CHECK_TICKS {
            return Err(ElementaError::range("ticks", f64::from(s.ticks), 1.0, f64::from(MAX_CHECK_TICKS)));
        }
        if s.acts.len() > MAX_ACTS_PER_TICK as usize {
            return Err(ElementaError::limit("acts", u64::from(MAX_ACTS_PER_TICK), s.acts.len() as u64, "too many acts at a scenario's start"));
        }
        let mut sim = self.clone();
        sim.take_events();
        if let Some(w) = s.wind {
            sim.set_wind(w)?;
        }
        if let Some(a) = s.ambient {
            sim.set_ambient(a)?;
        }
        let mut p = Prediction { bound_ops: self.check_bound(s.ticks, s.acts.len()), ..Prediction::default() };
        for a in &s.acts {
            p.ops += u64::from(sim.act(*a)?.work);
        }
        let mut was_burning = sim.burning() > 0;
        for t in 1..=s.ticks {
            if p.ops >= s.max_ops {
                p.cut = true;
                break;
            }
            let r = *sim.step();
            p.ticks = t;
            p.ops += r.ops();
            p.worst_step_ops = p.worst_step_ops.max(r.ops());
            p.worst_step_work = p.worst_step_work.max(r.work);
            p.deferred_steps += u32::from(r.deferred > 0 || r.truncated > 0);
            p.most_waiting = p.most_waiting.max(r.waiting);
            p.peak_burning = p.peak_burning.max(r.burning);
            let ev = sim.take_events();
            if ev.count(EventKind::Ignited) > 0 {
                p.first_ignition.get_or_insert(t);
                p.last_ignition = Some(t);
            }
            for e in ev.list.iter().filter(|e| e.kind == EventKind::Burnt) {
                if p.burnt_ids.len() < MAX_NAMED {
                    p.burnt_ids.push(e.id);
                }
            }
            p.ignited += ev.count(EventKind::Ignited);
            p.burnt += ev.count(EventKind::Burnt);
            p.extinguished += ev.count(EventKind::Extinguished);
            p.frozen += ev.count(EventKind::Frozen);
            p.thawed += ev.count(EventKind::Thawed);
            if r.spread {
                if was_burning && r.burning == 0 {
                    p.out_at = Some(t);
                } else if r.burning > 0 {
                    p.out_at = None;
                }
                was_burning = r.burning > 0;
            }
        }
        p.burning_at_end = sim.burning();
        p.hash = sim.hash();
        Ok(p)
    }

    /// The model's own answer for one gap: how many spread steps a burning `from` takes to make
    /// `to` catch, in the wind and ambient heat set now, counting from the first step `from`
    /// gives heat; `None` if `to` never catches (it cannot burn, or it loses heat as fast as it
    /// gains it) within [`MAX_IGNITE_STEPS`]. The same arithmetic as a step, in the same order,
    /// so a run equals it exactly when nothing else heats `to`.
    pub fn steps_to_ignite(&self, from: &Node, to: &Node) -> Option<u32> {
        let mf = &self.config.materials[usize::from(from.material)];
        let mt = &self.config.materials[usize::from(to.material)];
        mt.ignite?;
        let reach = self.config.reach_m;
        let gap = crate::grid::box_gap(from.center, from.half, to.center, to.half);
        if gap > reach {
            return None;
        }
        let dt = 1.0 / self.config.spread_hz as f32;
        let q = heat_rate(mf.heat_out, gap, reach, lean(self.wind, from.center, to.center)) * dt;
        let mut n = to.clone();
        n.phase = if n.phase == Phase::Frozen { Phase::Frozen } else { Phase::Intact };
        for s in 1..=MAX_IGNITE_STEPS {
            if q > 0.0 {
                absorb(&mut n, q);
            }
            if n.phase == Phase::Frozen {
                if let Some(f) = mt.freeze {
                    if n.heat > f + crate::sim::THAW_MARGIN {
                        n.phase = Phase::Intact;
                    }
                }
            }
            if ready(&n, mt) {
                return Some(s);
            }
            relax(&mut n, mt, self.ambient, dt);
        }
        None
    }

    /// The speed a front moves along a line of things like `from` and `to` (their centres'
    /// distance apart), m/s, by [`Elementa::steps_to_ignite`].
    pub fn front_speed(&self, from: &Node, to: &Node) -> Option<f32> {
        let steps = self.steps_to_ignite(from, to)?;
        let dist = (to.center - from.center).length();
        Some(dist * self.config.spread_hz as f32 / steps as f32)
    }
}
