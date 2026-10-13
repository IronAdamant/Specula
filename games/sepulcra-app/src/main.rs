//! `sepulcra`: the game in a window (or headless), served to agents over MCP.

fn main() {
    if let Err(e) = sepulcra_app::run() {
        eprintln!("sepulcra: {e}");
        std::process::exit(1);
    }
}
