//! The localisation tools (`l10n_*`, docs/design/LOCALISATION.md): string tables as project
//! files (`assets/strings/<locale>.json`, kind `strings`), read and written through the project
//! store (claims, versions, the journal, undo, disk, hot reload), and checked against each other,
//! the screens and the game's interface in every language.
//!
//! - `l10n_get`: the tables and their state, or one locale's entries (resolved through locale,
//!   language, source), filtered and paged, with previews of plural forms.
//! - `l10n_set`: writes entries into one locale's table in one commit: a translation must use
//!   the source's placeholders and is stamped with the source text it was made from (`src`) and
//!   `unreviewed`; marking entries reviewed needs reviewer authority (here and through
//!   `project_write`: [`review_gate`]).
//! - `l10n_check`: the tables against each other and the screens (missing, unknown, stale,
//!   placeholders, plural forms, undefined and unused keys, words screens write themselves), and
//!   with `ui` the game's interface laid out in each language at every display and scale asked
//!   (`ui_check` with the locale's strings).
//! - `l10n_tasks`: the translation flow: a board task per locale with work (translate, then
//!   review), posted as one plan.
//!
//! Everything runs on the calling agent's connection: tables are parsed here (cached by path and
//! version), fonts for a locale are read here, and the engine only lends the store's texts,
//! commits, and lays the game's interface out (`ui_check`, one display per step).

use crate::project::FileKind;
use crate::project_tools::{ProjectCall, DEFERRED_WAIT};
use crate::tools::{argument_error, bool_arg, check_keys, int_arg, string_arg, Command, Tools};
use specula_core::Json;
use specula_lingua::check::{Severity, Used};
use specula_lingua::table::{source_hash, valid_key};
use specula_lingua::{self as lingua, Arg, Catalog, Entry, LinguaError, Locale, Status, Table, Text};
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};
use std::sync::{mpsc, Arc, Mutex};

/// Most string tables a project may have (each at most 1 MiB).
pub const MAX_TABLES: usize = 64;
/// Most keys `l10n_get` takes by name.
pub const MAX_KEYS_ARG: usize = 64;
/// Most entries one `l10n_set` writes, removes or reviews (together).
pub const MAX_SET: usize = 200;

/// `l10n_get` pages (default, most).
const GET_LIMITS: (usize, usize) = (50, 200);
/// Bytes of entries one `l10n_get` answers (then `next_cursor`).
const ANSWER_BUDGET: usize = 160 * 1024;
/// Locales `l10n_check` lays the interface out in, and screen groups per locale.
pub const MAX_CHECK_LOCALES: usize = 8;
pub const MAX_GROUPS: usize = 12;
/// Layouts one `ui_check` call makes (its own limit).
const LAYOUTS_PER_CHECK: usize = 16;
/// Layouts one `l10n_check` makes at most (locales x groups x targets x scales; a layout of
/// Sepulcra's screens is a few milliseconds, in slices between frames).
pub const MAX_CHECK_LAYOUTS: usize = 2048;
/// Bytes of screen and timeline files one `l10n_check` scans for keys.
const MAX_SCAN_BYTES: usize = 8 << 20;
/// Findings quoted (counts cover all).
const MAX_FINDINGS: usize = 200;
/// UI screen files scanned for `@key` texts.
const MAX_SCREENS_SCANNED: usize = 512;

fn err(code: &str, message: impl Into<String>) -> ToolOutput {
    ToolOutput::error(Json::obj([("error", Json::from(code)), ("message", Json::from(message.into()))]).to_string())
}

fn lingua_err(e: &LinguaError, path: Option<&str>) -> ToolOutput {
    let mut j = e.to_json();
    if let Some(p) = path {
        j.set("path", Json::from(p));
    }
    ToolOutput::error(j.to_string())
}

/// Answers an `l10n_*` call on the caller's connection thread.
pub fn call(tools: &Tools, session: &SessionInfo, name: &str, args: &Json) -> ToolOutput {
    let r = match name {
        "l10n_get" => get_tool(tools, session, args),
        "l10n_set" => set_tool(tools, session, args),
        "l10n_check" => check_tool(tools, session, args),
        "l10n_tasks" => tasks_tool(tools, session, args),
        other => Err(format!("unknown localisation tool '{other}'")),
    };
    r.unwrap_or_else(|m| ToolOutput::error(argument_error(&m).to_string()))
}

// ---- the project's tables ----

/// One table file.
pub struct Loaded {
    pub path: String,
    pub version: u64,
    pub text: Arc<str>,
    pub table: Arc<Table>,
}

/// The project's string tables, parsed.
pub struct Tables {
    pub files: Vec<Loaded>,
    /// Tables that don't parse: `(path, why)`.
    pub broken: Vec<(String, LinguaError)>,
}

/// Parsed tables by path, version and the text's stable hash (the cache is process-wide: two
/// engines in one process may hold other texts at one path and version): at most this many.
const PARSED_CAP: usize = MAX_TABLES;
type ParsedKey = (String, u64, u64);
static PARSED: Mutex<Vec<(ParsedKey, Arc<Table>)>> = Mutex::new(Vec::new());

fn parse_cached(path: &str, version: u64, text: &str) -> Result<Arc<Table>, LinguaError> {
    let mut h = specula_core::hash::StableHasher::new();
    h.write_str(text);
    let key: ParsedKey = (path.to_owned(), version, h.finish());
    if let Some((_, t)) = PARSED.lock().unwrap_or_else(|p| p.into_inner()).iter().find(|(k, _)| *k == key) {
        return Ok(Arc::clone(t));
    }
    let t = Arc::new(Table::parse(text)?);
    let mut c = PARSED.lock().unwrap_or_else(|p| p.into_inner());
    c.retain(|(k, _)| k.0 != path);
    c.push((key, Arc::clone(&t)));
    if c.len() > PARSED_CAP {
        c.remove(0);
    }
    Ok(t)
}

fn files_of(tools: &Tools, session: &SessionInfo, kind: FileKind, max: usize) -> Result<Vec<(String, u64, Arc<str>)>, ToolOutput> {
    let (tx, rx) = mpsc::channel();
    let out = tools.send(session, Command::Project(ProjectCall::OfKind { kind, max, reply: tx }));
    if out.is_error {
        return Err(out);
    }
    rx.recv_timeout(DEFERRED_WAIT).map_err(|_| err("no_answer", "the engine didn't lend the project's files; nothing was changed"))
}

impl Tables {
    pub fn load(tools: &Tools, session: &SessionInfo) -> Result<Tables, ToolOutput> {
        let files = files_of(tools, session, FileKind::Strings, MAX_TABLES + 1)?;
        if files.len() > MAX_TABLES {
            return Err(err("limit", format!("the project has more than {MAX_TABLES} string tables (assets/strings/*.json); delete some")));
        }
        Ok(Tables::from_files(files))
    }

    /// From `(path, version, text)` (tests make these without an engine).
    pub fn from_files(files: Vec<(String, u64, Arc<str>)>) -> Tables {
        let mut out = Tables { files: Vec::new(), broken: Vec::new() };
        for (path, version, text) in files {
            match parse_cached(&path, version, &text) {
                Ok(table) => out.files.push(Loaded { path, version, text, table }),
                Err(e) => out.broken.push((path, e)),
            }
        }
        out
    }

    pub fn source(&self) -> Result<&Loaded, ToolOutput> {
        let s: Vec<&Loaded> = self.files.iter().filter(|f| f.table.source).collect();
        match s.as_slice() {
            [one] => Ok(one),
            [] => Err(err("no_source", "no string table says \"source\": true: write the English one first (l10n_set {locale: \"en\", source: true, entries})")),
            many => Err(err("no_source", format!("more than one table says \"source\": true ({}): keep one", many.iter().map(|f| f.path.as_str()).collect::<Vec<_>>().join(", ")))),
        }
    }

    pub fn of(&self, l: &Locale) -> Option<&Loaded> {
        self.files.iter().find(|f| &f.table.locale == l)
    }

    pub fn refs(&self) -> Vec<&Table> {
        self.files.iter().map(|f| &*f.table).collect()
    }

    /// The catalog for `l` (the pseudo-locale from the source).
    pub fn catalog(&self, l: &Locale) -> Result<Catalog, ToolOutput> {
        Catalog::build(l, &self.refs()).map_err(|e| lingua_err(&e, None))
    }

    /// A UI's strings for `l`, with its locale's faces (read here, never on the engine thread).
    /// For the pseudo-locale: the source's strings, which `ui_check {pseudo}` turns pseudo.
    pub fn ui_strings(&self, l: &Locale) -> Result<(Arc<specula_ui::Strings>, bool), ToolOutput> {
        let pseudo = l.is_pseudo();
        let src = self.source()?;
        let at = if pseudo { src.table.locale.clone() } else { l.clone() };
        let catalog = self.catalog(&at)?;
        let (fonts, problems) = specula_ui::Strings::load_fonts(&catalog);
        let mut s = specula_ui::Strings::new(catalog, Some(Arc::clone(&src.table)), fonts);
        s.problems.extend(problems);
        // Here, on the caller's thread: the engine thread's check only switches to it.
        if pseudo {
            s.warm_pseudo();
        }
        Ok((Arc::new(s), pseudo))
    }
}

