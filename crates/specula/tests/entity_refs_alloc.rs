//! Review R3 of the editor lane (major 2): with the stores full (8192 notes and 8192 critiques on
//! entities), references cost nothing while the world moves, and a lookup allocates nothing.
//!
//! References hold identities (`specula::entity_ref::EntityRef`), so nothing follows the world:
//! no per-loop or per-write work, and the board and review queue never move (so the engine's
//! per-loop "anything to save?" check stays a comparison). What is left is a lookup per reference
//! when listed (one map read, no allocation) and an allocation-free prefix filter.
//!
//! The only test in this file, so the counting allocator (per thread) sees nothing else.

use specula::board::{Board, NoteQuery, NoteTarget, MAX_NOTES};
use specula::entity_ref::{EntityRef, Prefix};
use specula::journal::Writer;
use specula::review::{CritiqueSpec, Review, Target, MAX_CRITIQUES};
use specula::world::{EntityKey, Recording, Shape, SpawnSpec, UpdateSpec, World};

#[global_allocator]
static ALLOC: specula_core::alloc::Counting<std::alloc::System> = specula_core::alloc::Counting::new(std::alloc::System);

fn allocations() -> u64 {
    specula_core::alloc::thread_counts().0
}

const OTHER: Writer<'static> = Writer { name: "other", session: Some("other"), via: None };

#[test]
fn full_reference_stores_cost_nothing_while_the_world_moves() {
    let n = MAX_NOTES;
    assert_eq!(n, MAX_CRITIQUES);
    let mut w = World::default();
    let ids: Vec<u64> = (0..n)
        .map(|i| {
            let mut s = SpawnSpec::new(Shape::Group);
            s.name = Some(format!("r{i}"));
            w.spawn(s, "t").unwrap().id
        })
        .collect();
    let mut board = Board::default();
    let mut review = Review::default();
    for (i, &id) in ids.iter().enumerate() {
        board.add_note(OTHER, 1, NoteTarget::Entity(EntityRef::of(&w, id).unwrap()), format!("n{i}")).unwrap();
    }
    for chunk in ids.chunks(32) {
        let specs = chunk
            .iter()
            .map(|&id| CritiqueSpec {
                text: format!("c{id}"),
                severity: "note".into(),
                source: "probe".into(),
                target: Some(Target::Entity(EntityRef::of(&w, id).unwrap())),
                ..Default::default()
            })
            .collect();
        review.add_critiques(OTHER, specula::authority::Authority::Agent, 1, specs).unwrap();
    }
    let refs: Vec<EntityRef> = ids.iter().map(|&id| EntityRef::of(&w, id).unwrap()).collect();
    let (b0, r0) = (board.revision(), review.revision());

    // The world moving as a game's ticks move it (recorded by snapshot), 1000 renames a tick,
    // with the engine's per-loop question of the stores ("changed since saved?") after each.
    w.set_recording(Recording::Snapshot);
    let mut loop_allocs = 0;
    for tick in 0..20 {
        for (i, &id) in ids.iter().take(1000).enumerate() {
            let name = format!("{}{i}", if tick % 2 == 0 { "z" } else { "r" });
            w.update(&EntityKey::Id(id), UpdateSpec { name: Some(name), ..UpdateSpec::default() }, "game").unwrap();
        }
        drop(w.take_changes());
        let before = allocations();
        let moved = board.revision() != b0 || review.revision() != r0;
        loop_allocs += allocations() - before;
        assert!(!moved, "tick {tick}: the stores never move with the world");
    }
    assert_eq!(loop_allocs, 0);

    // Every reference resolved (what a listing does per item), and a prefix filter over all of
    // them: no allocation at all.
    let prefix = Prefix::new(&w, "/r5");
    let before = allocations();
    let attached = refs.iter().filter(|r| r.attached(&w).is_some()).count();
    let under = refs.iter().filter(|r| r.under(&w, &prefix)).count();
    let lookups = allocations() - before;
    assert_eq!((attached, under), (n, 1));
    assert_eq!(lookups, 0, "resolving {n} references and filtering them allocated {lookups} times");

    // A listing page costs its page, not the store: the same allocations with 8192 notes as the
    // page would cost alone (its items' JSON and paths), whatever the total.
    let q = NoteQuery { limit: 20, ..NoteQuery::default() };
    board.list_notes(&q, &w);
    let before = allocations();
    let page = board.list_notes(&q, &w);
    let full = allocations() - before;
    assert_eq!(page.get("total").and_then(|t| t.as_u64()), Some(n as u64));
    let mut small = Board::default();
    for (i, &id) in ids.iter().take(40).enumerate() {
        small.add_note(OTHER, 1, NoteTarget::Entity(EntityRef::of(&w, id).unwrap()), format!("n{i}")).unwrap();
    }
    small.list_notes(&q, &w);
    let before = allocations();
    small.list_notes(&q, &w);
    let alone = allocations() - before;
    assert_eq!(full, alone, "a page of 20 allocated {full} times over 8192 notes, {alone} over 40");

    // Remembering every reference's path before a replacement: once the paths are current, it
    // checks them in place (no allocation per reference).
    board.remember_entity_paths(&w);
    review.remember_entity_paths(&w);
    let before = allocations();
    let changed = board.remember_entity_paths(&w) + review.remember_entity_paths(&w);
    let remembered = allocations() - before;
    assert_eq!(changed, 0);
    // (Its id lists grow by doubling: a few dozen allocations in all, never one per reference.)
    assert!(remembered <= 64, "checking {} paths allocated {remembered} times", 2 * n);
}
