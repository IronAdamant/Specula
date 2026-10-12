//! Minimal MCP stdio server for Grok Build agents.
//!
//! Implements the core MCP surface (initialize, tools/list, tools/call) over
//! newline-delimited JSON-RPC on stdin/stdout. Designed to complement
//! `bevy_brp_mcp` (full BRP tool surface) with Specula-specific tools:
//! environment readiness, BRP query/mutate, and viewport capture as images.

use anyhow::{anyhow, Context, Result};
use specula_brp::{
    apply_game_profile, capture_viewport_image, packet_to_mcp_content, see_diff, see_entity,
    see_motion, see_pack, see_region, see_scene, see_verify, BrpClient, BrpTarget, CapturedImage,
    ProjectionMode, SeeOptions, SubjectFilterMode, TargetRegistry, DEFAULT_CROP_HALF,
    DEFAULT_MAX_SUBJECTS, DEFAULT_MOTION_FRAMES, DEFAULT_MOTION_INTERVAL_MS, DEFAULT_PORT,
};
use specula_env::{check_readiness, DoctorOptions, SystemCommandRunner};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

struct ServerState {
    targets: TargetRegistry,
    /// Child process of a launched sample / app (if any).
    child: Option<tokio::process::Child>,
}

impl ServerState {
    fn new() -> Self {
        let mut targets = TargetRegistry::new();
        targets.register(BrpTarget::default_local());
        Self {
            targets,
            child: None,
        }
    }
}

pub async fn run_stdio_server() -> Result<()> {
    // MCP logs must not go to stdout.
    tracing::info!("Specula MCP server starting on stdio");
    let state = Arc::new(Mutex::new(ServerState::new()));
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("invalid JSON on stdin: {e}");
                continue;
            }
        };

        // Notifications have no id and no response.
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(json!({}));

        if id.is_none() {
            // notification
            if method == "notifications/initialized" {
                tracing::info!("client initialized");
            }
            continue;
        }

        let result = handle_request(method, params, state.clone()).await;
        let response = match result {
            Ok(r) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": r,
            }),
            Err(e) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32000,
                    "message": e.to_string(),
                }
            }),
        };
        let mut out = serde_json::to_string(&response)?;
        out.push('\n');
        stdout.write_all(out.as_bytes()).await?;
        stdout.flush().await?;
    }
    Ok(())
}

async fn handle_request(method: &str, params: Value, state: Arc<Mutex<ServerState>>) -> Result<Value> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {},
                "prompts": {}
            },
            "serverInfo": {
                "name": "specula",
                "version": env!("CARGO_PKG_VERSION")
            },
            "instructions": mcp_instructions()
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_defs() })),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(|n| n.as_str())
                .ok_or_else(|| anyhow!("missing tool name"))?;
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            call_tool(name, args, state).await
        }
        "resources/list" => Ok(json!({ "resources": [] })),
        "prompts/list" => Ok(json!({ "prompts": prompts_list_json() })),
        "prompts/get" => {
            let name = params
                .get("name")
                .and_then(|n| n.as_str())
                .ok_or_else(|| anyhow!("missing prompt name"))?;
            prompts_get_json(name)
        }
        other => Err(anyhow!("method not found: {other}")),
    }
}

/// High-signal agent instructions returned on MCP `initialize`.
pub fn mcp_instructions() -> String {
    concat!(
        "Specula MCP (alpha game factory): env readiness, BRP query/mutate/call, viewport capture, app launch. ",
        "Complete short demos must meet docs/GAME_DOD.md (menu, objective, challenge, win/lose)—not movement-only. ",
        "Skills: bevy-demo-game + bevy-production + bevy-2d-game or bevy-3d-game; art game-asset-core; live bevy-agent-loop. ",
        "Scaffold: `specula scaffold --kind 2d|3d|demo --path DIR` (2d/3d kits; demo = BRP fixture only). ",
        "Dogfood (when present): games/demo-2d, games/demo-3d. Roadmap: docs/ROADMAP.md. ",
        "MCP prompts: start_2d_game, start_3d_game, build_demo_2d, build_demo_3d, iterate_scene, prepare_ship, package_demo. ",
        "bevy_workflow goals: new_2d|new_3d|complete_demo_2d|complete_demo_3d|verify_scene|ship|package_demo|add_sprite. ",
        "Asset roots: assets/sprites, models, ui, audio. Ship: cargo build --release; package binary + assets. ",
        "Loop: bevy_env_check → bevy_launch_app (wait_secs=0) → bevy_wait_brp → bevy_see_scene (acuity) → mutate → re-see. Port 15702. ",
        "Agent sight (not editor, not taste): bevy_see_scene|verify|entity|region|motion|diff|pack. ",
        "Profiles: crystal-drift (2D ortho) | iron-feud (3D topdown, require_playing). primary_subject ranked (Player/WaterBody over Crystal/OreCrystal). ",
        "Packs: entity_craft|landscape|water|physics_jump|lighting|diagnostic|hud|env_2d. Landscape notes height_bands when TerrainFlat/Hill/Peak present. ",
        "Open every PNG abs_path. bevy_see_verify = full+fovea (use first). After asset/env change: save_baseline then compare_baseline. ",
        "Iron Feud: IRON_FEUD_AUTO_PLAY=1. Sequential BRP 15702 only. New Names need GAMEPLAY_NAME_HINTS score>0. ",
        "Sprites: transparent BG only — zero true-magenta plates (png_true_magenta_pixel_count). Full-frame nonblack gate for env. ",
        "Landscape: if alt similar, side-orbit second path; views_similar if still match. No livestream. ",
        "Taste/design human-owned. Plan: docs/AGENT_SIGHT_FIDELITY_PLAN.md. Skill: bevy-agent-loop. ",
        "Cold compile: prefer shell `cargo run --features remote,capture` then bevy_wait_brp; MCP launch is best after a warm target/. ",
        "bevy_workflow is a router (skills+steps), not an autopilot. Optional full BRP: cargo install bevy_brp_mcp --locked."
    )
    .to_string()
}

/// One MCP production prompt (list + get share this catalog).
#[derive(Debug, Clone, Copy)]
pub struct PromptDef {
    pub name: &'static str,
    pub description: &'static str,
    pub body: &'static str,
}