/// The hosted game's language and the keys its code uses.
fn game_locale(tools: &Tools, session: &SessionInfo) -> Option<crate::game::GameLocale> {
    let (tx, rx) = mpsc::channel();
    let out = tools.send(session, Command::Ui(crate::ui_tools::UiCall::Locale(tx)));
    if out.is_error {
        return None;
    }
    rx.recv_timeout(DEFERRED_WAIT).ok()
}

fn locale_arg(args: &Json, key: &str) -> Result<Option<Locale>, String> {
    match string_arg(args, key)? {
        None => Ok(None),
        Some(s) => Locale::parse(s).map(Some).map_err(|e| format!("'{key}': {}", e.message)),
    }
}

fn keys_arg(args: &Json, key: &str, max: usize) -> Result<Option<Vec<String>>, String> {
    let Some(v) = args.get(key).filter(|v| !v.is_null()) else { return Ok(None) };
    let a = v.as_array().ok_or_else(|| format!("'{key}' must be an array of string-table keys"))?;
    if a.len() > max {
        return Err(format!("'{key}' holds at most {max} keys"));
    }
    let mut out: Vec<String> = Vec::with_capacity(a.len());
    for (i, k) in a.iter().enumerate() {
        let s = k.as_str().filter(|s| valid_key(s)).ok_or_else(|| format!("'{key}[{i}]' is not a key (1..96 of letters, digits, _ - . starting with a letter)"))?;
        if out.iter().any(|o| o == s) {
            return Err(format!("'{key}[{i}]' ({s}) is listed twice"));
        }
        out.push(s.to_owned());
    }
    Ok(Some(out))
}

/// JSON preview arguments: numbers and strings.
fn preview_args(v: &Json) -> Result<Vec<(String, Json)>, String> {
    let o = v.as_object().ok_or("'args' must be an object of placeholder: number or text")?;
    if o.len() > lingua::message::MAX_PLACEHOLDERS {
        return Err(format!("'args' holds at most {} values", lingua::message::MAX_PLACEHOLDERS));
    }
    for (k, x) in o {
        if !lingua::message::valid_name(k) || !(x.as_f64().is_some() || x.as_str().is_some_and(|s| s.len() <= 256)) {
            return Err(format!("'args.{}' must be a placeholder name with a number or a text of at most 256 bytes", crate::bounds::echo(k)));
        }
    }
    Ok(o.to_vec())
}

fn to_args(a: &[(String, Json)]) -> Vec<(&str, Arg<'_>)> {
    a.iter()
        .map(|(k, v)| {
            let arg = match v {
                Json::Str(s) => Arg::Str(s),
                Json::Num(n) if specula_core::dmath::fract_f64(*n) == 0.0 && n.abs() < 9e15 => Arg::Int(*n as i64),
                Json::Num(n) => Arg::Num(*n),
                _ => Arg::Str(""),
            };
            (k.as_str(), arg)
        })
        .collect()
}

fn placeholder_names(t: &Text) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (_, s) in t.texts() {
        for p in lingua::message::placeholders(s, "").unwrap_or_default() {
            if !out.contains(&p.name) {
                out.push(p.name);
            }
        }
    }
    out
}

// ---- l10n_get ----

fn get_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "l10n_get", &[&["locale", "keys", "prefix", "status", "cursor", "limit", "args"]])?;
    let locale = locale_arg(args, "locale")?;
    let keys = keys_arg(args, "keys", MAX_KEYS_ARG)?;
    let prefix = string_arg(args, "prefix")?.map(str::to_owned);
    if prefix.as_ref().is_some_and(|p| p.len() > lingua::table::MAX_KEY_BYTES) {
        return Err(format!("'prefix' is at most {} bytes", lingua::table::MAX_KEY_BYTES));
    }
    let status = string_arg(args, "status")?.unwrap_or("all");
    if !["all", "missing", "translated", "unreviewed", "reviewed", "stale"].contains(&status) {
        return Err("'status' must be all, missing, translated, unreviewed, reviewed or stale".into());
    }
    let cursor = int_arg(args, "cursor", 0, lingua::table::MAX_KEYS as u64)?.unwrap_or(0) as usize;
    let limit = int_arg(args, "limit", 1, GET_LIMITS.1 as u64)?.map_or(GET_LIMITS.0, |l| l as usize);
    let preview = match args.get("args").filter(|a| !a.is_null()) {
        Some(a) => Some(preview_args(a)?),
        None => None,
    };
    if preview.is_some() && keys.is_none() {
        return Err("'args' previews named entries: give 'keys' too".into());
    }
    let tables = match Tables::load(tools, session) {
        Ok(t) => t,
        Err(out) => return Ok(out),
    };
    let game = game_locale(tools, session);
    let Some(locale) = locale else {
        return Ok(ToolOutput::text(overview(&tables, game.as_ref()).to_string()));
    };
    let src = match tables.source() {
        Ok(s) => s,
        Err(out) => return Ok(out),
    };
    let catalog = match tables.catalog(&locale) {
        Ok(c) => c,
        Err(out) => return Ok(out),
    };
    let own = tables.of(&locale);
    let is_source = locale == src.table.locale;
    let mut entries = Vec::new();
    let mut total = 0usize;
    let mut bytes = 0usize;
    let mut next = None;
    let mut unknown: Vec<String> = Vec::new();
    if let Some(k) = &keys {
        unknown = k.iter().filter(|k| catalog.get(k).is_none()).cloned().collect();
    }
    for (key, r) in catalog.entries() {
        if keys.as_ref().is_some_and(|k| !k.iter().any(|x| x == key)) || prefix.as_ref().is_some_and(|p| !key.starts_with(p.as_str())) {
            continue;
        }
        let translated = r.from != src.table.locale || is_source || locale.is_pseudo();
        let keep = match status {
            "missing" => !translated,
            "translated" => translated && !is_source,
            "unreviewed" => r.status == Status::Unreviewed,
            "reviewed" => r.status == Status::Reviewed,
            "stale" => r.stale,
            _ => true,
        };
        if !keep {
            continue;
        }
        total += 1;
        if total <= cursor || next.is_some() {
            continue;
        }
        if entries.len() >= limit || bytes > ANSWER_BUDGET {
            next = Some(total - 1);
            continue;
        }
        let mut f = vec![("key", Json::from(key)), ("text", r.text.to_json()), ("from", Json::from(r.from.as_str())), ("status", Json::from(r.status.name()))];
        if r.stale {
            f.push(("stale", Json::Bool(true)));
        }
        if !is_source {
            if let Some(se) = src.table.entries.get(key) {
                f.push(("source", se.text.to_json()));
            }
        }
        let notes = own.and_then(|o| o.table.entries.get(key)).and_then(|e| e.notes.as_deref()).or_else(|| src.table.entries.get(key).and_then(|e| e.notes.as_deref()));
        if let Some(n) = notes {
            f.push(("notes", Json::from(n)));
        }
        let names = placeholder_names(&r.text);
        if !names.is_empty() {
            f.push(("placeholders", Json::arr(names.into_iter().map(Json::from))));
        }
        if let Some(p) = &preview {
            let mut out = String::new();
            catalog.format(key, &to_args(p), &mut out);
            f.push(("preview", Json::from(out)));
        }
        let j = Json::obj(f);
        bytes += j.to_string().len();
        entries.push(j);
    }
    let mut out = vec![
        ("locale", Json::from(locale.as_str())),
        ("name", Json::from(catalog.name.as_str())),
        ("source", Json::from(src.table.locale.as_str())),
        ("plural_categories", Json::arr(catalog.rules.categories().into_iter().map(|c| Json::from(c.name())))),
        ("plural_rules", Json::from(own.map_or_else(|| src.table.plural_rules_from(), |o| o.table.plural_rules_from()))),
    ];
    if let Some(o) = own {
        out.push(("path", Json::from(o.path.as_str())));
        out.push(("version", Json::from(o.version)));
    } else if !locale.is_pseudo() {
        out.push(("table", Json::from("none of its own: every entry comes from its language's or the source's table (l10n_set {create: {name}} makes one)")));
    }
    out.push(("entries", Json::Arr(entries)));
    out.push(("total", Json::from(total)));
    if let Some(n) = next {
        out.push(("next_cursor", Json::from(n)));
    }
    if !unknown.is_empty() {
        out.push(("unknown_keys", Json::arr(unknown.into_iter().map(Json::from))));
    }
    if let Some(g) = &game {
        if let Some(l) = &g.locale {
            out.push(("game_locale", Json::from(l.as_str())));
        }
    }
    Ok(ToolOutput::text(Json::obj(out).to_string()))
}

