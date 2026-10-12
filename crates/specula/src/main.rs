//! Specula CLI — environment doctor, BRP helpers, scaffold, and MCP server.

mod mcp;
mod component_paths;
mod launch_plan;
mod scaffold;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use specula_brp::{
    apply_game_profile, capture_viewport_image, see_diff, see_entity, see_motion, see_pack,
    see_region, see_scene, see_verify, BrpClient, BrpTarget, ProjectionMode, SeeOptions,
    SubjectFilterMode, DEFAULT_CROP_HALF, DEFAULT_MOTION_FRAMES, DEFAULT_MOTION_INTERVAL_MS,
    DEFAULT_PORT,
};
use specula_env::{
    check_readiness, format_report_text, DoctorOptions, SystemCommandRunner,
};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(
    name = "specula",
    version,
    about = "Specula: environment checks, scaffolding, and MCP/BRP control for Bevy",
    long_about = "Companion tooling for AI coding agents (Grok Build and other MCP clients)\n\
                  to prepare a machine for Bevy and control running Bevy apps via BRP.\n\
                  Full-featured MCP control integrates with bevy_brp_mcp; this binary also\n\
                  ships a focused MCP server and thin BRP client."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Check whether this machine can build and run Bevy apps.
    #[command(alias = "env-check", alias = "check")]
    Doctor {
        /// Also create and compile a minimal Bevy probe project (slow).
        #[arg(long)]
        compile_probe: bool,
        /// Emit JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
        /// Bevy version for the compile probe.
        #[arg(long, default_value = "0.19")]
        bevy_version: String,
    },
    /// Run the Specula MCP server on stdio (for Grok Build / MCP clients).
    Mcp {
        /// Instead of the Specula server, exec `bevy_brp_mcp` (full BRP tool surface).
        #[arg(long)]
        delegate_brp_mcp: bool,
    },
    /// Scaffold a Bevy game from in-repo templates (`2d`, `3d`, or `demo` fixture).
    Scaffold {
        /// Destination directory.
        #[arg(long, default_value = "my-bevy-game")]
        path: PathBuf,
        /// Template kind: `2d` (production), `3d` (production), or `demo` (BRP fixture).
        #[arg(long, default_value = "2d")]
        kind: String,
        /// Cargo package / crate name (default: derived from --path).
        #[arg(long)]
        name: Option<String>,
        /// Overwrite if the directory exists.
        #[arg(long)]
        force: bool,
    },
    /// Call a BRP JSON-RPC method on a running Bevy app.
    Brp {
        #[command(subcommand)]
        cmd: BrpCommands,
    },
    /// Print compatibility matrix and install tips for bevy_brp_mcp.
    Compat,
    /// Print Grok Build MCP registration snippets.
    McpConfig {
        /// Path to the specula binary to embed (default: this executable).
        #[arg(long)]
        bin: Option<PathBuf>,
    },
    /// Agent eyesight (see_scene / entity / region / motion / diff / pack) — not an editor.
    See {
        #[command(subcommand)]
        cmd: SeeCommands,
    },
}

