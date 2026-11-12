//! Sepulcra's languages (docs/design/LOCALISATION.md): the string tables in
//! `games/sepulcra/assets/strings/` (`en.json` is the source, built in too), the player's
//! language, and the words the game's code shows (toasts, the HUD's state, confirmations,
//! subtitles), by key.
//!
//! Screens show table entries with `"@key"`; the code puts messages in the HUD's state with
//! [`put`] (a [`Said`]: a key and its arguments, nested messages included), so the HUD lays out
//! in the player's language, `ui_check` can check every language, and the pseudo-locale reaches
//! every word. The game's log for agents (`play_state`) stays in the source language.
//!
//! A locale whose table names system faces (CJK) reads them on a thread of its own; until they
//! come, its text shows with the bundled faces.

use specula_lingua::{Arg, Catalog, Locale, Table};
use specula_ui::{StateMap, Strings};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};

/// The English source table, built in (the assets folder's copy wins when it is there).
pub const EN: &str = include_str!("../../sepulcra/assets/strings/en.json");

/// Most string tables read from the assets folder.
const MAX_TABLES: usize = 32;

/// A message the game shows: a table key and its arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct Said {
    pub key: String,
    pub args: Vec<(&'static str, SaidArg)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SaidArg {
    Int(i64),
    Num(f64),
    /// Words shown as they are (a cell's id, an error from the engine).
    Text(String),
    /// Another entry, shown in the same language.
    Said(Box<Said>),
}

impl Said {
    pub fn new(key: impl Into<String>) -> Said {
        Said { key: key.into(), args: Vec::new() }
    }

    pub fn int(mut self, name: &'static str, v: impl Into<i64>) -> Said {
        self.args.push((name, SaidArg::Int(v.into())));
        self
    }

    pub fn num(mut self, name: &'static str, v: f64) -> Said {
        self.args.push((name, SaidArg::Num(v)));
        self
    }

    pub fn text(mut self, name: &'static str, v: impl Into<String>) -> Said {
        self.args.push((name, SaidArg::Text(v.into())));
        self
    }

    /// Argument `name` is entry `key` (no arguments of its own).
    pub fn key(self, name: &'static str, key: impl Into<String>) -> Said {
        self.said(name, Said::new(key))
    }

    pub fn said(mut self, name: &'static str, s: Said) -> Said {
        self.args.push((name, SaidArg::Said(Box::new(s))));
        self
    }
}

/// Puts `said` in the HUD's state as message `key` (its storage reused frame to frame).
pub fn put(s: &mut StateMap, key: &str, said: &Said) {
    let args = s.set_msg(key, &said.key);
    for (name, a) in &said.args {
        match a {
            SaidArg::Int(n) => {
                args.set_int(name, *n);
            }
            SaidArg::Num(x) => {
                args.set_num(name, *x);
            }
            SaidArg::Text(t) => {
                args.set_str(name, t);
            }
            SaidArg::Said(inner) => put(args, name, inner),
        }
    }
}

/// Strings made on a thread for a locale (with its faces read): `(locale, strings, problems)`.
type Made = (Locale, Arc<Strings>, Vec<(String, String)>);

/// The game's languages.
pub struct L10n {
    dir: Option<PathBuf>,
    tables: Vec<Arc<Table>>,
    locale: Locale,
    active: Arc<Strings>,
    /// The source language's words (the agents' log).
    source: Catalog,
    /// Each table's strings, made when the tables were read (at start, or on the reload's
    /// thread) with the pseudo-locale made too: choosing a language takes a reference, and the
    /// engine thread never builds a catalog (review R2 of m5-l10n, major 3).
    built: Vec<(Locale, Arc<Strings>)>,
    /// Strings a thread is making: a language's faces, or a locale with no table of its own.
    faces: Option<mpsc::Receiver<Made>>,
    /// Tables read again on a thread ([`L10n::reload`]): installed by [`L10n::poll`].
    reloading: Option<mpsc::Receiver<L10n>>,
    /// What didn't load (tables that don't parse, a locale with no table).
    pub problems: Vec<String>,
}