/// Production entry prompts advertised on `prompts/list`.
pub fn prompt_catalog() -> &'static [PromptDef] {
    &[
        PromptDef {
            name: "start_2d_game",
            description:
                "Scaffold a 2D Bevy game kit and build toward GAME_DOD (short demo, not movement-only).",
            body: concat!(
                "You are building a 2D Bevy 0.19 short demo with Specula — not a cube fixture.\n\n",
                "1. Load: bevy-demo-game + bevy-production + bevy-2d-game. Art: game-asset-core (+ specialist).\n",
                "2. Prefer games/demo-2d if present; else `specula scaffold --kind 2d --path <dir>`.\n",
                "3. Meet docs/GAME_DOD.md: menu, objective, challenge, win/lose, pause, assets, README.\n",
                "4. Features remote,capture. Verify with bevy-agent-loop (captures: menu, play, end).\n",
                "5. bevy_workflow goal complete_demo_2d or new_2d.\n",
            ),
        },
        PromptDef {
            name: "start_3d_game",
            description:
                "Scaffold a 3D Bevy game kit and build toward GAME_DOD (short demo, not movement-only).",
            body: concat!(
                "You are building a 3D Bevy 0.19 short demo with Specula — not a static BRP cube.\n\n",
                "1. Load: bevy-demo-game + bevy-production + bevy-3d-game. Art: game-asset-core as needed.\n",
                "2. Prefer games/demo-3d if present; else `specula scaffold --kind 3d --path <dir>`.\n",
                "3. Meet docs/GAME_DOD.md: menu, objective, challenge, win/lose, pause, assets, lighting.\n",
                "4. Features remote,capture. Captures must show lit play + end state.\n",
                "5. bevy_workflow goal complete_demo_3d or new_3d.\n",
            ),
        },
        PromptDef {
            name: "build_demo_2d",
            description:
                "Finish the 2D short demo to GAME_DOD (objective, challenge, win/lose, captures).",
            body: concat!(
                "Complete the 2D Bevy short demo per docs/GAME_DOD.md.\n\n",
                "1. Skills: bevy-demo-game + bevy-2d-game + bevy-production + bevy-agent-loop.\n",
                "2. Work in games/demo-2d or scaffolded 2d project.\n",
                "3. Implement objective + challenge + Victory/GameOver; not movement-only.\n",
                "4. Capture menu, mid-play, end state via bevy_capture_viewport.\n",
                "5. bevy_workflow: complete_demo_2d.\n",
            ),
        },
        PromptDef {
            name: "build_demo_3d",
            description:
                "Finish the 3D short demo to GAME_DOD (objective, challenge, win/lose, captures).",
            body: concat!(
                "Complete the 3D Bevy short demo per docs/GAME_DOD.md.\n\n",
                "1. Skills: bevy-demo-game + bevy-3d-game + bevy-production + bevy-agent-loop.\n",
                "2. Work in games/demo-3d or scaffolded 3d project.\n",
                "3. Implement objective + challenge + end states; keep scene lit for captures.\n",
                "4. Capture menu, mid-play, end state.\n",
                "5. bevy_workflow: complete_demo_3d.\n",
            ),
        },
        PromptDef {
            name: "iterate_scene",
            description:
                "Live BRP/MCP loop: launch, query/mutate, capture viewport, fix, recapture.",
            body: concat!(
                "Iterate on a running Bevy app with agent eyesight (pixels + subjects), not guesses.\n\n",
                "1. Load skill: bevy-agent-loop (and bevy-production / bevy-demo-game if incomplete DoD).\n",
                "2. Ensure the app uses features remote,capture and BRP port 15702 (or registered target).\n",
                "3. MCP loop: bevy_env_check → bevy_launch_app (wait_secs=0) → bevy_wait_brp → bevy_see_scene (open PNGs) → ",
                "optional bevy_see_entity / bevy_see_motion / bevy_see_diff → describe defects → patch → re-see.\n",
                "4. Aesthetic claims require opened capture paths. Prefer Name entities.\n",
                "5. Packs: bevy_see_pack (entity_craft|landscape|water|physics_jump|lighting|diagnostic|hud|env_2d).\n",
                "6. Optional: bevy_workflow goal \"verify_scene\". Plan: docs/AGENT_EYESIGHT_PLAN.md.\n",
            ),
        },
        PromptDef {
            name: "prepare_ship",
            description:
                "Ship checklist: GAME_DOD playability, release build, assets next to binary.",
            body: concat!(
                "Prepare a Bevy demo/game for engineering release (not Steam cert).\n\n",
                "1. Confirm docs/GAME_DOD.md (or production ship checklist) is met.\n",
                "2. Load bevy-production; assets under assets/sprites|models|ui|audio.\n",
                "3. cargo build --release; ship assets/ beside the binary.\n",
                "4. README controls + objective. Optional captures before done.\n",
                "5. bevy_workflow goal ship. See docs/SHIPPING.md and docs/ROADMAP.md (G4 packaging).\n",
            ),
        },
        PromptDef {
            name: "package_demo",
            description:
                "Package a non-Steam distributable demo (binary + assets folder/zip).",
            body: concat!(
                "Package a playable Bevy demo for sharing (zip/folder), not Steam upload yet.\n\n",
                "1. Confirm GAME_DOD playability first.\n",
                "2. cargo build --release; copy binary + assets/ into dist/<name>/.\n",
                "3. Document: run from dist so AssetServer finds assets/.\n",
                "4. Prefer scripts/package-demo when present; bevy_workflow goal package_demo.\n",
                "5. Steam is later (docs/STEAM_PATH / G5)—do not block packaging on Steamworks.\n",
            ),
        },
    ]
}

pub fn prompts_list_json() -> Value {
    Value::Array(
        prompt_catalog()
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "description": p.description,
                })
            })
            .collect(),
    )
}

pub fn prompts_get_json(name: &str) -> Result<Value> {
    let prompt = prompt_catalog()
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| anyhow!("unknown prompt: {name}"))?;
    Ok(json!({
        "description": prompt.description,
        "messages": [{
            "role": "user",
            "content": {
                "type": "text",
                "text": prompt.body
            }
        }]
    }))
}

/// Production goals accepted by the `bevy_workflow` tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowGoal {
    New2d,
    New3d,
    CompleteDemo2d,
    CompleteDemo3d,
    VerifyScene,
    Ship,
    PackageDemo,
    AddSprite,
}

impl WorkflowGoal {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "new_2d" | "2d" | "start_2d" | "start_2d_game" => Ok(Self::New2d),
            "new_3d" | "3d" | "start_3d" | "start_3d_game" => Ok(Self::New3d),
            "complete_demo_2d" | "build_demo_2d" | "demo_2d" | "finish_2d" => {
                Ok(Self::CompleteDemo2d)
            }
            "complete_demo_3d" | "build_demo_3d" | "demo_3d" | "finish_3d" => {
                Ok(Self::CompleteDemo3d)
            }
            "verify_scene" | "iterate" | "iterate_scene" | "capture" => Ok(Self::VerifyScene),
            "ship" | "prepare_ship" | "release" => Ok(Self::Ship),
            "package_demo" | "package" | "dist" | "zip" => Ok(Self::PackageDemo),
            "add_sprite" | "art" | "sprite" => Ok(Self::AddSprite),
            other => bail_unknown_goal(other),
        }
    }
}

fn bail_unknown_goal(other: &str) -> Result<WorkflowGoal> {
    Err(anyhow!(
        "unknown workflow goal '{other}' (expected new_2d, new_3d, complete_demo_2d, complete_demo_3d, verify_scene, ship, package_demo, add_sprite)"
    ))
}

