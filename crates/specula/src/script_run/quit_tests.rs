//! A run against a game played by a stand-in engine (a thread of its own answering the run's
//! requests as the engine does): what a quit does to the script, `--at`, `--follow` and a
//! search, and what `run.ticks` counts while the game holds its clock.

use super::*;
use crate::ui_tools::{Step, UiCall};

/// The module's actions: a button and an axis of two.
const MAP: &str = r#"{"format": "specula.input.v1", "actions": [{"name": "jump", "kind": "button", "bindings": ["Key.Space"]}, {"name": "move", "kind": "axis2", "output": "value", "bindings": [{"composite": {"up": "Key.W", "down": "Key.S", "left": "Key.A", "right": "Key.D"}}]}]}"#;

/// The stand-in game: its clock runs from the start, is held from tick `hold_at` (a run that
/// ended, a pause), its rules ask to quit on tick `quit_at`, and a press of `accept` is its
/// Quit button. A quit is told with the ticks it ends, as the engine tells it.
struct Fake {
    hold_at: Option<u64>,
    quit_at: Option<u64>,
}

struct Played {
    answer: Result<Json, Json>,
    /// Every interface press, with the game's tick it came at.
    presses: Vec<(u64, String)>,
    /// Ticks asked for after the script was told of the quit.
    plays_after_quit: u64,
    /// What the module heard at each tick it ran: its `press` events, in order.
    heard: Vec<(u64, Vec<String>)>,
}

