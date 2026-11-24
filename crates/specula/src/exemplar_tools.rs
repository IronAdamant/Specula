//! The library of sample assets in the engine (docs/design/EXEMPLARIA.md, A.8 and A.9): the
//! read-only tools `exemplar_list` (the catalogue) and `exemplar_get` (one kind, and with a seed
//! the specimen it draws), the writing tool `exemplar_take` (a copy of a kind into the project),
//! and what the model library and the project's validation call to read a `*.kind.json` as a
//! recipe ([`expand_for_build`], [`check_kind`]).
//!
//! **Where the work runs.** All three tools are answered on the calling agent's own protocol
//! thread (`CallerThread::Exemplar`), never on the engine thread: the library is built into the
//! engine and pure. `exemplar_take` writes through `project_write` (the project tools' rule of
//! the write policy, claim-checked and journaled as the caller; one commit). A kind is expanded
//! to its recipe on the model library's build worker ([`expand_for_build`]), and checked at the
//! mean where a recipe is parsed (a few dozen bones and parts: bounded by the format's ceilings).
//!
//! **Bounds.** Every argument has a limit in its schema; the first answer of `exemplar_list` and
//! the short answer of `exemplar_get` are under [`MAX_INDEX_BYTES`] and [`MAX_SHORT_BYTES`] (held
//! by tests), a page of rows under [`MAX_ROWS_BYTES`], anything else under [`MAX_ANSWER_BYTES`].

use crate::catalog_names;
use crate::tools::{argument_error, check_keys, int_arg, string_arg, Tools};
use specula_core::Json;
use specula_exemplaria::library::{self, Entry};
use specula_exemplaria::kind::Taken;
use specula_exemplaria::{expand, AsNamed, Kind, Resolved, Roles, Why};
use specula_model::{ErrorCode, RecipeError, Style};
use specula_protocol::{SessionInfo, ToolDef, ToolOutput};
use std::collections::BTreeMap;

/// The ceiling of the first answer (`exemplar_list` with nothing asked), bytes.
pub const MAX_INDEX_BYTES: usize = 3 * 1024;
/// What `exemplar_get` answers without `full` stays under, bytes.
pub const MAX_SHORT_BYTES: usize = 3 * 1024;
/// A page of rows' ceiling, bytes.
pub const MAX_ROWS_BYTES: usize = 8 * 1024;
/// The ceiling of any answer, bytes.
pub const MAX_ANSWER_BYTES: usize = 64 * 1024;
/// Rows a page (default, most).
pub const LIMITS: (usize, usize) = (10, 100);
/// The longest word or name argument.
pub const MAX_WORD: usize = 64;
/// The folder of the project's assets a kind is taken into.
pub const TAKE_FOLDER: &str = "assets/models";

/// A string argument of at most `max` bytes.
fn text(args: &Json, key: &str, max: usize) -> Result<Option<String>, String> {
    match string_arg(args, key)? {
        Some(s) if s.len() > max => Err(format!("'{key}' is {} bytes; at most {max}", s.len())),
        s => Ok(s.map(str::to_string)),
    }
}

fn flag(args: &Json, key: &str) -> Result<bool, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(false),
        Some(v) => v.as_bool().ok_or_else(|| format!("'{key}' is true or false")),
    }
}

/// Answers an `exemplar_*` tool call (on the calling agent's protocol thread).
pub fn call(tools: &Tools, session: &SessionInfo, name: &str, args: &Json) -> ToolOutput {
    let out = match name {
        "exemplar_list" => list_tool(args),
        "exemplar_get" => get_tool(args),
        "exemplar_take" => return take_tool(tools, session, args).unwrap_or_else(|m| ToolOutput::error(argument_error(&m).to_pretty())),
        other => Err(format!("unknown exemplar tool '{other}'")),
    };
    match out {
        Ok(j) => ToolOutput::text(j.to_string()),
        Err(m) => ToolOutput::error(argument_error(&m).to_pretty()),
    }
}

/// A word looked up in the library (a name or an alias), or the nearest names across every
/// list an agent meets (shapes too).
fn look_up(word: &str) -> Result<&'static Entry, String> {
    library::find(word).ok_or_else(|| {
        let all = catalog_names::sources();
        let near = catalog_names::nearest(word, &all, catalog_names::MAX_NEAREST);
        let said = if near.is_empty() {
            format!("nothing near it; look in {}", catalog_names::also(&all, ""))
        } else {
            near.iter().map(|n| format!("{} (a {}: {})", n.name, n.what, n.call)).collect::<Vec<_>>().join("; ")
        };
        // (The field first in quotes, so the error names it; the code at the end.)
        format!("'name' names no kind: no kind is called \"{word}\"; nearest across the lists: {said} [error: not_found]")
    })
}