#[derive(Subcommand, Debug)]
enum SeeCommands {
    /// Full-frame capture + filtered subjects → eyesight packet JSON.
    Scene {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        #[arg(long, default_value = ".")]
        out_dir: PathBuf,
        #[arg(long, default_value = "verify scene appearance")]
        intent: String,
        #[arg(long)]
        style_intent: Option<String>,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long, default_value = "gameplay_prefer")]
        subject_filter: String,
        #[arg(long)]
        wait_for: Vec<String>,
        #[arg(long, default_value_t = false)]
        require_playing: bool,
        #[arg(long, default_value = "ortho2d")]
        projection: String,
        #[arg(long, default_value_t = 640.0)]
        visible_half_w: f64,
        #[arg(long, default_value_t = 360.0)]
        visible_half_h: f64,
        #[arg(long)]
        save_baseline: Option<PathBuf>,
        #[arg(long)]
        compare_baseline: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        auto_baseline: bool,
        #[arg(long, default_value_t = false)]
        include_primary_fovea: bool,
    },
    /// Full + ranked primary fovea (+zoom) one-shot verify.
    Verify {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        #[arg(long, default_value = ".")]
        out_dir: PathBuf,
        #[arg(long, default_value = "verify scene")]
        intent: String,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        save_baseline: Option<PathBuf>,
        #[arg(long)]
        compare_baseline: Option<PathBuf>,
    },
    /// True fovea crop around a named entity (world→screen when possible).
    Entity {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        #[arg(long, default_value = ".")]
        out_dir: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        screen_x: Option<u32>,
        #[arg(long)]
        screen_y: Option<u32>,
        #[arg(long, default_value_t = DEFAULT_CROP_HALF)]
        half: u32,
        #[arg(long, default_value = "inspect entity craft")]
        intent: String,
        /// Prefer profile over raw projection defaults (iron-feud → topdown3d XZ).
        #[arg(long)]
        profile: Option<String>,
        #[arg(long, default_value = "ortho2d")]
        projection: String,
        #[arg(long, default_value_t = 640.0)]
        visible_half_w: f64,
        #[arg(long, default_value_t = 360.0)]
        visible_half_h: f64,
        #[arg(long, default_value_t = false)]
        diagnostic_bounds: bool,
    },
    /// Pixel-rect region crop.
    Region {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        #[arg(long, default_value = ".")]
        out_dir: PathBuf,
        #[arg(long)]
        x: u32,
        #[arg(long)]
        y: u32,
        #[arg(long)]
        w: u32,
        #[arg(long)]
        h: u32,
        #[arg(long, default_value = "region")]
        label: String,
        #[arg(long, default_value = "inspect region")]
        intent: String,
    },
    /// Temporal strip of frames (optional keys stimulus).
    Motion {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        #[arg(long, default_value = ".")]
        out_dir: PathBuf,
        #[arg(long, default_value_t = DEFAULT_MOTION_FRAMES)]
        frames: u32,
        #[arg(long, default_value_t = DEFAULT_MOTION_INTERVAL_MS)]
        interval_ms: u64,
        #[arg(long = "key")]
        keys: Vec<String>,
        #[arg(long, default_value = "judge motion / physics feel")]
        intent: String,
    },
    /// Before/after vs baseline PNG.
    Diff {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        #[arg(long, default_value = ".")]
        out_dir: PathBuf,
        #[arg(long)]
        baseline: PathBuf,
        #[arg(long, default_value = "before/after refinement")]
        intent: String,
    },
    /// Multi-view pack: entity_craft | landscape | water | physics_jump | lighting | diagnostic | hud | env_2d.
    Pack {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        #[arg(long, default_value = ".")]
        out_dir: PathBuf,
        pack: String,
        #[arg(long, default_value = "multi-view eyesight pack")]
        intent: String,
        #[arg(long)]
        style_intent: Option<String>,
        #[arg(long, default_value = "ortho2d")]
        projection: String,
        #[arg(long, default_value_t = false)]
        require_playing: bool,
        #[arg(long)]
        profile: Option<String>,
    },
}

fn parse_projection(s: &str) -> ProjectionMode {
    match s.to_ascii_lowercase().as_str() {
        "topdown3d" | "top_down" | "3d" | "topdown" => ProjectionMode::TopDown3d,
        _ => ProjectionMode::Ortho2d,
    }
}