/// Ordered production plan for a workflow goal (skills + tools/CLI).
pub fn workflow_plan(goal: WorkflowGoal) -> String {
    match goal {
        WorkflowGoal::New2d => concat!(
            "Goal: new_2d — start a 2D Bevy short demo (aim at GAME_DOD)\n",
            "Skills to load:\n",
            "  1. bevy-demo-game\n",
            "  2. bevy-production\n",
            "  3. bevy-2d-game\n",
            "  4. game-asset-core (+ specialist when generating art)\n",
            "  5. bevy-agent-loop (when verifying live)\n",
            "Steps:\n",
            "  1. bevy_env_check (MCP) or `specula doctor`\n",
            "  2. Prefer games/demo-2d if present; else `specula scaffold --kind 2d --path <game-dir>`\n",
            "  3. Implement toward docs/GAME_DOD.md (not movement-only)\n",
            "  4. Cold first build: shell `cargo run --features remote,capture`; else MCP bevy_launch_app (wait_secs=0)\n",
            "  5. MCP: bevy_wait_brp (timeout_secs 180 cold / 30 warm) then captures: menu, play, end\n",
            "  6. Prompt: start_2d_game or build_demo_2d; goal complete_demo_2d when finishing\n",
            "Note: bevy_workflow is a router (skills+steps), not an autopilot executor.\n",
        )
        .to_string(),
        WorkflowGoal::New3d => concat!(
            "Goal: new_3d — start a 3D Bevy short demo (aim at GAME_DOD)\n",
            "Skills to load:\n",
            "  1. bevy-demo-game\n",
            "  2. bevy-production\n",
            "  3. bevy-3d-game\n",
            "  4. game-asset-core (+ specialist for UI/art)\n",
            "  5. bevy-agent-loop (when verifying live)\n",
            "Steps:\n",
            "  1. bevy_env_check or `specula doctor`\n",
            "  2. Prefer games/demo-3d if present; else `specula scaffold --kind 3d --path <game-dir>`\n",
            "  3. Implement toward docs/GAME_DOD.md; keep lighting for captures\n",
            "  4. Cold first build: shell cargo run; else MCP bevy_launch_app (wait_secs=0)\n",
            "  5. MCP: bevy_wait_brp then captures: menu, play, end\n",
            "  6. Prompt: start_3d_game or build_demo_3d; goal complete_demo_3d when finishing\n",
            "Note: bevy_workflow is a router (skills+steps), not an autopilot executor.\n",
        )
        .to_string(),
        WorkflowGoal::CompleteDemo2d => concat!(
            "Goal: complete_demo_2d — finish 2D short demo to GAME_DOD\n",
            "Skills to load:\n",
            "  1. bevy-demo-game (required)\n",
            "  2. bevy-2d-game\n",
            "  3. bevy-production\n",
            "  4. bevy-agent-loop\n",
            "Steps:\n",
            "  1. Read docs/GAME_DOD.md — reject movement-only\n",
            "  2. Ensure objective, challenge, Victory and/or GameOver, pause, assets, README\n",
            "  3. cargo run --features remote,capture\n",
            "  4. MCP captures: MainMenu, Playing (HUD/objective), end state\n",
            "  5. Only then call the demo complete; optional package_demo next\n",
        )
        .to_string(),
        WorkflowGoal::CompleteDemo3d => concat!(
            "Goal: complete_demo_3d — finish 3D short demo to GAME_DOD\n",
            "Skills to load:\n",
            "  1. bevy-demo-game (required)\n",
            "  2. bevy-3d-game\n",
            "  3. bevy-production\n",
            "  4. bevy-agent-loop\n",
            "Steps:\n",
            "  1. Read docs/GAME_DOD.md — reject movement-only\n",
            "  2. Ensure objective, challenge, end states, pause, assets, lit scene, README\n",
            "  3. cargo run --features remote,capture\n",
            "  4. MCP captures: menu, play, end (non-black)\n",
            "  5. Optional package_demo next\n",
        )
        .to_string(),
        WorkflowGoal::VerifyScene => concat!(
            "Goal: verify_scene — agent eyesight packet + judgment (not editor)\n",
            "Skills to load:\n",
            "  1. bevy-agent-loop\n",
            "  2. bevy-demo-game / bevy-production if DoD incomplete\n",
            "Steps:\n",
            "  1. Confirm app features remote,capture; BRP port 15702\n",
            "  2. MCP: bevy_launch_app (wait_secs=0) if not running; cold compile prefer shell cargo run\n",
            "  3. MCP: bevy_wait_brp → bevy_see_verify profile=crystal-drift|iron-feud (OPEN abs_paths)\n",
            "  4. Or bevy_see_scene + bevy_see_entity; primary_subject is ranked\n",
            "  5. bevy_see_pack landscape|water|diagnostic; views_similar if alt≈game\n",
            "  6. compare_baseline / save_baseline after visual changes\n",
            "  7. Iron Feud: IRON_FEUD_AUTO_PLAY=1 + profile iron-feud (require_playing)\n",
            "  8. Patch to human requirements; re-see — taste human-owned; no livestream\n",
            "  9. Plan: docs/AGENT_SIGHT_NEXT_PLAN.md\n",
        )
        .to_string(),
        WorkflowGoal::Ship => concat!(
            "Goal: ship — release readiness (non-Steam)\n",
            "Skills to load:\n",
            "  1. bevy-production (ship checklist)\n",
            "  2. bevy-demo-game if short demo DoD not yet met\n",
            "  3. bevy-agent-loop (optional final capture)\n",
            "Steps:\n",
            "  1. Confirm GAME_DOD / menu→play→end and disk assets under assets/\n",
            "  2. CLI: `cargo build --release` in the game project\n",
            "  3. Place/document assets/ next to the release binary\n",
            "  4. README controls + objective; docs/SHIPPING.md\n",
            "  5. Optional: package_demo for zip layout\n",
        )
        .to_string(),
        WorkflowGoal::PackageDemo => concat!(
            "Goal: package_demo — distributable folder/zip (non-Steam)\n",
            "Skills to load:\n",
            "  1. bevy-demo-game (playability gate)\n",
            "  2. bevy-production\n",
            "Steps:\n",
            "  1. Do not package until GAME_DOD captures pass\n",
            "  2. cargo build --release\n",
            "  3. Copy binary + assets/ → dist/<name>/ (script when available)\n",
            "  4. Run from dist/ so assets resolve; zip for sharing\n",
            "  5. Steam is later (G5)—do not block on Steamworks\n",
            "  6. Prompt: package_demo\n",
        )
        .to_string(),
        WorkflowGoal::AddSprite => concat!(
            "Goal: add_sprite — engine-ready art into a Bevy game\n",
            "Skills to load:\n",
            "  1. game-asset-core (+ game-character-consistency / game-animation-frames / game-tilesets / game-ui-icons as needed)\n",
            "  2. bevy-2d-game or bevy-3d-game depending on project\n",
            "  3. bevy-agent-loop for capture verify\n",
            "Steps:\n",
            "  1. Generate engine-ready art (keyable background for sprites)\n",
            "  2. Write under assets/sprites (or ui/models as appropriate)\n",
            "  3. Load via AssetServer path relative to assets/\n",
            "  4. MCP: bevy_launch_app → bevy_wait_brp → bevy_capture_viewport to confirm scale/pivot\n",
        )
        .to_string(),
    }
}

