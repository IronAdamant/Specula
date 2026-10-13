//! The stock engine binary: `cargo run -p specula`.

fn main() {
    if let Err(e) = specula::run() {
        eprintln!("specula: {e}");
        std::process::exit(1);
    }
}