#[derive(Subcommand, Debug)]
enum BrpCommands {
    /// Discover available BRP methods (`rpc.discover`).
    Discover {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        #[arg(long)]
        host: Option<String>,
    },
    /// Query entities for components.
    Query {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        /// Fully-qualified component type paths (repeatable).
        #[arg(long = "component", default_values_t = vec![
            "bevy_transform::components::transform::Transform".to_string()
        ])]
        components: Vec<String>,
    },
    /// Mutate a component field on an entity.
    Mutate {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        #[arg(long)]
        entity: u64,
        #[arg(long)]
        component: String,
        #[arg(long)]
        path: String,
        /// JSON value to write.
        #[arg(long)]
        value: String,
    },
    /// Generic BRP method call.
    Call {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        method: String,
        /// Optional JSON params object/array.
        #[arg(long)]
        params: Option<String>,
    },
    /// Capture a screenshot via brp_extras/screenshot and print path + metadata.
    Screenshot {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        #[arg(long, default_value = "captures/scene.png")]
        path: PathBuf,
        /// Also print base64 length (for MCP image path validation).
        #[arg(long)]
        with_image_meta: bool,
    },
    /// Wait until BRP responds on the port.
    Wait {
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        #[arg(long, default_value_t = 30)]
        timeout_secs: u64,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Doctor {
            compile_probe,
            json,
            bevy_version,
        } => cmd_doctor(compile_probe, json, bevy_version),
        Commands::Mcp { delegate_brp_mcp } => {
            if delegate_brp_mcp {
                delegate_to_bevy_brp_mcp()
            } else {
                // MCP must own stdin/stdout; run async server.
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()?;
                rt.block_on(mcp::run_stdio_server())
            }
        }
        Commands::Scaffold {
            path,
            kind,
            name,
            force,
        } => {
            let kind = scaffold::ScaffoldKind::parse(&kind)?;
            scaffold::scaffold_app(&path, kind, name.as_deref(), force)
        }
        Commands::Brp { cmd } => cmd_brp(cmd),
        Commands::Compat => {
            print_compat();
            Ok(())
        }
        Commands::McpConfig { bin } => {
            print_mcp_config(bin)?;
            Ok(())
        }
        Commands::See { cmd } => cmd_see(cmd),
    }
}

fn cmd_see(cmd: SeeCommands) -> Result<()> {
    match cmd {
        SeeCommands::Scene {
            port,
            out_dir,
            intent,
            style_intent,
            profile,
            subject_filter,
            wait_for,
            require_playing,
            projection,
            visible_half_w,
            visible_half_h,
            save_baseline,
            compare_baseline,
            auto_baseline,
            include_primary_fovea,
        } => {
            let client = BrpClient::with_port(port);
            let mut opts = SeeOptions {
                out_dir,
                intent,
                style_intent,
                profile: profile.clone(),
                subject_filter: SubjectFilterMode::parse(&subject_filter),
                wait_for_subjects: wait_for,
                require_playing,
                projection: parse_projection(&projection),
                visible_half_w,
                visible_half_h,
                save_baseline_as: save_baseline,
                compare_baseline,
                auto_baseline,
                include_primary_fovea,
                ..SeeOptions::default()
            };
            if let Some(ref p) = profile {
                apply_game_profile(&mut opts, p);
            }
            let packet = see_scene(&client, &opts)?;
            println!("{}", packet.to_pretty_json()?);
        }
        SeeCommands::Verify {
            port,
            out_dir,
            intent,
            profile,
            save_baseline,
            compare_baseline,
        } => {
            let client = BrpClient::with_port(port);
            let mut opts = SeeOptions {
                out_dir,
                intent,
                profile: profile.clone(),
                save_baseline_as: save_baseline,
                compare_baseline,
                ..SeeOptions::default()
            };
            if let Some(ref p) = profile {
                apply_game_profile(&mut opts, p);
            }
            let packet = see_verify(&client, &opts)?;
            println!("{}", packet.to_pretty_json()?);
        }
        SeeCommands::Entity {
            port,
            out_dir,
            name,
            screen_x,
            screen_y,
            half,
            intent,
            profile,
            projection,
            visible_half_w,
            visible_half_h,
            diagnostic_bounds,
        } => {
            let client = BrpClient::with_port(port);
            let mut opts = SeeOptions {
                out_dir,
                intent,
                subject_class: "entity".into(),
                projection: parse_projection(&projection),
                visible_half_w,
                visible_half_h,
                diagnostic_bounds,
                profile: profile.clone(),
                zoom_ladder: true,
                ..SeeOptions::default()
            };
            // Profile sets topdown3d + IF half-extents — required for 3D fovea aim.
            if let Some(ref p) = profile {
                apply_game_profile(&mut opts, p);
            }
            // Explicit projection flag still wins if user passed a non-default string after profile.
            // (apply_game_profile already set topdown for iron-feud; keep CLI projection if topdown3d.)
            if projection != "ortho2d" {
                opts.projection = parse_projection(&projection);
            }
            let packet = see_entity(&client, &opts, &name, screen_x, screen_y, half)?;
            println!("{}", packet.to_pretty_json()?);
        }
        SeeCommands::Region {
            port,
            out_dir,
            x,
            y,
            w,
            h,
            label,
            intent,
        } => {
            let client = BrpClient::with_port(port);
            let opts = SeeOptions {
                out_dir,
                intent,
                subject_class: "landscape".into(),
                ..SeeOptions::default()
            };
            let packet = see_region(&client, &opts, x, y, w, h, &label)?;
            println!("{}", packet.to_pretty_json()?);
        }
        SeeCommands::Motion {
            port,
            out_dir,
            frames,
            interval_ms,
            keys,
            intent,
        } => {
            let client = BrpClient::with_port(port);
            let opts = SeeOptions {
                out_dir,
                intent,
                subject_class: "physics_motion".into(),
                ..SeeOptions::default()
            };
            let keys = if keys.is_empty() { None } else { Some(keys) };
            let packet = see_motion(&client, &opts, frames, interval_ms, keys)?;
            println!("{}", packet.to_pretty_json()?);
        }
        SeeCommands::Diff {
            port,
            out_dir,
            baseline,
            intent,
        } => {
            let client = BrpClient::with_port(port);
            let opts = SeeOptions {
                out_dir,
                intent,
                ..SeeOptions::default()
            };
            let packet = see_diff(&client, &opts, baseline)?;
            println!("{}", packet.to_pretty_json()?);
        }
        SeeCommands::Pack {
            port,
            out_dir,
            pack,
            intent,
            style_intent,
            projection,
            require_playing,
            profile,
        } => {
            let client = BrpClient::with_port(port);
            let mut opts = SeeOptions {
                out_dir,
                intent,
                style_intent,
                projection: parse_projection(&projection),
                require_playing,
                profile: profile.clone(),
                ..SeeOptions::default()
            };
            if let Some(ref p) = profile {
                apply_game_profile(&mut opts, p);
            }
            let packet = see_pack(&client, &opts, &pack)?;
            println!("{}", packet.to_pretty_json()?);
        }
    }
    Ok(())
}