/// The snippets that use a kind once it is in the project.
fn after_take(name: &str, seed: u64) -> Json {
    let recipe = format!("models/{name}.kind.json");
    Json::obj([
        ("take", Json::from(format!("exemplar_take {{\"name\": \"{name}\"}}: a copy into {TAKE_FOLDER}/{name}.kind.json"))),
        ("build", Json::obj([("recipe", Json::from(recipe.as_str())), ("seed", Json::from(seed))])),
        ("spawn", Json::obj([("shape", Json::from("model")), ("recipe", Json::from(recipe.as_str())), ("seed", Json::from(seed))])),
    ])
}

fn why_json(w: &Why) -> (&'static str, Json) {
    match w {
        Why::Source(s) => ("source", Json::from(s.as_str())),
        Why::Own(s) => ("own", Json::from(s.as_str())),
    }
}

/// One row of the catalogue: what the kind is, its size and roles, and what to paste.
fn row(e: &Entry, r: &Resolved) -> Json {
    let k = &r.kind;
    let size = r.plan.size();
    let (mean, unit) = k.size.as_ref().map_or((size.mean, size.unit.name().to_string()), |s| (s.mean, s.unit.clone()));
    let mut o = vec![
        ("name", Json::from(k.name.as_str())),
        ("plan", Json::from(r.plan.name())),
        ("about", Json::from(k.about.as_str())),
        ("size", Json::from(format!("{} {mean} {unit}", size.name))),
        ("roles", Json::from(k.roles.iter().map(|(n, h)| format!("{n} {h}")).collect::<Vec<_>>().join(", "))),
        ("clips", Json::from(k.motion.keys().cloned().collect::<Vec<_>>().join(", "))),
    ];
    if !k.aliases.is_empty() {
        o.push(("aliases", Json::from(k.aliases.join(", "))));
    }
    o.push(("own_numbers", Json::from(r.own().len())));
    o.push(("from", Json::from(e.from())));
    o.push(("use", after_take(&k.name, 1)));
    Json::obj(o)
}

/// The first answer: every kind by plan, a line a plan, the other lists, and how to ask.
fn index() -> Json {
    let mut by_plan: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for e in library::ENTRIES {
        if let Ok(k) = e.kind() {
            by_plan.entry(specula_exemplaria::plan(&k.plan).map_or("?", |p| p.name())).or_default().push(k.name);
        }
    }
    let plans = specula_exemplaria::plans::PLANS;
    let answer = |cut: Option<usize>| {
        let lines: Vec<Json> = plans
            .iter()
            .map(|p| {
                let kinds = by_plan.get(p.name()).cloned().unwrap_or_default();
                let shown = match cut {
                    Some(n) if kinds.len() > n => format!("{} +{} more", kinds[..n].join(" "), kinds.len() - n),
                    _ => kinds.join(" "),
                };
                Json::from(format!("{} ({}): {}", p.name(), p.about(), if shown.is_empty() { "no kinds yet".to_string() } else { shown }))
            })
            .collect();
        Json::obj([
            ("kinds", Json::from(library::ENTRIES.len())),
            // (The other lists first: an agent that wanted a shape is one call from it.)
            ("also", Json::from(format!("also: {}", catalog_names::also(&catalog_names::sources(), "exemplar_list")))),
            ("plans", Json::arr(lines)),
            (
                "ask",
                Json::from(
                    "exemplar_get {\"name\": NAME}: one kind, its numbers and where each came from. With \"seed\": N, the specimen that seed draws (its numbers, bones, parts, clips). \
                     exemplar_take {\"name\": NAME}: a copy into your project (assets/models/NAME.kind.json, where the project keeps its models), then model_build {\"recipe\": \"models/NAME.kind.json\", \"seed\": N} \
                     or spawn {\"shape\": \"model\", \"recipe\": ..., \"seed\": N}: each seed is another specimen. exemplar_list {\"plan\" | \"word\" | \"role\"}: rows.",
                ),
            ),
        ])
    };
    let fits = |j: &Json| j.to_string().len() <= MAX_INDEX_BYTES;
    let widest = by_plan.values().map(Vec::len).max().unwrap_or(0);
    let all = answer(None);
    if fits(&all) {
        return all;
    }
    for n in (0..widest).rev() {
        let cut = answer(Some(n));
        if fits(&cut) {
            return cut;
        }
    }
    answer(Some(0))
}