fn read_tables(dir: Option<&Path>, problems: &mut Vec<String>) -> Vec<Arc<Table>> {
    let mut tables: Vec<Arc<Table>> = Vec::new();
    if let Some(entries) = dir.and_then(|d| std::fs::read_dir(d).ok()) {
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
        paths.sort();
        for p in paths.into_iter().take(MAX_TABLES) {
            let r = std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| Table::parse(&t).map_err(|e| e.to_string()));
            match r {
                Ok(t) if tables.iter().any(|x| x.locale == t.locale) => problems.push(format!("strings/{}: a second table for {}", p.file_name().map(|f| f.to_string_lossy()).unwrap_or_default(), t.locale)),
                Ok(t) => tables.push(Arc::new(t)),
                Err(e) => problems.push(format!("strings/{}: {e}", p.file_name().map(|f| f.to_string_lossy()).unwrap_or_default())),
            }
        }
    }
    if !tables.iter().any(|t| t.source) {
        match Table::parse(EN) {
            Ok(t) => {
                tables.retain(|x| x.locale != t.locale);
                tables.push(Arc::new(t));
            }
            Err(e) => problems.push(format!("the built-in English table: {e}")),
        }
    }
    tables
}

impl L10n {
    /// The tables in `dir` (else the built-in English one), showing `wanted` (else English).
    pub fn load(dir: Option<&Path>, wanted: Option<&str>) -> L10n {
        let mut problems = Vec::new();
        let tables = read_tables(dir, &mut problems);
        let en = Locale::parse("en").unwrap_or_else(|_| unreachable!("a valid id"));
        let source_locale = tables.iter().find(|t| t.source).map_or(en, |t| t.locale.clone());
        let refs: Vec<&Table> = tables.iter().map(|t| &**t).collect();
        let source = Catalog::build(&source_locale, &refs).unwrap_or_else(|_| Catalog::pseudo(&Table::parse(EN).unwrap_or_else(|_| unreachable!("tested"))));
        let src_table = tables.iter().find(|t| t.source).cloned();
        let mut built: Vec<(Locale, Arc<Strings>)> = Vec::new();
        for t in &tables {
            match Catalog::build(&t.locale, &refs) {
                Ok(c) => {
                    let s = Strings::new(c, src_table.clone(), Vec::new());
                    s.warm_pseudo();
                    built.push((t.locale.clone(), Arc::new(s)));
                }
                Err(e) => problems.push(format!("{}: {e}", t.locale)),
            }
        }
        let active = match built.iter().find(|(l, _)| *l == source_locale) {
            Some((_, s)) => Arc::clone(s),
            None => {
                let s = Strings::new(source.clone(), src_table, Vec::new());
                s.warm_pseudo();
                Arc::new(s)
            }
        };
        let mut l = L10n { dir: dir.map(Path::to_path_buf), tables, locale: source_locale, active, source, built, faces: None, reloading: None, problems };
        if let Some(w) = wanted {
            if let Err(e) = Locale::parse(w).map_err(|e| e.to_string()).and_then(|loc| l.set_locale(&loc)) {
                l.problems.push(format!("--locale {w}: {e}"));
            }
        }
        l
    }

    /// Reads the tables again (one changed on disk or through the project tools) on a thread,
    /// keeping the player's language when it still has a table: the engine thread never reads
    /// them ([`L10n::poll`] installs the result).
    pub fn reload(&mut self) {
        let (tx, rx) = mpsc::channel();
        let dir = self.dir.clone();
        let wanted = self.locale.clone();
        let spawned = std::thread::Builder::new().name("sepulcra-strings".into()).spawn(move || {
            let mut fresh = L10n::load(dir.as_deref(), None);
            if fresh.set_locale(&wanted).is_err() {
                fresh.problems.push(format!("{wanted} has no table any more: showing {}", fresh.locale));
            }
            let _ = tx.send(fresh);
        });
        match spawned {
            Ok(_) => self.reloading = Some(rx),
            Err(e) => self.problems.push(format!("strings: no thread to read them again ({e})")),
        }
    }

