//! `ui_act`, `ui_tree` and the `ui` overlay on a real engine (review R1 of m5-uitools: each
//! probe of the review is a test here). A small hosted game with a HUD button and a modal menu
//! stands in for Sepulcra: its frames write `/cmd` when the player's input sends `go` (a command:
//! the caller's write), and move `/mover` in every frame that presents (the game's own
//! interpolation). Needs Vulkan for the engine; skips cleanly without it.

use super::*;
use crate::authority::{Authority, ROLE_OWNER};
use crate::claims::{ClaimSpec, Scope, DEFAULT_LEASE};
use crate::game::{Frame, Game};
use crate::world::{EntityKey, Shape, SpawnSpec, UpdateSpec};
use specula_core::math::{Mat4, Vec3};
use specula_input::Input;

const HUD: &str = r#"{"format": "specula.ui.screen.v1", "name": "hud", "input": "hud", "root": {"type": "box", "children": [
    {"type": "button", "id": "go", "text": "Go", "action": "go", "shortcut": "go", "anchor": "top_right"}]}}"#;
const MENU: &str = r#"{"format": "specula.ui.screen.v1", "name": "menu", "input": "menu", "modal": true, "layer": 10, "focus": "a", "back": "close", "opened_by": ["open"],
    "root": {"type": "box", "children": [{"type": "panel", "anchor": "center", "layout": "column", "gap": 8, "children": [
        {"type": "button", "id": "a", "text": "Alpha", "action": "a"},
        {"type": "button", "id": "b", "text": "Bravo", "action": "go"},
        {"type": "button", "id": "c", "text": "Charlie", "action": "close"}]}]}}"#;
const MAP: &str = r#"{"format": "specula.input.v1", "actions": [
    {"name": "go", "kind": "button", "bindings": ["Key.G", "Pad.West"]},
    {"name": "open", "kind": "button", "bindings": ["Key.P", "Pad.Start"]}]}"#;

struct TestGame {
    ui: Ui,
    state: StateMap,
    input: Input,
    display: Display,
    t: f64,
    /// Engine-thread time each frame costs (busy, as a real game's work is).
    cost: Duration,
    presented: u32,
    commands: u32,
    /// The display each overlay (a capture's interface) was drawn for.
    overlays: Arc<std::sync::Mutex<Vec<Display>>>,
}

impl TestGame {
    fn new() -> TestGame {
        let theme = specula_ui::Theme::from_json_str(include_str!("../../../../games/sepulcra/assets/ui/theme.json")).unwrap();
        let fonts = specula_ui::FontSet::new().with("ui", specula_ui::testfont::box_font_parsed());
        let mut ui = Ui::new(theme, fonts);
        ui.load_screen(HUD).unwrap();
        ui.load_screen(MENU).unwrap();
        ui.show("hud").unwrap();
        let mut map = ActionMap::from_json_str(MAP).unwrap();
        specula_ui::add_ui_actions(&mut map).unwrap();
        TestGame { ui, state: StateMap::new(), input: Input::new(map), display: Display::new(HEADLESS_SIZE), t: 0.0, cost: Duration::ZERO, presented: 0, commands: 0, overlays: Arc::default() }
    }
}

fn moved(x: f32) -> UpdateSpec {
    UpdateSpec { position: Some(Vec3::new(x, 0.5, 0.0)), ..UpdateSpec::default() }
}

impl Game for TestGame {
    fn name(&self) -> &str {
        "testgame"
    }

