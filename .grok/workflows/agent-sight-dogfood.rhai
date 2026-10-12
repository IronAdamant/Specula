// Specula agent sight regression harness + Crystal Drift / Iron Feud dogfood.
// Project-specific: platform gates and packet↔pixel honesty first; dogfood second.
// Sequential BRP 15702 only. No livestream, editor, or taste scoring.
// P0 (2026-07-25): skip_install must not reinstall; IF restore camera before see entity.
// Closeout bar for soft / still-soft / trust sight plans: ALWAYS mode=full (default below).
// mode=gates is debug-only — do not change the default to gates.

let meta = #{
    name: "agent-sight-dogfood",
    description: "Specula agent sight gates + sequential Crystal Drift / Iron Feud dogfood + packet-pixel skeptic",
    when_to_use: "After eyesight/ranking changes, before claiming dogfood green, or to re-verify CD+IF sight honesty",
    phases: [
        #{ title: "Gates", detail: "cargo test sight crates; stems/magenta/nonblack; optional MCP install" },
        #{ title: "Dogfood-2D", detail: "Crystal Drift remote+capture; see verify + env packs" },
        #{ title: "Dogfood-3D", detail: "Iron Feud AUTO_PLAY; see verify + landscape/water" },
        #{ title: "Skeptic", detail: "adversarial packet vs opened PNG claims" },
        #{ title: "Report", detail: "structured summary + scratch report" },
    ],
};

// --- schemas ---
let gates_schema = #{
    "type": "object",
    "required": ["passed", "tests_ok", "notes", "evidence_paths"],
    "properties": #{
        "passed": #{ "type": "boolean" },
        "tests_ok": #{ "type": "boolean" },
        "mcp_rebuilt": #{ "type": "boolean" },
        "notes": #{ "type": "string" },
        "evidence_paths": #{
            "type": "array",
            "items": #{ "type": "string" },
            "maxItems": 12,
        },
    },
};

let dogfood_schema = #{
    "type": "object",
    "required": ["game", "passed", "app_state", "primary_subject", "new_name_present", "notes", "capture_paths"],
    "properties": #{
        "game": #{ "type": "string" },
        "passed": #{ "type": "boolean" },
        "app_state": #{ "type": "string" },
        "primary_subject": #{ "type": "string" },
        "new_name_present": #{ "type": "boolean" },
        "magenta_ok": #{ "type": "boolean" },
        "multi_view_honest": #{ "type": "boolean" },
        "notes": #{ "type": "string" },
        "capture_paths": #{
            "type": "array",
            "items": #{ "type": "string" },
            "maxItems": 16,
        },
    },
};

let skeptic_schema = #{
    "type": "object",
    "required": ["green", "claims_checked", "failures", "notes"],
    "properties": #{
        "green": #{ "type": "boolean" },
        "claims_checked": #{ "type": "integer" },
        "failures": #{
            "type": "array",
            "maxItems": 12,
            "items": #{
                "type": "object",
                "required": ["claim", "reason"],
                "properties": #{
                    "claim": #{ "type": "string" },
                    "reason": #{ "type": "string" },
                },
            },
        },
        "notes": #{ "type": "string" },
    },
};

// --- args (guard unit) ---
let mode = "full";
let cd_path = "/Users/aron/Documents/coding_projects/Crystal Drift";
let if_path = "/Users/aron/Documents/coding_projects/Iron Feud";
let require_cd_name = "CargoPod";
let require_if_name = "OreCrusher";
let skip_install = true;
let specula_root = "/Users/aron/Documents/coding_projects/Specula";

if args != () {
    if args.mode != () { mode = args.mode; }
    if args.cd_path != () { cd_path = args.cd_path; }
    if args.if_path != () { if_path = args.if_path; }
    if args.require_cd_name != () { require_cd_name = args.require_cd_name; }
    if args.require_if_name != () { require_if_name = args.require_if_name; }
    if args.skip_install != () { skip_install = args.skip_install; }
    if args.specula_root != () { specula_root = args.specula_root; }
}

if mode != "gates" && mode != "full" {
    pause("verification", "args.mode must be \"gates\" or \"full\" (default full).");
}

log("agent-sight-dogfood mode=" + mode);

// ============================================================================
// Phase: Gates (project-specific — not dogfood)
// ============================================================================
phase("Gates");