fn cmd_doctor(compile_probe: bool, json: bool, bevy_version: String) -> Result<()> {
    let options = DoctorOptions {
        compile_probe,
        bevy_version,
        ..DoctorOptions::default()
    };
    let runner = SystemCommandRunner;
    let report = check_readiness(&runner, &options);
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", format_report_text(&report));
    }
    if report.ready {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn cmd_brp(cmd: BrpCommands) -> Result<()> {
    match cmd {
        BrpCommands::Discover { port, host } => {
            let mut target = BrpTarget::new("default", port);
            if let Some(h) = host {
                target.host = h;
            }
            let client = BrpClient::new(target);
            let result = client.call("rpc.discover", None)?.into_result()?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        BrpCommands::Query { port, components } => {
            let client = BrpClient::with_port(port);
            let comps: Vec<&str> = components.iter().map(|s| s.as_str()).collect();
            let result = client.query(&comps)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        BrpCommands::Mutate {
            port,
            entity,
            component,
            path,
            value,
        } => {
            let client = BrpClient::with_port(port);
            let v: serde_json::Value = serde_json::from_str(&value).context("parse --value JSON")?;
            let result = client.mutate_components(entity, &component, &path, v)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        BrpCommands::Call {
            port,
            method,
            params,
        } => {
            let client = BrpClient::with_port(port);
            let p = match params {
                Some(s) => Some(serde_json::from_str(&s).context("parse --params JSON")?),
                None => None,
            };
            let result = client.call(&method, p)?.into_result()?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        BrpCommands::Screenshot {
            port,
            path,
            with_image_meta,
        } => {
            let client = BrpClient::with_port(port);
            if with_image_meta {
                let img = capture_viewport_image(&client, &path)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "path": img.path,
                        "byte_len": img.byte_len,
                        "mime_type": img.mime_type,
                        "width": img.width_hint,
                        "height": img.height_hint,
                        "base64_len": img.png_base64.len(),
                    }))?
                );
            } else {
                let published = client.screenshot(&path)?;
                println!("{}", published.display());
            }
        }
        BrpCommands::Wait { port, timeout_secs } => {
            let client = BrpClient::with_port(port);
            let result = client.wait_until_ready(Duration::from_secs(timeout_secs))?;
            println!(
                "BRP ready on port {port}: {}",
                serde_json::to_string(&result)?
                    .chars()
                    .take(200)
                    .collect::<String>()
            );
        }
    }
    Ok(())
}

fn delegate_to_bevy_brp_mcp() -> Result<()> {
    let bin = which::which("bevy_brp_mcp").context(
        "bevy_brp_mcp not found on PATH. Install with:\n  cargo install bevy_brp_mcp --locked\n\
         Or run without --delegate-brp-mcp to use the built-in Specula MCP server.",
    )?;
    tracing::info!("delegating MCP stdio to {}", bin.display());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = Command::new(&bin)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .exec();
        bail!("failed to exec {}: {err}", bin.display());
    }

    #[cfg(not(unix))]
    {
        let status = Command::new(&bin)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("spawn {}", bin.display()))?;
        if status.success() {
            Ok(())
        } else {
            bail!("bevy_brp_mcp exited with {status}");
        }
    }
}