fn tool_defs() -> Value {
    json!([
        {
            "name": "bevy_env_check",
            "description": "Check whether this machine can build and run Bevy apps (OS, Rust/Cargo, optional compile probe). Returns a structured readiness report with OS-specific install guidance.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "compile_probe": {
                        "type": "boolean",
                        "description": "If true, create and compile a minimal Bevy probe (slow).",
                        "default": false
                    }
                }
            }
        },
        {
            "name": "bevy_register_target",
            "description": "Register a named BRP target (host/port) for multi-instance control.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "port": { "type": "integer", "default": 15702 },
                    "host": { "type": "string", "default": "127.0.0.1" }
                },
                "required": ["name"]
            }
        },
        {
            "name": "bevy_list_targets",
            "description": "List named BRP targets known to this MCP server.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "bevy_brp_discover",
            "description": "Call rpc.discover on a running Bevy app's BRP HTTP port.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "port": { "type": "integer", "default": 15702 },
                    "target": { "type": "string", "description": "Named target (optional)" }
                }
            }
        },
        {
            "name": "bevy_brp_query",
            "description": "Query entities/components via world.query. Components: short aliases Name, Transform, GlobalTransform or fully-qualified Reflect paths (contain ::). Default: Name + Transform.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "port": { "type": "integer", "default": 15702 },
                    "target": { "type": "string" },
                    "components": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Aliases (Name, Transform, GlobalTransform) or FQNs (e.g. bevy_ecs::name::Name)"
                    }
                }
            }
        },
        {
            "name": "bevy_brp_mutate",
            "description": "Mutate a component field via world.mutate_components. Component may be alias (Transform, Name) or FQN.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "port": { "type": "integer", "default": 15702 },
                    "target": { "type": "string" },
                    "entity": { "type": "integer" },
                    "component": {
                        "type": "string",
                        "description": "Alias (Transform, Name, GlobalTransform) or fully-qualified Reflect path"
                    },
                    "path": { "type": "string" },
                    "value": {}
                },
                "required": ["entity", "component", "path", "value"]
            }
        },
        {
            "name": "bevy_brp_call",
            "description": "Generic BRP JSON-RPC method call (any registered method including brp_extras/*).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "port": { "type": "integer", "default": 15702 },
                    "target": { "type": "string" },
                    "method": { "type": "string" },
                    "params": {}
                },
                "required": ["method"]
            }
        },
        {
            "name": "bevy_capture_viewport",
            "description": "Capture the Bevy primary window via brp_extras/screenshot and return PNG (works for 2D and 3D). Text block always includes abs_path and byte size if chat UI truncates the image.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "port": { "type": "integer", "default": 15702 },
                    "target": { "type": "string" },
                    "path": {
                        "type": "string",
                        "description": "Filesystem path for the PNG",
                        "default": "captures/specula-scene.png"
                    }
                }
            }
        },
        {
            "name": "bevy_launch_app",
            "description": "Spawn a Bevy app (non-blocking, wait_secs=0 default). Uses target/debug binary when warm; otherwise cargo run (cold). Prefer shell cargo run for first compile. ALWAYS follow with bevy_wait_brp before query/capture. Sets cwd to package dir. Logs to a temp file.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "manifest_path": {
                        "type": "string",
                        "description": "Path to Cargo.toml of the Bevy app"
                    },
                    "features": {
                        "type": "string",
                        "description": "Cargo features, e.g. remote,capture",
                        "default": "remote,capture"
                    },
                    "port": {
                        "type": "integer",
                        "description": "Expected BRP port after launch",
                        "default": 15702
                    },
                    "name": {
                        "type": "string",
                        "description": "Named target to register",
                        "default": "launched"
                    },
                    "wait_secs": {
                        "type": "integer",
                        "description": "Optional BRP wait after spawn (default 0 = return immediately). Cap 60; use bevy_wait_brp for longer cold waits.",
                        "default": 0
                    }
                },
                "required": ["manifest_path"]
            }
        },
        {
            "name": "bevy_wait_brp",
            "description": "Poll until BRP rpc.discover succeeds on a port (or timeout). Use after bevy_launch_app or shell cargo run. Prefer timeout_secs 180 for cold first compiles, 30 for warm restarts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "port": {
                        "type": "integer",
                        "description": "BRP port",
                        "default": 15702
                    },
                    "target": {
                        "type": "string",
                        "description": "Named target (optional; overrides port if registered)"
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Max seconds to wait",
                        "default": 30
                    }
                }
            }
        },
        {
            "name": "bevy_brp_mcp_status",
            "description": "Check whether bevy_brp_mcp is installed (full BRP MCP tool surface) and print install guidance.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "bevy_workflow",
            "description": "Production workflow router (not autopilot): goal → ordered skills + MCP/CLI steps. Goals: new_2d, new_3d, complete_demo_2d, complete_demo_3d, verify_scene, ship, package_demo, add_sprite.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "goal": {
                        "type": "string",
                        "description": "new_2d | new_3d | complete_demo_2d | complete_demo_3d | verify_scene | ship | package_demo | add_sprite (aliases: build_demo_2d, demo_3d, iterate_scene, prepare_ship, zip, sprite, …)"
                    }
                },
                "required": ["goal"]
            }
        },
        {
            "name": "bevy_see_scene",
            "description": "Agent sight: full capture + ranked primary + collapsed subjects. profile=crystal-drift|iron-feud. OPEN abs_path. Not editor/taste.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "port": { "type": "integer", "default": 15702 },
                    "target": { "type": "string" },
                    "out_dir": { "type": "string", "default": "." },
                    "intent": { "type": "string", "default": "verify scene appearance" },
                    "style_intent": { "type": "string" },
                    "subject_class": { "type": "string", "default": "scene" },
                    "profile": { "type": "string", "description": "crystal-drift|iron-feud|default" },
                    "subject_filter": { "type": "string", "default": "gameplay_prefer" },
                    "max_subjects": { "type": "integer", "default": 24 },
                    "wait_for_subjects": { "type": "array", "items": { "type": "string" } },
                    "wait_timeout_secs": { "type": "integer", "default": 15 },
                    "require_playing": { "type": "boolean", "default": false },
                    "projection": { "type": "string", "default": "ortho2d" },
                    "visible_half_w": { "type": "number", "default": 640 },
                    "visible_half_h": { "type": "number", "default": 360 },
                    "save_baseline": { "type": "string" },
                    "compare_baseline": { "type": "string" },
                    "auto_baseline": { "type": "boolean", "default": false },
                    "include_primary_fovea": { "type": "boolean", "default": false }
                }
            }
        },
        {
            "name": "bevy_see_verify",
            "description": "One-shot agent verify: full scene + ranked primary fovea (+zoom). Prefer profile=crystal-drift|iron-feud. OPEN all abs_paths.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "port": { "type": "integer", "default": 15702 },
                    "target": { "type": "string" },
                    "out_dir": { "type": "string", "default": "." },
                    "intent": { "type": "string", "default": "verify scene" },
                    "profile": { "type": "string" },
                    "save_baseline": { "type": "string" },
                    "compare_baseline": { "type": "string" }
                }
            }
        },
        {
            "name": "bevy_see_entity",
            "description": "Agent eyesight A1 true fovea: world→screen crop for named entity (+ zoom ladder). OPEN crop PNGs. Use profile=iron-feud for 3D top-down (XZ) or crystal-drift for 2D ortho — without profile, 3D fovea aims wrong.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "port": { "type": "integer", "default": 15702 },
                    "target": { "type": "string" },
                    "out_dir": { "type": "string", "default": "." },
                    "name": { "type": "string", "description": "Entity Name to inspect" },
                    "profile": { "type": "string", "description": "crystal-drift|iron-feud — sets projection + visible half extents" },
                    "screen_x": { "type": "integer" },
                    "screen_y": { "type": "integer" },
                    "half": { "type": "integer", "default": 96 },
                    "intent": { "type": "string", "default": "inspect entity craft" },
                    "style_intent": { "type": "string" },
                    "projection": { "type": "string", "default": "ortho2d" },
                    "visible_half_w": { "type": "number", "default": 640 },
                    "visible_half_h": { "type": "number", "default": 360 },
                    "zoom_ladder": { "type": "boolean", "default": true },
                    "diagnostic_bounds": { "type": "boolean", "default": false }
                },
                "required": ["name"]
            }
        },
        {
            "name": "bevy_see_region",
            "description": "Agent eyesight E1 region crop by pixel rect (landscape patch, water surface, HUD).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "port": { "type": "integer", "default": 15702 },
                    "target": { "type": "string" },
                    "out_dir": { "type": "string", "default": "." },
                    "x": { "type": "integer" },
                    "y": { "type": "integer" },
                    "w": { "type": "integer" },
                    "h": { "type": "integer" },
                    "label": { "type": "string", "default": "region" },
                    "intent": { "type": "string", "default": "inspect region" },
                    "subject_class": { "type": "string", "default": "landscape" }
                },
                "required": ["x", "y", "w", "h"]
            }
        },
        {
            "name": "bevy_see_motion",
            "description": "Agent eyesight E2: short temporal strip (default 6 frames) + optional key stimulus for physics/feel judgment.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "port": { "type": "integer", "default": 15702 },
                    "target": { "type": "string" },
                    "out_dir": { "type": "string", "default": "." },
                    "frames": { "type": "integer", "default": 6 },
                    "interval_ms": { "type": "integer", "default": 80 },
                    "keys": { "type": "array", "items": { "type": "string" } },
                    "intent": { "type": "string", "default": "judge motion / physics feel" }
                }
            }
        },
        {
            "name": "bevy_see_diff",
            "description": "Agent eyesight E3: capture after + compare to baseline PNG (abs-diff image + packet).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "port": { "type": "integer", "default": 15702 },
                    "target": { "type": "string" },
                    "out_dir": { "type": "string", "default": "." },
                    "baseline": { "type": "string", "description": "Path to baseline PNG" },
                    "intent": { "type": "string", "default": "before/after refinement" }
                },
                "required": ["baseline"]
            }
        },
        {
            "name": "bevy_see_pack",
            "description": "Agent eyesight multi-view pack: entity_craft|landscape|water|physics_jump|lighting|diagnostic|hud|env_2d (2D HUD/env + 3D height landscape).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "port": { "type": "integer", "default": 15702 },
                    "target": { "type": "string" },
                    "out_dir": { "type": "string", "default": "." },
                    "pack": {
                        "type": "string",
                        "description": "entity_craft | landscape | water | physics_jump | lighting | diagnostic | hud | env_2d"
                    },
                    "intent": { "type": "string", "default": "multi-view eyesight pack" },
                    "style_intent": { "type": "string" },
                    "projection": { "type": "string", "default": "ortho2d" },
                    "require_playing": { "type": "boolean", "default": false },
                    "subject_filter": { "type": "string", "default": "gameplay_prefer" },
                    "profile": { "type": "string", "description": "crystal-drift|iron-feud (2D vs 3D defaults)" }
                },
                "required": ["pack"]
            }
        }
    ])
}