let gates_prompt = "";
gates_prompt += "You are verifying Specula AGENT SIGHT platform gates (not game craft).\n";
gates_prompt += "Repo root: " + specula_root + "\n";
gates_prompt += "Do this with shell + read tools; do not answer from memory.\n";
gates_prompt += "1. cd to repo root. Run: cargo test -p specula -p specula-brp\n";
gates_prompt += "   Capture pass/fail. tests_ok=true only if all tests PASS.\n";
gates_prompt += "2. Confirm eyesight helpers exist (grep or read):\n";
gates_prompt += "   png_nonblack_fraction, png_true_magenta_pixel_count, DOGFOOD_NAME_STEMS,\n";
gates_prompt += "   side_orbit_camera_translation, gameplay_subject_score, is_child_mesh_part.\n";
gates_prompt += "3. MCP install policy (follow exactly; do not improvise):\n";
if skip_install {
    gates_prompt += "   skip_install=TRUE for this run.\n";
    gates_prompt += "   Do NOT run cargo install. Do NOT rebuild/reinstall specula.\n";
    gates_prompt += "   Set mcp_rebuilt=false. (Even if eyesight.rs looks dirty — still skip install.)\n";
} else {
    gates_prompt += "   skip_install=FALSE for this run.\n";
    gates_prompt += "   Run: cargo install --path crates/specula --force\n";
    gates_prompt += "   Set mcp_rebuilt=true only if that install succeeds.\n";
}
gates_prompt += "4. Write evidence under the Specula repo or a captures/ subdir — NOT shared /tmp if avoidable.\n";
gates_prompt += "   List evidence_paths (absolute).\n";
gates_prompt += "5. passed=true only if tests_ok=true and helpers exist.\n";
gates_prompt += "Exclusions: do not launch games, do not score taste, no livestream.\n";
gates_prompt += "Return the schema fields only after running the commands.\n";

let gates_r = agent(gates_prompt, #{
    label: "gates",
    capability_mode: "execute",
    output_schema: gates_schema,
});

let gates_passed = false;
let gates_notes = "gates agent failed or missing output";
let gates_tests_ok = false;
let gates_mcp = false;
let gates_evidence = [];

if gates_r != () && gates_r.success && gates_r.output != () {
    gates_passed = gates_r.output.passed == true;
    gates_tests_ok = gates_r.output.tests_ok == true;
    if gates_r.output.mcp_rebuilt != () {
        gates_mcp = gates_r.output.mcp_rebuilt == true;
    }
    if gates_r.output.notes != () {
        gates_notes = gates_r.output.notes;
    }
    if gates_r.output.evidence_paths != () {
        gates_evidence = gates_r.output.evidence_paths;
    }
}

if !gates_passed {
    let fail_report = "";
    fail_report += "# Agent sight dogfood — FAILED at Gates\n\n";
    fail_report += "tests_ok=" + gates_tests_ok.to_string() + "\n";
    fail_report += "notes: " + gates_notes + "\n";
    let fail_path = write_scratch_file("agent-sight-report.md", fail_report);
    complete(#{
        summary: "FAILED gates — platform tests or helpers not green",
        mode: mode,
        passed: false,
        phase_failed: "Gates",
        gates_notes: gates_notes,
        path: fail_path,
    });
}

log("Gates passed; tests_ok=" + gates_tests_ok.to_string() + " mcp_rebuilt=" + gates_mcp.to_string());

// Short path: platform only
if mode == "gates" {
    phase("Report");
    let rep = "";
    rep += "# Agent sight — gates only\n\n";
    rep += "passed: true\n";
    rep += "tests_ok: " + gates_tests_ok.to_string() + "\n";
    rep += "mcp_rebuilt: " + gates_mcp.to_string() + "\n";
    rep += "notes: " + gates_notes + "\n";
    let p = write_scratch_file("agent-sight-report.md", rep);
    complete(#{
        summary: "Gates-only PASS",
        mode: "gates",
        passed: true,
        gates_notes: gates_notes,
        mcp_rebuilt: gates_mcp,
        path: p,
    });
}

// ============================================================================
// Phase: Dogfood-2D Crystal Drift (sequential BRP)
// ============================================================================
phase("Dogfood-2D");

