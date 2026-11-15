//! The editor outliner's walk of the entity tree (docs/design/EDITOR.md, "Outliner"): a cursor
//! that pages through a huge world in path order and resumes on a later frame, and a search that
//! reads one slice of the world per window frame within a step and time budget.
//!
//! **Cost (rule 15, F3).** Stepping allocates nothing: each sibling run is one ordered range of
//! the world's child index ([`World::children_from`]), and a cursor copies a name per level only
//! when a walk stops. A search slice checks the clock every [`CHECK_EVERY`] entities and stops at
//! its [`Budget`], so a 100k-entity world costs a fraction of a millisecond per frame, never a
//! walk of the world (`tests/outliner_alloc.rs` pins both).

use crate::world::{Entity, World};
use std::time::{Duration, Instant};

/// A search slice reads the clock once per this many entities.
pub const CHECK_EVERY: usize = 256;

/// A resumable walk of the entity tree in path order: per level, the parent, the last child name
/// read, and whether that name is still to be read (a cursor made [`Cursor::at`] an entity).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cursor {
    frames: Vec<(Option<u64>, Option<String>, bool)>,
}

impl Cursor {
    /// From the first top-level entity.
    pub fn start() -> Cursor {
        Cursor { frames: vec![(None, None, false)] }
    }

    /// A cursor whose next entity is `id` (its ancestors' later siblings follow it).
    pub fn at(world: &World, id: u64) -> Cursor {
        let mut chain = Vec::new();
        let mut at = world.get(id);
        while let Some(e) = at {
            chain.push((e.parent, e.name.clone()));
            at = e.parent.and_then(|p| world.get(p));
        }
        chain.reverse();
        let n = chain.len();
        Cursor { frames: chain.into_iter().enumerate().map(|(i, (parent, name))| (parent, Some(name), i + 1 == n)).collect() }
    }

    /// The next entity in path order and its depth (1 = top level); `descend(id)` says whether
    /// to walk into its children.
    pub fn next(&mut self, world: &World, descend: &dyn Fn(u64) -> bool) -> Option<(u64, usize)> {
        let mut got = None;
        self.walk(world, descend, &mut |id, depth| {
            got = Some((id, depth));
            false
        });
        got
    }

    /// Walks on in path order, calling `visit(id, depth)` for each entity until it returns
    /// `false` (the cursor then resumes after that entity) or the tree ends (returns `true`).
    /// No allocation per entity: one range per sibling run, and one name copy per level when it
    /// stops.
    pub fn walk(&mut self, world: &World, descend: &dyn Fn(u64) -> bool, visit: &mut dyn FnMut(u64, usize) -> bool) -> bool {
        // The levels walked in this call (the deepest last): their ranges and the name last read.
        let mut live: Vec<(crate::world::Children<'_>, Option<&str>)> = Vec::with_capacity(8);
        loop {
            if live.is_empty() {
                let Some((parent, after, inclusive)) = self.frames.last() else { return true };
                live.push((world.children_from(*parent, after.as_deref(), *inclusive), None));
            }
            let depth = self.frames.len();
            let Some((range, last)) = live.last_mut() else { return true };
            match range.next() {
                None => {
                    live.pop();
                    self.frames.pop();
                }
                Some((id, name)) => {
                    *last = Some(name);
                    if world.has_children(id) && descend(id) {
                        self.frames.push((Some(id), None, false));
                        live.push((world.children_from(Some(id), None, false), None));
                    }
                    if !visit(id, depth) {
                        break;
                    }
                }
            }
        }
        // Where each level walked here stopped (levels above them are as they were).
        let base = self.frames.len() - live.len();
        for (i, (_, last)) in live.iter().enumerate() {
            if let Some(name) = last {
                let f = &mut self.frames[base + i];
                f.1 = Some((*name).to_owned());
                f.2 = false;
            }
        }
        false
    }
}

/// What a search looks for: names containing the text (ASCII case ignored), or a tag (`#tag`).
#[derive(Clone, Debug, PartialEq)]
pub enum Query {
    Name(String),
    Tag(String),
}

impl Query {
    pub fn parse(search: &str) -> Query {
        let s = search.trim().to_ascii_lowercase();
        match s.strip_prefix('#') {
            Some(t) => Query::Tag(t.to_owned()),
            None => Query::Name(s),
        }
    }

    /// Whether `e` matches (allocates nothing).
    pub fn matches(&self, e: &Entity) -> bool {
        match self {
            Query::Tag(t) => e.tags.iter().any(|x| x.eq_ignore_ascii_case(t)),
            Query::Name(n) => contains_ci(&e.name, n),
        }
    }
}

/// Whether `hay` contains `needle` (lower case), ignoring ASCII case, without allocating.
pub fn contains_ci(hay: &str, needle: &str) -> bool {
    needle.is_empty() || hay.as_bytes().windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

/// How much one slice of a search may do.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub max_steps: usize,
    pub max_time: Duration,
}

/// A search in progress: a page of matches in path order.
#[derive(Clone, Debug, Default)]
pub struct Scan {
    pub cursor: Cursor,
    /// The page's matches so far.
    pub matches: Vec<u64>,
    /// Entities read so far.
    pub read: usize,
    /// No more entities, or the page is full.
    pub done: bool,
    /// The page was full and another match follows: the next page starts at it.
    pub next: Option<Cursor>,
}

impl Scan {
    /// A search of a page of `page` matches from `cursor`.
    pub fn new(cursor: Cursor, page: usize) -> Scan {
        Scan { cursor, matches: Vec::with_capacity(page), read: 0, done: false, next: None }
    }