async fn call_tool(name: &str, args: Value, state: Arc<Mutex<ServerState>>) -> Result<Value> {
    match name {
        "bevy_env_check" => {
            let compile_probe = args
                .get("compile_probe")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let report = tokio::task::spawn_blocking(move || {
                check_readiness(
                    &SystemCommandRunner,
                    &DoctorOptions {
                        compile_probe,
                        ..DoctorOptions::default()
                    },
                )
            })
            .await?;
            // Stamp MCP binary version so agents detect stale installs (assessment residual).
            let mut v = serde_json::to_value(&report)?;
            if let Some(obj) = v.as_object_mut() {
                obj.insert(
                    "specula_version".into(),
                    json!(env!("CARGO_PKG_VERSION")),
                );
                let bin = std::env::current_exe()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "unknown".into());
                obj.insert("server_binary".into(), json!(bin));
                obj.insert(
                    "reload_hint".into(),
                    json!("cargo install --path crates/specula --force && reload MCP"),
                );
            }
            text_result(serde_json::to_string_pretty(&v)?)
        }
        "bevy_register_target" => {
            let name = args["name"].as_str().ok_or_else(|| anyhow!("name required"))?;
            let port = args.get("port").and_then(|p| p.as_u64()).unwrap_or(DEFAULT_PORT as u64)
                as u16;
            let host = args
                .get("host")
                .and_then(|h| h.as_str())
                .unwrap_or("127.0.0.1")
                .to_string();
            let mut t = BrpTarget::new(name, port);
            t.host = host;
            let mut st = state.lock().await;
            st.targets.register(t.clone());
            text_result(format!("registered target {} -> {}", t.name, t.base_url()))
        }
        "bevy_list_targets" => {
            let st = state.lock().await;
            text_result(serde_json::to_string_pretty(st.targets.list())?)
        }
        "bevy_brp_discover" => {
            let client = client_from_args(&args, &state).await?;
            let result = tokio::task::spawn_blocking(move || {
                client.call("rpc.discover", None)?.into_result()
            })
            .await??;
            text_result(serde_json::to_string_pretty(&result)?)
        }
        "bevy_brp_query" => {
            let client = client_from_args(&args, &state).await?;
            let raw: Vec<String> = args
                .get("components")
                .and_then(|c| c.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_else(crate::component_paths::default_query_components);
            let comps = crate::component_paths::expand_component_paths(&raw)?;
            let result = tokio::task::spawn_blocking(move || {
                let refs: Vec<&str> = comps.iter().map(|s| s.as_str()).collect();
                client.query(&refs)
            })
            .await??;
            text_result(serde_json::to_string_pretty(&result)?)
        }
        "bevy_brp_mutate" => {
            let client = client_from_args(&args, &state).await?;
            let entity = args["entity"]
                .as_u64()
                .ok_or_else(|| anyhow!("entity required"))?;
            let component_raw = args["component"]
                .as_str()
                .ok_or_else(|| anyhow!("component required"))?
                .to_string();
            let component = crate::component_paths::expand_component_path(&component_raw)?;
            let path = args["path"]
                .as_str()
                .ok_or_else(|| anyhow!("path required"))?
                .to_string();
            let value = args
                .get("value")
                .cloned()
                .ok_or_else(|| anyhow!("value required"))?;
            let result = tokio::task::spawn_blocking(move || {
                client.mutate_components(entity, &component, &path, value)
            })
            .await??;
            text_result(serde_json::to_string_pretty(&json!({
                "ok": true,
                "result": result
            }))?)
        }
        "bevy_brp_call" => {
            let client = client_from_args(&args, &state).await?;
            let method = args["method"]
                .as_str()
                .ok_or_else(|| anyhow!("method required"))?
                .to_string();
            let params = args.get("params").cloned();
            let result = tokio::task::spawn_blocking(move || {
                client.call(&method, params)?.into_result()
            })
            .await??;
            text_result(serde_json::to_string_pretty(&result)?)
        }
        "bevy_capture_viewport" => {
            let client = client_from_args(&args, &state).await?;
            let path = args
                .get("path")
                .and_then(|p| p.as_str())
                .unwrap_or("captures/specula-scene.png")
                .to_string();
            let img: CapturedImage = tokio::task::spawn_blocking(move || {
                capture_viewport_image(&client, PathBuf::from(path))
            })
            .await??;
            // Image + text (abs_path + bytes) so agents survive chat UI truncation.
            Ok(json!({
                "content": img.to_mcp_content_blocks(),
                "isError": false
            }))
        }
        "bevy_launch_app" => {
            let manifest = args["manifest_path"]
                .as_str()
                .ok_or_else(|| anyhow!("manifest_path required"))?
                .to_string();
            let features = args
                .get("features")
                .and_then(|f| f.as_str())
                .unwrap_or("remote,capture")
                .to_string();
            let port = args.get("port").and_then(|p| p.as_u64()).unwrap_or(DEFAULT_PORT as u64)
                as u16;
            let name = args
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("launched")
                .to_string();
            // Default 0: return immediately so host tool_timeout cannot kill cold compiles.
            let mut wait_secs = args
                .get("wait_secs")
                .and_then(|w| w.as_u64())
                .unwrap_or(0);
            if wait_secs > 60 {
                wait_secs = 60;
            }

            let manifest_path = PathBuf::from(&manifest);
            let package_dir = manifest_path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));

            // Resolve package name for warm-binary detection.
            let pkg_name = {
                let cargo_txt = std::fs::read_to_string(&manifest_path).unwrap_or_default();
                crate::launch_plan::parse_package_name_from_cargo_toml(&cargo_txt)
                    .unwrap_or_else(|| name.replace('-', "_"))
            };
            let mode = crate::launch_plan::resolve_launch_mode(&package_dir, &pkg_name);

            let log_path = std::env::temp_dir().join(format!(
                "specula-launch-{}-{}.log",
                name,
                std::process::id()
            ));
            let log_file = std::fs::File::create(&log_path)
                .with_context(|| format!("create log {}", log_path.display()))?;
            let log_err = log_file.try_clone()?;

            let mut cmd = match &mode {
                crate::launch_plan::LaunchMode::WarmBinary { binary } => {
                    let mut c = Command::new(binary);
                    c.current_dir(&package_dir)
                        .stdin(Stdio::null())
                        .stdout(Stdio::from(log_file))
                        .stderr(Stdio::from(log_err))
                        .kill_on_drop(true);
                    c
                }
                crate::launch_plan::LaunchMode::ColdCargoRun => {
                    let mut c = Command::new("cargo");
                    c.arg("run")
                        .arg("--manifest-path")
                        .arg(&manifest)
                        .arg("--features")
                        .arg(&features)
                        .current_dir(&package_dir)
                        .stdin(Stdio::null())
                        .stdout(Stdio::from(log_file))
                        .stderr(Stdio::from(log_err))
                        .kill_on_drop(true);
                    c
                }
            };

            let child = cmd.spawn().context("spawn game process")?;
            let child_pid = child.id();
            {
                let mut st = state.lock().await;
                st.child = Some(child);
                st.targets.register(BrpTarget::new(&name, port));
            }

            let spawn_msg = crate::launch_plan::format_launch_spawn_message(
                &mode,
                &manifest,
                &features,
                port,
                &name,
                &package_dir,
                &log_path,
                wait_secs,
                child_pid,
            );

            if wait_secs == 0 {
                let cold_hint = if matches!(mode, crate::launch_plan::LaunchMode::ColdCargoRun) {
                    format!(
                        "\n{}",
                        crate::launch_plan::format_missing_warm_binary(&package_dir, &pkg_name)
                    )
                } else {
                    String::new()
                };
                return text_result(format!("{spawn_msg}{cold_hint}"));
            }

            let client = BrpClient::with_port(port);
            let wait = tokio::task::spawn_blocking(move || {
                client.wait_until_ready(std::time::Duration::from_secs(wait_secs))
            })
            .await?;

            match wait {
                Ok(_) => text_result(format!(
                    "status=ready {spawn_msg}"
                )),
                Err(e) => text_result(format!(
                    "status=timeout error={e} {spawn_msg} \
                     note=app may still be compiling; call bevy_wait_brp again or use shell cargo run for cold builds"
                )),
            }
        }
        "bevy_wait_brp" => {
            let client = client_from_args(&args, &state).await?;
            let port = client.target.port;
            let timeout_secs = args
                .get("timeout_secs")
                .and_then(|t| t.as_u64())
                .unwrap_or(30)
                .min(600);
            let wait = tokio::task::spawn_blocking(move || {
                client.wait_until_ready(std::time::Duration::from_secs(timeout_secs))
            })
            .await?;
            match wait {
                Ok(_) => text_result(format!(
                    "status=ready port={port} timeout_secs={timeout_secs} note=BRP rpc.discover succeeded"
                )),
                Err(e) => text_result(format!(
                    "status=timeout port={port} timeout_secs={timeout_secs} error={e} \
                     note=check cargo compile logs; cold Bevy builds often need 180s+; ensure features remote,capture"
                )),
            }
        }
        "bevy_brp_mcp_status" => {
            let status = match which::which("bevy_brp_mcp") {
                Ok(p) => format!(
                    "bevy_brp_mcp found at {}\nRegister it in Grok Build for the full BRP tool surface.\nRecommended version: {}",
                    p.display(),
                    specula_env::compat::BEVY_BRP_MCP
                ),
                Err(_) => format!(
                    "bevy_brp_mcp not on PATH.\nInstall:\n  cargo install bevy_brp_mcp --locked\nThen:\n  grok mcp add bevy-brp -- bevy_brp_mcp\nPinned version: {}",
                    specula_env::compat::BEVY_BRP_MCP
                ),
            };
            text_result(status)
        }
        "bevy_workflow" => {
            let goal_str = args
                .get("goal")
                .and_then(|g| g.as_str())
                .ok_or_else(|| anyhow!("goal required (new_2d|new_3d|verify_scene|ship|add_sprite)"))?;
            let goal = WorkflowGoal::parse(goal_str)?;
            text_result(workflow_plan(goal))
        }
        "bevy_see_scene" => {
            let client = client_from_args(&args, &state).await?;
            let opts = see_opts_from_args(&args);
            let packet = tokio::task::spawn_blocking(move || see_scene(&client, &opts)).await??;
            packet_to_mcp_content(&packet)
        }
        "bevy_see_verify" => {
            let client = client_from_args(&args, &state).await?;
            let opts = see_opts_from_args(&args);
            let packet = tokio::task::spawn_blocking(move || see_verify(&client, &opts)).await??;
            packet_to_mcp_content(&packet)
        }
        "bevy_see_entity" => {
            let client = client_from_args(&args, &state).await?;
            let opts = see_opts_from_args(&args);
            let name = args["name"]
                .as_str()
                .ok_or_else(|| anyhow!("name required"))?
                .to_string();
            let sx = args.get("screen_x").and_then(|v| v.as_u64()).map(|v| v as u32);
            let sy = args.get("screen_y").and_then(|v| v.as_u64()).map(|v| v as u32);
            let half = args
                .get("half")
                .and_then(|v| v.as_u64())
                .unwrap_or(DEFAULT_CROP_HALF as u64) as u32;
            let packet = tokio::task::spawn_blocking(move || {
                see_entity(&client, &opts, &name, sx, sy, half)
            })
            .await??;
            packet_to_mcp_content(&packet)
        }
        "bevy_see_region" => {
            let client = client_from_args(&args, &state).await?;
            let opts = see_opts_from_args(&args);
            let x = args["x"].as_u64().ok_or_else(|| anyhow!("x required"))? as u32;
            let y = args["y"].as_u64().ok_or_else(|| anyhow!("y required"))? as u32;
            let w = args["w"].as_u64().ok_or_else(|| anyhow!("w required"))? as u32;
            let h = args["h"].as_u64().ok_or_else(|| anyhow!("h required"))? as u32;
            let label = args
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("region")
                .to_string();
            let packet = tokio::task::spawn_blocking(move || {
                see_region(&client, &opts, x, y, w, h, &label)
            })
            .await??;
            packet_to_mcp_content(&packet)
        }
        "bevy_see_motion" => {
            let client = client_from_args(&args, &state).await?;
            let mut opts = see_opts_from_args(&args);
            if let Some(e) = args.get("mutate_entity").and_then(|v| v.as_u64()) {
                opts.motion_mutate_entity = Some(e);
            }
            if let Some(tr) = args.get("mutate_translation").cloned() {
                opts.motion_mutate_translation = Some(tr);
            }
            let frames = args
                .get("frames")
                .and_then(|v| v.as_u64())
                .unwrap_or(DEFAULT_MOTION_FRAMES as u64) as u32;
            let interval_ms = args
                .get("interval_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(DEFAULT_MOTION_INTERVAL_MS);
            let keys = args.get("keys").and_then(|v| v.as_array()).map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            });
            let packet = tokio::task::spawn_blocking(move || {
                see_motion(&client, &opts, frames, interval_ms, keys)
            })
            .await??;
            packet_to_mcp_content(&packet)
        }
        "bevy_see_diff" => {
            let client = client_from_args(&args, &state).await?;
            let opts = see_opts_from_args(&args);
            let baseline = args["baseline"]
                .as_str()
                .ok_or_else(|| anyhow!("baseline required"))?
                .to_string();
            let packet = tokio::task::spawn_blocking(move || {
                see_diff(&client, &opts, PathBuf::from(baseline))
            })
            .await??;
            packet_to_mcp_content(&packet)
        }
        "bevy_see_pack" => {
            let client = client_from_args(&args, &state).await?;
            let opts = see_opts_from_args(&args);
            let pack = args["pack"]
                .as_str()
                .ok_or_else(|| anyhow!("pack required"))?
                .to_string();
            let packet =
                tokio::task::spawn_blocking(move || see_pack(&client, &opts, &pack)).await??;
            packet_to_mcp_content(&packet)
        }
        other => Err(anyhow!("unknown tool: {other}")),
    }
}