let cd_prompt = "";
cd_prompt += "You dogfood Crystal Drift for Specula agent sight. Bevy 0.19, BRP port 15702, features remote,capture.\n";
cd_prompt += "Game path: " + cd_path + "\n";
cd_prompt += "Required Name in subjects (if spawned): " + require_cd_name + "\n";
cd_prompt += "Hard rules:\n";
cd_prompt += "- Sequential only: killall crystal_drift iron_feud before launch.\n";
cd_prompt += "- Build: cargo build --features remote,capture in the game path.\n";
cd_prompt += "- Optional: run Specula scripts/check_sprite_transparency.py on assets/sprites (true-magenta 0).\n";
cd_prompt += "- Launch: cargo run --features remote,capture (background). Then specula brp wait --port 15702.\n";
cd_prompt += "- Prefer warm binary; do not block forever on cold compile without logging.\n";
cd_prompt += "- Run: specula see verify --profile crystal-drift --out-dir <game_or_scratch> --save-baseline once.\n";
cd_prompt += "- Packs: specula see pack --profile crystal-drift env_2d  and  hud  (positional pack name).\n";
cd_prompt += "- OPEN every PNG abs_path returned. primary_subject should be Player.\n";
cd_prompt += "- new_name_present=true only if " + require_cd_name + " appears in verify subjects (or document if intentionally absent).\n";
cd_prompt += "- magenta_ok=true if sprite audit passes or no true-magenta plates in craft sprites.\n";
cd_prompt += "- multi_view_honest=true for 2D if packs ran or N/A noted honestly.\n";
cd_prompt += "- game field must be \"crystal-drift\". passed=true only if Playing-like app, Player primary, captures non-empty, no purple square plates in opened full frame.\n";
cd_prompt += "- List capture_paths (absolute). notes = what you saw.\n";
cd_prompt += "- Stop the game after captures (killall crystal_drift).\n";
cd_prompt += "Taste/design human-owned. No livestream.\n";

let cd_r = agent(cd_prompt, #{
    label: "dogfood-cd",
    capability_mode: "all",
    output_schema: dogfood_schema,
});

let cd_passed = false;
let cd_notes = "CD dogfood agent failed";
let cd_primary = "";
let cd_state = "";
let cd_name_ok = false;
let cd_captures = [];

if cd_r != () && cd_r.success && cd_r.output != () {
    cd_passed = cd_r.output.passed == true;
    if cd_r.output.notes != () { cd_notes = cd_r.output.notes; }
    if cd_r.output.primary_subject != () { cd_primary = cd_r.output.primary_subject; }
    if cd_r.output.app_state != () { cd_state = cd_r.output.app_state; }
    cd_name_ok = cd_r.output.new_name_present == true;
    if cd_r.output.capture_paths != () { cd_captures = cd_r.output.capture_paths; }
}

if !cd_passed {
    let fail_report = "";
    fail_report += "# Agent sight dogfood — FAILED at Dogfood-2D (Crystal Drift)\n\n";
    fail_report += "primary=" + cd_primary + " state=" + cd_state + "\n";
    fail_report += "new_name_present=" + cd_name_ok.to_string() + " require=" + require_cd_name + "\n";
    fail_report += "notes: " + cd_notes + "\n";
    let fail_path = write_scratch_file("agent-sight-report.md", fail_report);
    complete(#{
        summary: "FAILED Crystal Drift dogfood",
        mode: mode,
        passed: false,
        phase_failed: "Dogfood-2D",
        cd_notes: cd_notes,
        path: fail_path,
    });
}

log("Dogfood-2D passed; primary=" + cd_primary);

// ============================================================================
// Phase: Dogfood-3D Iron Feud (after CD stopped)
// ============================================================================
phase("Dogfood-3D");

