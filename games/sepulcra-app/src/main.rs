//! `sepulcra`: the game in a window (or headless), served to agents over MCP.

/// Counts heap use for the engine's perf_memory and per-zone allocations.
#[global_allocator]
static ALLOC: specula_core::alloc::Counting<std::alloc::System> = specula_core::alloc::Counting::new(std::alloc::System);

fn main() {
    if let Err(e) = sepulcra_app::run() {
        eprintln!("sepulcra: {e}");
        std::process::exit(1);
    }
}
