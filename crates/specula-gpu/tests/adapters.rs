//! What the gate (`tools/examen`) reads before it runs any test: the adapters this machine's tests will
//! see. GPU tests skip themselves cleanly when Vulkan or an adapter is missing (rule 7: CI
//! without a GPU stays green), and a passing test's output is hidden, so a gate on a machine
//! whose driver stopped loading would pass with every GPU test skipped. The gate runs this with
//! `--nocapture` and fails on "none" unless it was told to expect that (`--no-gpu`).

#[test]
fn the_machines_adapters_are_reported() {
    if !specula_gpu::is_available() {
        eprintln!("gate-gpu: none (Vulkan is not available)");
        return;
    }
    match specula_gpu::enumerate_adapters() {
        Ok(adapters) if adapters.is_empty() => eprintln!("gate-gpu: none (Vulkan loaded, and lists no adapter)"),
        Ok(adapters) => {
            let names: Vec<&str> = adapters.iter().map(|a| a.name.as_str()).collect();
            eprintln!("gate-gpu: {} adapter(s): {}", adapters.len(), names.join(", "));
        }
        Err(e) => eprintln!("gate-gpu: none ({e})"),
    }
}