    /// One slice: reads entities until the page fills, the world ends or `budget` runs out (the
    /// clock is read every [`CHECK_EVERY`] entities). Returns the entities read. Allocates
    /// nothing per entity.
    pub fn step(&mut self, world: &World, query: &Query, page: usize, budget: Budget) -> usize {
        if self.done {
            return 0;
        }
        let started = Instant::now();
        let mut n = 0usize;
        let (matches, next, done) = (&mut self.matches, &mut self.next, &mut self.done);
        let ended = self.cursor.walk(world, &|_| true, &mut |id, _| {
            n += 1;
            if world.get(id).is_some_and(|e| query.matches(e)) {
                if matches.len() >= page {
                    *next = Some(Cursor::at(world, id));
                    *done = true;
                    return false;
                }
                matches.push(id);
            }
            if n >= budget.max_steps {
                return false;
            }
            !(n.is_multiple_of(CHECK_EVERY) && started.elapsed() >= budget.max_time)
        });
        if ended {
            self.done = true;
        }
        self.read += n;
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{EntityKey, Shape, SpawnSpec};

    fn world_with(paths: &[&str]) -> World {
        let mut w = World::default();
        for p in paths {
            let (parent, name) = p.rsplit_once('/').unwrap();
            let mut s = SpawnSpec::new(Shape::Cube);
            s.name = Some(name.into());
            s.parent = (!parent.is_empty()).then(|| EntityKey::Id(w.find_path(parent).unwrap()));
            w.spawn(s, "t").unwrap();
        }
        w
    }

    #[test]
    fn cursors_walk_in_path_order_and_resume() {
        let w = world_with(&["/a", "/a/x", "/a/y", "/b", "/b/z", "/c"]);
        let all = |c: &mut Cursor, descend: bool| {
            let mut out = Vec::new();
            while let Some((id, depth)) = c.next(&w, &|_| descend) {
                out.push((w.path_of(id), depth));
            }
            out
        };
        let full = all(&mut Cursor::start(), true);
        let paths: Vec<&str> = full.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, ["/a", "/a/x", "/a/y", "/b", "/b/z", "/c"]);
        assert_eq!(full[1].1, 2, "depth");
        let top = all(&mut Cursor::start(), false);
        assert_eq!(top.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(), ["/a", "/b", "/c"]);
        // Resuming mid-way gives the rest.
        let mut c = Cursor::start();
        c.next(&w, &|_| true);
        c.next(&w, &|_| true);
        let rest = all(&mut c.clone(), true);
        assert_eq!(rest.first().map(|r| r.0.as_str()), Some("/a/y"));
        // A cursor at an entity starts with it.
        let at = all(&mut Cursor::at(&w, w.find_path("/b/z").unwrap()), true);
        assert_eq!(at.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(), ["/b/z", "/c"]);
        // A walk stopped part-way resumes exactly where it stopped, whatever the depth.
        for stop in 1..=6 {
            let mut c = Cursor::start();
            let mut seen = Vec::new();
            let mut k = 0;
            c.walk(&w, &|_| true, &mut |id, _| {
                seen.push(w.path_of(id));
                k += 1;
                k < stop
            });
            while let Some((id, _)) = c.next(&w, &|_| true) {
                seen.push(w.path_of(id));
            }
            assert_eq!(seen, paths, "stopped after {stop}");
        }
    }

    #[test]
    fn a_search_pages_by_matches_within_its_budget() {
        let w = world_with(&["/a", "/a/crate_1", "/b", "/b/crate_2", "/b/crate_3", "/c", "/c/Crate_4"]);
        let q = Query::parse("CRATE");
        let all = Budget { max_steps: 1000, max_time: Duration::from_secs(5) };
        let mut s = Scan::new(Cursor::start(), 2);
        s.step(&w, &q, 2, all);
        assert!(s.done);
        assert_eq!(s.matches.iter().map(|id| w.path_of(*id)).collect::<Vec<_>>(), ["/a/crate_1", "/b/crate_2"]);
        let next = s.next.clone().expect("a third match starts the next page");
        let mut s2 = Scan::new(next, 2);
        s2.step(&w, &q, 2, all);
        assert_eq!(s2.matches.iter().map(|id| w.path_of(*id)).collect::<Vec<_>>(), ["/b/crate_3", "/c/Crate_4"]);
        assert!(s2.done && s2.next.is_none());
        // One step at a time finds the same, resuming every frame.
        let one = Budget { max_steps: 1, max_time: Duration::from_secs(5) };
        let mut s3 = Scan::new(Cursor::start(), 10);
        let mut slices = 0;
        while !s3.done {
            s3.step(&w, &q, 10, one);
            slices += 1;
            assert!(slices < 20);
        }
        assert_eq!(s3.matches.len(), 4);
        assert_eq!(s3.read, 7);
        // Tags.
        assert_eq!(Query::parse(" #Boss "), Query::Tag("boss".into()));
    }

    #[test]
    fn a_search_slice_stops_at_its_time_budget() {
        let mut w = World::default();
        for i in 0..20_000 {
            let mut s = SpawnSpec::new(Shape::Group);
            s.name = Some(format!("e{i:05}"));
            w.spawn(s, "t").unwrap();
        }
        let mut s = Scan::new(Cursor::start(), 18);
        let read = s.step(&w, &Query::parse("zq"), 18, Budget { max_steps: usize::MAX, max_time: Duration::ZERO });
        assert_eq!(read, CHECK_EVERY, "a spent budget stops at the first clock check");
        assert!(!s.done);
    }
}