    /// Waits (at most `timeout`) for a reload in progress and installs it; for tests and tools
    /// that need the new words now.
    pub fn finish_reload(&mut self, timeout: std::time::Duration) -> Option<Arc<Strings>> {
        let fresh = self.reloading.as_ref()?.recv_timeout(timeout).ok()?;
        Some(self.install(fresh))
    }

    /// Takes tables read again. The player may have chosen another language while they were
    /// read (review of m5-l10n, finding 4): theirs wins.
    fn install(&mut self, mut fresh: L10n) -> Arc<Strings> {
        if fresh.locale != self.locale && fresh.set_locale(&self.locale).is_err() {
            fresh.problems.push(format!("{} has no table any more: showing {}", self.locale, fresh.locale));
        }
        *self = fresh;
        Arc::clone(&self.active)
    }

    /// Shows `l` from now on: a reference to strings already made. Nothing is built or read
    /// here (the Language button runs on the engine thread).
    /// - A language whose table names system faces shows at once without them; a thread reads
    ///   them and makes the strings again ([`L10n::poll`] installs them, and keeps them).
    /// - A locale with no table of its own (a region of a language that has one) shows its
    ///   language's table at once; a thread makes its own strings (its number format).
    /// - A locale with no table at all, nor its language's, is refused.
    pub fn set_locale(&mut self, l: &Locale) -> Result<(), String> {
        let exact = self.built.iter().find(|(b, _)| b == l).map(|(_, s)| Arc::clone(s));
        // Without a table of its own, the nearest table its fallback chain reaches (the source
        // only when it is the same language: a language nobody translated is refused, and
        // Traditional Chinese never shows a Simplified table).
        let source = self.tables.iter().find(|t| t.source).map(|t| t.locale.clone());
        let nearest = || {
            let chain = source.as_ref().map_or_else(|| vec![l.clone()], |s| l.chain(s));
            chain.iter().skip(1).filter(|c| Some(*c) != source.as_ref() || c.language() == l.language()).find_map(|c| self.built.iter().find(|(b, _)| b == c).map(|(_, s)| Arc::clone(s)))
        };
        let Some(shown) = exact.clone().or_else(nearest) else {
            let mut have: Vec<&str> = self.built.iter().map(|(b, _)| b.as_str()).collect();
            have.sort_unstable();
            return Err(format!("no string table for {l} (nor its language); there are: {}", have.join(", ")));
        };
        // What a thread still has to make: its faces (none read yet), or its own catalog.
        let wants_faces = !shown.catalog().fonts.is_empty() && shown.fonts_len() == 0;
        self.faces = None;
        if exact.is_none() || wants_faces {
            let (tx, rx) = mpsc::channel();
            let (loc, tables, from) = (l.clone(), self.tables.clone(), exact.clone());
            let spawned = std::thread::Builder::new().name("sepulcra-strings".into()).spawn(move || {
                let refs: Vec<&Table> = tables.iter().map(|t| &**t).collect();
                let catalog = match &from {
                    Some(s) => Ok(s.catalog().clone()),
                    None => Catalog::build(&loc, &refs),
                };
                let Ok(catalog) = catalog else { return };
                let (faces, problems) = Strings::load_fonts(&catalog);
                let s = Strings::new(catalog, tables.iter().find(|t| t.source).cloned(), faces);
                s.warm_pseudo();
                let _ = tx.send((loc, Arc::new(s), problems));
            });
            self.faces = spawned.ok().map(|_| rx);
        }
        self.active = shown;
        self.locale = l.clone();
        Ok(())
    }