fn list_tool(args: &Json) -> Result<Json, String> {
    check_keys(args, "exemplar_list", &[&["plan", "word", "role", "cursor", "limit", "rows"]])?;
    let rows_asked = flag(args, "rows")?;
    let filtered = ["plan", "word", "role", "cursor", "limit"].iter().any(|k| args.get(k).is_some_and(|v| !v.is_null()));
    if !filtered && !rows_asked {
        return Ok(index());
    }
    let plan = text(args, "plan", MAX_WORD)?;
    let word = text(args, "word", MAX_WORD)?.map(|w| w.to_ascii_lowercase());
    let role = text(args, "role", MAX_WORD)?;
    let limit = int_arg(args, "limit", 1, LIMITS.1 as u64)?.map_or(LIMITS.0, |l| l as usize);
    let cursor = match text(args, "cursor", 16)? {
        None => 0,
        Some(c) => c.parse::<usize>().map_err(|_| "'cursor' is the next_cursor of the page before".to_string())?,
    };
    if let Some(p) = &plan {
        if specula_exemplaria::plan(p).is_none() {
            let names: Vec<&str> = specula_exemplaria::plans::PLANS.iter().map(|x| x.name()).collect();
            return Err(format!("no plan is called '{p}' (plans: {}; nearest: {})", names.join(", "), specula_exemplaria::words::nearest(p, names.iter().copied(), 3).join(", ")));
        }
    }
    let mut matched = Vec::new();
    for e in library::ENTRIES {
        let Ok(r) = e.resolved() else { continue };
        let k = &r.kind;
        let ok = plan.as_deref().is_none_or(|p| r.plan.name() == p)
            && role.as_deref().is_none_or(|x| k.roles.contains_key(x))
            && word.as_deref().is_none_or(|w| k.name.contains(w) || k.about.to_ascii_lowercase().contains(w) || k.aliases.iter().any(|a| a.contains(w)));
        if ok {
            matched.push((e, r));
        }
    }
    let mut rows = Vec::new();
    let mut bytes = 0;
    let mut next = None;
    for (i, (e, r)) in matched.iter().enumerate().skip(cursor) {
        let j = row(e, r);
        let n = j.to_string().len();
        if rows.len() >= limit || bytes + n > MAX_ROWS_BYTES {
            next = Some(i);
            break;
        }
        bytes += n;
        rows.push(j);
    }
    let mut out = vec![("kinds", Json::arr(rows)), ("matched", Json::from(matched.len())), ("of", Json::from(library::ENTRIES.len()))];
    if matched.is_empty() {
        if let Some(w) = &word {
            out.push(("elsewhere", catalog_names::not_found(w, &catalog_names::sources())));
        }
    }
    out.push(("next", Json::from("exemplar_get {\"name\": NAME, \"seed\": N} for one kind and the specimen a seed draws")));
    if let Some(n) = next {
        out.push(("next_cursor", Json::from(n.to_string())));
    }
    Ok(Json::obj(out))
}

/// A seed argument: 0..=the model library's most.
fn seed_arg(args: &Json) -> Result<Option<u64>, String> {
    int_arg(args, "seed", 0, crate::world::MAX_MODEL_SEED)
}