let if_prompt = "";
if_prompt += "You dogfood Iron Feud for Specula agent sight. Bevy 0.19, BRP port 15702, features remote,capture.\n";
if_prompt += "Game path: " + if_path + "\n";
if_prompt += "Required Name in subjects: " + require_if_name + "\n";
if_prompt += "Hard rules:\n";
if_prompt += "- Sequential: killall crystal_drift iron_feud first (CD must be down).\n";
if_prompt += "- Build: cargo build --features remote,capture.\n";
if_prompt += "- Launch with IRON_FEUD_AUTO_PLAY=1 cargo run --features remote,capture (background).\n";
if_prompt += "- specula brp wait --port 15702.\n";
if_prompt += "- Use one out-dir under the game tree or Specula captures/ (not shared /tmp).\n";
if_prompt += "ORDERED steps (do not reorder):\n";
if_prompt += "A) specula see verify --profile iron-feud --out-dir <path> --save-baseline once.\n";
if_prompt += "B) Packs: specula see pack --profile iron-feud landscape  then  water.\n";
if_prompt += "   Note: landscape/water alt may BRP-mutate StrategyCamera translation.\n";
if_prompt += "C) CAMERA RESTORE: specula see_pack now restores StrategyCamera after alt/side (T2 code path).\n";
if_prompt += "   Still verify: after packs, optionally re-query StrategyCamera translation ≈ pre-pack, or re-run\n";
if_prompt += "   see verify once on game view before entity fovea. Do NOT claim craft on a leftover alt frame.\n";
if_prompt += "D) specula see entity --name " + require_if_name + " --profile iron-feud\n";
if_prompt += "   MUST pass --profile iron-feud. Note must say topdown3d XZ. Fovea aim must be off false-center\n";
if_prompt += "   (not ~1303,776 class miss). OPEN crop; claim craft only if crusher visible.\n";
if_prompt += "E) OPEN every PNG. app_state must be Playing. primary must NOT be OreCrystal*.\n";
if_prompt += "- new_name_present=true only if " + require_if_name + " is in ranked subjects (not only child mesh parts).\n";
if_prompt += "- multi_view_honest=true if landscape game and alt differ OR packet honestly sets views_similar / side path notes — never claim multi-angle if identical without warning.\n";
if_prompt += "- game=\"iron-feud\". passed=true only if Playing, sensible primary, non-empty landscape,\n";
if_prompt += "  height/relief readable or noted, required Name present.\n";
if_prompt += "  Entity step is required for craft check; if restore fails, set passed=false and explain.\n";
if_prompt += "- List capture_paths. Stop iron_feud after.\n";
if_prompt += "No taste scoring. No dual games on 15702.\n";

let if_r = agent(if_prompt, #{
    label: "dogfood-if",
    capability_mode: "all",
    output_schema: dogfood_schema,
});

let if_passed = false;
let if_notes = "IF dogfood agent failed";
let if_primary = "";
let if_state = "";
let if_name_ok = false;
let if_captures = [];
let if_mv = false;

if if_r != () && if_r.success && if_r.output != () {
    if_passed = if_r.output.passed == true;
    if if_r.output.notes != () { if_notes = if_r.output.notes; }
    if if_r.output.primary_subject != () { if_primary = if_r.output.primary_subject; }
    if if_r.output.app_state != () { if_state = if_r.output.app_state; }
    if_name_ok = if_r.output.new_name_present == true;
    if if_r.output.capture_paths != () { if_captures = if_r.output.capture_paths; }
    if if_r.output.multi_view_honest != () {
        if_mv = if_r.output.multi_view_honest == true;
    }
}

if !if_passed {
    let fail_report = "";
    fail_report += "# Agent sight dogfood — FAILED at Dogfood-3D (Iron Feud)\n\n";
    fail_report += "primary=" + if_primary + " state=" + if_state + "\n";
    fail_report += "new_name_present=" + if_name_ok.to_string() + " require=" + require_if_name + "\n";
    fail_report += "notes: " + if_notes + "\n";
    let fail_path = write_scratch_file("agent-sight-report.md", fail_report);
    complete(#{
        summary: "FAILED Iron Feud dogfood",
        mode: mode,
        passed: false,
        phase_failed: "Dogfood-3D",
        if_notes: if_notes,
        path: fail_path,
    });
}

log("Dogfood-3D passed; primary=" + if_primary);

// ============================================================================
// Phase: Skeptic (project-specific — packet vs pixels)
// ============================================================================
phase("Skeptic");