    fn frame(&mut self, ctx: &mut Ctx, f: &mut Frame) {
        let spin = Instant::now();
        while spin.elapsed() < self.cost {
            std::hint::spin_loop();
        }
        self.t += 1.0 / 60.0;
        self.input.update(self.t, f.input, f.pads);
        if self.input.pressed("open") {
            self.ui.show("menu").unwrap();
        }
        let view = self.display.viewport(self.ui.theme(), 1.0);
        let acts: Vec<String> = self.ui.frame(&self.input, &self.state, &view).events.actions().map(str::to_owned).collect();
        let mut go = false;
        for a in acts {
            match a.as_str() {
                "go" => go = true,
                "close" => self.ui.hide("menu").unwrap(),
                _ => {}
            }
        }
        // The HUD button's shortcut, pressed on the device.
        if go || self.input.pressed("go") {
            // A command the player's input sent: it moves /cmd.
            self.commands += 1;
            ctx.world.update(&EntityKey::Path("/cmd".into()), moved(self.commands as f32), "testgame").unwrap();
        }
        if !f.synthetic {
            // Presentation: interpolating something that moves.
            self.presented += 1;
            ctx.world.update(&EntityKey::Path("/mover".into()), moved(self.presented as f32 * 0.01), "testgame").unwrap();
        }
    }

    fn display(&mut self, display: &Display) {
        self.display = *display;
    }

    fn overlay(&mut self, _ctx: &mut Ctx, _overlay: &mut crate::game::Overlay) {
        self.overlays.lock().unwrap().push(self.display);
    }

    fn ui(&mut self, _pick: UiPick) -> Option<GameUi<'_>> {
        Some(GameUi { ui: &mut self.ui, state: &self.state, user_scale: 1.0, input_map: Some(self.input.map()) })
    }
}

fn engine(tag: &str) -> Option<Engine> {
    if !specula_gpu::is_available() {
        crate::skip_test(&format!("ui_engine::engine_tests::{tag}"), "Vulkan is not available");
        return None;
    }
    let gpu = match specula_gpu::Gpu::new(&specula_gpu::GpuOptions { validation: specula_gpu::Validation::Auto, ..Default::default() }) {
        Ok(g) => g,
        Err(e) => {
            crate::skip_test(&format!("ui_engine::engine_tests::{tag}"), &e.to_string());
            return None;
        }
    };
    let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets");
    let captures = std::env::temp_dir().join(format!("specula-ui-engine-{tag}-{}", std::process::id()));
    let mut e = Engine::new(gpu, &captures, crate::models::Library::new(&assets, None)).expect("engine");
    let w = Writer::new("t", Some("t"));
    for name in ["cmd", "mover"] {
        let mut s = SpawnSpec::new(Shape::Cube);
        s.name = Some(name.into());
        e.world.commit(&mut e.swarm, 0, w, journal::Kind::Spawn, crate::write_policy::Rule::World, |wd| wd.spawn(s, "t").map(|x| x.id)).unwrap();
    }
    e.game = Some(Box::new(TestGame::new()));
    e.game_name = "testgame".into();
    e.ui_tell_display();
    Some(e)
}

fn session(id: &str) -> SessionInfo {
    SessionInfo { id: id.into(), client_name: "c".into(), client_version: "1".into(), protocol_version: "x".into() }
}

/// Introduces `name` on session `id` (with every session so far live).
fn hello(e: &mut Engine, id: &str, name: &str, role: &str) -> SessionInfo {
    let s = session(id);
    let live = vec![session("alice1"), session("boss01"), session("malo01"), session("other1"), s.clone()];
    e.swarm.hello(&s, name.into(), role.into(), None, &live).unwrap();
    s
}

/// A `ui_*` call as `who`: its answer and whether it was an error.
fn call(e: &mut Engine, s: &SessionInfo, who: &'static str, tool: &str, args: &str) -> (Json, bool) {
    let c = crate::ui_tools::parse(tool, &Json::parse(args).unwrap()).unwrap().unwrap();
    let w = Writer { name: who, session: Some(&s.id), via: None };
    let tick = e.clock.tick();
    let out = e.ui_call(c, w, tick, s);
    (crate::playtest::answer_json(&out), out.is_error)
}