fn get_tool(args: &Json) -> Result<Json, String> {
    check_keys(args, "exemplar_get", &[&["name", "seed", "full"]])?;
    let name = text(args, "name", MAX_WORD)?.ok_or("'name' (a kind's name or alias) is required")?;
    let seed = seed_arg(args)?;
    let full = flag(args, "full")?;
    let entry = look_up(&name)?;
    let r = entry.resolved().map_err(|e| format!("the library's {} does not resolve: {e}", entry.path))?;
    let k = &r.kind;
    let plan = r.plan;
    let mut out: Vec<(&'static str, Json)> = vec![("name", Json::from(k.name.as_str()))];
    if name != k.name {
        out.push(("alias_of", Json::from(k.name.as_str())));
    }
    out.push(("plan", Json::from(format!("{}: {}", plan.name(), plan.about()))));
    out.push(("about", Json::from(k.about.as_str())));
    let sdef = plan.size();
    out.push((
        "size",
        Json::from(match &k.size {
            Some(s) => format!(
                "{} mean {} {}{}{}; {}",
                s.name,
                s.mean,
                s.unit,
                s.sd.map_or(String::new(), |x| format!(", sd {x}")),
                s.range.map_or(String::new(), |[a, b]| format!(", range {a}..{b}")),
                if s.why.is_own() { "the engine's own" } else { "measured" }
            ),
            None => format!("{} mean {} {} (the plan's default)", sdef.name, sdef.mean, sdef.unit.name()),
        }),
    ));
    let own = r.own();
    out.push(("numbers", Json::from(format!("{} set by the kind, {} measured, {} the engine's own; {} at the plan's default share", k.numbers.len(), k.numbers.len() - own.iter().filter(|o| o.0.starts_with("numbers.")).count(), own.iter().filter(|o| o.0.starts_with("numbers.")).count(), r.defaulted().len()))));
    out.push(("vary", Json::from(plan.traits().iter().map(|t| format!("{} {}", t.name, k.vary.get(t.name).map_or(t.spread, |v| v.spread))).collect::<Vec<_>>().join(", "))));
    out.push(("roles", Json::from(k.roles.iter().map(|(n, h)| format!("{n} {h}")).collect::<Vec<_>>().join(", "))));
    out.push(("clips", Json::from(k.motion.keys().cloned().collect::<Vec<_>>().join(", "))));
    out.push(("own", Json::from(if own.is_empty() { "none: every number is measured".to_string() } else { own.iter().map(|o| o.0.as_str()).collect::<Vec<_>>().join(", ") })));
    out.push(("from", Json::from(entry.from())));
    let x = expand(&r, seed, &AsNamed, &entry.from()).map_err(|e| format!("{e}"))?;
    if let Some(s) = seed {
        let drawn: Vec<String> = std::iter::once(&x.specimen.size).chain(&x.specimen.numbers).map(|v| format!("{}={}", v.name, v.value)).collect();
        let mut sp = vec![
            ("seed", Json::from(s)),
            ("drawn", Json::from(drawn.join(" "))),
            ("traits", Json::from(x.specimen.traits.iter().map(|(t, v)| format!("{t} {v:+}")).collect::<Vec<_>>().join(", "))),
            ("written", Json::from(format!("{} bones, {} parts, {} clips; recipe {} bytes", x.counts.bones, x.counts.parts, x.counts.clips, x.recipe.len()))),
            ("facts", Json::from(x.facts.iter().map(|(n, v, u)| format!("{n} {v:.4} {u}")).collect::<Vec<_>>().join(", "))),
        ];
        if !x.specimen.assumed.is_empty() {
            sp.push(("assumed", Json::arr(x.specimen.assumed.iter().map(|a| Json::from(a.as_str())))));
        }
        out.push(("specimen", Json::obj(sp)));
    }
    out.push(("use", after_take(&k.name, seed.unwrap_or(1))));
    if !full {
        out.push(("full", Json::from("\"full\": true adds every number with its source or reason, the kind file, and the recipe and clips the seed writes (roles named as roles: your style.json resolves them when it builds)")));
        let answer = Json::obj(out);
        if answer.to_string().len() > MAX_SHORT_BYTES {
            return Err(format!("the short answer for '{}' would pass {MAX_SHORT_BYTES} bytes: ask with \"full\": true", k.name));
        }
        return Ok(answer);
    }
    let mut numbers: Vec<Json> = Vec::new();
    for n in plan.numbers() {
        let mut o = vec![("name", Json::from(n.name)), ("unit", Json::from(n.unit.name())), ("about", Json::from(n.about))];
        match k.numbers.get(n.name) {
            Some(set) => {
                if let Some(s) = set.share {
                    o.push(("share", Json::from(s)));
                }
                if let Some(m) = set.mean {
                    o.push(("mean", Json::from(m)));
                }
                if let Some(sp) = set.spread {
                    o.push(("spread", Json::from(sp)));
                }
                o.push(why_json(&set.why));
            }
            None => {
                o.push(("share", Json::from(n.share)));
                o.push(("own", Json::from("the plan's default")));
            }
        }
        if !n.traits.is_empty() {
            o.push(("moved_by", Json::from(n.traits.iter().map(|(t, w)| format!("{t} x{w}")).collect::<Vec<_>>().join(", "))));
        }
        numbers.push(Json::obj(o));
    }
    out.push(("number_rows", Json::arr(numbers)));
    out.push(("kind_file", Json::from(entry.text)));
    out.push(("recipe", Json::from(x.recipe.as_str())));
    if let Some(a) = &x.anims {
        out.push(("anims", Json::from(a.as_str())));
    }
    let answer = Json::obj(out);
    if answer.to_string().len() > MAX_ANSWER_BYTES {
        return Err(format!("the answer for '{}' would pass {MAX_ANSWER_BYTES} bytes", k.name));
    }
    Ok(answer)
}

/// What `exemplar_take` writes.
#[derive(Debug)]
struct Copy {
    entry: &'static Entry,
    as_name: String,
    /// Its project path (`assets/models/NAME.kind.json`).
    path: String,
    body: String,
    /// The library's kind had a base, now written into the copy.
    based: bool,
}

/// The copy of a library kind a project takes: the kind with its base under it (one file, so one
/// commit), named `as_name`, with where it came from.
fn copy_of(name: &str, as_name: Option<&str>) -> Result<Copy, String> {
    let entry = look_up(name)?;
    let r = entry.resolved().map_err(|e| format!("the library's {} does not resolve: {e}", entry.path))?;
    let as_name = as_name.map_or_else(|| r.kind.name.clone(), str::to_string);
    if !specula_exemplaria::words::valid_name(&as_name) {
        return Err(format!("'as' is a kind's name in the project: a lowercase letter, then lowercase letters, digits and _ (at most {})", specula_exemplaria::words::MAX_NAME));
    }
    let mut copy: Kind = r.kind.clone();
    copy.name = as_name.clone();
    copy.base = None;
    copy.aliases.retain(|a| *a != as_name);
    copy.from = Some(entry.from());
    let checked = checked_at_take(&r, &entry.from())?;
    let own = u32::try_from(r.own().len()).unwrap_or(u32::MAX);
    copy.taken = Some(Taken { library: format!("{:016x}", library::version()), base: r.base.clone(), checked, own });
    let body = copy.to_json().to_pretty() + "\n";
    Ok(Copy { entry, path: format!("{TAKE_FOLDER}/{as_name}.kind.json"), as_name, body, based: r.base.is_some() })
}

/// The seeds a take writes and checks before it is written (the mean and these).
const TAKE_SEEDS: [u64; 3] = [1, 2, 3];

/// What a take checks of a kind, and says it checked in the copy: written at its mean and at
/// [`TAKE_SEEDS`], each recipe parsed (its roles as materials of their hint colours) and its clips
/// generated on its rig. A few dozen bones and parts a specimen: bounded by the format.
fn checked_at_take(r: &Resolved, from: &str) -> Result<String, String> {
    let materials: Vec<(String, Json)> = r.kind.roles.iter().map(|(n, h)| (n.clone(), Json::obj([("color", Json::from(h.as_str()))]))).collect();
    let style = Style::parse(&Json::obj([("materials", Json::obj(materials))]).to_string()).map_err(|e| format!("the kind's roles as a style: {}", e.message))?;
    for seed in std::iter::once(None).chain(TAKE_SEEDS.map(Some)) {
        let at = seed.map_or("its mean".to_string(), |s| format!("seed {s}"));
        let x = expand(r, seed, &AsNamed, from).map_err(|e| format!("the library's kind does not write at {at}: {e}"))?;
        let recipe = specula_model::Recipe::parse(&x.recipe, &style).map_err(|e| format!("the library's kind writes a recipe that does not parse at {at}: {e}"))?;
        if let Some(anims) = &x.anims {
            let params = specula_model::BuildParams { seed, ..specula_model::BuildParams::new(&style) };
            let rig = specula_model::recipe::build_rig_only(&recipe, &params).map_err(|e| format!("its rig at {at}: {e}"))?.ok_or_else(|| format!("its clips have no rig at {at}"))?;
            specula_anim::Skeleton::from_rig(&rig).and_then(|s| specula_anim::AnimSet::load(anims, s)).map_err(|e| format!("its clips at {at}: {e}"))?;
        }
    }
    Ok(format!("at the take: written at its mean and at seeds {}; each recipe parsed and its clips generated on its rig", TAKE_SEEDS.map(|s| s.to_string()).join(", ")))
}

/// The text of a tool's answer, as JSON.
fn answer_json(out: &ToolOutput) -> Option<Json> {
    out.content.iter().find_map(|c| match c {
        specula_protocol::Content::Text(t) => Json::parse(t).ok(),
        specula_protocol::Content::Image { .. } => None,
    })
}

/// A take refused because the project's style resolves some of the kind's roles to nothing (a
/// new project's style has none of them) gets the one call that adds them all, each to the
/// preset nearest in colour to the role's hint: `project_patch` of the style the copy is checked
/// against, at its version. Without it an agent learns one role a refusal (the check stops at
/// the first). Nothing is written here: the agent makes the patch, and may choose other presets.
fn with_roles_fix(tools: &Tools, session: &SessionInfo, out: ToolOutput, roles: &BTreeMap<String, String>) -> ToolOutput {
    let Some(mut answer) = answer_json(&out) else { return out };
    if !answer.to_string().contains("role_unresolved") {
        return out;
    }
    // The style a recipe in the take folder is checked against: the nearest above it.
    let mut found = None;
    let mut dir = TAKE_FOLDER.to_string();
    loop {
        let path = format!("{dir}/style.json");
        let read = crate::project_tools::call(tools, session, "project_read", &Json::obj([("path", Json::from(path.as_str()))]));
        if !read.is_error {
            found = answer_json(&read).map(|a| (path, a));
            break;
        }
        match dir.rsplit_once('/') {
            Some((up, _)) => dir = up.to_string(),
            None => break,
        }
    }
    let Some((path, read)) = found else { return out };
    let (Some(text), Some(version)) = (read.get("text").and_then(Json::as_str), read.get("version").and_then(Json::as_u64)) else { return out };
    let Ok(style) = Style::parse(text) else { return out };
    let ops: Vec<Json> = roles
        .iter()
        .filter_map(|(role, hint)| style.role(role, Some(hint)).err().and_then(|m| m.nearest.into_iter().next()).map(|p| Json::obj([("set", Json::from(format!("roles.{role}"))), ("value", Json::from(p))])))
        .collect();
    if ops.is_empty() {
        return out;
    }
    answer.set(
        "fix",
        Json::obj([
            ("why", Json::from(format!("{path} resolves {} of the kind's roles to no preset: this one call names each to the preset nearest its hint colour (choose others if you like); then exemplar_take again", ops.len()))),
            ("call", Json::from("project_patch")),
            ("args", Json::obj([("path", Json::from(path.as_str())), ("expect_version", Json::from(version)), ("ops", Json::arr(ops))])),
        ]),
    );
    ToolOutput::error(answer.to_string())
}

/// `exemplar_take`: the copy ([`copy_of`]) through `project_write`.
fn take_tool(tools: &Tools, session: &SessionInfo, args: &Json) -> Result<ToolOutput, String> {
    check_keys(args, "exemplar_take", &[&["name", "as", "expect_version"]])?;
    let name = text(args, "name", MAX_WORD)?.ok_or("'name' (a kind's name or alias) is required")?;
    let as_name = text(args, "as", specula_exemplaria::words::MAX_NAME)?;
    let expect = int_arg(args, "expect_version", 0, u64::MAX >> 11)?.unwrap_or(0);
    let Copy { entry, as_name, path, body, based } = copy_of(&name, as_name.as_deref())?;
    let write = Json::obj([("path", Json::from(path.as_str())), ("text", Json::from(body.as_str())), ("expect_version", Json::from(expect))]);
    let out = crate::project_tools::call(tools, session, "project_write", &write);
    if out.is_error {
        let roles = entry.kind().map(|k| k.roles).unwrap_or_default();
        return Ok(with_roles_fix(tools, session, out, &roles));
    }
    let answer = answer_json(&out);
    let mut o = vec![("taken", Json::from(path.as_str())), ("from", Json::from(entry.from())), ("bytes", Json::from(body.len()))];
    if based {
        o.push(("base", Json::from("its base is written into the copy: one file holds the whole kind")));
    }
    for key in ["version", "journal", "unchanged"] {
        if let Some(v) = answer.as_ref().and_then(|a| a.get(key)) {
            o.push((key, v.clone()));
        }
    }
    let recipe = format!("models/{as_name}.kind.json");
    o.push(("build", Json::obj([("recipe", Json::from(recipe.as_str())), ("seed", Json::from(1))])));
    o.push(("next", Json::from(format!("model_build {{\"recipe\": \"{recipe}\", \"seed\": N}}: each seed another specimen (no seed: the mean); world_spawn {{\"shape\": \"model\", \"recipe\": \"{recipe}\", \"seed\": N}}; change the copy's numbers to change every specimen"))));
    Ok(ToolOutput::text(Json::obj(o).to_string()))
}

pub fn definitions() -> Vec<ToolDef> {
    let s = |max: usize, d: &str| Json::obj([("type", Json::from("string")), ("maxLength", Json::from(max)), ("description", Json::from(d))]);
    let int = |lo: u64, hi: u64, d: &str| Json::obj([("type", Json::from("integer")), ("minimum", Json::from(lo)), ("maximum", Json::from(hi)), ("description", Json::from(d))]);
    let boolean = |d: &str| Json::obj([("type", Json::from("boolean")), ("description", Json::from(d))]);
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| Json::obj([("type", Json::from("object")), ("properties", Json::obj(props)), ("required", Json::arr(required.iter().map(|r| Json::from(*r))))]);
    vec![
        ToolDef {
            name: "exemplar_list".into(),
            description: format!(
                "The library of sample assets: kinds (animals, creatures, machines, trees, plants...) made by code from measured numbers, each seed a different specimen \
                 (two seeds of a kind are two individuals). With nothing asked: every kind by plan on a few lines (under {MAX_INDEX_BYTES} bytes), the other \
                 lists (shapes), and how to ask. With plan, word or role, or rows: true: rows with each kind's size, roles, clips and the calls that use it; \
                 at most {} rows and {MAX_ROWS_BYTES} bytes a page, next_cursor for the next. Read-only.",
                LIMITS.1
            ),
            input_schema: schema(
                vec![
                    ("plan", s(MAX_WORD, "Only this plan's kinds (box_creature...: exemplar_list with nothing asked names them).")),
                    ("word", s(MAX_WORD, "Only kinds whose name, an alias or its line has this word.")),
                    ("role", s(MAX_WORD, "Only kinds drawn in this role (coat, claw, hull...).")),
                    ("cursor", s(16, "next_cursor of the page before.")),
                    ("limit", int(1, LIMITS.1 as u64, "Rows a page (default 10).")),
                    ("rows", boolean("Rows of every kind, paged, with no filter.")),
                ],
                &[],
            ),
        },
        ToolDef {
            name: "exemplar_get".into(),
            description: format!(
                "One kind of the library by its name or an alias, short (under {MAX_SHORT_BYTES} bytes): its plan, size, how far it varies, roles, clips, which \
                 numbers are the engine's own, and the calls that take and build it. seed (0..{}): the specimen that seed draws: its numbers, and its bones, \
                 parts and clips counted before any build. full: true adds every number with its source or reason, the kind file and the recipe and clips \
                 the seed writes. An unknown name answers with the nearest across the kinds and the shapes. Read-only.",
                crate::world::MAX_MODEL_SEED
            ),
            input_schema: schema(
                vec![
                    ("name", s(MAX_WORD, "The kind's name or an alias.")),
                    ("seed", int(0, crate::world::MAX_MODEL_SEED, "A specimen's seed (left out: the kind's mean).")),
                    ("full", boolean("Add the numbers with their sources, the kind file and the written recipe and clips.")),
                ],
                &["name"],
            ),
        },
        ToolDef {
            name: "exemplar_take".into(),
            description: format!(
                "Copies a kind of the library into the project, as {TAKE_FOLDER}/NAME.kind.json (its base, if any, written into it: one file), with where it \
                 came from. Then model_build {{\"recipe\": \"models/NAME.kind.json\", \"seed\": N}} (or model_build_all) or a model entity with that recipe and seed builds a \
                 specimen in the project's own style (its roles resolve to style.json's presets of the same name, or through style.json's \"roles\"). A \
                 copy, never a reference: the project's kind changes only when it is edited. Written through project_write (validated, claim-checked, \
                 journaled as you; expect_version 0, the default, means the file must not exist yet)."
            ),
            input_schema: schema(
                vec![
                    ("name", s(MAX_WORD, "The kind's name or an alias.")),
                    ("as", s(specula_exemplaria::words::MAX_NAME, "The copy's name in the project (default: the kind's).")),
                    ("expect_version", int(0, u64::MAX >> 11, "The copy's current version, to overwrite it (0: it must not exist yet).")),
                ],
                &["name"],
            ),
        },
    ]
}

// ---------------------------------------------------------------------------------------------
// A kind read as a recipe (the model library and the project's validation)
// ---------------------------------------------------------------------------------------------

/// The style's presets for a kind's roles ([`Style::role`]).
struct ByStyle<'a>(&'a Style);