    /// Tables read again, or the faces a locale waited for, when they have come: the strings to
    /// install.
    pub fn poll(&mut self) -> Option<Arc<Strings>> {
        if let Some(rx) = &self.reloading {
            match rx.try_recv() {
                Ok(fresh) => return Some(self.install(fresh)),
                Err(mpsc::TryRecvError::Disconnected) => self.reloading = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let rx = self.faces.as_ref()?;
        let (loc, made, problems) = match rx.try_recv() {
            Ok(m) => m,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.faces = None;
                return None;
            }
        };
        self.faces = None;
        self.problems.extend(problems.into_iter().map(|(at, why)| format!("{loc} {at}: {why}")));
        // Kept either way: the next time this language is chosen its faces are already there.
        match self.built.iter().position(|(b, _)| *b == loc) {
            Some(at) => self.built[at].1 = Arc::clone(&made),
            None if self.built.len() < MAX_TABLES * 2 => self.built.push((loc.clone(), Arc::clone(&made))),
            None => {}
        }
        if loc != self.locale {
            return None;
        }
        self.active = made;
        Some(Arc::clone(&self.active))
    }

    pub fn locale(&self) -> &Locale {
        &self.locale
    }

    pub fn strings(&self) -> Arc<Strings> {
        Arc::clone(&self.active)
    }

    /// The locales with a table, the source's first.
    pub fn locales(&self) -> Vec<Locale> {
        let mut v: Vec<&Arc<Table>> = self.tables.iter().collect();
        v.sort_by(|a, b| b.source.cmp(&a.source).then_with(|| a.locale.cmp(&b.locale)));
        v.into_iter().map(|t| t.locale.clone()).collect()
    }

    /// The next language in the settings' cycle.
    pub fn next_locale(&self) -> Locale {
        let all = self.locales();
        let i = all.iter().position(|l| *l == self.locale).map_or(0, |i| (i + 1) % all.len().max(1));
        all.get(i).cloned().unwrap_or_else(|| self.locale.clone())
    }

    /// `said` in the player's language.
    pub fn text(&self, said: &Said) -> String {
        render(self.active.catalog(), said, 0)
    }

    /// `said` in the source language (English): what the game's log for agents says.
    pub fn source_text(&self, said: &Said) -> String {
        render(&self.source, said, 0)
    }
}

fn render(c: &Catalog, said: &Said, depth: u8) -> String {
    let nested: Vec<(&str, String)> = said
        .args
        .iter()
        .filter_map(|(n, a)| match a {
            SaidArg::Said(s) if depth < 4 => Some((*n, render(c, s, depth + 1))),
            SaidArg::Said(s) => Some((*n, s.key.clone())),
            _ => None,
        })
        .collect();
    let args: Vec<(&str, Arg<'_>)> = said
        .args
        .iter()
        .map(|(n, a)| {
            let v = match a {
                SaidArg::Int(i) => Arg::Int(*i),
                SaidArg::Num(x) => Arg::Num(*x),
                SaidArg::Text(t) => Arg::Str(t),
                SaidArg::Said(_) => Arg::Str(nested.iter().find(|(m, _)| m == n).map_or("", |(_, s)| s.as_str())),
            };
            (*n, v)
        })
        .collect();
    let mut out = String::new();
    c.format(&said.key, &args, &mut out);
    out
}

/// The table keys the game's code shows (its screens' `@key` texts are read from the screens),
/// for `l10n_check`'s unused and undefined checks: the fixed ones, and one per stone, enemy, prop
/// and input action of the content.
pub fn code_keys(content: &sepulcra::Content, actions: &[String]) -> Vec<String> {
    let mut keys: Vec<String> = CODE_KEYS.iter().map(|k| (*k).to_owned()).collect();
    keys.extend(content.stones.iter().map(|s| format!("stone.{}.name", s.id)));
    keys.extend(content.enemies.iter().map(|e| format!("enemy.{}.name", e.id)));
    keys.extend(content.map.props.iter().map(|p| format!("prop.{}.what", p.name)));
    keys.extend(actions.iter().map(|a| format!("action.{a}")));
    keys
}

/// The fixed keys the code uses.
pub const CODE_KEYS: &[&str] = &[
    "language.name",
    // The shell: settings values, rebinding, captions (shell.rs).
    "controls.also",
    "scale.chosen",
    "scale.auto",
    "scale.option",
    "scale.option_chosen",
    "scale.option_auto",
    "scale.option_auto_chosen",
    "scale.note.limited_by_window",
    "scale.note.smallest",
    "scale.note.changes",
    "scale.now",
    "scale.now_handheld",
    "text_size.percent",
    "text_size.auto",
    "vision.standard",
    "vision.deuteranopia",
    "vision.protanopia",
    "vision.tritanopia",
    "vision.high_contrast",
    "subtitle_size.small",
    "subtitle_size.medium",
    "subtitle_size.large",
    "subtitle_size.huge",
    "camera_drag.toggle",
    "camera_drag.hold",
    "toast.settings_problem",
    "toast.quit_is_players",
    "toast.no_save_to_continue",
    "toast.pad_lost",
    "rebind.reset",
    "rebind.agents_refused",
    "rebind.only_buttons",
    "rebind.press_pad",
    "rebind.press_kbm",
    "rebind.cant",
    "rebind.cancelled",
    "rebind.bound",
    "rebind.bound_clash",
    "rebind.cant_bind",
    "caption.plain",
    "caption.sided",
    "caption.repeated",
    "caption.side.left",
    "caption.side.right",
    "caption.side.behind",
    "caption.boss_call",
    "caption.colossus_collapse",
    "caption.wave_start",
    "caption.wave_cleared",
    "caption.tombstone_lost",
    "caption.wall_broken",
    "caption.wall_hit",
    "caption.victory",
    "caption.defeat",
    "caption.brazier_whoosh",
    "caption.ui_error",
    "common.on",
    "common.off",
    "common.text",
    "glyphs.auto",
    "glyphs.xbox",
    "glyphs.playstation",
    "glyphs.deck",
    "tier.1",
    "tier.2",
    "tier.3",
    "stone.tier_name",
    "stone.role.single",
    "stone.role.area_slow",
    "stone.role.splash",
    "stone.role.anti_air",
    "stone.detail.bolt",
    "stone.detail.lob",
    "stone.detail.pulse",
    "stone.upgrade",
    "stone.fully_carved",
    "wall.name",
    "wall.role",
    "wall.detail",
    "wall.inspect",
    "threat.north",
    "threat.east",
    "threat.west",
    "threat.south",
    "threat.air",
    "face.top",
    "face.north",
    "face.east",
    "face.south",
    "face.west",
    "hover.cell",
    "hover.stone",
    "hover.wall",
    "hover.rooted",
    "hover.prop",
    "toast.wave_started",
    "toast.wave_started_early",
    "toast.wave_cleared",
    "toast.tombstones_lost",
    "toast.wall_broken",
    "toast.early_bonus",
    "toast.stone_sold",
    "toast.wall_removed",
    "toast.victory",
    "toast.defeat",
    "toast.lex_off",
    "toast.lex_off_replay",
    "toast.lex_off_save",
    "toast.rules_desync",
    "toast.new_game",
    "toast.seed_start",
    "toast.run_going",
    "toast.not_enough_dust",
    "toast.loaded_replay",
    "toast.cutscene_skipped",
    "toast.refused",
    "refuse.what.place",
    "refuse.what.upgrade",
    "refuse.what.sell",
    "refuse.what.wall",
    "refuse.what.remove_wall",
    "refuse.what.call_wave",
    "refuse.why.not_enough_dust",
    "refuse.why.occupied",
    "refuse.why.not_buildable",
    "refuse.why.prop_blocks",
    "refuse.why.a_prop",
    "refuse.why.not_wallable",
    "refuse.why.enemy_in_the_way",
    "refuse.why.max_tier",
    "refuse.why.other",
    "confirm.restart.title",
    "confirm.restart.body",
    "confirm.desync.title",
    "confirm.desync.body",
    "confirm.overwrite.title",
    "confirm.overwrite.body",
    "confirm.load.title",
    "confirm.load.body",
    "confirm.delete.title",
    "confirm.delete.body",
    "saves.this_save",
    "saves.slot_name",
    "saves.autosave_name",
    "saves.empty",
    "saves.damaged",
    "saves.damaged_detail",
    "saves.a_saved_run",
    "saves.label",
    "saves.played",
    "saves.date",
    "saves.saving",
    "saves.loading",
    "month.1",
    "month.2",
    "month.3",
    "month.4",
    "month.5",
    "month.6",
    "month.7",
    "month.8",
    "month.9",
    "month.10",
    "month.11",
    "month.12",
    "toast.autosaved",
    "toast.game_saved",
    "toast.game_loaded",
    "toast.save_deleted",
    "toast.save_gone",
    "toast.autosave_skipped",
    "toast.save_failed",
    "toast.load_failed",
    "toast.load_in_playtest",
    "toast.delete_failed",
    "subtitle.line",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_table_parses_and_has_every_code_key() {
        let t = Table::parse(EN).unwrap();
        assert!(t.source);
        let content = sepulcra::Content::builtin();
        let missing: Vec<String> = code_keys(&content, &[]).into_iter().filter(|k| !t.entries.contains_key(k)).collect();
        assert!(missing.is_empty(), "keys the code uses that en.json lacks: {missing:?}");
    }

    #[test]
    fn said_renders_nested_messages_in_each_language() {
        let l = L10n::load(None, None);
        let s = Said::new("toast.refused").said("what", Said::new("refuse.what.place").key("stone", "stone.warding.name").text("cell", "top:3,4")).said("why", Said::new("refuse.why.not_enough_dust").int("need", 50).int("have", 12));
        assert_eq!(l.text(&s), "Can't place Warding stone at top:3,4: need 50 dust, have 12");
        assert_eq!(l.source_text(&s), l.text(&s));
        let lost = |n: i64| Said::new("toast.tombstones_lost").key("enemy", "enemy.wraith.name").int("lost", 2).int("remaining", n);
        assert_eq!(l.text(&lost(1)), "Wraith raised 2 dead: 1 tombstone left");
        assert_eq!(l.text(&lost(4)), "Wraith raised 2 dead: 4 tombstones left");
        assert_eq!(l.text(&Said::new("no.such.key")), "no.such.key");
        // Into the HUD's state as a message the UI resolves.
        let mut st = StateMap::new();
        put(&mut st, "hint", &s);
        assert!(matches!(st.get("hint"), Some(specula_ui::Value::Msg(m)) if m.key == "toast.refused"));
    }

    /// Review R2 of m5-l10n, blocker 1: the game's messages reach the player two ways, as text
    /// the game renders itself (toasts, the agents' log: `L10n::text`) and as messages its
    /// screens resolve (`put`, then the UI's templates). The two must say the same, in every
    /// language, for everything the game says: the HUD's hover and the saves screen showed raw
    /// keys ("@face.top 12,9 · @stone.warding.name @tier.2") because the UI counted nesting
    /// twice. Each of these is the shape the game's code builds (game.rs hover_said,
    /// hud.rs stone_name, saves.rs slots_of and date_said).
    #[test]
    fn the_screens_say_what_the_game_says_in_every_language() {
        use specula_ui::state::{Scope, Template};
        let cell = |face: &str| Said::new("hover.cell").key("face", format!("face.{face}")).int("col", 12).int("row", 9);
        let stone = Said::new("stone.tier_name").key("name", "stone.warding.name").key("tier", "tier.2");
        let date = Said::new("saves.date").int("day", 17).key("month", "month.10").int("year", 2026).text("clock", "20:03");
        let said = [
            ("a bare cell", cell("top")),
            ("a stone", Said::new("hover.stone").said("cell", cell("top")).said("stone", stone.clone())),
            ("a wall", Said::new("hover.wall").said("cell", cell("north")).int("hp", 3).int("max_hp", 5)),
            ("a rooted cell", Said::new("hover.rooted").said("cell", cell("east"))),
            ("a prop", Said::new("hover.prop").said("cell", cell("top")).key("prop", "prop.mausoleum_domed.what")),
            ("a save slot", Said::new("saves.played").int("played", 200).said("when", date)),
            ("a refusal", Said::new("toast.refused").said("what", Said::new("refuse.what.place").key("stone", "stone.warding.name").text("cell", "top:3,4")).said("why", Said::new("refuse.why.not_enough_dust").int("need", 50).int("have", 12))),
        ];
        let tables = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sepulcra/assets/strings");
        let mut l = L10n::load(Some(&tables), None);
        let template = Template::parse("{x}", "t").unwrap();
        for locale in l.locales() {
            l.set_locale(&locale).unwrap();
            let strings = l.strings();
            for (what, s) in &said {
                let own = l.text(s);
                let mut st = StateMap::new();
                put(&mut st, "x", s);
                let mut shown = String::new();
                template.render(&Scope::new(&st).with_strings(Some(&strings)), &mut shown);
                assert_eq!(shown, own, "{what} in {locale:?}: the screen and the game's own text differ");
                assert!(!shown.contains('@') && !shown.is_empty(), "{what} in {locale:?} shows a raw key: {shown}");
            }
        }
        assert!(l.locales().len() >= 2, "English and German at least");
    }

    /// A table changed on disk is read again off the calling thread and keeps the language.
    #[test]
    fn tables_reload_on_a_thread_and_keep_the_language() {
        let dir = std::env::temp_dir().join(format!("sepulcra-l10n-reload-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("en.json"), EN).unwrap();
        let de = |menu: &str| format!(r#"{{"format": "specula.strings.v1", "locale": "de", "name": "Deutsch", "strings": {{"hud.menu": "{menu}"}}}}"#);
        std::fs::write(dir.join("de.json"), de("Menü")).unwrap();
        let mut l = L10n::load(Some(&dir), Some("de"));
        assert_eq!(l.text(&Said::new("hud.menu")), "Menü");
        assert_eq!(l.locales().iter().map(|l| l.as_str()).collect::<Vec<_>>(), ["en", "de"], "the source first");
        std::fs::write(dir.join("de.json"), de("Hauptmenü")).unwrap();
        l.reload();
        assert_eq!(l.text(&Said::new("hud.menu")), "Menü", "the old words until the new ones are in");
        assert!(l.finish_reload(std::time::Duration::from_secs(10)).is_some());
        assert_eq!((l.locale().as_str(), l.text(&Said::new("hud.menu")).as_str()), ("de", "Hauptmenü"));
        // A language chosen while the tables are read again stays chosen.
        l.reload();
        l.set_locale(&Locale::parse("en").unwrap()).unwrap();
        assert!(l.finish_reload(std::time::Duration::from_secs(10)).is_some());
        assert_eq!(l.locale().as_str(), "en");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn locales_cycle_and_unknown_ones_are_refused() {
        let mut l = L10n::load(None, Some("xx"));
        assert_eq!(l.locale().as_str(), "en");
        assert!(l.problems.iter().any(|p| p.contains("xx")), "{:?}", l.problems);
        assert_eq!(l.next_locale().as_str(), "en", "only English is built in");
        assert!(l.set_locale(&Locale::parse("fr").unwrap()).is_err());
    }

    /// Review R2 of m5-l10n, major 3: choosing a language builds nothing where it is chosen
    /// (the engine thread): every table's strings, and their pseudo-locale, are made when the
    /// tables are read, and a language is a reference to them.
    #[test]
    fn choosing_a_language_takes_strings_already_made() {
        let tables = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sepulcra/assets/strings");
        let mut l = L10n::load(Some(&tables), None);
        let en = l.strings();
        assert!(en.pseudo_ready(), "made with the tables");
        let de = Locale::parse("de").unwrap();
        l.set_locale(&de).unwrap();
        let first = l.strings();
        assert!(first.pseudo_ready() && first.locale() == &de);
        l.set_locale(&Locale::parse("en").unwrap()).unwrap();
        assert!(Arc::ptr_eq(&l.strings(), &en), "the same strings, not built again");
        l.set_locale(&de).unwrap();
        assert!(Arc::ptr_eq(&l.strings(), &first));
        // A region of a language with a table shows that table at once (its own strings follow
        // from a thread); a language with no table is refused.
        l.set_locale(&Locale::parse("de-AT").unwrap()).unwrap();
        assert_eq!(l.locale().as_str(), "de-AT");
        assert!(Arc::ptr_eq(&l.strings(), &first), "German's words meanwhile");
        let t0 = std::time::Instant::now();
        let own = loop {
            if let Some(s) = l.poll() {
                break s;
            }
            assert!(t0.elapsed() < std::time::Duration::from_secs(20), "the thread never answered");
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert!(own.locale().as_str() == "de-AT" && own.pseudo_ready());
        assert!(l.set_locale(&Locale::parse("tlh").unwrap()).is_err());
        assert_eq!(l.locale().as_str(), "de-AT", "a refused choice changes nothing");
    }
}
