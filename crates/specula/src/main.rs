//! The stock engine binary: `cargo run -p specula`.

/// Counts heap use for perf_memory, perf_frame's heap budget and per-zone allocations.
#[global_allocator]
static ALLOC: specula_core::alloc::Counting<std::alloc::System> = specula_core::alloc::Counting::new(std::alloc::System);

fn main() {
    if let Err(e) = specula::run() {
        eprintln!("specula: {e}");
        std::process::exit(1);
    }
}