impl Roles for ByStyle<'_> {
    fn preset(&self, role: &str, hint: &str) -> Result<String, String> {
        self.0.role(role, Some(hint)).map(str::to_string).map_err(|e| e.to_string())
    }
}

/// A kind file's error as a recipe's (its code in the message).
fn recipe_error(e: &specula_exemplaria::Error) -> RecipeError {
    let code = match e.code {
        "json" => ErrorCode::Json,
        "missing_field" | "no_source" => ErrorCode::MissingField,
        "unknown_field" => ErrorCode::UnknownField,
        "wrong_type" | "bad_name" | "control_character" | "duplicate_key" | "two_sources" => ErrorCode::WrongType,
        "role_unresolved" => ErrorCode::UnknownMaterial,
        "base_not_found" | "unknown_plan" => ErrorCode::UnknownAsset,
        "base_too_deep" | "base_cycle" => ErrorCode::IncludeCycle,
        "unknown_name" | "base_other_plan" => ErrorCode::UnknownName,
        "too_large" | "too_many" => ErrorCode::Limit,
        _ => ErrorCode::OutOfRange,
    };
    let near = if e.near.is_empty() { String::new() } else { format!(" (nearest: {})", e.near.join(", ")) };
    RecipeError::new(e.path.as_str(), code, format!("[{}] {}{near}", e.code, e.message))
}