fn overview(tables: &Tables, game: Option<&crate::game::GameLocale>) -> Json {
    let pairs: Vec<(&str, &Table)> = tables.files.iter().map(|f| (f.path.as_str(), &*f.table)).collect();
    let report = lingua::check::check(&pairs, None);
    let mut list = Vec::new();
    for s in &report.locales {
        let mut j = s.to_json();
        if let Some(f) = tables.files.iter().find(|f| f.table.locale.as_str() == s.locale) {
            j.set("path", Json::from(f.path.as_str()));
            j.set("version", Json::from(f.version));
        }
        list.push(j);
    }
    let mut out = vec![("tables", Json::Arr(list)), ("pseudo", Json::from(lingua::PSEUDO))];
    if let Some(s) = &report.source {
        out.push(("source", Json::from(s.as_str())));
    }
    if let Some(l) = game.and_then(|g| g.locale.as_deref()) {
        out.push(("game_locale", Json::from(l)));
    }
    if !tables.broken.is_empty() {
        out.push(("broken", Json::arr(tables.broken.iter().map(|(p, e)| {
            let mut j = e.to_json();
            j.set("path", Json::from(p.as_str()));
            j
        }))));
    }
    out.push(("next", Json::from("l10n_get {locale, status: \"missing\"} lists what to translate; l10n_set writes it; l10n_check checks it in the interface")));
    Json::obj(out)
}

// ---- the review gate ----

/// Marking entries reviewed (in `l10n_set` or by writing the table with `project_*`) needs
/// reviewer authority: an entry reviewed in `new` must have been reviewed in `old` with the same
/// words and source, **in the same language**, unless the caller is a reviewer. A table whose
/// `locale` or `source` flag changes carries none of its reviews over: a reviewer approved those
/// words as German, not as whatever the file says next (review R2 of m5-l10n: a plain agent
/// relabelled a reviewed German table as French, and `l10n_get` showed French as reviewed).
/// Disk imports (a human's editor) never come here.
///
/// An `old` that doesn't parse holds no reviews as far as this gate can tell. The engine never
/// saves a string table that doesn't parse (`project_kinds::never_allowed`), so that only
/// happens after a human's edit on disk broke the file; a reviewer (or the human) puts it right.
pub fn review_gate(session: &SessionInfo, old: Option<&str>, new: Option<&str>) -> Result<(), ToolOutput> {
    let Some(new) = new.and_then(|n| Table::parse(n).ok()) else { return Ok(()) };
    let old = old.and_then(|o| Table::parse(o).ok());
    let before = |k: &String| old.as_ref().and_then(|o| o.entries.get(k));
    let relabelled = old.as_ref().is_some_and(|o| o.locale != new.locale || o.source != new.source);
    let forged: Vec<&String> = new
        .entries
        .iter()
        .filter(|(_, e)| e.status == Status::Reviewed)
        .filter(|(k, e)| relabelled || !before(k).is_some_and(|o| o.status == Status::Reviewed && o.text == e.text && o.src == e.src))
        .map(|(k, _)| k)
        .collect();
    // `src` says which source words a translation was made from: only l10n_set writes it (it
    // stamps the source's hash as it is now), so nobody hides a stale translation by editing the
    // file (review of m5-l10n, finding 7). A reviewer may.
    let stamped = WRITING_SRC.with(std::cell::Cell::get);
    let moved_src: Vec<&String> = if stamped { Vec::new() } else { new.entries.iter().filter(|(k, e)| before(k).map_or(e.src.is_some(), |o| o.src != e.src)).map(|(k, _)| k).collect() };
    let have = crate::authority::of_session(&session.id);
    let list = |v: &[&String]| format!("{}{}", v.iter().take(4).map(|k| k.as_str()).collect::<Vec<_>>().join(", "), if v.len() > 4 { ", ..." } else { "" });
    if !forged.is_empty() {
        let what = if relabelled {
            format!("changing the language (locale or source) of a table that holds {} reviewed string(s) ({}): their reviews were for the language it had", forged.len(), list(&forged))
        } else {
            format!("marking {} string(s) reviewed ({})", forged.len(), list(&forged))
        };
        crate::authority::require(have, crate::authority::Authority::Reviewer, crate::authority::Site::L10nReview, &what)
            .map_err(|e| ToolOutput::error(e.to_json().to_string()))?;
    }
    if !moved_src.is_empty() {
        crate::authority::require(have, crate::authority::Authority::Reviewer, crate::authority::Site::L10nReview, &format!("changing the source hash (src) of {} string(s) by hand ({}): l10n_set writes it", moved_src.len(), list(&moved_src)))
            .map_err(|e| ToolOutput::error(e.to_json().to_string()))?;
    }
    Ok(())
}