let sk_prompt = "";
sk_prompt += "Adversarially verify Specula agent sight claims for this dogfood run. FAIL CLOSED.\n";
sk_prompt += "You must OPEN and inspect capture PNGs (read_file on image paths) and/or re-check packet JSON on disk.\n";
sk_prompt += "Do not trust dogfood agent notes without pixel or packet evidence.\n\n";
sk_prompt += "Crystal Drift claimed:\n";
sk_prompt += "- primary=" + cd_primary + " state=" + cd_state + " name_ok=" + cd_name_ok.to_string() + "\n";
sk_prompt += "- notes: " + cd_notes + "\n";
sk_prompt += "- capture_paths: " + json_encode(cd_captures) + "\n\n";
sk_prompt += "Iron Feud claimed:\n";
sk_prompt += "- primary=" + if_primary + " state=" + if_state + " name_ok=" + if_name_ok.to_string() + " multi_view_honest=" + if_mv.to_string() + "\n";
sk_prompt += "- notes: " + if_notes + "\n";
sk_prompt += "- capture_paths: " + json_encode(if_captures) + "\n\n";
sk_prompt += "Check at least:\n";
sk_prompt += "1) CD primary is Player (or document contradiction).\n";
sk_prompt += "2) CD full frame not pure black craft-less void if env claimed; no true-magenta square plates.\n";
sk_prompt += "3) IF app Playing; primary not OreCrystal*.\n";
sk_prompt += "4) IF required Name " + require_if_name + " present in a verify packet if name_ok claimed.\n";
sk_prompt += "5) IF multi-view: game vs alt differ OR honest views_similar — refute fake multi-angle claims.\n";
sk_prompt += "6) Child mesh Names should not be the only evidence of a feature (e.g. CrusherChassis alone ≠ OreCrusher).\n";
sk_prompt += "7) IF entity fovea for " + require_if_name + ": crop must match game-view craft, not a leftover\n";
sk_prompt += "   side-orbit/alt frame. Misaligned fovea after packs without restore = failure.\n";
sk_prompt += "Set green=true only if no real failures. failures[] = concrete {claim, reason} with evidence.\n";
sk_prompt += "claims_checked = integer count of claims you actually inspected.\n";
sk_prompt += "If capture_paths empty or unreadable, green=false and report that.\n";

let sk_r = agent(sk_prompt, #{
    label: "skeptic",
    capability_mode: "read-only",
    output_schema: skeptic_schema,
});

let sk_green = false;
let sk_notes = "skeptic failed or missing";
let sk_failures = [];
let sk_checked = 0;

if sk_r != () && sk_r.success && sk_r.output != () {
    sk_green = sk_r.output.green == true;
    if sk_r.output.notes != () { sk_notes = sk_r.output.notes; }
    if sk_r.output.failures != () { sk_failures = sk_r.output.failures; }
    if sk_r.output.claims_checked != () { sk_checked = sk_r.output.claims_checked; }
}

// Fail closed: skeptic must succeed and green
let overall = gates_passed && cd_passed && if_passed && sk_green
    && sk_r != () && sk_r.success;

// ============================================================================
// Phase: Report
// ============================================================================
phase("Report");

let report = "";
report += "# Agent sight dogfood report\n\n";
report += "mode: " + mode + "\n";
report += "overall_passed: " + overall.to_string() + "\n\n";
report += "## Gates\n";
report += "- passed: " + gates_passed.to_string() + "\n";
report += "- tests_ok: " + gates_tests_ok.to_string() + "\n";
report += "- mcp_rebuilt: " + gates_mcp.to_string() + "\n";
report += "- notes: " + gates_notes + "\n\n";
report += "## Crystal Drift\n";
report += "- passed: " + cd_passed.to_string() + "\n";
report += "- primary: " + cd_primary + "\n";
report += "- state: " + cd_state + "\n";
report += "- require_name: " + require_cd_name + " present=" + cd_name_ok.to_string() + "\n";
report += "- notes: " + cd_notes + "\n\n";
report += "## Iron Feud\n";
report += "- passed: " + if_passed.to_string() + "\n";
report += "- primary: " + if_primary + "\n";
report += "- state: " + if_state + "\n";
report += "- require_name: " + require_if_name + " present=" + if_name_ok.to_string() + "\n";
report += "- multi_view_honest: " + if_mv.to_string() + "\n";
report += "- notes: " + if_notes + "\n\n";
report += "## Skeptic\n";
report += "- green: " + sk_green.to_string() + "\n";
report += "- claims_checked: " + sk_checked.to_string() + "\n";
report += "- notes: " + sk_notes + "\n";
report += "- failures: " + json_encode(sk_failures) + "\n\n";
report += "Exclusions held: no livestream, no taste scoring, sequential BRP 15702.\n";

let report_path = write_scratch_file("agent-sight-report.md", report);

let summary = "agent-sight-dogfood PASS";
if !overall {
    summary = "agent-sight-dogfood FAIL (see report)";
}

complete(#{
    summary: summary,
    mode: mode,
    passed: overall,
    gates_passed: gates_passed,
    cd_passed: cd_passed,
    if_passed: if_passed,
    skeptic_green: sk_green,
    require_cd_name: require_cd_name,
    require_if_name: require_if_name,
    cd_primary: cd_primary,
    if_primary: if_primary,
    path: report_path,
});