/// Whether a recipe path or file stem is a kind (`x.kind.json`, stem `x.kind`).
pub fn is_kind(path_or_stem: &str) -> bool {
    path_or_stem.ends_with(".kind.json") || path_or_stem.ends_with(".kind")
}

/// A kind's text with its base found by `base_text` (the base's name → its file's text).
fn resolved(text: &str, base_text: &dyn Fn(&str) -> Option<String>) -> Result<Resolved, RecipeError> {
    let kind = Kind::parse(text).map_err(|e| recipe_error(&e))?;
    let base = match &kind.base {
        Some(b) => match base_text(b) {
            Some(t) => Some(Kind::parse(&t).map_err(|e| recipe_error(&specula_exemplaria::Error { path: format!("base ({b}).{}", e.path), ..e }))?),
            None => None,
        },
        None => None,
    };
    specula_exemplaria::resolve(&kind, base.as_ref()).map_err(|e| recipe_error(&e))
}

/// The recipe and anims texts a kind writes at `seed` in `style` (the model library's build
/// worker calls this; `library` is the recipe folder's files by stem, where a base is
/// `<name>.kind`).
pub fn expand_for_build(text: &str, library: &BTreeMap<String, String>, seed: Option<u64>, style: &Style, from: &str) -> Result<(String, Option<String>), RecipeError> {
    let r = resolved(text, &|b| library.get(&format!("{b}.kind")).cloned())?;
    let x = expand(&r, seed, &ByStyle(style), from).map_err(|e| recipe_error(&e))?;
    Ok((x.recipe, x.anims))
}