fn print_compat() {
    use specula_env::compat;
    println!("Specula compatibility matrix");
    println!("==============================");
    println!("Bevy              : {}", compat::BEVY);
    println!("bevy_brp_mcp      : {}", compat::BEVY_BRP_MCP);
    println!("bevy_brp_extras   : {}", compat::BEVY_BRP_EXTRAS);
    println!("Default BRP port  : {}", compat::DEFAULT_BRP_PORT);
    println!();
    println!("Install full BRP MCP (recommended alongside specula):");
    println!("  cargo install bevy_brp_mcp --locked");
    println!();
    println!("In your Bevy app Cargo.toml:");
    println!("  bevy = {{ version = \"{}\", features = [\"bevy_remote\", \"png\"] }}", compat::BEVY);
    println!("  bevy_brp_extras = \"{}\"", compat::BEVY_BRP_EXTRAS);
}

fn print_mcp_config(bin: Option<PathBuf>) -> Result<()> {
    let exe = bin.unwrap_or(std::env::current_exe()?);
    let exe_str = exe.display().to_string();
    println!("# Grok Build — ~/.grok/config.toml (or project .grok/config.toml)\n");
    println!("[mcp_servers.specula]");
    println!("command = {exe_str:?}");
    println!("args = [\"mcp\"]");
    println!("enabled = true");
    println!("startup_timeout_sec = 30");
    // Launch is non-blocking; wait is a separate tool (bevy_wait_brp). 120s is enough per call.
    println!("tool_timeout_sec = 120");
    println!();
    // G6: templates are embedded in the binary. Optional override still supported.
    println!(
        "# Embedded templates in binary: {}",
        crate::scaffold::embedded_templates_available()
    );
    match crate::scaffold::template_root_with_origin() {
        Ok((root, origin)) => {
            println!(
                "# Templates resolved via {:?} → {}",
                origin,
                root.display()
            );
            println!("# Optional override (skip embedded/monorepo discovery):");
            println!("# [mcp_servers.specula.env]");
            println!("# SPECULA_TEMPLATE_ROOT = {:?}", root.display().to_string());
            println!();
        }
        Err(e) => {
            println!("# Template resolution failed: {e}");
            println!("# Scaffold embeds kits in the binary; rebuild or set SPECULA_TEMPLATE_ROOT.");
            println!();
        }
    }
    println!("# Optional: full bevy_brp_mcp tool surface (after cargo install bevy_brp_mcp)");
    println!("[mcp_servers.bevy-brp]");
    println!("command = \"bevy_brp_mcp\"");
    println!("args = []");
    println!("enabled = true");
    println!();
    println!("# CLI equivalent:");
    println!("#   grok mcp add specula -- {exe_str} mcp");
    println!("#   grok mcp add bevy-brp -- bevy_brp_mcp");
    println!();
    println!("# Claude Code — project scope writes .mcp.json (commit it to share):");
    println!("#   claude mcp add --scope project specula -- {exe_str} mcp");
    println!("#   claude mcp add --scope project bevy-brp -- bevy_brp_mcp");
    Ok(())
}