fn display_of(e: &mut Engine, s: &SessionInfo, who: &'static str) -> Json {
    let (t, err) = call(e, s, who, "ui_tree", r#"{"limit": 1, "fresh": true}"#);
    assert!(!err, "{t}");
    t.get("display").cloned().unwrap()
}

fn mover_x(e: &Engine, path: &str) -> f32 {
    e.world.get(e.world.find_path(path).unwrap()).map(|ent| ent.position.x).unwrap()
}

/// Finding 1, headless: each session's display is its own, a failed call keeps the one it had,
/// and it goes when the session closes.
#[test]
fn headless_displays_are_per_session() {
    let Some(mut e) = engine("per_session") else { return };
    let a = hello(&mut e, "alice1", "alice", "builder");
    let m = hello(&mut e, "malo01", "mallory", "builder");
    let (r, err) = call(&mut e, &m, "mallory", "ui_act", r#"{"display": {"size": [1024, 768], "safe_area": [100, 100, 100, 100], "os_scale": 2}, "steps": []}"#);
    assert!(!err, "{r}");
    assert_eq!(r.get("display").and_then(|d| d.get("set_by")).and_then(Json::as_str), Some("mallory"), "{r}");
    let mine = display_of(&mut e, &m, "mallory");
    assert_eq!(mine.get("size").unwrap().to_string(), "[1024,768]");
    let hers = display_of(&mut e, &a, "alice");
    assert_eq!(hers.get("size").unwrap().to_string(), "[1280,800]", "alice keeps the default: {hers}");
    assert!(hers.get("set_by").is_none() && hers.get("os_scale").is_some_and(Json::is_null), "{hers}");
    // A call that fails changes no display.
    let (r, err) = call(&mut e, &m, "mallory", "ui_act", r#"{"display": {"size": [800, 600]}, "steps": [{"focus": "nowhere"}]}"#);
    assert!(err, "{r}");
    assert_eq!(display_of(&mut e, &m, "mallory").get("size").unwrap().to_string(), "[1024,768]", "the failed call's display was put back");
    // Mallory's session closes: her display goes with it.
    e.ui_forget_sessions(&BTreeSet::from(["alice1".to_owned()]));
    assert!(e.ui_state.sessions.is_empty());
    assert_eq!(display_of(&mut e, &m, "mallory").get("size").unwrap().to_string(), "[1280,800]");
}

/// Finding 1, windowed: only the owner overrides the window's display; nobody else's call
/// changes what the human sees. The owner's override goes when the owner's session closes.
#[test]
fn only_the_owner_overrides_a_windows_display() {
    let Some(mut e) = engine("windowed") else { return };
    e.swarm.grants.grant("boss", Authority::Owner).unwrap();
    let b = hello(&mut e, "boss01", "boss", ROLE_OWNER);
    let m = hello(&mut e, "malo01", "mallory", "builder");
    e.windowed = true;
    e.ui_state.window = Some(([1280, 720], Some(1.75), Instant::now()));
    e.ui_tell_display();
    let before = e.ui_display_for(None).0;
    let (r, err) = call(&mut e, &m, "mallory", "ui_act", r#"{"display": {"safe_area": [288, 512, 288, 512], "os_scale": 0.5}, "steps": []}"#);
    assert!(err && r.get("error").and_then(Json::as_str) == Some("owner_only"), "{r}");
    assert_eq!(e.ui_display_for(None).0, before, "the window's display is unchanged");
    assert_eq!(e.ui_state.told, Some(before), "and so is what the game was told");
    // The owner may (not the size: the window's).
    let (r, err) = call(&mut e, &b, "boss", "ui_act", r#"{"display": {"size": [800, 600]}, "steps": []}"#);
    assert!(err && r.get("error").and_then(Json::as_str) == Some("window_size"), "{r}");
    let (r, err) = call(&mut e, &b, "boss", "ui_act", r#"{"display": {"safe_area": [20, 0, 20, 0]}, "steps": []}"#);
    assert!(!err, "{r}");
    assert_eq!(e.ui_display_for(None).0.safe_area, [20.0, 0.0, 20.0, 0.0]);
    assert_eq!(e.ui_display_for(None).1, Some("boss"), "attributed");
    e.ui_forget_sessions(&BTreeSet::from(["malo01".to_owned()]));
    assert_eq!(e.ui_display_for(None).0, before, "gone with the owner's session");
    assert_eq!(e.ui_state.told, Some(before), "and the game was told");
}

/// Finding 4: the game's own interpolation isn't charged to the caller (a claim on a moving
/// entity doesn't refuse a wait), but a command the caller's input causes is checked (refused,
/// nothing changes).
#[test]
fn claims_check_what_the_callers_input_causes_and_only_that() {
    let Some(mut e) = engine("claims") else { return };
    let a = hello(&mut e, "alice1", "alice", "builder");
    let m = hello(&mut e, "malo01", "mallory", "builder");
    let region = Scope::region(Vec3::new(-50.0, -5.0, -50.0), Vec3::new(50.0, 5.0, 50.0)).unwrap();
    e.swarm.take_claim(&e.world, &a, ClaimSpec { scope: region, lease: DEFAULT_LEASE, note: String::new() }).unwrap();
    let (mover, cmd) = (mover_x(&e, "/mover"), mover_x(&e, "/cmd"));
    for steps in [r#"[{"wait": 5}]"#, r#"[{"press": "open"}, {"press": "back"}]"#] {
        let (r, err) = call(&mut e, &m, "mallory", "ui_act", &format!(r#"{{"steps": {steps}}}"#));
        assert!(!err, "{steps}: {r}");
    }
    assert_eq!(mover_x(&e, "/mover"), mover, "synthetic frames don't present");
    // Pressing Go (its shortcut on a gamepad) moves /cmd: alice's claim refuses it.
    let (r, err) = call(&mut e, &m, "mallory", "ui_act", r#"{"steps": [{"activate": "go"}]}"#);
    assert!(err, "{r}");
    assert_eq!(r.get("error").and_then(Json::as_str), Some("claimed"), "{r}");
    assert_eq!(mover_x(&e, "/cmd"), cmd, "refused: nothing changed");
    // Alice's own input may.
    let (r, err) = call(&mut e, &a, "alice", "ui_act", r#"{"steps": [{"activate": "go"}]}"#);
    assert!(!err, "{r}");
    assert_ne!(mover_x(&e, "/cmd"), cmd);
    let last = e.swarm.journal().entries().last().unwrap();
    assert_eq!((&*last.by, last.via.as_deref()), ("testgame", Some("alice")), "journaled as alice's input to the game");
    assert!(!e.world.has_unjournaled());
}

/// Finding 5: frames are budgeted by time. A call that would run past it stops between units
/// and says what is left; sending `rest` back finishes the work. The 160-frame cap stops the
/// same way.
#[test]
fn ui_act_stops_at_its_time_budget_and_hands_back_the_rest() {
    let Some(mut e) = engine("budget") else { return };
    let m = hello(&mut e, "malo01", "mallory", "builder");
    // 0.5 ms a frame: 160 frames would be 80 ms.
    set_cost(&mut e, Duration::from_micros(500));
    let started = Instant::now();
    let (r, err) = call(&mut e, &m, "mallory", "ui_act", r#"{"steps": [{"wait": 120}, {"press": "open"}, {"wait": 40}], "settle": 0}"#);
    let took = started.elapsed();
    assert!(!err, "{r}");
    assert_eq!(r.get("stopped").and_then(Json::as_str), Some("time_budget"), "{r}");
    assert!(took < Duration::from_millis(30), "{took:?}");
    let left = r.get("frames_left").and_then(Json::as_u64).unwrap();
    let frames = r.get("frames").and_then(Json::as_u64).unwrap();
    assert!(left > 0 && frames + left == 121, "the first settle frame and the wait's: {frames} + {left}: {r}");
    let rest: Vec<Json> = r.get("rest").and_then(Json::as_array).unwrap().to_vec();
    assert_eq!(Json::Arr(rest.clone()).to_string(), format!(r#"[{{"wait":{left}}},{{"press":"open","device":"gamepad"}},{{"wait":40}}]"#));
    // Send the rest back until done.
    let mut rest = rest;
    for _ in 0..10 {
        let (r, err) = call(&mut e, &m, "mallory", "ui_act", &format!(r#"{{"steps": {}, "settle": 0}}"#, Json::Arr(rest.clone())));
        assert!(!err, "{r}");
        match r.get("rest").and_then(Json::as_array) {
            Some(more) => rest = more.to_vec(),
            None => {
                assert_eq!(r.get("screens").and_then(Json::as_array).and_then(|s| s.first()).and_then(Json::as_str), Some("menu"), "the press ran: {r}");
                set_cost(&mut e, Duration::ZERO);
                // Fast frames: the cap stops it.
                let (r, _) = call(&mut e, &m, "mallory", "ui_act", r#"{"steps": [{"wait": 120}, {"wait": 120}], "settle": 0}"#);
                assert_eq!(r.get("stopped").and_then(Json::as_str), Some("frame_cap"), "{r}");
                assert_eq!(r.get("frames").and_then(Json::as_u64), Some(crate::ui_tools::MAX_FRAMES as u64));
                return;
            }
        }
    }
    panic!("the rest never finished");
}

fn set_cost(e: &mut Engine, cost: Duration) {
    let g = e.game.take().unwrap();
    // The engine holds the game as `dyn Game`: rebuild it with the cost (its state is fresh).
    drop(g);
    let mut t = TestGame::new();
    t.cost = cost;
    t.display = e.ui_display_for(None).0;
    e.game = Some(Box::new(t));
}

/// Findings 3 and 8: the engine screens' rules. While the review screen holds the window, a
/// game ui_act is refused, the game's outlines in a capture are all blocked with no focus, and
/// only the owner may drive the review screen itself.
#[test]
fn engine_screens_block_the_game_and_are_the_owners() {
    let Some(mut e) = engine("modal") else { return };
    e.swarm.grants.grant("boss", Authority::Owner).unwrap();
    let b = hello(&mut e, "boss01", "boss", ROLE_OWNER);
    let m = hello(&mut e, "malo01", "mallory", "builder");
    // The menu open with its focus on Bravo.
    let (r, err) = call(&mut e, &m, "mallory", "ui_act", r#"{"steps": [{"press": "open"}, {"focus": "b"}]}"#);
    assert!(!err && r.get("focused").and_then(Json::as_str) == Some("b"), "{r}");
    e.review_ui.open(None, &e.swarm.review);
    e.review_frame((1280, 800));
    let (r, err) = call(&mut e, &m, "mallory", "ui_act", r#"{"surface": "game", "steps": [{"press": "back"}]}"#);
    assert!(err && r.get("error").and_then(Json::as_str) == Some("engine_screen_open"), "{r}");
    let (r, err) = call(&mut e, &m, "mallory", "ui_act", r#"{"surface": "review", "device": "keyboard", "steps": [{"press": "Key.F9"}]}"#);
    assert!(err && r.get("error").and_then(Json::as_str) == Some("owner_only"), "{r}");
    let mut p = Painter::new(Mat4::IDENTITY, 1280, 800);
    let j = e.paint_ui_outlines(&mut p, 64, true);
    let outlines = j.get("outlines").and_then(Json::as_array).unwrap();
    let game: Vec<&Json> = outlines.iter().filter(|o| o.get("surface").and_then(Json::as_str) == Some("game")).collect();
    assert!(!game.is_empty(), "{j}");
    assert!(game.iter().all(|o| o.get("blocked") == Some(&Json::Bool(true)) && o.get("focused").is_none()), "{j}");
    // The owner closes it from the review screen's own keys.
    let (r, err) = call(&mut e, &b, "boss", "ui_act", r#"{"surface": "review", "device": "keyboard", "steps": [{"press": "Key.F9"}]}"#);
    assert!(!err, "{r}");
    assert!(!e.review_ui.is_open());
}

/// The answer's `rest` is ui_act's own step format.
#[test]
fn steps_round_trip_through_their_json() {
    let args = r#"{"steps": [{"press": "pause", "device": "keyboard", "hold": 3}, {"focus": "x"}, {"activate": "y", "device": "touch"}, {"click": [10, 20.5]}, {"click": "z", "device": "touch"}, {"type": "hi"}, {"scroll": -3, "at": "list"}, {"scroll": 2, "device": "gamepad"}, {"wait": 7}]}"#;
    let Ok(UiCall::Act(a)) = crate::ui_tools::parse("ui_act", &Json::parse(args).unwrap()).unwrap() else { panic!() };
    let again = Json::obj([("steps", Json::arr(a.steps.iter().map(Step::to_json)))]);
    let Ok(UiCall::Act(b)) = crate::ui_tools::parse("ui_act", &again).unwrap() else { panic!("{again}") };
    assert_eq!(a.steps, b.steps);
}

/// Review R2 of m5-uitools, finding 2 (probe p6 G): headless, a session's display reaches only its
/// own `ui_*` calls and `ui_frame` captures. Another agent's `sight_frame {include_ui}` draws the
/// interface for the default display, after mallory's `ui_act` set a safe area and an OS scale,
/// and even if the game was left on her display.
#[test]
fn a_sessions_display_never_reaches_other_agents_captures() {
    let Some(mut e) = engine("leak") else { return };
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut t = TestGame::new();
    t.overlays = Arc::clone(&seen);
    t.display = e.ui_display_for(None).0;
    e.game = Some(Box::new(t));
    let m = hello(&mut e, "malo01", "mallory", "builder");
    let default = e.ui_display_for(None).0;
    let (r, err) = call(&mut e, &m, "mallory", "ui_act", r#"{"display": {"safe_area": [100, 120, 100, 120], "os_scale": 2}, "steps": [{"wait": 1}]}"#);
    assert!(!err, "{r}");
    let hers = e.ui_display_for(Some(&m.id)).0;
    assert_ne!(hers, default);
    assert_eq!(e.ui_state.told, Some(default), "the default display is told again after her frames");
    // Another agent's sight_frame {include_ui}: the capture path with no display of its own.
    let cam = Camera::new(Vec3::new(0.0, 2.0, 8.0), Vec3::new(0.0, 0.0, 0.0), 60.0);
    let plain = ov::Spec::default();
    e.render_view(cam, 640, 400, &with_ui(&plain)).expect("render");
    assert_eq!(seen.lock().unwrap().last(), Some(&default), "drawn for the default display");
    // Even with the game left on her display (any path that tells it), a plain capture draws the
    // default, and the game gets hers back after.
    e.ui_tell(hers);
    e.render_view(cam, 640, 400, &with_ui(&plain)).expect("render");
    assert_eq!(seen.lock().unwrap().last(), Some(&default), "drawn for the default display");
    assert_eq!(e.ui_state.told, Some(hers), "what the game was told is put back after the capture");
    // Her own ui_frame draws hers.
    let own = ov::Spec { ui_display: Some(Box::new(hers)), ..ov::Spec::default() };
    e.render_view(cam, 640, 400, &with_ui(&own)).expect("render");
    assert_eq!(seen.lock().unwrap().last(), Some(&hers), "ui_frame draws the caller's display");
}

/// A capture's options with the interface drawn (`include_ui`), overlays `o`.
fn with_ui(o: &ov::Spec) -> crate::capture::ViewOpts<'_> {
    crate::capture::ViewOpts { include_ui: true, lod: None, time_s: 0.0, overlays: o, caption: None, ids: false, hud: true, fill: None }
}