fn see_opts_from_args(args: &Value) -> SeeOptions {
    let profile = args
        .get("profile")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let projection = match args
        .get("projection")
        .and_then(|v| v.as_str())
        .unwrap_or("ortho2d")
        .to_ascii_lowercase()
        .as_str()
    {
        "topdown3d" | "top_down" | "3d" | "topdown" => ProjectionMode::TopDown3d,
        _ => ProjectionMode::Ortho2d,
    };
    let wait_for_subjects = args
        .get("wait_for_subjects")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let mut opts = SeeOptions {
        out_dir: PathBuf::from(
            args.get("out_dir")
                .and_then(|v| v.as_str())
                .unwrap_or("."),
        ),
        subject_class: args
            .get("subject_class")
            .and_then(|v| v.as_str())
            .unwrap_or("scene")
            .to_string(),
        intent: args
            .get("intent")
            .and_then(|v| v.as_str())
            .unwrap_or("verify scene appearance")
            .to_string(),
        style_intent: args
            .get("style_intent")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        app_state: args
            .get("app_state")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        target_name: args
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        port: args.get("port").and_then(|v| v.as_u64()).map(|v| v as u16),
        subject_filter: SubjectFilterMode::parse(
            args.get("subject_filter")
                .and_then(|v| v.as_str())
                .unwrap_or("gameplay_prefer"),
        ),
        max_subjects: args
            .get("max_subjects")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_MAX_SUBJECTS as u64) as usize,
        wait_for_subjects,
        wait_timeout_secs: args
            .get("wait_timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(15),
        require_playing: args
            .get("require_playing")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        projection,
        visible_half_w: args
            .get("visible_half_w")
            .and_then(|v| v.as_f64())
            .unwrap_or(640.0),
        visible_half_h: args
            .get("visible_half_h")
            .and_then(|v| v.as_f64())
            .unwrap_or(360.0),
        compare_baseline: args
            .get("compare_baseline")
            .and_then(|v| v.as_str())
            .map(PathBuf::from),
        save_baseline_as: args
            .get("save_baseline")
            .and_then(|v| v.as_str())
            .map(PathBuf::from),
        zoom_ladder: args
            .get("zoom_ladder")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        diagnostic_bounds: args
            .get("diagnostic_bounds")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        profile: profile.clone(),
        include_primary_fovea: args
            .get("include_primary_fovea")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        motion_mutate_entity: args.get("mutate_entity").and_then(|v| v.as_u64()),
        motion_mutate_translation: args.get("mutate_translation").cloned(),
        auto_baseline: args
            .get("auto_baseline")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    };
    if let Some(ref p) = profile {
        apply_game_profile(&mut opts, p);
        // Explicit projection override after profile if user passed projection key
        if args.get("projection").and_then(|v| v.as_str()).is_some() {
            opts.projection = projection;
        }
        if args.get("require_playing").and_then(|v| v.as_bool()).is_some() {
            opts.require_playing = args["require_playing"].as_bool().unwrap_or(opts.require_playing);
        }
    }
    opts
}