fn play_against(game: Fake, spec: &Spec, steps: &str) -> Played {
    let (tx, rx) = std::sync::mpsc::channel::<crate::tools::Request>();
    let engine = std::thread::spawn(move || {
        let (mut tick, mut running, mut quit, mut told) = (0u64, true, false, false);
        let (mut presses, mut plays_after_quit, mut heard) = (Vec::new(), 0u64, Vec::new());
        let mut module = crate::logic::input::ModuleInput::new(MAP).unwrap();
        for request in rx {
            let answer = match &request.command {
                Command::Time(TimeOp::Play(n)) => {
                    if told {
                        plays_after_quit += *n;
                    }
                    let mut played = 0u64;
                    while played < *n && running && !quit {
                        tick += 1;
                        heard.push((tick, module.take_tick_and_presses().1));
                        played += 1;
                        running &= game.hold_at != Some(tick);
                        quit |= game.quit_at == Some(tick);
                    }
                    let mut j = Json::obj([("played", Json::from(played)), ("tick", Json::from(tick)), ("game_clock", Json::from(if running { "running" } else { "held" }))]);
                    if std::mem::take(&mut quit) {
                        told = true;
                        j.set("quit", Json::Bool(true));
                    }
                    j
                }
                Command::Logic(crate::logic::tools::LogicCall::Input { actions, ticks, .. }) => match module.hold(actions, *ticks, None) {
                    Ok(holds) => holds,
                    Err(why) => {
                        request.reply.send(ToolOutput::error(refusal("bad_input", why).to_string()));
                        continue;
                    }
                },
                Command::Ui(UiCall::Act(args)) => {
                    for step in &args.steps {
                        if let Step::Press { name, .. } = step {
                            presses.push((tick, name.clone()));
                            quit |= name == "accept";
                        }
                    }
                    Json::obj::<&str>([])
                }
                _ => Json::obj([("widgets", Json::Arr(Vec::new())), ("screens", Json::Arr(Vec::new()))]),
            };
            request.reply.send(ToolOutput::text(answer.to_string()));
        }
        (presses, plays_after_quit, heard)
    });
    let tools = Tools::new(tx, Vec::new());
    let file = GameFile::parse(r#"{"format": "specula.game.v1", "report": {"tick": "run.ticks"}}"#).unwrap();
    let mut run = Run { tools: &tools, session: session(), modules: vec![("m".to_owned(), vec!["jump".to_owned(), "move".to_owned()])], held: Vec::new(), hold_left: 0, at: 0, spans: Vec::new(), words: Vec::new(), lines: lines_of(&file, &spec.keys, &spec.also), quit_at: None };
    let answer = played(&mut run, &file, spec, &mut std::io::Cursor::new(steps.as_bytes().to_vec()));
    drop(run);
    drop(tools);
    let (presses, plays_after_quit, heard) = engine.join().unwrap();
    Played { answer, presses, plays_after_quit, heard }
}

fn spec_of(ticks: u64, inputs: &str, at: &[u64]) -> Spec {
    Spec { ticks, seed: None, inputs: parse_inputs(inputs).unwrap(), frame: None, size: (960, 540), follow: false, at: at.to_vec(), keys: Vec::new(), also: Vec::new(), set: Vec::new(), find: None, out: PathBuf::from("a") }
}

fn report(p: &Played) -> String {
    p.answer.as_ref().unwrap().get("report").map(Json::to_string).unwrap_or_default()
}

/// A quit ends the run on the tick it was asked: whatever the clock does on that tick (a run
/// that ended or paused there), no later input is given and no tick plays, `run.ticks` and
/// `run.quit` are that tick, and no state of `--at` after it is read (a reviewer, 2026-11-24:
/// a quit on the tick the game won was seen at the next input, 45 ticks on).
#[test]
fn a_quit_ends_the_run_on_its_tick_whatever_the_clock_does() {
    for hold_at in [None, Some(5)] {
        let p = play_against(Fake { hold_at, quit_at: Some(5) }, &spec_of(100, "1:down,2:down,50:up", &[]), "");
        assert_eq!(report(&p), r#"{"tick":5,"run.quit":5}"#, "held at {hold_at:?}");
        assert_eq!(p.presses, [(1, "down".to_owned()), (2, "down".to_owned())], "the up after the quit is not given");
        assert_eq!(p.plays_after_quit, 0);
        let p = play_against(Fake { hold_at, quit_at: Some(5) }, &spec_of(100, "1:down", &[3, 10, 60]), "");
        assert_eq!(p.answer.as_ref().unwrap().get("at").map(Json::to_string).as_deref(), Some(r#"[{"tick":3}]"#), "no state on the way after the quit");
        assert_eq!(report(&p), r#"{"tick":5,"run.quit":5}"#);
    }
    // Quit pressed at tick 5 (the game's own button): no word after it, at that tick or later.
    let p = play_against(Fake { hold_at: None, quit_at: None }, &spec_of(30, "5:enter,5:down,7:up", &[]), "");
    assert_eq!(report(&p), r#"{"tick":5,"run.quit":5}"#);
    assert_eq!(p.presses, [(5, "accept".to_owned())]);
    // The last tick of `--at` at the run's last is not the report when the game quit first.
    let p = play_against(Fake { hold_at: None, quit_at: Some(5) }, &spec_of(30, "", &[30]), "");
    assert_eq!(p.answer.as_ref().unwrap().get("at_is_all"), None);
    assert_eq!(report(&p), r#"{"tick":5,"run.quit":5}"#);
    // A key a check or `--until` reads by itself (`run.quit`), and `run.ticks`, are the tick.
    let mut keyed = spec_of(30, "", &[]);
    keyed.also = vec![QUIT_KEY.to_owned()];
    let p = play_against(Fake { hold_at: None, quit_at: Some(7) }, &keyed, "");
    let state = p.answer.as_ref().unwrap().get("report").cloned().unwrap();
    assert!(Until::parse("run.quit==7&&tick==7").unwrap().holds(&state), "{state}");
    // A followed run of a game that quit in its script reads no step.
    let mut follow = spec_of(30, "", &[]);
    follow.follow = true;
    let p = play_against(Fake { hold_at: None, quit_at: Some(5) }, &follow, "10 down\n10 up\n");
    assert_eq!(p.answer.as_ref().unwrap().get("followed").and_then(Json::as_u64), Some(0));
    assert!(p.presses.is_empty() && p.plays_after_quit == 0);
    // A game that quits on a step of a followed run: that state is the last.
    follow.ticks = 0;
    let p = play_against(Fake { hold_at: None, quit_at: Some(15) }, &follow, "10 down\n10 up\n10 left\n");
    assert_eq!(report(&p), r#"{"tick":15,"run.quit":15}"#);
    assert_eq!(p.answer.as_ref().unwrap().get("followed").and_then(Json::as_u64), Some(2), "the third step is not read");
    assert_eq!(p.plays_after_quit, 0);
    // A search whose own script quit is refused, with the state it quit in.
    let mut search = spec_of(100, "1:enter", &[]);
    search.find = Some(Find::from_json(&Json::parse(r#"{"until": "tick>=50", "words": ["up"]}"#).unwrap()).unwrap());
    let p = play_against(Fake { hold_at: None, quit_at: None }, &search, "");
    assert_eq!(p.answer.as_ref().unwrap_err().get("error").and_then(Json::as_str), Some("quit"));
}

/// The script's count of ticks goes on whatever screen is up: while the game holds its clock
/// (a pause, a run that ended) and has not quit, `run.ticks` is the script's ticks, as an
/// input's tick is counted, and the game's own tick stands where it stopped.
#[test]
fn a_held_clock_without_a_quit_is_the_scripts_ticks_passing() {
    let p = play_against(Fake { hold_at: Some(5), quit_at: None }, &spec_of(100, "1:down,50:up", &[10, 60]), "");
    let answer = p.answer.as_ref().unwrap();
    assert_eq!(report(&p), r#"{"tick":100}"#);
    assert_eq!(answer.get("at").map(Json::to_string).as_deref(), Some(r#"[{"tick":10},{"tick":60}]"#));
    assert_eq!(p.presses, [(1, "down".to_owned()), (5, "up".to_owned())], "the up is given at the script's tick 50, the game's clock standing at its tick 5");
}

/// A word that changes the value of an action already held is one press, followed or not: the
/// word is looked at without letting go of what it names, so a followed run (and a search,
/// which looks at its words the same way) hears what `specula run` of the same script hears
/// (a reviewer, 2026-11-24: followed, `move=0/1` over a held `move=1/0` was two presses).
#[test]
fn a_followed_run_hears_the_presses_its_script_would() {
    let plain = play_against(Fake { hold_at: None, quit_at: None }, &spec_of(3, "0:move=1/0,1:move=0/1,2:jump", &[]), "");
    let mut follow = spec_of(0, "", &[]);
    follow.follow = true;
    let followed = play_against(Fake { hold_at: None, quit_at: None }, &follow, "1 move=1/0\n1 move=0/1\n1 jump\n");
    let expected = vec![(1, vec!["move".to_owned()]), (2, vec!["move".to_owned()]), (3, vec!["jump".to_owned()])];
    assert_eq!(plain.heard, expected);
    assert_eq!(followed.heard, expected, "one press a word, followed as run");
    // A value of another kind for a held action is refused, and changes nothing held.
    let bad = play_against(Fake { hold_at: None, quit_at: None }, &follow, "1 move=1/0\n1 move\n1\n");
    assert_eq!(bad.heard, vec![(1, vec!["move".to_owned()]), (2, Vec::new())], "the refused word presses nothing");
}