thread_local! {
    /// Set while `l10n_set` commits on this connection thread: the `src` it wrote is the source's
    /// hash it worked out itself.
    static WRITING_SRC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// ---- l10n_set ----

/// One requested entry, checked against the source.
struct Edit {
    key: String,
    value: Option<Json>,
}

fn set_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "l10n_set", &[&["locale", "entries", "remove", "review", "create", "source", "expect_version"]])?;
    let locale = locale_arg(args, "locale")?.ok_or("'locale' is required: the table to write (en, de, pt-BR...)")?;
    if locale.is_pseudo() {
        return Err(format!("{} is the pseudo-locale, made from the source table: write the source (en) instead", lingua::PSEUDO));
    }
    let entries = match args.get("entries").filter(|e| !e.is_null()) {
        Some(e) => {
            let o = e.as_object().ok_or("'entries' must be an object of key: text (or {text | plural + forms, notes?})")?;
            if o.len() > MAX_SET {
                return Err(format!("'entries' holds at most {MAX_SET} keys per call (this has {}): split it", o.len()));
            }
            o.to_vec()
        }
        None => Vec::new(),
    };
    let remove = keys_arg(args, "remove", MAX_SET)?.unwrap_or_default();
    let review = keys_arg(args, "review", MAX_SET)?.unwrap_or_default();
    let n = entries.len() + remove.len() + review.len();
    if n == 0 && args.get("create").is_none() {
        return Err("give 'entries' to write, 'remove' or 'review' keys, or 'create' a table".into());
    }
    if n > MAX_SET {
        return Err(format!("one call writes, removes and reviews at most {MAX_SET} entries together (this asks {n}): split it"));
    }
    // A key in two of entries, remove and review is ambiguous (review of m5-l10n, finding 6).
    {
        let mut all: Vec<&str> = entries.iter().map(|(k, _)| k.as_str()).chain(remove.iter().map(String::as_str)).chain(review.iter().map(String::as_str)).collect();
        all.sort_unstable();
        if let Some(w) = all.windows(2).find(|w| w[0] == w[1]) {
            return Err(format!("\"{}\" is in more than one of entries, remove and review: one change per key per call", crate::bounds::echo(w[0])));
        }
    }
    let create_name = match args.get("create").filter(|c| !c.is_null()) {
        Some(c) => {
            check_keys(c, "l10n_set.create", &[&["name"]])?;
            let name = string_arg(c, "name")?.ok_or("'create.name' is required: the language's own name (Deutsch, 日本語)")?;
            if name.is_empty() || name.len() > lingua::table::MAX_NAME_BYTES || name.chars().any(char::is_control) {
                return Err(format!("'create.name' is the language's own name, 1..{} bytes", lingua::table::MAX_NAME_BYTES));
            }
            Some(name.to_owned())
        }
        None => None,
    };
    let make_source = bool_arg(args, "source")?.unwrap_or(false);
    let expect = int_arg(args, "expect_version", 0, u64::MAX >> 11)?;
    if !review.is_empty() {
        if let Err(e) = crate::authority::require(crate::authority::of_session(&session.id), crate::authority::Authority::Reviewer, crate::authority::Site::L10nReview, "l10n_set 'review'") {
            return Ok(ToolOutput::error(e.to_json().to_string()));
        }
    }
    for _ in 0..4 {
        let tables = match Tables::load(tools, session) {
            Ok(t) => t,
            Err(out) => return Ok(out),
        };
        if let Some((p, e)) = tables.broken.iter().find(|(p, _)| tables.of(&locale).is_none() && p.ends_with(&format!("/{locale}.json"))) {
            return Ok(lingua_err(e, Some(p)));
        }
        let own = tables.of(&locale);
        let src = tables.source().ok();
        let is_source = own.is_some_and(|o| o.table.source) || (src.is_none() && make_source);
        if src.is_none() && !is_source {
            return Ok(err("no_source", "there is no source table yet: write the English one first (l10n_set {locale: \"en\", source: true, create: {name: \"English\"}, entries})"));
        }
        let (path, base_text, base_version) = match own {
            Some(o) => (o.path.clone(), o.text.to_string(), o.version),
            None => {
                let Some(name) = &create_name else {
                    return Ok(err("no_table", format!("{locale} has no table: pass create: {{name}} (its own name for the language) to make assets/strings/{locale}.json")));
                };
                let dir = src.and_then(|s| s.path.rsplit_once('/').map(|(d, _)| d.to_owned())).unwrap_or_else(|| "assets/strings".into());
                let mut t = Table::empty_json(&locale, name);
                if is_source {
                    t.set("source", Json::Bool(true));
                    if let Json::Obj(f) = &mut t {
                        f.retain(|(k, _)| k != "status");
                    }
                }
                (format!("{dir}/{locale}.json"), format!("{}\n", t.to_pretty()), 0)
            }
        };

        let default = match own {
            Some(o) => o.table.status,
            None if is_source => Status::Source,
            None => Status::Unreviewed,
        };
        // Each entry against the source: its key, its placeholders, its plural count.
        let mut edits: Vec<Edit> = Vec::new();
        for (key, v) in &entries {
            let field = lingua::error::key_path("entries", key);
            if !valid_key(key) {
                return Ok(lingua_err(&LinguaError::new("bad_key", field, format!("\"{}\" is not a key", crate::bounds::echo(key))).allowed("1..96 of letters, digits, _ - . starting with a letter"), None));
            }
            if let Some(o) = v.as_object() {
                if let Some((k, _)) = o.iter().find(|(k, _)| matches!(k.as_str(), "status" | "src")) {
                    return Ok(lingua_err(&LinguaError::new("invalid_value", lingua::error::key_path(&field, k), format!("'{k}' is set by l10n_set itself: translations are unreviewed and remember their source; reviewers mark them with 'review'")), None));
                }
            }
            let wrapped = Json::obj([("format", Json::from(lingua::table::FORMAT)), ("locale", Json::from(locale.as_str())), ("strings", Json::obj([(key.as_str(), v.clone())]))]);
            let parsed = match Table::from_json(&wrapped) {
                Ok(t) => t,
                Err(mut e) => {
                    e.field = e.field.replacen("strings", "entries", 1);
                    return Ok(lingua_err(&e, None));
                }
            };
            let Some(mut entry) = parsed.entries.get(key.as_str()).cloned() else { continue };
            let notes = entry.notes.clone().or_else(|| own.and_then(|o| o.table.entries.get(key.as_str())).and_then(|e| e.notes.clone()));
            entry.notes = notes;
            if is_source {
                entry.status = Status::Source;
                entry.src = None;
            } else {
                let Some(se) = src.and_then(|s| s.table.entries.get(key.as_str())) else {
                    return Ok(lingua_err(&LinguaError::new("unknown_key", field, format!("the source table has no \"{key}\": translate its keys (l10n_get {{locale: \"{locale}\", status: \"missing\"}}), or add the key to the source first")), None));
                };
                if let Err(e) = same_holes(&se.text, &entry.text, &field) {
                    return Ok(lingua_err(&e, None));
                }
                entry.status = Status::Unreviewed;
                entry.src = Some(source_hash(&se.text));
            }
            edits.push(Edit { key: key.clone(), value: Some(entry.to_json(default)) });
        }
        for key in &remove {
            if !own.is_some_and(|o| o.table.entries.contains_key(key)) {
                return Ok(err("not_found", format!("{locale} has no entry \"{key}\" to remove")));
            }
            edits.push(Edit { key: key.clone(), value: None });
        }
        for key in &review {
            let Some(e) = own.and_then(|o| o.table.entries.get(key)) else {
                return Ok(err("not_found", format!("{locale} has no entry \"{key}\" of its own to review (a key it takes from its language or the source is reviewed there)")));
            };
            if e.src.is_none() && !is_source {
                return Ok(err("no_src", format!("\"{key}\" doesn't say which source words it translates: write it with l10n_set first (which records them), then review it")));
            }
            if is_source {
                return Ok(err("invalid_value", "the source table's words are the source: review translations"));
            }
            if let Some(se) = src.and_then(|s| s.table.entries.get(key)) {
                if e.src.is_some_and(|h| h != source_hash(&se.text)) {
                    return Ok(err("stale", format!("\"{key}\" was translated from other source words than the source has now: translate it again before reviewing")));
                }
            }
            let reviewed = Entry { status: Status::Reviewed, ..e.clone() };
            edits.push(Edit { key: key.clone(), value: Some(reviewed.to_json(default)) });
        }
        // Apply to the document, then write it in the tables' one form (one string per line, so
        // a diff is one line per string): one parse and one print of the table, whatever the
        // number of edits; then the whole table must parse.
        let mut doc = match Json::parse(&base_text) {
            Ok(d) => d,
            Err(e) => return Ok(err("invalid", format!("{path} doesn't parse ({e}); fix it with project_write first"))),
        };
        if let Json::Obj(fields) = &mut doc {
            if let Some((_, Json::Obj(strings))) = fields.iter_mut().find(|(k, _)| k == "strings") {
                for e in &edits {
                    match &e.value {
                        Some(v) => match strings.iter_mut().find(|(k, _)| *k == e.key) {
                            Some(slot) => slot.1 = v.clone(),
                            None => strings.push((e.key.clone(), v.clone())),
                        },
                        None => strings.retain(|(k, _)| *k != e.key),
                    }
                }
            }
        }
        let text = crate::project_text::with_eol(&render_table(&doc), crate::project_text::eol_of(&base_text));
        if is_source && make_source && own.is_some_and(|o| !o.table.source) {
            return Ok(err("invalid_value", format!("{path} is a translation: a table's role isn't changed by l10n_set (project_patch {{set: \"source\"}} does)")));
        }
        if let Err(e) = Table::parse(&text) {
            return Ok(lingua_err(&e, Some(&path)));
        }
        if text == base_text {
            return Ok(ToolOutput::text(Json::obj([("path", Json::from(path.as_str())), ("locale", Json::from(locale.as_str())), ("version", Json::from(base_version)), ("unchanged", Json::Bool(true))]).to_string()));
        }
        let w = Json::obj([("path", Json::from(path.as_str())), ("text", Json::from(text)), ("expect_version", Json::from(expect.unwrap_or(base_version)))]);
        WRITING_SRC.with(|c| c.set(true));
        let out = crate::project_tools::call(tools, session, "project_write", &w);
        WRITING_SRC.with(|c| c.set(false));
        let answer = out.content.iter().find_map(|c| match c {
            specula_protocol::Content::Text(t) => Json::parse(t).ok(),
            _ => None,
        });
        // Someone wrote the table between our read and our commit: read it again (unless the
        // caller pinned a version: then they get the conflict help).
        if out.is_error && expect.is_none() && answer.as_ref().and_then(|a| a.get("error")).and_then(Json::as_str) == Some("stale_version") {
            continue;
        }
        if out.is_error {
            return Ok(out);
        }
        let mut j = answer.unwrap_or(Json::obj([("path", Json::from(path.as_str()))]));
        j.set("locale", Json::from(locale.as_str()));
        j.set("written", Json::from(entries.len()));
        j.set("removed", Json::from(remove.len()));
        j.set("reviewed", Json::from(review.len()));
        if !entries.is_empty() && !is_source {
            j.set("status", Json::from("unreviewed"));
        }
        return Ok(ToolOutput::text(j.to_string()));
    }
    Ok(err("busy", "others kept changing this table while the edit was made (4 tries): claim_take /project/<its path> first"))
}