/// The kinds a build's recipe folder holds, written as specimens for a scatter item that names a
/// `kind` (`{"kind": "box_creature", "variants": 32}`: each variant's seed another specimen).
/// `library` is the build's library (files by stem: a kind is `<name>.kind`, its base beside it).
#[derive(Debug)]
pub struct FolderKinds<'a> {
    pub library: &'a BTreeMap<String, String>,
    /// The recipe being built (what each specimen's text says it was written for).
    pub from: &'a str,
}

impl specula_model::Specimens for FolderKinds<'_> {
    fn specimen(&self, name: &str, seed: u64, style: &Style) -> Result<String, RecipeError> {
        let text = self.library.get(&format!("{name}.kind")).ok_or_else(|| {
            let kinds: Vec<&str> = self.library.keys().filter_map(|k| k.strip_suffix(".kind")).collect();
            let near = specula_exemplaria::words::nearest(name, kinds.iter().copied(), 3);
            RecipeError::new(
                "",
                ErrorCode::UnknownAsset,
                format!(
                    "no kind '{name}' beside this recipe (a kind is a {name}.kind.json in the recipe's folder: exemplar_take copies one from the library){}",
                    if near.is_empty() { String::new() } else { format!("; nearest: {}", near.join(", ")) }
                ),
            )
        })?;
        expand_for_build(text, self.library, Some(seed), style, &format!("{} ({name})", self.from)).map(|(recipe, _)| recipe)
    }
}

/// A kind checked as a recipe is at a spawn or a write: parsed, resolved over its base (found by
/// `base_text`), and written at its mean in `style` (every role resolved). Its mean recipe.
pub fn check_kind(text: &str, base_text: &dyn Fn(&str) -> Option<String>, style: &Style, from: &str) -> Result<String, RecipeError> {
    let r = resolved(text, base_text)?;
    expand(&r, None, &ByStyle(style), from).map(|x| x.recipe).map_err(|e| recipe_error(&e))
}

#[cfg(test)]
mod tests;