async fn client_from_args(args: &Value, state: &Arc<Mutex<ServerState>>) -> Result<BrpClient> {
    let port = args.get("port").and_then(|p| p.as_u64()).map(|p| p as u16);
    let name = args.get("target").and_then(|t| t.as_str());
    let st = state.lock().await;
    let target = st.targets.resolve(name, port);
    Ok(BrpClient::new(target))
}

fn text_result(text: impl Into<String>) -> Result<Value> {
    Ok(json!({
        "content": [{ "type": "text", "text": text.into() }],
        "isError": false
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_instructions_cover_production_and_scaffold() {
        let s = mcp_instructions();
        assert!(!s.is_empty());
        assert!(s.contains("bevy-production"));
        assert!(s.contains("bevy-2d-game") || s.contains("2d"));
        assert!(s.contains("scaffold"));
        assert!(s.contains("2d") && s.contains("3d") && s.contains("demo"));
        assert!(s.contains("game-asset-core") || s.contains("assets/sprites"));
        assert!(s.contains("cargo build --release") || s.contains("Ship"));
        assert!(s.contains("start_2d_game") && s.contains("bevy_workflow"));
        assert!(s.contains("GAME_DOD") || s.contains("bevy-demo-game"));
        assert!(s.contains("complete_demo_2d") || s.contains("package_demo"));
    }

    #[test]
    fn prompt_catalog_has_production_and_demo_entry_points() {
        assert!(prompt_catalog().len() >= 7);
        for p in prompt_catalog() {
            assert!(!p.description.is_empty(), "{} empty description", p.name);
            assert!(!p.body.is_empty(), "{} empty body", p.name);
        }
        let names: Vec<&str> = prompt_catalog().iter().map(|p| p.name).collect();
        for expected in [
            "start_2d_game",
            "start_3d_game",
            "build_demo_2d",
            "build_demo_3d",
            "iterate_scene",
            "prepare_ship",
            "package_demo",
        ] {
            assert!(names.contains(&expected), "missing prompt {expected}");
        }
    }

    #[test]
    fn prompts_list_and_get_drive_catalog() {
        let list = prompts_list_json();
        let arr = list.as_array().expect("array");
        assert_eq!(arr.len(), prompt_catalog().len());
        for item in arr {
            assert!(item.get("name").and_then(|n| n.as_str()).is_some());
            assert!(item
                .get("description")
                .and_then(|d| d.as_str())
                .is_some_and(|s| !s.is_empty()));
        }

        let two_d = prompts_get_json("start_2d_game").unwrap();
        let text = two_d["messages"][0]["content"]["text"]
            .as_str()
            .unwrap();
        assert!(text.contains("bevy-2d-game"));
        assert!(text.contains("GAME_DOD") || text.contains("bevy-demo-game"));

        let build = prompts_get_json("build_demo_2d").unwrap();
        let text = build["messages"][0]["content"]["text"].as_str().unwrap();
        assert!(text.contains("GAME_DOD") || text.contains("bevy-demo-game"));

        let three_d = prompts_get_json("start_3d_game").unwrap();
        let text = three_d["messages"][0]["content"]["text"]
            .as_str()
            .unwrap();
        assert!(text.contains("bevy-3d-game"));

        let iterate = prompts_get_json("iterate_scene").unwrap();
        let text = iterate["messages"][0]["content"]["text"]
            .as_str()
            .unwrap();
        assert!(text.contains("bevy-agent-loop") || text.contains("bevy_capture_viewport"));

        let ship = prompts_get_json("prepare_ship").unwrap();
        let text = ship["messages"][0]["content"]["text"].as_str().unwrap();
        assert!(text.contains("cargo build --release") || text.contains("release"));

        let pkg = prompts_get_json("package_demo").unwrap();
        let text = pkg["messages"][0]["content"]["text"].as_str().unwrap();
        assert!(text.contains("assets") || text.contains("dist"));

        assert!(prompts_get_json("no_such_prompt").is_err());
    }

    #[test]
    fn workflow_plan_lists_skills_and_tools_for_goals() {
        let two_d = workflow_plan(WorkflowGoal::parse("new_2d").unwrap());
        assert!(two_d.contains("bevy-production"));
        assert!(two_d.contains("bevy-2d-game"));
        assert!(two_d.contains("bevy-demo-game"));
        assert!(two_d.contains("scaffold --kind 2d") || two_d.contains("--kind 2d") || two_d.contains("demo-2d"));
        assert!(two_d.contains("bevy_launch_app") || two_d.contains("bevy_capture_viewport"));

        let complete = workflow_plan(WorkflowGoal::parse("complete_demo_2d").unwrap());
        assert!(complete.contains("GAME_DOD") || complete.contains("bevy-demo-game"));
        assert!(complete.contains("Victory") || complete.contains("GameOver") || complete.contains("end"));

        let ship = workflow_plan(WorkflowGoal::parse("ship").unwrap());
        assert!(ship.contains("bevy-production"));
        assert!(ship.contains("cargo build --release"));

        let package = workflow_plan(WorkflowGoal::parse("package_demo").unwrap());
        assert!(package.contains("dist") || package.contains("assets"));

        let iterate = workflow_plan(WorkflowGoal::parse("verify_scene").unwrap());
        assert!(iterate.contains("bevy-agent-loop"));
        assert!(
            iterate.contains("bevy_see_scene") || iterate.contains("bevy_capture_viewport")
        );

        assert!(WorkflowGoal::parse("not_a_goal").is_err());
    }

    #[test]
    fn tool_defs_include_bevy_workflow() {
        let tools = tool_defs();
        let arr = tools.as_array().unwrap();
        let names: Vec<&str> = arr
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(names.contains(&"bevy_workflow"));
        assert!(names.contains(&"bevy_capture_viewport"));
        assert!(names.contains(&"bevy_wait_brp"));
        assert!(names.contains(&"bevy_launch_app"));
        for see in [
            "bevy_see_scene",
            "bevy_see_verify",
            "bevy_see_entity",
            "bevy_see_region",
            "bevy_see_motion",
            "bevy_see_diff",
            "bevy_see_pack",
        ] {
            assert!(names.contains(&see), "missing eyesight tool {see}");
        }
    }

    #[test]
    fn instructions_mention_profiles() {
        let s = mcp_instructions();
        assert!(s.contains("crystal-drift") || s.contains("profile"));
        assert!(s.contains("iron-feud") || s.contains("IRON_FEUD"));
    }

    #[test]
    fn instructions_mention_eyesight() {
        let s = mcp_instructions();
        assert!(s.contains("bevy_see_scene") || s.contains("bevy_see_verify") || s.contains("sight"));
        assert!(
            s.contains("AGENT_SIGHT")
                || s.contains("AGENT_EYESIGHT")
                || s.contains("abs_path")
                || s.contains("profile")
        );
    }

    #[test]
    fn verify_scene_mentions_see_scene() {
        let plan = workflow_plan(WorkflowGoal::VerifyScene);
        assert!(plan.contains("bevy_see_scene"));
        assert!(plan.contains("abs_path") || plan.contains("OPEN"));
    }

    #[test]
    fn workflow_verify_mentions_wait_brp() {
        let plan = workflow_plan(WorkflowGoal::VerifyScene);
        assert!(plan.contains("bevy_wait_brp"));
        assert!(plan.contains("bevy_launch_app"));
    }

    #[test]
    fn instructions_mention_nonblocking_launch() {
        let s = mcp_instructions();
        assert!(s.contains("bevy_wait_brp") || s.contains("wait_secs"));
        assert!(s.contains("router") || s.contains("autopilot"));
    }

    #[test]
    fn capture_tool_description_is_dimension_neutral() {
        let tools = tool_defs();
        let arr = tools.as_array().unwrap();
        let cap = arr
            .iter()
            .find(|t| t.get("name").and_then(|n| n.as_str()) == Some("bevy_capture_viewport"))
            .expect("capture tool");
        let desc = cap["description"].as_str().unwrap();
        assert!(
            !desc.contains("3D scene") || desc.contains("2D"),
            "capture must not be 3D-only: {desc}"
        );
        assert!(
            desc.to_lowercase().contains("primary window")
                || desc.contains("2D")
                || desc.contains("2d"),
            "expected primary window / 2D language: {desc}"
        );
    }

    #[test]
    fn query_tool_mentions_aliases() {
        let tools = tool_defs();
        let arr = tools.as_array().unwrap();
        let q = arr
            .iter()
            .find(|t| t.get("name").and_then(|n| n.as_str()) == Some("bevy_brp_query"))
            .unwrap();
        let desc = q["description"].as_str().unwrap();
        assert!(desc.contains("Name") && desc.contains("Transform"));
    }
}