/// A table as `l10n_set` writes it: one header field per line, then one string per line, each
/// value compact with `": "` and `", "` (Python's `json.dumps` style), so a diff shows one line
/// per string changed.
pub fn render_table(doc: &Json) -> String {
    let mut out = String::from("{\n");
    let fields = doc.as_object().unwrap_or(&[]);
    for (i, (k, v)) in fields.iter().enumerate() {
        out.push_str("  ");
        compact(&Json::from(k.as_str()), &mut out);
        out.push_str(": ");
        match (k.as_str(), v) {
            ("strings", Json::Obj(es)) if !es.is_empty() => {
                out.push_str("{\n");
                for (j, (ek, ev)) in es.iter().enumerate() {
                    out.push_str("    ");
                    compact(&Json::from(ek.as_str()), &mut out);
                    out.push_str(": ");
                    compact(ev, &mut out);
                    if j + 1 < es.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str("  }");
            }
            _ => compact(v, &mut out),
        }
        if i + 1 < fields.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

fn compact(v: &Json, out: &mut String) {
    match v {
        Json::Obj(f) => {
            out.push('{');
            for (i, (k, x)) in f.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&Json::from(k.as_str()).to_string());
                out.push_str(": ");
                compact(x, out);
            }
            out.push('}');
        }
        Json::Arr(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                compact(x, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// A translation's placeholders are the source's (names; formats may differ), and its plural
/// forms are picked by the same placeholder.
fn same_holes(src: &Text, tr: &Text, field: &str) -> Result<(), LinguaError> {
    let mut a = placeholder_names(src);
    let mut b = placeholder_names(tr);
    a.sort();
    b.sort();
    if a != b {
        let lack: Vec<String> = a.iter().filter(|n| !b.contains(n)).map(|n| format!("{{{n}}}")).collect();
        let extra: Vec<String> = b.iter().filter(|n| !a.contains(n)).map(|n| format!("{{{n}}}")).collect();
        let mut m = String::from("its placeholders must be the source's");
        if !lack.is_empty() {
            m.push_str(&format!("; missing {}", lack.join(" ")));
        }
        if !extra.is_empty() {
            m.push_str(&format!("; not in the source {}", extra.join(" ")));
        }
        return Err(LinguaError::new("placeholders", field, m).allowed(a.iter().map(|n| format!("{{{n}}}")).collect::<Vec<_>>().join(" ")));
    }
    if let (Text::Plural { count: sc, .. }, Text::Plural { count, .. }) = (src, tr) {
        if sc != count {
            return Err(LinguaError::new("plural_count", field, format!("the source picks forms by {{{sc}}}: use \"plural\": \"{sc}\"")));
        }
    }
    Ok(())
}

// ---- screens: the keys they use, the words they write ----

/// Which `@key` texts the project's screens and timelines (subtitles) use, and their words
/// written in place (as findings).
fn scan_screens(tools: &Tools, session: &SessionInfo, used: &mut Used, hardcoded: &mut Vec<lingua::check::Finding>) -> Result<usize, ToolOutput> {
    let (mut n, mut scanned) = (0, 0usize);
    for kind in [FileKind::UiScreen, FileKind::Timeline] {
        let files = files_of(tools, session, kind, MAX_SCREENS_SCANNED)?;
        for (path, _, text) in &files {
            if scanned + text.len() > MAX_SCAN_BYTES {
                hardcoded.push(lingua::check::Finding {
                    rule: "scan_limit",
                    severity: Severity::Warning,
                    locale: String::new(),
                    key: String::new(),
                    message: format!("{path} and later files weren't scanned for keys (at most {MAX_SCAN_BYTES} bytes per call): undefined and unused may miss some"),
                });
                return Ok(n);
            }
            scanned += text.len();
            let Ok(j) = Json::parse(text) else { continue };
            walk(&j, path, "", used, hardcoded);
            n += 1;
        }
    }
    Ok(n)
}

fn walk(j: &Json, file: &str, at: &str, used: &mut Used, hardcoded: &mut Vec<lingua::check::Finding>) {
    match j {
        Json::Obj(f) => {
            for (k, v) in f {
                let here = if at.is_empty() { k.clone() } else { format!("{at}.{k}") };
                match (k.as_str(), v) {
                    ("text" | "label" | "detail" | "tooltip" | "speaker", Json::Str(s)) => {
                        if let Some(key) = s.strip_prefix('@').filter(|r| !r.starts_with('@')) {
                            used.add(key.trim(), &format!("{file} {here}"));
                        } else {
                            let words: String = lingua::message::pieces(s, "")
                                .map(|p| {
                                    p.into_iter()
                                        .filter_map(|x| match x {
                                            lingua::message::Piece::Text(t) => Some(t),
                                            lingua::message::Piece::Hole(_) => None,
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            if words.chars().filter(|c| c.is_alphabetic()).nth(1).is_some() && hardcoded.len() < 4096 {
                                hardcoded.push(lingua::check::Finding {
                                    rule: "hardcoded",
                                    severity: Severity::Warning,
                                    locale: String::new(),
                                    key: String::new(),
                                    message: format!("{file} {here}: \"{}\" is written in the screen, so no language changes it: move it to the source table and write \"@key\"", lingua::error::echo(s, 60)),
                                });
                            }
                        }
                    }
                    _ => walk(v, file, &here, used, hardcoded),
                }
            }
        }
        Json::Arr(a) => {
            for (i, v) in a.iter().enumerate() {
                walk(v, file, &format!("{at}[{i}]"), used, hardcoded);
            }
        }
        _ => {}
    }
}

// ---- l10n_check ----

fn findings_json(f: &[lingua::check::Finding], max: usize) -> (Json, Json) {
    let mut by_rule: std::collections::BTreeMap<&str, (usize, usize)> = std::collections::BTreeMap::new();
    for x in f {
        let e = by_rule.entry(x.rule).or_default();
        if x.severity == Severity::Error {
            e.0 += 1;
        } else {
            e.1 += 1;
        }
    }
    (
        Json::arr(f.iter().take(max).map(lingua::check::Finding::to_json)),
        Json::obj(by_rule.into_iter().map(|(r, (e, w))| (r, Json::obj([("errors", Json::from(e)), ("warnings", Json::from(w))])))),
    )
}

fn check_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "l10n_check", &[&["locales", "ui", "groups", "targets", "scales", "rules", "max_findings"]])?;
    let locales: Option<Vec<Locale>> = match args.get("locales").filter(|l| !l.is_null()) {
        Some(v) => {
            let a = v.as_array().ok_or("'locales' must be an array of locale ids")?;
            if a.is_empty() || a.len() > MAX_CHECK_LOCALES {
                return Err(format!("'locales' holds 1..{MAX_CHECK_LOCALES} locales"));
            }
            let mut out: Vec<Locale> = Vec::new();
            for (i, l) in a.iter().enumerate() {
                let l = Locale::parse(l.as_str().ok_or_else(|| format!("'locales[{i}]' must be a string"))?).map_err(|e| format!("'locales[{i}]': {}", e.message))?;
                if out.contains(&l) {
                    return Err(format!("'locales[{i}]' ({l}) is listed twice"));
                }
                out.push(l);
            }
            Some(out)
        }
        None => None,
    };
    let ui = bool_arg(args, "ui")?.unwrap_or(true);
    let groups: Option<Vec<Vec<String>>> = match args.get("groups").filter(|g| !g.is_null()) {
        Some(v) => {
            let a = v.as_array().ok_or("'groups' must be an array of screen-name arrays (screens shown together)")?;
            if a.is_empty() || a.len() > MAX_GROUPS {
                return Err(format!("'groups' holds 1..{MAX_GROUPS} groups"));
            }
            let mut out = Vec::new();
            for (i, g) in a.iter().enumerate() {
                let names = g.as_array().filter(|n| !n.is_empty() && n.len() <= crate::ui_tools::MAX_SCREENS).ok_or_else(|| format!("'groups[{i}]' must hold 1..{} screen names", crate::ui_tools::MAX_SCREENS))?;
                let names: Vec<String> = names.iter().map(|n| n.as_str().filter(|s| !s.is_empty() && s.len() <= 64).map(str::to_owned).ok_or_else(|| format!("'groups[{i}]' holds screen names (1..64 bytes)"))).collect::<Result<_, _>>()?;
                out.push(names);
            }
            Some(out)
        }
        None => None,
    };
    let targets = args.get("targets").filter(|t| !t.is_null()).cloned();
    let scales: Vec<Json> = match args.get("scales").filter(|s| !s.is_null()) {
        Some(s) => s.as_array().filter(|a| !a.is_empty() && a.len() <= crate::ui_tools::MAX_SCALES).ok_or_else(|| format!("'scales' holds 1..{} UI scales", crate::ui_tools::MAX_SCALES))?.to_vec(),
        None => vec![Json::from(1)],
    };
    let rules = args.get("rules").filter(|r| !r.is_null()).cloned();
    let max = int_arg(args, "max_findings", 1, MAX_FINDINGS as u64)?.unwrap_or(50) as usize;
    let tables = match Tables::load(tools, session) {
        Ok(t) => t,
        Err(out) => return Ok(out),
    };
    // The tables against each other, the screens and the game's keys.
    let mut used = Used::default();
    let mut hardcoded = Vec::new();
    let screens = match scan_screens(tools, session, &mut used, &mut hardcoded) {
        Ok(n) => n,
        Err(out) => return Ok(out),
    };
    let game = game_locale(tools, session);
    if let Some(keys) = game.as_ref().and_then(|g| g.keys.as_ref()) {
        for k in keys {
            used.add(k, "game");
        }
        used.complete = true;
    }
    let pairs: Vec<(&str, &Table)> = tables.files.iter().map(|f| (f.path.as_str(), &*f.table)).collect();
    let mut report = lingua::check::check(&pairs, Some(&used));
    for (p, e) in &tables.broken {
        report.findings.insert(0, lingua::check::Finding { rule: "invalid_table", severity: Severity::Error, locale: String::new(), key: String::new(), message: format!("{p}: {e}") });
    }
    report.findings.extend(hardcoded);
    report.findings.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.rule.cmp(b.rule)));
    let errors = report.findings.iter().filter(|f| f.severity == Severity::Error).count();
    let (shown, by_rule) = findings_json(&report.findings, max);
    let mut out = vec![
        ("tables", Json::obj([("errors", Json::from(errors)), ("warnings", Json::from(report.findings.len() - errors)), ("by_rule", by_rule), ("findings", shown)])),
        ("locales", Json::arr(report.locales.iter().map(lingua::check::LocaleSummary::to_json))),
        ("screens_scanned", Json::from(screens)),
        ("game_keys", Json::from(game.as_ref().and_then(|g| g.keys.as_ref()).map(Vec::len))),
    ];
    if report.findings.len() > max {
        out.push(("findings_left_out", Json::from(report.findings.len() - max)));
    }
    // The interface in each language.
    let mut ui_errors = 0;
    if ui && tables.source().is_ok() {
        let list: Vec<Locale> = match locales {
            Some(l) => l,
            None => {
                let mut l: Vec<Locale> = tables.files.iter().map(|f| f.table.locale.clone()).collect();
                l.sort();
                if let Ok(p) = Locale::parse(lingua::PSEUDO) {
                    l.push(p);
                }
                if l.len() > MAX_CHECK_LOCALES {
                    return Ok(err("limit", format!("{} locales to lay out, at most {MAX_CHECK_LOCALES} per call: name them ('locales')", l.len())));
                }
                l
            }
        };
        let per_call = (LAYOUTS_PER_CHECK / scales.len().max(1)).max(1);
        // Findings quoted per locale: the answer stays under its ceiling however many locales.
        let quota = (max / list.len().max(1)).max(5);
        let target_list: Vec<Json> = match &targets {
            Some(Json::Arr(t)) => t.clone(),
            Some(_) => return Err("'targets' must be an array (ui_check's targets)".into()),
            None => ["deck", "1080p", "4k"].iter().map(|t| Json::from(*t)).collect(),
        };
        if target_list.is_empty() || target_list.len() > 8 {
            return Err("'targets' holds 1..8 displays".into());
        }
        // Cost before work: every layout the call would make, refused over the budget before
        // any is made (review of m5-l10n, finding 12).
        let layouts = list.len() * groups.as_ref().map_or(1, Vec::len) * target_list.len() * scales.len();
        if layouts > MAX_CHECK_LAYOUTS {
            return Ok(ToolOutput::error(
                Json::obj([
                    ("error", Json::from("limit")),
                    ("field", Json::from("locales x groups x targets x scales")),
                    ("limit", Json::from(MAX_CHECK_LAYOUTS)),
                    ("got", Json::from(layouts)),
                    ("message", Json::from(format!("{layouts} layouts ({} locales x {} groups x {} targets x {} scales), at most {MAX_CHECK_LAYOUTS} per call: split it", list.len(), groups.as_ref().map_or(1, Vec::len), target_list.len(), scales.len()))),
                ])
                .to_string(),
            ));
        }
        let mut per_locale = Vec::new();
        for l in &list {
            let (strings, pseudo) = match tables.ui_strings(l) {
                Ok(s) => s,
                Err(out) => return Ok(out),
            };
            let mut e = 0usize;
            let mut w = 0usize;
            let mut by: std::collections::BTreeMap<String, (u64, u64)> = std::collections::BTreeMap::new();
            let mut found: Vec<Json> = Vec::new();
            let group_list: Vec<Option<Vec<String>>> = match &groups {
                Some(g) => g.iter().cloned().map(Some).collect(),
                None => vec![None],
            };
            for g in &group_list {
                for chunk in target_list.chunks(per_call) {
                    let mut a = Json::obj([("surface", Json::from("game")), ("targets", Json::Arr(chunk.to_vec())), ("scales", Json::Arr(scales.clone())), ("max_findings", Json::from(max)), ("pseudo", Json::Bool(pseudo))]);
                    if let Some(g) = g {
                        a.set("screens", Json::arr(g.iter().map(|s| Json::from(s.as_str()))));
                    }
                    if let Some(r) = &rules {
                        a.set("rules", r.clone());
                    }
                    let parsed = match crate::ui_tools::parse("ui_check", &a) {
                        Some(Ok(crate::ui_tools::UiCall::Check(mut c))) => {
                            c.strings = Some(Arc::clone(&strings));
                            c.locale = Some(l.to_string());
                            c
                        }
                        Some(Err(m)) => return Err(m),
                        _ => return Err("ui_check's arguments".into()),
                    };
                    let answer = tools.send(session, Command::Ui(crate::ui_tools::UiCall::Check(parsed)));
                    let j = answer.content.iter().find_map(|c| match c {
                        specula_protocol::Content::Text(t) => Json::parse(t).ok(),
                        _ => None,
                    });
                    if answer.is_error {
                        // No game interface, an unknown screen: the same for every locale.
                        let mut jj = j.unwrap_or(Json::Null);
                        jj.set("locale", Json::from(l.as_str()));
                        out.push(("ui", jj));
                        out.insert(0, ("ok", Json::Bool(false)));
                        return Ok(ToolOutput::text(Json::obj(out).to_string()));
                    }
                    let Some(j) = j else { continue };
                    e += j.get("errors").and_then(Json::as_u64).unwrap_or(0) as usize;
                    w += j.get("warnings").and_then(Json::as_u64).unwrap_or(0) as usize;
                    for (rule, c) in j.get("by_rule").and_then(Json::as_object).unwrap_or(&[]) {
                        let x = by.entry(rule.clone()).or_default();
                        x.0 += c.get("errors").and_then(Json::as_u64).unwrap_or(0);
                        x.1 += c.get("warnings").and_then(Json::as_u64).unwrap_or(0);
                    }
                    for f in j.get("findings").and_then(Json::as_array).unwrap_or(&[]) {
                        if found.len() < quota {
                            let mut f = f.clone();
                            if let Some(g) = g {
                                f.set("group", Json::from(g.join("+")));
                            }
                            found.push(f);
                        }
                    }
                }
            }
            ui_errors += e;
            let mut lj = vec![
                ("locale", Json::from(l.as_str())),
                ("errors", Json::from(e)),
                ("warnings", Json::from(w)),
                ("by_rule", Json::obj(by.into_iter().map(|(r, (e, w))| (r, Json::obj([("errors", Json::from(e)), ("warnings", Json::from(w))]))))),
                ("findings", Json::Arr(found)),
            ];
            if !strings.problems.is_empty() {
                lj.push(("problems", Json::arr(strings.problems.iter().take(16).map(|(at, why)| Json::obj([("at", Json::from(at.as_str())), ("message", Json::from(why.as_str()))])))));
            }
            per_locale.push(Json::obj(lj));
        }
        out.push(("ui", Json::obj([("targets", Json::Arr(target_list)), ("scales", Json::Arr(scales)), ("locales", Json::Arr(per_locale))])));
    }
    out.insert(0, ("ok", Json::Bool(errors == 0 && ui_errors == 0)));
    Ok(ToolOutput::text(Json::obj(out).to_string()))
}

// ---- ui_check {locale} ----

/// `ui_check {locale}`: the locale's strings are made here (tables parsed, faces read), then the
/// check runs on the engine like any `ui_check`.
pub fn ui_check(tools: &Tools, session: &SessionInfo, args: &Json) -> ToolOutput {
    let mut a = match crate::ui_tools::parse("ui_check", args) {
        Some(Ok(crate::ui_tools::UiCall::Check(a))) => a,
        Some(Err(m)) => return ToolOutput::error(argument_error(&m).to_string()),
        _ => return err("invalid_argument", "ui_check's arguments"),
    };
    let Some(l) = a.locale.as_deref().and_then(|l| Locale::parse(l).ok()) else { return err("invalid_argument", "'locale' must be a locale id") };
    let tables = match Tables::load(tools, session) {
        Ok(t) => t,
        Err(out) => return out,
    };
    let (strings, pseudo) = match tables.ui_strings(&l) {
        Ok(s) => s,
        Err(out) => return out,
    };
    a.strings = Some(strings);
    a.pseudo |= pseudo;
    tools.send(session, Command::Ui(crate::ui_tools::UiCall::Check(a)))
}

// ---- l10n_tasks ----

fn tasks_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "l10n_tasks", &[&["locales", "dry_run", "assign"]])?;
    let only = keys_arg_locales(args)?;
    let dry_run = bool_arg(args, "dry_run")?.unwrap_or(false);
    let assign = string_arg(args, "assign")?.unwrap_or("none");
    if !["none", "by_role"].contains(&assign) {
        return Err("'assign' must be \"by_role\" or \"none\"".into());
    }
    let tables = match Tables::load(tools, session) {
        Ok(t) => t,
        Err(out) => return Ok(out),
    };
    let pairs: Vec<(&str, &Table)> = tables.files.iter().map(|f| (f.path.as_str(), &*f.table)).collect();
    let report = lingua::check::check(&pairs, None);
    let mut items = Vec::new();
    for s in report.locales.iter().filter(|s| !s.source) {
        if only.as_ref().is_some_and(|o| !o.iter().any(|l| l.as_str() == s.locale)) {
            continue;
        }
        let path = tables.files.iter().find(|f| f.table.locale.as_str() == s.locale).map_or("", |f| f.path.as_str());
        let todo = s.missing + s.stale;
        let tkey = format!("translate-{}", s.locale);
        if todo > 0 {
            items.push(Json::obj([
                ("key", Json::from(tkey.as_str())),
                ("title", Json::from(format!("Translate {todo} strings into {} ({})", s.name, s.locale))),
                ("detail", Json::from(format!(
                    "{} missing, {} stale. l10n_get {{locale: \"{l}\", status: \"missing\"}} (and \"stale\") lists them with the source words and notes; claim_take /project/{path}; l10n_set {{locale: \"{l}\", entries}} writes up to {MAX_SET} per call (placeholders must match the source's; plural forms for {l}'s categories); l10n_check {{locales: [\"{l}\"]}} checks the interface for overflow.",
                    s.missing, s.stale, l = s.locale
                ))),
                ("role", Json::from("translator")),
            ]));
        }
        if s.unreviewed > 0 || todo > 0 {
            let mut r = vec![
                ("key", Json::from(format!("review-{}", s.locale))),
                ("title", Json::from(format!("Review the {} ({}) strings", s.name, s.locale))),
                ("detail", Json::from(format!("{} unreviewed now. l10n_get {{locale: \"{l}\", status: \"unreviewed\"}}; fix with l10n_set entries, approve with l10n_set {{locale: \"{l}\", review: [keys]}} (reviewer authority). Check the screens with ui_frame and l10n_check.", s.unreviewed, l = s.locale))),
                ("role", Json::from(crate::authority::ROLE_REVIEWER)),
            ];
            if todo > 0 {
                r.push(("after", Json::arr([Json::from(tkey.as_str())])));
            }
            items.push(Json::obj(r));
        }
    }
    if items.is_empty() {
        return Ok(ToolOutput::text(Json::obj([("tasks", Json::from(0)), ("message", Json::from("every locale is translated and reviewed: nothing to post"))]).to_string()));
    }
    let plan = Json::obj([("items", Json::Arr(items)), ("title", Json::from("Localisation")), ("assign", Json::from(assign)), ("dry_run", Json::Bool(dry_run))]);
    match crate::review_tools::parse("task_plan", &plan) {
        Some(Ok(c)) => Ok(tools.send(session, Command::Review(c))),
        Some(Err(m)) => Err(m),
        None => Err("task_plan".into()),
    }
}

fn keys_arg_locales(args: &Json) -> Result<Option<Vec<Locale>>, String> {
    let Some(v) = args.get("locales").filter(|v| !v.is_null()) else { return Ok(None) };
    let a = v.as_array().filter(|a| !a.is_empty() && a.len() <= MAX_TABLES).ok_or_else(|| format!("'locales' holds 1..{MAX_TABLES} locale ids"))?;
    a.iter().enumerate().map(|(i, l)| Locale::parse(l.as_str().unwrap_or("")).map_err(|e| format!("'locales[{i}]': {}", e.message))).collect::<Result<Vec<_>, _>>().map(Some)
}

// ---- definitions ----

pub fn definitions() -> Vec<ToolDef> {
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let s = |max: usize, d: &str| Json::obj([("type", Json::from("string")), ("maxLength", Json::from(max)), ("description", Json::from(d))]);
    let keys = |max: usize, d: &str| Json::obj([("type", Json::from("array")), ("maxItems", Json::from(max)), ("uniqueItems", Json::Bool(true)), ("items", Json::obj([("type", Json::from("string")), ("maxLength", Json::from(lingua::table::MAX_KEY_BYTES))])), ("description", Json::from(d))]);
    let int = |lo: u64, hi: u64, d: &str| Json::obj([("type", Json::from("integer")), ("minimum", Json::from(lo)), ("maximum", Json::from(hi)), ("description", Json::from(d))]);
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(required.iter().map(|r| Json::from(*r))))]);
    let locale = || s(lingua::locale::MAX_LOCALE_BYTES, "A locale id: en, de, pt-BR, zh-Hant-TW; qps-ploc is the pseudo-locale (made from the source)");
    let locales = || Json::obj([("type", Json::from("array")), ("maxItems", Json::from(MAX_CHECK_LOCALES)), ("uniqueItems", Json::Bool(true)), ("items", locale()), ("description", Json::from("Locales to check (default: every table's and the pseudo-locale)"))]);
    vec![
        ToolDef {
            name: "l10n_get".into(),
            description: "String tables (localisation; project files assets/strings/<locale>.json, kind strings: format specula.strings.v1). \
                          Without 'locale': every table {locale, name, path, version, keys, translated, missing, unreviewed, reviewed, stale}, the \
                          source locale (English), the game's locale and tables that don't parse. With 'locale': its entries resolved through the \
                          locale, its language, then the source {key, text (a string, or {plural: count, one, few, many, other}), from (the table \
                          it came from), status (source|unreviewed|reviewed), stale (translated from other source words), source (the source's \
                          words), notes, placeholders}, filtered by 'keys', 'prefix' and 'status', paged by cursor; with 'args' (and keys) each \
                          'preview' is the text filled in as players see it (plural form and number format of the locale). Texts use the UI \
                          template syntax: {name}, {name:.N}, {name:time}, {name:pct}, {name:plain}; screens show an entry with \"@key\"."
                .into(),
            input_schema: schema(
                vec![
                    ("locale", locale()),
                    ("keys", keys(MAX_KEYS_ARG, "Only these keys")),
                    ("prefix", s(lingua::table::MAX_KEY_BYTES, "Only keys starting with this (hud., stone.)")),
                    ("status", s(16, "all (default), missing (shows the source's words), translated, unreviewed, reviewed, stale")),
                    ("cursor", int(0, lingua::table::MAX_KEYS as u64, "next_cursor from the previous page")),
                    ("limit", int(1, GET_LIMITS.1 as u64, "Entries per page (default 50)")),
                    ("args", t("object", "Placeholder values for previews: {n: 3, name: \"Ada\"} (needs keys)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "l10n_set".into(),
            description: "Writes entries into one locale's string table in one commit, through project_write (claims on \
                          /project/<path>, the journal, disk, world_undo, hot reload into the game). entries: {key: \"text\" | {text, notes?} | \
                          {plural: \"n\", one, few, many, other, notes?}}. A translation's key must be in the source table and use the source's \
                          placeholders (and its plural count); it is stored unreviewed with the hash of the source words it translates (src), so \
                          a later source change makes it stale. remove: keys to delete from this table. review: keys to mark reviewed (reviewer \
                          authority; refused for stale ones). create: {name} makes the table assets/strings/<locale>.json (name: the language's own \
                          name); with source: true (and no source yet) it is the source table. At most 200 edits per call; tables are written one \
                          string per line."
                .into(),
            input_schema: schema(
                vec![
                    ("locale", locale()),
                    ("entries", t("object", "key: text, or {text | plural forms, notes?}")),
                    ("remove", keys(MAX_SET, "Keys to delete from this table")),
                    ("review", keys(MAX_SET, "Keys to mark reviewed (reviewer authority)")),
                    ("create", Json::obj([("type", Json::from("object")), ("properties", Json::obj([("name", s(lingua::table::MAX_NAME_BYTES, "The language's own name (Deutsch, 日本語)"))])), ("description", Json::from("Make this locale's table"))])),
                    ("source", t("boolean", "With create and no source table yet: this is the source (English)")),
                    ("expect_version", int(0, u64::MAX >> 11, "Only if the table is at this version (0: it must not exist)")),
                ],
                &["locale"],
            ),
        },
        ToolDef {
            name: "l10n_check".into(),
            description: "Checks localisation. The tables: missing (a locale lacks a source key: English shows), unknown_key, placeholders \
                          (a translation's placeholders aren't the source's: error), placeholder_format, plural_forms (a form the locale's CLDR \
                          rules use is missing), plural_count, stale (translated from older source words), empty, invalid_table; against the \
                          screens' \"@key\" texts and the game's code: undefined (a used key the source lacks: error), unused (when the game lists \
                          its keys), hardcoded (words a screen writes itself). With ui (default true): the game's interface laid out in each \
                          locale (and the pseudo-locale, 35% longer) at every target and scale asked (ui_check's rules: text_overflow, \
                          missing_glyph, overlap, offscreen, missing_key...), per screen group. Answers {ok, tables {errors, warnings, by_rule, \
                          findings}, locales (each table's counts), ui {locales [{locale, errors, warnings, by_rule, findings}]}}."
                .into(),
            input_schema: schema(
                vec![
                    ("locales", locales()),
                    ("ui", t("boolean", "Lay the game's interface out in each locale (default true)")),
                    ("groups", Json::obj([("type", Json::from("array")), ("maxItems", Json::from(MAX_GROUPS)), ("items", Json::obj([("type", Json::from("array")), ("maxItems", Json::from(crate::ui_tools::MAX_SCREENS)), ("items", s(64, "A screen name"))])), ("description", Json::from("Screen groups shown together, each checked (default: the screens showing now)"))])),
                    ("targets", Json::obj([("type", Json::from("array")), ("maxItems", Json::from(8)), ("description", Json::from("ui_check's targets: presets (deck, 1080p, 4k, ultrawide, 16:10, 4:3, 1440p, phone) or displays (default deck, 1080p, 4k)"))])),
                    ("scales", Json::obj([("type", Json::from("array")), ("maxItems", Json::from(crate::ui_tools::MAX_SCALES)), ("items", Json::obj([("type", Json::from("number")), ("minimum", Json::from(0.5)), ("maximum", Json::from(3))])), ("description", Json::from("Player UI-scale settings per target (default [1])"))])),
                    ("rules", Json::obj([("type", Json::from("array")), ("maxItems", Json::from(crate::ui_tools::RULES.len())), ("items", Json::obj([("type", Json::from("string")), ("enum", Json::arr(crate::ui_tools::RULES.map(Json::from)))])), ("description", Json::from("Only these ui_check rules"))])),
                    ("max_findings", int(1, MAX_FINDINGS as u64, "Findings quoted per part (default 50; counts cover all)")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "l10n_tasks".into(),
            description: "The translation flow: posts one board plan (task_plan) with, per locale with work, a task to translate its missing \
                          and stale strings (role translator) and a task to review them after (role reviewer). dry_run shows the plan. Each task's \
                          detail says which l10n_get / l10n_set / l10n_check calls do it."
                .into(),
            input_schema: schema(
                vec![
                    ("locales", Json::obj([("type", Json::from("array")), ("maxItems", Json::from(MAX_TABLES)), ("uniqueItems", Json::Bool(true)), ("items", locale()), ("description", Json::from("Only these locales"))])),
                    ("dry_run", t("boolean", "Show the plan, post nothing")),
                    ("assign", s(16, "\"by_role\" gives each task to connected agents of that role, or \"none\" (default)")),
                ],
                &[],
            ),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str) -> SessionInfo {
        SessionInfo { id: id.into(), client_name: "t".into(), client_version: "1".into(), protocol_version: "x".into() }
    }

    const DE: &str = r#"{"format": "specula.strings.v1", "locale": "de", "strings": {"a": {"text": "A", "status": "reviewed"}, "b": "B"}}"#;

    #[test]
    fn marking_strings_reviewed_needs_a_reviewer() {
        let s = session("l10n-gate-agent");
        // Unchanged reviewed entries pass; a newly reviewed one is refused for an agent.
        assert!(review_gate(&s, Some(DE), Some(DE)).is_ok());
        let forged = DE.replace(r#""b": "B""#, r#""b": {"text": "B", "status": "reviewed"}"#);
        let out = review_gate(&s, Some(DE), Some(&forged)).unwrap_err();
        assert!(out.is_error);
        // Changing a reviewed text keeps it reviewed only for a reviewer.
        let changed = DE.replace(r#""text": "A""#, r#""text": "AA""#);
        assert!(review_gate(&s, Some(DE), Some(&changed)).is_err());
        // A table-wide "status": "reviewed" reviews every plain entry.
        let all = DE.replace(r#""locale": "de","#, r#""locale": "de", "status": "reviewed","#);
        assert!(review_gate(&s, Some(DE), Some(&all)).is_err());
        // Review finding 7: src is l10n_set's to write; editing it by hand hides staleness.
        let with_src = DE.replace(r#""b": "B""#, r#""b": {"text": "B", "src": "0011223344556677"}"#);
        assert!(review_gate(&s, Some(DE), Some(&with_src)).is_err(), "adding a src by hand");
        let moved = with_src.replace("0011223344556677", "8899aabbccddeeff");
        assert!(review_gate(&s, Some(&with_src), Some(&moved)).is_err(), "changing a src by hand");
        assert!(review_gate(&s, Some(&with_src), Some(DE)).is_err(), "dropping a src by hand");
        WRITING_SRC.with(|c| c.set(true));
        assert!(review_gate(&s, Some(DE), Some(&with_src)).is_ok(), "l10n_set stamps it");
        WRITING_SRC.with(|c| c.set(false));
        // Review R2, major 2: a reviewed table relabelled as another language (or as the
        // source) carries no reviews over; with nothing reviewed in it, a relabel is ordinary.
        let relabelled = DE.replace(r#""locale": "de""#, r#""locale": "fr""#);
        assert!(relabelled != DE && Table::parse(&relabelled).is_ok());
        let out = review_gate(&s, Some(DE), Some(&relabelled)).unwrap_err();
        assert!(out.is_error);
        let as_source = DE.replace(r#""locale": "de","#, r#""locale": "de", "source": true,"#);
        // (A table with reviewed entries can't be made the source at all: the format refuses a
        // status in the source table, and the engine saves no string table that doesn't parse.)
        assert!(Table::parse(&as_source).is_err());
        let plain = DE.replace(r#"{"text": "A", "status": "reviewed"}"#, r#""A""#);
        assert!(Table::parse(&plain).is_ok_and(|t| t.entries.values().all(|e| e.status != Status::Reviewed)), "the fixture's one reviewed entry");
        assert!(review_gate(&s, Some(&plain), Some(&plain.replace(r#""locale": "de""#, r#""locale": "fr""#))).is_ok());
        // Unparseable text is the validator's business (which never lets one be saved).
        assert!(review_gate(&s, Some(DE), Some("{")).is_ok());
        assert!(crate::project_kinds::never_allowed(crate::project::FileKind::Strings, "invalid_json"), "no allow_errors drafts of string tables");
        // A reviewer may.
        crate::authority::publish("l10n-gate-reviewer", crate::authority::Authority::Reviewer);
        assert!(review_gate(&session("l10n-gate-reviewer"), Some(DE), Some(&forged)).is_ok());
    }

    /// The tables' one form: the committed source table is already in it (a rewrite changes no
    /// byte), and a new table starts with an empty strings object.
    #[test]
    fn tables_are_written_one_string_per_line() {
        let en = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../games/sepulcra/assets/strings/en.json")).unwrap().replace("\r\n", "\n");
        assert_eq!(render_table(&Json::parse(&en).unwrap()), en);
        let empty = render_table(&Table::empty_json(&Locale::parse("de").unwrap(), "Deutsch"));
        assert!(empty.ends_with("  \"strings\": {}\n}\n"), "{empty}");
        assert!(Table::parse(&empty).is_ok());
        let big = Json::obj([("format", Json::from(lingua::table::FORMAT)), ("locale", Json::from("de")), ("strings", Json::obj([("a", Json::from("x\ny \"q\"")), ("b", Json::obj([("text", Json::from("ü")), ("src", Json::from("0011223344556677"))]))]))]);
        let t = render_table(&big);
        assert_eq!(t.lines().filter(|l| l.starts_with("    \"")).count(), 2, "{t}");
        assert_eq!(Json::parse(&t).unwrap(), big);
    }

    #[test]
    fn translations_must_use_the_sources_placeholders() {
        let src = Text::One("Wave {wave}/{waves}".into());
        assert!(same_holes(&src, &Text::One("Welle {waves}: {wave}".into()), "e").is_ok(), "order may change");
        let e = same_holes(&src, &Text::One("Welle {welle}/{waves}".into()), "entries.a").unwrap_err();
        assert_eq!((e.code, e.field.as_str()), ("placeholders", "entries.a"));
        assert!(e.message.contains("{wave}") && e.message.contains("{welle}"));
        let p = |c: &str| Text::Plural { count: c.into(), forms: [None, Some("{n} x".into()), None, None, None, Some("{n} y".into())] };
        assert!(same_holes(&p("n"), &p("n"), "e").is_ok());
        let src2 = Text::Plural { count: "n".into(), forms: [None, Some("{n} {m}".into()), None, None, None, Some("{n} {m}".into())] };
        let tr2 = Text::Plural { count: "m".into(), forms: [None, Some("{n} {m}".into()), None, None, None, Some("{n} {m}".into())] };
        assert_eq!(same_holes(&src2, &tr2, "e").unwrap_err().code, "plural_count");
    }

    #[test]
    fn screens_are_scanned_for_keys_and_written_words() {
        let screen = Json::parse(r#"{"root": {"type": "box", "children": [{"type": "label", "text": "@hud.wave"}, {"type": "label", "text": "Wave {wave}"}, {"type": "label", "text": "{dust}"}, {"type": "label", "text": "@@handle"}, {"type": "button", "id": "x", "text": "@hud.menu", "tooltip": "Menu"}]}}"#).unwrap();
        let mut used = Used::default();
        let mut hard = Vec::new();
        walk(&screen, "assets/ui/hud.json", "", &mut used, &mut hard);
        assert_eq!(used.keys.keys().cloned().collect::<Vec<_>>(), ["hud.menu", "hud.wave"]);
        assert_eq!(hard.len(), 3, "Wave, @@handle and the tooltip: {hard:?}");
        assert!(hard[0].message.contains("root.children[1].text"), "{}", hard[0].message);
    }

    #[test]
    fn tables_load_from_files_and_build_ui_strings() {
        let en = r#"{"format": "specula.strings.v1", "locale": "en", "source": true, "strings": {"a": "A {n}", "b": "B"}}"#;
        let t = Tables::from_files(vec![
            ("assets/strings/en.json".into(), 1, Arc::from(en)),
            ("assets/strings/de.json".into(), 3, Arc::from(DE)),
            ("assets/strings/fr.json".into(), 2, Arc::from("{")),
        ]);
        assert_eq!((t.files.len(), t.broken.len()), (2, 1));
        assert_eq!(t.source().ok().map(|s| s.path.as_str()), Some("assets/strings/en.json"));
        let (s, pseudo) = t.ui_strings(&Locale::parse("de").unwrap()).ok().unwrap();
        assert!(!pseudo);
        assert_eq!(s.locale().as_str(), "de");
        let (p, pseudo) = t.ui_strings(&Locale::parse("qps-ploc").unwrap()).ok().unwrap();
        assert!(pseudo && p.locale().as_str() == "en", "the pseudo-locale is the source, turned pseudo by ui_check");
        assert!(t.ui_strings(&Locale::parse("ja").unwrap()).is_err(), "no table for ja");
        let o = overview(&t, None);
        assert_eq!(o.get("source").and_then(Json::as_str), Some("en"));
        assert_eq!(o.get("broken").and_then(Json::as_array).map(<[Json]>::len), Some(1));
    }
}
