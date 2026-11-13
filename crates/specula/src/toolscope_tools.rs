//! The `tools_*` tools: the catalog, activation, invocation, requests to the lead and the
//! lead's `tools_scope` (see [`crate::toolscope`] and `docs/design/TOOLS.md`). They run on the
//! protocol threads; only `tools_scope` asks the engine thread (who the caller and the target are).
//!
//! [`call`] is also every tool call's way in: it refuses calls beyond the session's cap and gives
//! `agent_hello {tools}` its effect.

use crate::swarm::Swarm;
use crate::toolkits::{self, KITS, KIT_PREFIX, TAGS};
use crate::toolscope::{clip, listed_json, role_names, Change, ScopeError, Scopes, ToolSet, MAX_ENTRIES, MAX_ENTRY_CHARS, ROLES};
use crate::tools::{bool_arg, check_keys, string_arg, Command, Tools};
use specula_core::Json;
use specula_protocol::{Content, SessionInfo, ToolDef, ToolOutput};

/// Most bytes of a catalog page (the whole catalog is about 8 KB; filters make it smaller).
const MAX_CATALOG_BYTES: usize = 16 * 1024;
/// Most bytes of schemas one activation answers with; the rest are named.
const MAX_SCHEMA_BYTES: usize = 32 * 1024;
/// Search results a catalog filter or a request shows at most.
const MAX_HITS: usize = 40;
const MAX_CLOSEST: usize = 6;
/// A tool request's `need` and `why`.
const MAX_NEED: usize = 300;
const MAX_WHY: usize = 2000;
/// The board tag of tool requests.
pub const REQUEST_TAG: &str = "tool_request";
/// Open tool requests one agent may have on the board.
pub const MAX_OPEN_REQUESTS: usize = 8;

pub fn definitions() -> Vec<ToolDef> {
    let schema = |props: Vec<(&str, Json)>, required: &[&str]| {
        Json::obj([
            ("type", Json::from("object")),
            ("properties", Json::obj(props)),
            ("required", Json::arr(required.iter().map(|r| Json::from(*r)))),
        ])
    };
    let t = |ty: &str, d: &str| Json::obj([("type", Json::from(ty)), ("description", Json::from(d))]);
    let list = |d: &str| Json::obj([("type", Json::from("array")), ("items", Json::obj([("type", Json::from("string"))])), ("description", Json::from(d))]);
    let def = |name: &str, description: &str, input_schema: Json| ToolDef { name: name.into(), description: description.into(), input_schema };
    let entries = "Families (world, sight, board ...), tool names (anim_check), a job's kit (kit:make_menu), or \"all\"";
    vec![
        def(
            "tools_catalog",
            "Every tool the engine has, one line each, and your scope (your tools/list shows the core plus what was \
             activated for you). By job: kits: true lists the kits; kit: NAME shows one (its few tools, the order to use \
             them in, the check that says the job is done). By kind: families, narrowed by filter (words that must all \
             appear), family or tag; roles: true adds the role templates. Then tools_activate what you need.",
            schema(
                vec![
                    ("kits", t("boolean", "List the kits (make_menu, build_place ...)")),
                    ("kit", t("string", "Show one kit")),
                    ("filter", t("string", "Search words, e.g. \"navmesh path\"")),
                    ("family", t("string", "Only this family")),
                    ("tag", t("string", "reads, writes, checks, owned, open, lead, reviewer, owner or game")),
                    ("roles", t("boolean", "Add the role templates (families per role)")),
                    ("cursor", t("string", "next_cursor of the previous page")),
                ],
                &[],
            ),
        ),
        def(
            "tools_activate",
            "Add tools to your tools/list: tools (families or names) and/or role (a template's). Answers with their full \
             definitions (name, description, inputSchema; schemas: false leaves them out) and tells your client \
             notifications/tools/list_changed. If your client doesn't show them, call them with tools_invoke. Refused \
             beyond the cap the lead gave you (not_in_scope): ask with tools_request.",
            schema(
                vec![
                    ("tools", list(entries)),
                    ("role", t("string", "A role template's tools (tools_catalog roles: true)")),
                    ("schemas", t("boolean", "Answer with the definitions (default true)")),
                ],
                &[],
            ),
        ),
        def(
            "tools_deactivate",
            "Remove tools from your tools/list (the core stays): tools (families or names) or all: true. Short lists are cheap.",
            schema(vec![("tools", list(entries)), ("all", t("boolean", "Everything but the core"))], &[]),
        ),
        def(
            "tools_invoke",
            "Call a tool by name: for clients that don't re-read tools/list after tools_activate (or can't call a tool \
             they don't list). Answers exactly as the tool does; the lead's cap applies.",
            schema(vec![("tool", t("string", "The tool's name")), ("arguments", t("object", "Its arguments"))], &["tool"]),
        ),
        def(
            "tools_request",
            "No tool for what you need (or it is beyond your cap)? Ask the lead: searches the catalog for need, then posts a \
             board task tagged tool_request with what the search found, for the lead to build, compose or grant. Returns \
             the task and the matches (try them first).",
            schema(
                vec![
                    ("need", t("string", "What you want to do, in a line")),
                    ("why", t("string", "What it is for and what you tried")),
                ],
                &["need"],
            ),
        ),
        def(
            "tools_scope",
            "For the lead (role lead or director): set another agent's tool list. agent: its agent_hello name (applied to \
             its open sessions now, and to any session that says hello as it later); role and/or tools (families or \
             names); cap: true also makes that the most it may list or call; clear: true removes your setting. Scopes \
             are not a security boundary until auth tokens exist (M6).",
            schema(
                vec![
                    ("agent", t("string", "The agent's name")),
                    ("role", t("string", &format!("A role template: {}", role_names()))),
                    ("tools", list(entries)),
                    ("cap", t("boolean", "Also the most it may list or call (default false)")),
                    ("clear", t("boolean", "Remove this agent's setting")),
                ],
                &["agent"],
            ),
        ),
    ]
}

fn fail(e: &ScopeError) -> ToolOutput {
    ToolOutput::error(e.to_json().to_string())
}

fn bad(message: String) -> ToolOutput {
    fail(&ScopeError::new("invalid_argument", message))
}

/// `tools` as a list of entries: an array of strings, or one string with commas.
fn entries_arg(args: &Json, key: &str) -> Result<Option<Vec<String>>, String> {
    match args.get(key) {
        None | Some(Json::Null) => Ok(None),
        // Both forms hold at most MAX_ENTRIES entries; select() also ignores entries past
        // MAX_ENTRY_CHARS without searching for a suggestion.
        Some(Json::Str(s)) => {
            let entries: Vec<String> = s.split(',').take(MAX_ENTRIES + 1).map(|e| e.trim().to_owned()).filter(|e| !e.is_empty()).collect();
            if entries.len() > MAX_ENTRIES {
                return Err(format!("'{key}' names at most {MAX_ENTRIES} families or tools"));
            }
            Ok(Some(entries))
        }
        Some(Json::Arr(a)) if a.len() <= MAX_ENTRIES => a
            .iter()
            .map(|v| v.as_str().map(str::to_owned).ok_or_else(|| format!("'{key}' must be a list of family or tool names")))
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        Some(_) => Err(format!("'{key}' must be a list of at most {MAX_ENTRIES} family or tool names")),
    }
}

/// The set named by `tools` and/or `role` (`None` when neither is given).
fn selection(scopes: &Scopes, args: &Json) -> Result<Option<ToolSet>, ScopeError> {
    Ok(selection_and_reach(scopes, args)?.map(|(set, _)| set))
}

/// The tools `tools` and `role` name (what is listed), and what a cap of them allows: the same,
/// plus the tools a kit entry sometimes needs.
fn selection_and_reach(scopes: &Scopes, args: &Json) -> Result<Option<(ToolSet, ToolSet)>, ScopeError> {
    let tools = entries_arg(args, "tools").map_err(|m| ScopeError::new("invalid_argument", m))?;
    let role = string_arg(args, "role").map_err(|m| ScopeError::new("invalid_argument", m))?;
    if tools.is_none() && role.is_none() {
        return Ok(None);
    }
    let (mut set, mut reach) = (scopes.catalog.empty(), scopes.catalog.empty());
    if let Some(t) = &tools {
        set.union(&scopes.catalog.select(t.iter().map(String::as_str))?);
        reach.union(&scopes.catalog.select_reach(t.iter().map(String::as_str))?);
    }
    if let Some(r) = role {
        let s = scopes.catalog.role(r).ok_or_else(|| ScopeError::new("unknown_role", format!("'{}' is no role template: {}", clip(r, MAX_ENTRY_CHARS), role_names())).with("field", "role"))?;
        set.union(&s);
        reach.union(&s);
    }
    Ok(Some((set, reach)))
}

/// Every tool call comes through here (see the module docs).
pub(crate) fn call(tools: &Tools, session: &SessionInfo, name: &str, args: &Json) -> ToolOutput {
    let scopes = tools.scopes();
    // Every argument within its schema's limits before any parser sees it (tools_invoke's inner
    // call comes back through here; see `bounds`).
    if let Some(i) = scopes.catalog.index(name) {
        if let Err(e) = crate::bounds::check_args(scopes.catalog.def(i), args) {
            return e.to_output();
        }
    }
    match name {
        "tools_catalog" => return catalog(scopes, session, args),
        "tools_activate" => return activate(scopes, session, args),
        "tools_deactivate" => return deactivate(scopes, session, args),
        "tools_invoke" => return invoke(tools, session, args),
        "tools_request" => return request(tools, session, args),
        _ => {}
    }
    if !scopes.allowed_name(&session.id, name) {
        return fail(
            &ScopeError::new(
                "not_in_scope",
                format!("'{name}' is beyond the tools the lead capped you to; ask for it with tools_request {{need, why}} (tools_catalog shows your scope)"),
            )
            .with("tool", name),
        );
    }
    match name {
        "tools_scope" => scope(tools, session, args),
        "agent_hello" => hello(tools, session, args),
        _ => tools.call_engine(session, name, args),
    }
}

fn hello(tools: &Tools, session: &SessionInfo, args: &Json) -> ToolOutput {
    let scopes = tools.scopes();
    let wanted = match entries_arg(args, "tools") {
        Ok(None) => None,
        Ok(Some(e)) => match scopes.catalog.select(e.iter().map(String::as_str)) {
            Ok(s) => Some(s),
            Err(e) => return fail(&e.with("field", "tools")),
        },
        Err(m) => return bad(m),
    };
    let stripped = match args {
        Json::Obj(p) if wanted.is_some() || args.get("tools").is_some() => Json::Obj(p.iter().filter(|(k, _)| k != "tools").cloned().collect()),
        other => other.clone(),
    };
    let mut out = tools.call_engine(session, "agent_hello", &stripped);
    if out.is_error {
        return out;
    }
    if let Some(Content::Text(text)) = out.content.first_mut() {
        if let Ok(mut reply) = Json::parse(text) {
            let agent = reply.get("agent").and_then(Json::as_str).unwrap_or("").to_owned();
            // Only a role the agent named picks a template: a hello with just a name (whose role
            // defaults to builder) keeps the core-only list.
            let role = match args.get("role").and_then(Json::as_str) {
                Some(_) => reply.get("role").and_then(Json::as_str).unwrap_or("").to_owned(),
                None => String::new(),
            };
            // A frozen session's hello changes nothing (docs/design/SECURITY.md): its tool
            // list stays as it was too.
            if reply.get("unchanged").and_then(Json::as_bool) != Some(true) {
                let summary = scopes.hello(&session.id, &agent, &role, wanted.as_ref());
                reply.set("tool_scope", summary);
                *text = reply.to_string();
            }
        }
    }
    out
}

fn catalog(scopes: &Scopes, session: &SessionInfo, args: &Json) -> ToolOutput {
    if let Err(m) = check_keys(args, "tools_catalog", &[&["filter", "family", "roles", "cursor", "kit", "kits", "tag"]]) {
        return bad(m);
    }
    let c = &scopes.catalog;
    let (kit, tag) = match (string_arg(args, "kit"), string_arg(args, "tag")) {
        (Ok(k), Ok(t)) => (k, t),
        (Err(m), _) | (_, Err(m)) => return bad(m),
    };
    let kits = match bool_arg(args, "kits") {
        Ok(k) => k.unwrap_or(false),
        Err(m) => return bad(m),
    };
    if tag.is_some_and(|t| !TAGS.iter().any(|(n, _)| *n == t)) {
        return bad(format!("'tag' must be one of: {}", TAGS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")));
    }
    // By job: one kit, or the list of them. Nothing else is answered with either (small
    // answers), so the other arguments are refused with them, not ignored.
    if kit.is_some() || kits {
        let others = ["filter", "family", "roles", "cursor", "tag"].into_iter().filter(|k| args.get(k).is_some_and(|v| !v.is_null())).collect::<Vec<_>>();
        if !others.is_empty() || (kit.is_some() && kits) {
            return bad(format!(
                "'kit' and 'kits' each answer on their own: leave out {}",
                if others.is_empty() { "one of them".to_owned() } else { others.join(", ") }
            ));
        }
    }
    if let Some(k) = kit {
        let k = k.trim();
        let Some(found) = toolkits::kit(k.strip_prefix(KIT_PREFIX).unwrap_or(k)) else {
            return bad(format!("'kit' must be a kit: {}", toolkits::kit_names()));
        };
        let entry = found.to_json(c, &scopes.listed(&session.id));
        let used: Vec<&str> = found.tools.iter().flat_map(|t| toolkits::tags(t)).collect();
        let legend = Json::obj(TAGS.iter().filter(|(n, _)| used.contains(n)).map(|(n, what)| ((*n).to_owned(), Json::from(*what))));
        return ToolOutput::text(Json::obj([("kit", entry), ("tags", legend), ("scope", scopes.describe(&session.id))]).to_string());
    }
    if kits {
        let all = KITS.iter().map(|k| {
            let mut with_core = k.tools(c);
            with_core.union(c.core());
            (k.name.to_owned(), Json::obj([("about", Json::from(k.about)), ("tools", Json::arr(k.tools.iter().map(|t| Json::from(*t)))), ("list_bytes", Json::from(c.list_bytes(&with_core)))]))
        });
        let out = Json::obj([
            ("kits", Json::obj(all)),
            ("scope", scopes.describe(&session.id)),
            (
                "next",
                Json::from(
                    "tools_catalog {kit: NAME} shows a kit's steps, its done check and the tools it sometimes needs; tools_activate {tools: [\"kit:NAME\"]} lists its tools. \
                     No kit for your job? tools_catalog {filter} searches every tool, and tools_request asks the lead",
                ),
            ),
        ]);
        return ToolOutput::text(out.to_string());
    }
    let (filter, family, cursor) = match (string_arg(args, "filter"), string_arg(args, "family"), string_arg(args, "cursor")) {
        (Ok(f), Ok(fam), Ok(cur)) => (f, fam, cur),
        (Err(m), ..) | (_, Err(m), _) | (.., Err(m)) => return bad(m),
    };
    let roles = match bool_arg(args, "roles") {
        Ok(r) => r.unwrap_or(false),
        Err(m) => return bad(m),
    };
    let families: Vec<(usize, &str, &str)> = c.families().collect();
    for (key, value) in [("family", family), ("cursor", cursor)] {
        if let Some(v) = value {
            if !families.iter().any(|(_, n, _)| *n == v) {
                let all: Vec<&str> = families.iter().map(|(_, n, _)| *n).collect();
                return bad(format!("'{key}' must be a family: {}", all.join(", ")));
            }
        }
    }
    if filter.is_some_and(|f| f.chars().count() > MAX_NEED) {
        return bad(format!("'filter' is a few search words (at most {MAX_NEED} characters)"));
    }
    let hits: Option<Vec<usize>> = filter.map(|f| c.search(f, true).into_iter().take(MAX_HITS).collect());
    let listed = scopes.listed(&session.id);
    let mut fams = Json::obj::<&str>([]);
    let mut bytes = 0;
    let mut next = None;
    let mut started = cursor.is_none();
    for &(f, fname, about) in &families {
        if !started {
            started = Some(fname) == cursor;
            if !started {
                continue;
            }
        }
        if family.is_some_and(|want| want != fname) {
            continue;
        }
        let members: Vec<usize> = c.members(f).filter(|i| hits.as_ref().is_none_or(|h| h.contains(i)) && tag.is_none_or(|t| toolkits::tags(c.name(*i)).contains(&t))).collect();
        if members.is_empty() {
            continue;
        }
        let on: Vec<usize> = members.iter().copied().filter(|&i| listed.contains(i)).collect();
        let mut entry = Json::obj([("about", Json::from(about))]);
        if on.len() == members.len() {
            entry.set("listed", Json::from("all"));
        } else if !on.is_empty() {
            entry.set("listed", Json::arr(on.iter().map(|&i| Json::from(c.name(i)))));
        }
        entry.set("tools", Json::obj(members.iter().map(|&i| (c.name(i).to_owned(), Json::from(c.brief(i))))));
        let size = entry.to_string().len() + fname.len() + 4;
        if bytes + size > MAX_CATALOG_BYTES && bytes > 0 {
            next = Some(fname.to_owned());
            break;
        }
        bytes += size;
        fams.set(fname, entry);
    }
    let mut out = Json::obj([("tools", Json::from(c.len())), ("scope", scopes.describe(&session.id)), ("families", fams)]);
    if let Some(t) = tag.filter(|_| out.get("families").and_then(Json::as_object).is_some_and(|f| f.is_empty()) && next.is_none()) {
        out.set("hint", Json::from(format!("no tool here has the tag '{t}'{}", if t == "game" { " (no game is hosted, or the family filter leaves it out)" } else { "" })));
    }
    if cursor.is_none() && family.is_none() {
        // The other way in: by job.
        out.set("kits", Json::from(format!("by job: {} (tools_catalog {{kits: true}} lists them, {{kit: NAME}} shows one)", toolkits::kit_names())));
    }
    if let (Some(f), Some(h)) = (filter, &hits) {
        out.set("matches", Json::from(h.len()));
        if h.is_empty() {
            let closest = c.search(f, false).into_iter().take(MAX_CLOSEST).map(|i| (c.name(i).to_owned(), Json::from(c.brief(i))));
            out.set("closest", Json::obj(closest));
            out.set("hint", Json::from("no tool has every word; closest lists partial matches. Nothing fits? tools_request {need, why} asks the lead"));
        }
    }
    if roles {
        out.set(
            "roles",
            Json::obj(ROLES.iter().map(|r| {
                let set = c.role(r.name).unwrap_or_else(|| c.empty());
                let mut listed = set.clone();
                listed.union(c.core());
                let fams = if r.all { Json::from("all") } else { Json::arr(r.families.iter().map(|f| Json::from(*f))) };
                let mut j = Json::obj([("about", Json::from(r.about)), ("families", fams), ("listed", Json::from(listed.count())), ("list_bytes", Json::from(c.list_bytes(&listed)))]);
                if !r.tools.is_empty() {
                    j.set("tools", Json::arr(r.tools.iter().map(|t| Json::from(*t))));
                }
                (r.name.to_owned(), j)
            })),
        );
    }
    if let Some(n) = next {
        out.set("next_cursor", Json::from(n));
    }
    out.set(
        "next",
        Json::from("tools_activate {tools: [families, names or kit:NAME]} lists them and answers their schemas; tools within your cap work unlisted too (tools_invoke)"),
    );
    ToolOutput::text(out.to_string())
}

/// The definitions of `set` while they fit in [`MAX_SCHEMA_BYTES`], and the names of the rest.
/// The definitions of `set` as the session's list (`listed`) shows them: a tool whose sibling
/// is listed too points at the arguments they share (`Catalog::listing`), so an activation's
/// answer doesn't carry what the list beside it already says.
fn schemas(scopes: &Scopes, set: &ToolSet, listed: &ToolSet) -> (Json, Vec<String>) {
    let mut shown = Vec::new();
    let mut omitted = Vec::new();
    let mut bytes = 0;
    let mut shows = listed.clone();
    shows.union(set);
    let listing = scopes.catalog.listing(&shows);
    for (i, d) in shows.iter().zip(&listing).filter(|(i, _)| set.contains(*i)) {
        let j = listed_json(d);
        let size = j.to_string().len() + 1;
        if bytes + size > MAX_SCHEMA_BYTES {
            omitted.push(scopes.catalog.name(i).to_owned());
        } else {
            bytes += size;
            shown.push(j);
        }
    }
    (Json::Arr(shown), omitted)
}

fn change_json(scopes: &Scopes, ch: &Change) -> Json {
    let names = |v: &[usize]| Json::arr(v.iter().map(|&i| Json::from(scopes.catalog.name(i))));
    Json::obj([
        ("added", names(&ch.added)),
        ("removed", names(&ch.removed)),
        ("listed", Json::from(ch.listed.count())),
        ("list_bytes", Json::from(scopes.catalog.list_bytes(&ch.listed))),
    ])
}

fn activate(scopes: &Scopes, session: &SessionInfo, args: &Json) -> ToolOutput {
    if let Err(m) = check_keys(args, "tools_activate", &[&["tools", "role", "schemas"]]) {
        return bad(m);
    }
    let with_schemas = match bool_arg(args, "schemas") {
        Ok(s) => s.unwrap_or(true),
        Err(m) => return bad(m),
    };
    let set = match selection(scopes, args) {
        Ok(Some(s)) => s,
        Ok(None) => return bad("tools_activate needs 'tools' (families or tool names) or 'role'; tools_catalog lists them".into()),
        Err(e) => return fail(&e),
    };
    let ch = match scopes.activate(&session.id, &set) {
        Ok(ch) => ch,
        Err(e) => return fail(&e),
    };
    let mut out = change_json(scopes, &ch);
    if with_schemas {
        let (defs, omitted) = schemas(scopes, &set, &ch.listed);
        out.set("tools", defs);
        if !omitted.is_empty() {
            out.set("schemas_omitted", Json::arr(omitted.into_iter().map(Json::from)));
            out.set("schemas_hint", Json::from("too many schemas for one answer: tools/list has them all, or activate fewer at a time"));
        }
    }
    out.set(
        "next",
        Json::from(if ch.added.is_empty() {
            "nothing new: these were already listed"
        } else {
            "your client was sent notifications/tools/list_changed; if it doesn't show the new tools, call them with tools_invoke {tool, arguments}"
        }),
    );
    ToolOutput::text(out.to_string())
}

fn deactivate(scopes: &Scopes, session: &SessionInfo, args: &Json) -> ToolOutput {
    if let Err(m) = check_keys(args, "tools_deactivate", &[&["tools", "all"]]) {
        return bad(m);
    }
    let all = match bool_arg(args, "all") {
        Ok(a) => a.unwrap_or(false),
        Err(m) => return bad(m),
    };
    let set = match (all, selection(scopes, args)) {
        (true, Ok(None)) => scopes.catalog.full(),
        (false, Ok(Some(s))) => s,
        (_, Err(e)) => return fail(&e),
        _ => return bad("tools_deactivate needs 'tools' (families or tool names) or 'all': true, not both".into()),
    };
    let ch = scopes.deactivate(&session.id, &set);
    ToolOutput::text(change_json(scopes, &ch).to_string())
}

fn invoke(tools: &Tools, session: &SessionInfo, args: &Json) -> ToolOutput {
    if let Err(m) = check_keys(args, "tools_invoke", &[&["tool", "arguments"]]) {
        return bad(m);
    }
    let name = match string_arg(args, "tool") {
        Ok(Some(n)) => n,
        Ok(None) => return bad("'tool' is required: the name of the tool to call".into()),
        Err(m) => return bad(m),
    };
    let c = &tools.scopes().catalog;
    if name == "tools_invoke" {
        return bad("'tool' can't be tools_invoke itself".into());
    }
    if c.index(name).is_none() {
        // Tool names are at most 64 characters: a longer one isn't searched, and only its start
        // is echoed back.
        let hint = if name.chars().count() <= MAX_ENTRY_CHARS {
            c.search(name, false).first().map(|&i| format!(" (closest: '{}')", c.name(i))).unwrap_or_default()
        } else {
            String::new()
        };
        let shown = clip(name, MAX_ENTRY_CHARS);
        return fail(&ScopeError::new("unknown_tool", format!("'{shown}' is no tool{hint}; tools_catalog lists them")).with("tool", shown));
    }
    let empty = Json::obj::<&str>([]);
    let arguments = match args.get("arguments") {
        None | Some(Json::Null) => &empty,
        Some(a @ Json::Obj(_)) => a,
        Some(_) => return bad("'arguments' must be an object: the tool's arguments".into()),
    };
    // The trace records the tool called, not tools_invoke (docs/design/TRACE.md).
    crate::trace_sink::invoked(specula_core::trace::Ctx::current().call, name, arguments);
    call(tools, session, name, arguments)
}

fn request(tools: &Tools, session: &SessionInfo, args: &Json) -> ToolOutput {
    if let Err(m) = check_keys(args, "tools_request", &[&["need", "why"]]) {
        return bad(m);
    }
    let text = |key: &str, max: usize| -> Result<Option<String>, String> {
        match string_arg(args, key)? {
            None => Ok(None),
            Some(s) if s.trim().is_empty() => Err(format!("'{key}' is empty")),
            Some(s) if s.chars().count() > max => Err(format!("'{key}' is longer than {max} characters")),
            Some(s) => Ok(Some(s.trim().to_owned())),
        }
    };
    let (need, why) = match (text("need", MAX_NEED), text("why", MAX_WHY)) {
        (Ok(Some(n)), Ok(w)) => (n, w),
        (Ok(None), _) => return bad("'need' is required: what you want to do, in a line".into()),
        (Err(m), _) | (_, Err(m)) => return bad(m),
    };
    let scopes = tools.scopes();
    let c = &scopes.catalog;
    let full: Vec<usize> = c.search(&need, true).into_iter().take(MAX_CLOSEST).collect();
    let closest: Vec<usize> = if full.is_empty() { c.search(&need, false).into_iter().take(MAX_CLOSEST).collect() } else { Vec::new() };
    let shown = if full.is_empty() { &closest } else { &full };
    let beyond = scopes.outside_cap(&session.id, &ToolSet::from_indices(c.len(), shown));
    let line = |i: usize| format!("{} ({}){}", c.name(i), c.brief(i), if beyond.contains(i) { " [beyond the requester's cap]" } else { "" });
    let found = if !full.is_empty() {
        format!("matches every word: {}", full.iter().map(|&i| line(i)).collect::<Vec<_>>().join("; "))
    } else if !closest.is_empty() {
        format!("no tool matches every word; closest: {}", closest.iter().map(|&i| line(i)).collect::<Vec<_>>().join("; "))
    } else {
        "nothing matches".to_owned()
    };
    let scope = scopes.describe(&session.id);
    let detail = format!(
        "A tool request (tools_request).\nNeed: {need}\nWhy: {}\nCatalog search: {found}\nRequester's scope: {} (lists {} of {} tools{}).\n\
         Lead: build or compose the tool, or grant it with tools_scope; then task_done this.",
        why.as_deref().unwrap_or("(not given)"),
        scope.get("from").and_then(Json::as_str).unwrap_or(""),
        scope.get("listed").and_then(Json::as_u64).unwrap_or(0),
        c.len(),
        if scope.get("cap").is_some() { ", capped" } else { "" },
    );
    let title: String = format!("Tool request: {need}").chars().take(crate::board::MAX_TITLE).collect();
    let detail: String = detail.chars().take(crate::board::MAX_DETAIL).collect();
    // One request at a time (two identical ones at once must not both post), deduplicated
    // against the requester's open ones, at most MAX_OPEN_REQUESTS of them: tools_request is core
    // and uncapped, so a looping agent must not be able to fill the board.
    let _one_at_a_time = scopes.requests_lock();
    match open_requests(tools, session) {
        Ok(Some(open)) => {
            if let Some((id, _)) = open.iter().find(|(_, t)| normalise(t) == normalise(&title)) {
                return ToolOutput::text(
                    Json::obj([
                        ("task", Json::from(*id)),
                        ("tag", Json::from(REQUEST_TAG)),
                        ("duplicate", Json::from(true)),
                        ("next", Json::from("you already asked for this; the lead has it on the board (task_get shows its progress)")),
                    ])
                    .to_string(),
                );
            }
            if open.len() >= MAX_OPEN_REQUESTS {
                return fail(
                    &ScopeError::new(
                        "too_many_requests",
                        format!("you have {} open tool requests (at most {MAX_OPEN_REQUESTS}): wait for the lead, or task_done one you no longer need", open.len()),
                    )
                    .with("open", Json::arr(open.iter().map(|(id, _)| Json::from(*id))))
                    .with("max", MAX_OPEN_REQUESTS),
                );
            }
        }
        // Without agent_hello there is no requester to look up; task_post refuses it below.
        Ok(None) => {}
        Err(out) => return out,
    }
    let post = Json::obj([
        ("title", Json::from(title)),
        ("detail", Json::from(detail)),
        ("tags", Json::arr([Json::from(REQUEST_TAG)])),
        ("priority", Json::from(4u64)),
    ]);
    let posted = tools.call_engine(session, "task_post", &post);
    if posted.is_error {
        return posted;
    }
    let task = posted.content.iter().find_map(|c| match c {
        Content::Text(t) => Json::parse(t).ok(),
        Content::Image { .. } => None,
    });
    let id = task.as_ref().and_then(|t| t.get("id").or_else(|| t.get("task").and_then(|x| x.get("id"))).cloned()).unwrap_or(Json::Null);
    let names = |v: &[usize]| Json::obj(v.iter().map(|&i| (c.name(i).to_owned(), Json::from(c.brief(i)))));
    let mut out = Json::obj([("task", id), ("tag", Json::from(REQUEST_TAG))]);
    if !full.is_empty() {
        out.set("matches", names(&full));
    } else {
        out.set("closest", names(&closest));
    }
    if beyond.count() > 0 {
        out.set("beyond_your_cap", Json::arr(c.names(&beyond).into_iter().map(Json::from)));
    }
    out.set("next", Json::from("the lead sees it in task_list {tag: \"tool_request\"}; try the matches meanwhile (tools_activate)"));
    ToolOutput::text(out.to_string())
}

/// Lower case with runs of white space as one space: two requests that differ only in those are
/// the same request.
fn normalise(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// The requester's open tool requests (id, title) from the board; `None` before agent_hello.
fn open_requests(tools: &Tools, session: &SessionInfo) -> Result<Option<Vec<(u64, String)>>, ToolOutput> {
    let text = |out: &ToolOutput| match out.content.first() {
        Some(Content::Text(t)) => Json::parse(t).ok(),
        _ => None,
    };
    let me = tools.send(session, Command::ScopeLookup { agent: String::new() });
    if me.is_error {
        return Err(me);
    }
    let me = text(&me).unwrap_or(Json::Null);
    if me.get("introduced").and_then(Json::as_bool) != Some(true) {
        return Ok(None);
    }
    let name = me.get("caller").and_then(Json::as_str).unwrap_or("");
    let query = Json::obj([
        ("posted_by", Json::from(name)),
        ("tag", Json::from(REQUEST_TAG)),
        ("status", Json::arr(["open", "taken", "blocked"].map(Json::from))),
        ("limit", Json::from(MAX_OPEN_REQUESTS as u64 + 1)),
    ]);
    let listed = tools.call_engine(session, "task_list", &query);
    if listed.is_error {
        return Err(listed);
    }
    let tasks = text(&listed).and_then(|j| j.get("tasks").and_then(Json::as_array).map(<[Json]>::to_vec)).unwrap_or_default();
    Ok(Some(
        tasks
            .iter()
            .filter_map(|t| Some((t.get("id")?.as_u64()?, t.get("title")?.as_str()?.to_owned())))
            .collect(),
    ))
}

fn scope(tools: &Tools, session: &SessionInfo, args: &Json) -> ToolOutput {
    if let Err(m) = check_keys(args, "tools_scope", &[&["agent", "role", "tools", "cap", "clear"]]) {
        return bad(m);
    }
    let scopes = tools.scopes();
    let agent = match string_arg(args, "agent") {
        Ok(Some(a)) if !a.trim().is_empty() && a.chars().count() <= 64 => a.trim().to_owned(),
        Ok(_) => return bad("'agent' is required: the agent_hello name of the agent to scope".into()),
        Err(m) => return bad(m),
    };
    let (cap, clear) = match (bool_arg(args, "cap"), bool_arg(args, "clear")) {
        (Ok(c), Ok(cl)) => (c.unwrap_or(false), cl.unwrap_or(false)),
        (Err(m), _) | (_, Err(m)) => return bad(m),
    };
    let set = match selection_and_reach(scopes, args) {
        Ok(s) => s,
        Err(e) => return fail(&e),
    };
    if clear == set.is_some() || (clear && cap) {
        return bad("tools_scope needs 'tools' and/or 'role' (with 'cap' to cap it), or 'clear': true alone".into());
    }
    // Who the caller is and which open sessions are the agent: the engine thread knows.
    let found = tools.send(session, Command::ScopeLookup { agent: agent.clone() });
    let found = match found.content.first() {
        Some(Content::Text(t)) if !found.is_error => Json::parse(t).unwrap_or(Json::Null),
        _ => return found,
    };
    // Authority from the engine's one authority function (never the self-declared role).
    let have = found.get("authority").and_then(Json::as_str).and_then(crate::authority::Authority::parse).unwrap_or_default();
    if let Err(e) = crate::authority::require(have, crate::authority::Authority::Lead, crate::authority::Site::ToolsScope, "picking other agents' tools (anyone may activate their own: tools_activate)") {
        let mut s = ScopeError::new(e.code, e.message.clone());
        for (k, v) in e.details {
            s = s.with(k, v);
        }
        return fail(&s);
    }
    let by = found.get("caller").and_then(Json::as_str).unwrap_or("").to_owned();
    let sessions: Vec<String> = found.get("sessions").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(|s| s.as_str().map(str::to_owned)).collect();
    if clear {
        let had = scopes.clear(&agent, &sessions);
        return ToolOutput::text(
            Json::obj([
                ("agent", Json::from(agent.as_str())),
                ("cleared", Json::from(had.is_some())),
                ("sessions", Json::from(sessions.len())),
                ("next", Json::from("its open sessions are back to the engine's default list")),
            ])
            .to_string(),
        );
    }
    let (set, reach) = set.expect("checked above");
    // The label from the deduplicated entries (each a valid name by now), cut to MAX_LABEL in
    // assign: "world,world,..." can't make every later answer carry a long label.
    let mut label = Vec::new();
    if let Ok(Some(t)) = entries_arg(args, "tools") {
        let unique: std::collections::BTreeSet<&str> = t.iter().map(String::as_str).collect();
        label.push(format!("tools {}", unique.into_iter().collect::<Vec<_>>().join(",")));
    }
    if let Ok(Some(r)) = string_arg(args, "role") {
        label.push(format!("role {r}"));
    }
    let mut listed = set.clone();
    listed.union(scopes.catalog.core());
    // A cap by kit allows the tools the kit sometimes needs too (its steps name them).
    let changes = scopes.assign_capped(&by, &agent, &sessions, set, cap.then_some(reach), label.join(" + "));
    ToolOutput::text(
        Json::obj([
            ("agent", Json::from(agent.as_str())),
            ("listed", Json::from(listed.count())),
            ("list_bytes", Json::from(scopes.catalog.list_bytes(&listed))),
            ("capped", Json::from(cap)),
            ("sessions", Json::from(sessions.len())),
            ("changed", Json::from(changes.iter().filter(|c| !c.added.is_empty() || !c.removed.is_empty()).count())),
            (
                "next",
                Json::from(if sessions.is_empty() {
                    "kept for when it says agent_hello with this name"
                } else {
                    "its open sessions list these now (told with notifications/tools/list_changed at their next request); later sessions get it at agent_hello"
                }),
            ),
        ])
        .to_string(),
    )
}

/// The engine thread's answer to [`Command::ScopeLookup`]: the caller's name and role and the
/// open sessions that are `agent` (internal: session ids never reach an agent).
pub(crate) fn lookup(swarm: &Swarm, caller: &SessionInfo, agent: &str, live: &[SessionInfo]) -> ToolOutput {
    let sessions = live.iter().filter(|s| swarm.agent_name(s) == agent).map(|s| Json::from(s.id.as_str()));
    ToolOutput::text(
        Json::obj([
            ("caller", Json::from(swarm.agent_name(caller))),
            ("role", Json::from(swarm.role(&caller.id))),
            ("authority", Json::from(crate::authority::authority(swarm, &caller.id).name())),
            ("introduced", Json::from(swarm.introduced(&caller.id))),
            ("sessions", Json::arr(sessions)),
        ])
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toolscope::{brief_of, family_of, BRIEFS, CORE, FAMILIES, GAME_FAMILY};
    use crate::tools::Request;
    use std::collections::BTreeMap;
    use std::sync::mpsc;

    fn session(id: &str) -> SessionInfo {
        SessionInfo { id: id.into(), client_name: "test".into(), client_version: "0".into(), protocol_version: "2025-11-25".into() }
    }

    /// Tools with a stand-in engine thread: it answers agent_hello, the scope lookup and
    /// task_post, and "ran" for anything else.
    fn tools() -> Tools {
        let (tx, rx) = mpsc::channel::<Request>();
        std::thread::spawn(move || {
            let mut agents: BTreeMap<String, (String, String)> = BTreeMap::new();
            // Posted tasks: (id, title, poster, tags).
            let mut board: Vec<(u64, String, String, Vec<String>)> = Vec::new();
            for r in rx {
                let out = match r.command {
                    Command::Hello { name, role, .. } => {
                        agents.insert(r.session.id.clone(), (name.clone(), role.clone()));
                        ToolOutput::text(Json::obj([("agent", Json::from(name)), ("role", Json::from(role)), ("resume_token", Json::from("0".repeat(32)))]).to_pretty())
                    }
                    Command::ScopeLookup { agent } => {
                        let (caller, role) = agents.get(&r.session.id).cloned().unwrap_or(("anon".into(), "builder".into()));
                        let sessions = agents.iter().filter(|(_, (n, _))| *n == agent).map(|(s, _)| Json::from(s.as_str()));
                        ToolOutput::text(
                            Json::obj([
                                ("caller", Json::from(caller)),
                                ("authority", Json::from(if matches!(role.as_str(), "lead" | "director") { "lead" } else { "agent" })),
                                ("role", Json::from(role)),
                                ("introduced", Json::from(agents.contains_key(&r.session.id))),
                                ("sessions", Json::arr(sessions)),
                            ])
                            .to_string(),
                        )
                    }
                    Command::Swarm(crate::swarm_tools::SwarmCall::TaskPost(spec, _)) => {
                        let poster = agents.get(&r.session.id).map(|(n, _)| n.clone()).unwrap_or_default();
                        let id = 7 + board.len() as u64;
                        board.push((id, spec.title.clone(), poster, spec.tags.to_vec()));
                        ToolOutput::text(Json::obj([("id", Json::from(id)), ("title", Json::from(spec.title)), ("detail", Json::from(spec.detail))]).to_string())
                    }
                    Command::Swarm(crate::swarm_tools::SwarmCall::TaskList { query, .. }) => {
                        let tasks = board
                            .iter()
                            .filter(|(_, _, by, tags)| query.posted_by.as_ref().is_none_or(|p| p == by) && query.tag.as_ref().is_none_or(|t| tags.contains(t)))
                            .take(query.limit)
                            .map(|(id, title, _, _)| Json::obj([("id", Json::from(*id)), ("title", Json::from(title.as_str()))]));
                        ToolOutput::text(Json::obj([("tasks", Json::arr(tasks))]).to_string())
                    }
                    _ => ToolOutput::text("{\"ran\": true}"),
                };
                let _ = r.reply.send(out);
            }
        });
        let game = vec![ToolDef { name: "play_state".into(), description: "The game's state: wave, dust.".into(), input_schema: Json::obj([("type", Json::from("object"))]) }];
        Tools::new(tx, game)
    }

    fn json(out: &ToolOutput) -> Json {
        match &out.content[0] {
            Content::Text(t) => Json::parse(t).unwrap(),
            Content::Image { .. } => panic!("an image"),
        }
    }

    fn call_json(t: &Tools, s: &SessionInfo, name: &str, args: &str) -> (bool, Json) {
        let out = specula_protocol::ToolHandler::call(t, s, name, &Json::parse(args).unwrap());
        (out.is_error, json(&out))
    }

    fn listed(t: &Tools, s: &SessionInfo) -> Vec<String> {
        specula_protocol::ToolHandler::tools_for(t, s).into_iter().map(|d| d.name).collect()
    }

    fn opened(t: &Tools, s: &SessionInfo, query: &str) {
        specula_protocol::ToolHandler::session_opened(t, s, query).unwrap();
    }

    fn version(t: &Tools, s: &SessionInfo) -> u64 {
        specula_protocol::ToolHandler::list_version(t, s)
    }

    /// A new tool needs a family (by its prefix, or named in FAMILIES) and a catalog line; stale
    /// lines, empty families and roles naming nothing fail too.
    #[test]
    fn every_tool_has_a_family_and_a_brief() {
        let t = tools();
        let c = &t.scopes().catalog;
        for i in 0..c.len() {
            let name = c.name(i);
            if c.family_name(i) == GAME_FAMILY {
                continue;
            }
            assert!(family_of(name).is_some(), "'{name}' has no family: give its prefix (or the name) a family in toolscope::FAMILIES");
            let brief = brief_of(name).unwrap_or_else(|| panic!("'{name}' has no catalog line: add one to toolscope::BRIEFS"));
            assert!(!brief.is_empty() && brief.chars().count() <= 60, "'{name}': its catalog line is one short line (<= 60 chars)");
        }
        for (name, _) in BRIEFS {
            assert!(c.index(name).is_some(), "BRIEFS names '{name}', which is no tool");
        }
        for f in FAMILIES {
            assert!(c.families().any(|(_, n, _)| n == f.name), "the family '{}' has no tools", f.name);
        }
        for r in ROLES {
            for e in r.families.iter().chain(r.tools) {
                assert!(*e == GAME_FAMILY || c.select([*e]).is_ok(), "role '{}' names '{e}', which is no family or tool", r.name);
            }
        }
        for name in CORE {
            assert!(c.index(name).is_some(), "core tool '{name}' missing");
        }
    }

    /// A new tool says which job it is for: it is in a kit (listed, or among those the job
    /// sometimes needs) or in NOT_IN_A_KIT with the reason. Kits name only real tools.
    #[test]
    fn every_tool_is_in_a_kit_or_says_why() {
        let t = tools();
        let c = &t.scopes().catalog;
        let mut missing = Vec::new();
        for i in 0..c.len() {
            let name = c.name(i);
            if c.family_name(i) == GAME_FAMILY {
                continue;
            }
            let in_kit = KITS.iter().any(|k| k.tools.contains(&name) || k.more.contains(&name));
            let excused = toolkits::NOT_IN_A_KIT.iter().any(|(n, _)| *n == name);
            assert!(!(in_kit && excused), "'{name}' is in a kit and in NOT_IN_A_KIT");
            if !in_kit && !excused {
                missing.push(name.to_owned());
            }
        }
        assert!(
            missing.is_empty(),
            "no kit names {missing:?}: put each in the kit of the job it is for (toolkits::KITS: `tools` if the job starts with it, else `more`), or in NOT_IN_A_KIT with the reason"
        );
        for k in KITS {
            for e in k.tools.iter().chain(k.more) {
                assert!(*e == GAME_FAMILY || c.index(e).is_some(), "the kit '{}' names '{e}', which is no tool", k.name);
            }
            // Every tool the steps or the done check name is the kit's (listed, or among those it
            // sometimes needs): an agent capped to its kit can do what the kit tells it to
            // (review R1 of m5-toolkits: check_physics told agents to use world_set, which no
            // part of the kit held).
            for word in k.steps.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')).chain(k.done.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))) {
                if c.index(word).is_some() {
                    assert!(k.tools.contains(&word) || k.more.contains(&word), "the kit '{}' tells agents to use {word}, which is in neither its `tools` nor its `more`", k.name);
                }
            }
        }
        for (name, _) in toolkits::NOT_IN_A_KIT {
            assert!(c.index(name).is_some(), "NOT_IN_A_KIT names '{name}', which is no tool");
        }
    }

    /// The owner's ask (2026-11-12): an agent with a job finds its few tools at once, cheaply.
    #[test]
    fn a_kit_is_found_shown_and_activated_by_its_job() {
        let t = tools();
        let a = session("a");
        opened(&t, &a, "");
        // The catalog says kits exist; the list of them is small.
        let (_, cat) = call_json(&t, &a, "tools_catalog", "{}");
        assert!(cat.get("kits").and_then(Json::as_str).is_some_and(|k| k.contains("make_menu")), "{}", cat.get("kits").map(Json::to_string).unwrap_or_default());
        let (err, list) = call_json(&t, &a, "tools_catalog", r#"{"kits": true}"#);
        assert!(!err && list.get("families").is_none(), "the kit list carries no families");
        assert_eq!(list.get("kits").unwrap().as_object().unwrap().len(), KITS.len());
        assert!(list.to_string().len() < 6 * 1024, "{} bytes", list.to_string().len());
        // One kit: tools with lines and tags, the ones it sometimes needs, the steps, the done check.
        let (err, one) = call_json(&t, &a, "tools_catalog", r#"{"kit": "make_menu"}"#);
        assert!(!err, "{one}");
        let kit = one.get("kit").unwrap();
        let check = kit.get("tools").unwrap().get("ui_check").unwrap();
        assert_eq!(check.get("tags").unwrap().to_string(), r#"["reads","checks"]"#);
        assert_eq!(check.get("listed").and_then(Json::as_bool), Some(false));
        assert!(kit.get("more").unwrap().get("l10n_check").is_some() && kit.get("steps").is_some());
        assert!(kit.get("done").and_then(Json::as_str).is_some_and(|d| d.contains("ui_check")));
        assert!(one.get("tags").unwrap().get("checks").is_some() && one.get("tags").unwrap().get("owner").is_none(), "the legend holds the tags this kit uses");
        assert!(one.to_string().len() < 4 * 1024, "{} bytes", one.to_string().len());
        // Activating it lists exactly its tools beside the core.
        let (err, got) = call_json(&t, &a, "tools_activate", r#"{"tools": ["kit:make_menu"], "schemas": false}"#);
        assert!(!err, "{got}");
        let mut now = listed(&t, &a);
        now.retain(|n| !CORE.contains(&n.as_str()));
        now.sort();
        assert_eq!(now, ["project_read", "project_write", "ui_act", "ui_check", "ui_frame", "ui_tree"]);
        let (_, one) = call_json(&t, &a, "tools_catalog", r#"{"kit": "kit:make_menu"}"#);
        assert_eq!(one.get("kit").unwrap().get("tools").unwrap().get("ui_check").unwrap().get("listed").and_then(Json::as_bool), Some(true));
        // A kit is an entry wherever families are: the connect URL too.
        let b = session("b");
        opened(&t, &b, "tools=kit:translate");
        assert!(listed(&t, &b).contains(&"l10n_set".to_owned()) && !listed(&t, &b).contains(&"world_spawn".to_owned()));
        // Unknown kits and tags are refused with what exists, and a huge name isn't echoed.
        let (err, e) = call_json(&t, &a, "tools_activate", r#"{"tools": ["kit:make_menus"]}"#);
        assert!(err && e.to_string().contains("did you mean 'kit:make_menu'"), "{e}");
        let (err, e) = call_json(&t, &a, "tools_catalog", &Json::obj([("kit", Json::from("z".repeat(100_000)))]).to_string());
        assert!(err && e.to_string().len() < 1024, "{} bytes", e.to_string().len());
        let (err, e) = call_json(&t, &a, "tools_catalog", r#"{"kit": "make_menus"}"#);
        assert!(err && e.to_string().contains("build_place"), "an unknown kit is answered with the kits there are: {e}");
        let (err, e) = call_json(&t, &a, "tools_catalog", r#"{"tag": "fast"}"#);
        assert!(err && e.to_string().contains("checks"), "{e}");
        // By tag: what says a job is done, across every family.
        let (_, checks) = call_json(&t, &a, "tools_catalog", r#"{"tag": "checks"}"#);
        let fams = checks.get("families").unwrap();
        assert!(fams.get("ui").unwrap().get("tools").unwrap().get("ui_check").is_some() && fams.get("ui").unwrap().get("tools").unwrap().get("ui_act").is_none(), "{fams}");
        assert!(fams.get("lex").unwrap().get("tools").unwrap().get("lex_validate").is_some() && fams.get("world").is_none());
    }

    /// Review R1 of m5-toolkits, the major: a lead caps an agent to a kit. The agent lists the
    /// kit's tools, and may call the ones the kit sometimes needs (it could take a claim and
    /// never release it, and never run the kit's own done check). Beyond the kit it is refused.
    #[test]
    fn a_cap_by_kit_reaches_what_the_kit_sometimes_needs() {
        let t = tools();
        let (lead, smith) = (session("L"), session("S"));
        opened(&t, &lead, "");
        opened(&t, &smith, "");
        call_json(&t, &lead, "agent_hello", r#"{"name": "lena", "role": "lead"}"#);
        let (err, s) = call_json(&t, &lead, "tools_scope", r#"{"agent": "smith", "tools": ["kit:build_place"], "cap": true}"#);
        assert!(!err, "{s}");
        call_json(&t, &smith, "agent_hello", r#"{"name": "smith"}"#);
        let mut now = listed(&t, &smith);
        now.retain(|n| !CORE.contains(&n.as_str()));
        now.sort();
        assert_eq!(now, ["claim_take", "sight_frame", "world_query", "world_set", "world_spawn"], "it lists the kit's tools");
        // The kit's `more` tools answer (the stand-in engine "runs" them): not refused by the cap.
        for name in ["claim_release", "style_check", "world_place", "world_batch", "sight_compare"] {
            let (err, e) = call_json(&t, &smith, name, "{}");
            assert!(e.get("error").and_then(Json::as_str) != Some("not_in_scope"), "{name} is one of the kit's: {err} {e}");
        }
        let (err, got) = call_json(&t, &smith, "tools_activate", r#"{"tools": ["world_batch", "claim_release"], "schemas": false}"#);
        assert!(!err, "the kit's other tools can be listed: {got}");
        // Beyond the kit: refused, another kit too.
        for (name, args) in [("lex_edit", "{}"), ("tools_activate", r#"{"tools": ["kit:write_rules"]}"#), ("tools_invoke", r#"{"tool": "gi_bake"}"#)] {
            let (err, e) = call_json(&t, &smith, name, args);
            assert!(err && e.get("error").and_then(Json::as_str) == Some("not_in_scope"), "{name}: {e}");
        }
        // Without a cap, a kit only lists: nothing else changes.
        let (_, s) = call_json(&t, &lead, "tools_scope", r#"{"agent": "jo", "tools": ["kit:translate"]}"#);
        assert_eq!(s.get("capped").and_then(Json::as_bool), Some(false), "{s}");
    }

    /// Review R1 of m5-toolkits: an activation's answer shows the tools as the list beside it
    /// does (world_set by reference beside world_spawn), and the catalog refuses arguments it
    /// would otherwise ignore.
    #[test]
    fn an_activation_answers_what_the_list_shows_and_the_catalog_ignores_nothing() {
        let t = tools();
        let a = session("a");
        opened(&t, &a, "");
        let (err, got) = call_json(&t, &a, "tools_activate", r#"{"tools": ["kit:build_place"]}"#);
        assert!(!err, "{got}");
        let defs = got.get("tools").unwrap().as_array().unwrap();
        let set = defs.iter().find(|d| d.get("name").and_then(Json::as_str) == Some("world_set")).expect("world_set's definition");
        assert!(set.to_string().contains("as world_spawn's"), "by reference, as tools/list shows it");
        let spawn = defs.iter().find(|d| d.get("name").and_then(Json::as_str) == Some("world_spawn")).expect("world_spawn's definition");
        assert!(!spawn.to_string().contains("as world_set's") && spawn.to_string().len() > set.to_string().len());
        // Activated alone in a fresh session, world_set is answered in full.
        let b = session("b");
        opened(&t, &b, "");
        let (_, alone) = call_json(&t, &b, "tools_activate", r#"{"tools": ["world_set"]}"#);
        assert!(!alone.get("tools").unwrap().to_string().contains("as world_spawn's"));
        // 'kit' and 'kits' answer on their own.
        for args in [r#"{"kits": true, "filter": "menu"}"#, r#"{"kit": "make_menu", "tag": "checks"}"#, r#"{"kit": "make_menu", "kits": true}"#, r#"{"kits": true, "roles": true}"#] {
            let (err, e) = call_json(&t, &a, "tools_catalog", args);
            assert!(err && e.to_string().contains("on their own"), "{args}: {e}");
        }
        let (err, one) = call_json(&t, &a, "tools_catalog", r#"{"kit": "  make_menu "}"#);
        assert!(!err && one.get("kit").is_some(), "a kit's name is trimmed like any entry: {one}");
        // A tag nothing has says so.
        let (err, none) = call_json(&t, &a, "tools_catalog", r#"{"tag": "owner", "family": "world"}"#);
        assert!(!err && none.get("hint").and_then(Json::as_str).is_some_and(|h| h.contains("owner")), "{none}");
    }

    /// The numbers TOOLS.md quotes for kits (run with --nocapture), and their bound: a kit with
    /// the core costs a fraction of a role.
    #[test]
    fn kits_list_small() {
        let t = tools();
        let c = &t.scopes().catalog;
        let mut sizes = Vec::new();
        for k in KITS {
            let mut s = k.tools(c);
            s.union(c.core());
            let bytes = c.list_bytes(&s);
            println!("  kit {:16} {:2} tools {:6} bytes", k.name, s.count(), bytes);
            assert!(bytes <= 36 * 1024, "the kit '{}' lists {bytes} bytes: move tools to `more`", k.name);
            sizes.push(bytes);
        }
        sizes.sort_unstable();
        println!("  kits: median {} bytes, largest {} bytes", sizes[sizes.len() / 2], sizes[sizes.len() - 1]);
        assert!(sizes[sizes.len() / 2] <= 16 * 1024, "the median kit stays under 16 KB (about 4,000 tokens): {}", sizes[sizes.len() / 2]);
        // world_set beside world_spawn lists its components by reference; alone, in full.
        let both = c.select(["world_spawn", "world_set"]).unwrap();
        let alone = c.select(["world_set"]).unwrap();
        let spawn = c.select(["world_spawn"]).unwrap();
        let listing = c.listing(&both);
        let set = listing.iter().find(|d| d.name == "world_set").unwrap();
        assert!(set.description.contains("Listed beside world_spawn") && set.input_schema.to_string().contains("as world_spawn's material"), "{}", set.description);
        assert_eq!(c.listing(&alone)[0], *c.def(c.index("world_set").unwrap()), "alone it is listed in full");
        let json = Json::obj([("tools", Json::arr(listing.iter().map(listed_json)))]).to_string();
        assert_eq!(c.list_bytes(&both), json.len(), "list_bytes counts the shared listing");
        println!("  world_spawn + world_set: {} bytes listed ({} in full)", json.len(), c.list_bytes(&alone) + c.list_bytes(&spawn) - 12);
        assert!(json.len() + 6 * 1024 < c.list_bytes(&alone) + c.list_bytes(&spawn), "sharing saves at least 6 KB");
    }

    /// The numbers TOOLS.md quotes (run with --nocapture to see them), and their bounds.
    #[test]
    fn list_sizes_per_role_and_the_catalog() {
        let t = tools();
        let c = &t.scopes().catalog;
        let full = c.list_bytes(&c.full());
        let core = c.list_bytes(c.core());
        println!("tools/list: all {} tools {} bytes; core {} tools {} bytes", c.len(), full, c.core().count(), core);
        for r in ROLES {
            let mut s = c.role(r.name).unwrap();
            s.union(c.core());
            println!("  role {:14} {:3} tools {:6} bytes", r.name, s.count(), c.list_bytes(&s));
        }
        assert!(core < 8 * 1024, "the core list stays small: {core} bytes");
        let (err, cat) = call_json(&t, &session("a"), "tools_catalog", "{}");
        assert!(!err);
        let bytes = cat.to_string().len();
        println!("tools_catalog: {bytes} bytes, one page: {}", cat.get("next_cursor").is_none());
        assert!(bytes <= MAX_CATALOG_BYTES + 2048, "{bytes}");
        assert!(cat.get("next_cursor").is_none(), "the whole catalog fits one page");
        let names: usize = cat.get("families").unwrap().as_object().unwrap().iter().map(|(_, f)| f.get("tools").unwrap().as_object().unwrap().len()).sum();
        assert_eq!(names, c.len(), "every tool is in the catalog");
    }

    #[test]
    fn a_new_session_lists_the_core_then_activates() {
        let t = tools();
        let a = session("a");
        opened(&t, &a, "");
        assert_eq!(listed(&t, &a), ["tools_catalog", "tools_activate", "tools_deactivate", "tools_invoke", "tools_request", "agent_hello"]);
        let v0 = version(&t, &a);
        let (err, got) = call_json(&t, &a, "tools_activate", r#"{"tools": ["sight", "anim_check"]}"#);
        assert!(!err, "{got}");
        let added: Vec<&str> = got.get("added").unwrap().as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert!(added.contains(&"sight_frame") && added.contains(&"camera_set") && added.contains(&"window_view") && added.contains(&"anim_check"));
        let schemas = got.get("tools").unwrap().as_array().unwrap();
        assert!(schemas.iter().any(|d| d.get("name").and_then(Json::as_str) == Some("sight_frame") && d.get("inputSchema").is_some()), "the answer carries the schemas");
        assert_ne!(version(&t, &a), v0, "list_changed will be sent");
        assert!(listed(&t, &a).contains(&"anim_check".to_owned()));
        // Unlisted tools still answer (no cap): scripts and old clients keep working.
        let (err, _) = call_json(&t, &a, "world_hash", "{}");
        assert!(!err);
        let (err, e) = call_json(&t, &a, "tools_activate", r#"{"tools": ["wrld"]}"#);
        assert!(err && e.get("error").and_then(Json::as_str) == Some("unknown_tools") && e.to_string().contains("did you mean 'world'"), "{e}");
        let (_, d) = call_json(&t, &a, "tools_deactivate", r#"{"all": true}"#);
        assert_eq!(d.get("listed").and_then(Json::as_u64), Some(CORE.len() as u64));
        // Bad connect options are refused at initialize.
        assert!(specula_protocol::ToolHandler::session_opened(&t, &session("b"), "tools=nope").is_err());
    }

    #[test]
    fn hello_picks_a_role_or_the_tools_asked_for() {
        let t = tools();
        let a = session("a");
        opened(&t, &a, "");
        let (err, hi) = call_json(&t, &a, "agent_hello", r#"{"name": "ann", "role": "playtester"}"#);
        assert!(!err, "{hi}");
        assert_eq!(hi.get("tool_scope").unwrap().get("from").and_then(Json::as_str), Some("role playtester"));
        let l = listed(&t, &a);
        assert!(l.contains(&"play_state".to_owned()) && l.contains(&"playtest_run".to_owned()) && !l.contains(&"world_spawn".to_owned()), "{l:?}");
        let (err, hi) = call_json(&t, &a, "agent_hello", r#"{"name": "ann", "tools": ["world"]}"#);
        assert!(!err, "{hi}");
        assert!(listed(&t, &a).contains(&"world_spawn".to_owned()) && !listed(&t, &a).contains(&"play_state".to_owned()));
        let (err, e) = call_json(&t, &a, "agent_hello", r#"{"name": "ann", "tools": ["wrold"]}"#);
        assert!(err && e.get("field").and_then(Json::as_str) == Some("tools"), "{e}");
        // A hello that names no role (its role defaults to builder) keeps the core-only list.
        let c = session("c");
        opened(&t, &c, "");
        call_json(&t, &c, "agent_hello", r#"{"name": "cy"}"#);
        assert_eq!(listed(&t, &c).len(), CORE.len());
        // A URL choice stands over a role.
        let b = session("b");
        opened(&t, &b, "tools=all");
        call_json(&t, &b, "agent_hello", r#"{"name": "bob", "role": "builder"}"#);
        assert_eq!(listed(&t, &b).len(), t.scopes().catalog.len());
    }

    #[test]
    fn only_the_lead_scopes_others_and_the_cap_holds() {
        let t = tools();
        let (lead, worker, other) = (session("L"), session("W"), session("O"));
        for s in [&lead, &worker, &other] {
            opened(&t, s, "");
        }
        call_json(&t, &other, "agent_hello", r#"{"name": "olga", "role": "builder"}"#);
        let (err, e) = call_json(&t, &other, "tools_scope", r#"{"agent": "will", "tools": ["world"]}"#);
        assert!(err && e.get("error").and_then(Json::as_str) == Some("lead_only"), "{e}");
        call_json(&t, &lead, "agent_hello", r#"{"name": "lena", "role": "lead"}"#);
        // Scoped before the worker says hello: kept for its name.
        let (err, s) = call_json(&t, &lead, "tools_scope", r#"{"agent": "will", "tools": ["world", "sight_frame"], "cap": true}"#);
        assert!(!err, "{s}");
        assert_eq!(s.get("sessions").and_then(Json::as_u64), Some(0));
        let (_, hi) = call_json(&t, &worker, "agent_hello", r#"{"name": "will", "role": "builder"}"#);
        assert_eq!(hi.get("tool_scope").unwrap().get("capped").and_then(Json::as_bool), Some(true), "{hi}");
        let l = listed(&t, &worker);
        assert!(l.contains(&"world_spawn".to_owned()) && l.contains(&"sight_frame".to_owned()) && !l.contains(&"claim_take".to_owned()), "{l:?}");
        // Beyond the cap: refused directly, through tools_invoke and in activation.
        for (name, args) in [
            ("claim_take", "{}"),
            ("tools_invoke", r#"{"tool": "claim_take"}"#),
            ("tools_activate", r#"{"tools": ["claim"]}"#),
            ("tools_scope", r#"{"agent": "x", "role": "lead"}"#),
        ] {
            let (err, e) = call_json(&t, &worker, name, args);
            assert!(err && e.get("error").and_then(Json::as_str) == Some("not_in_scope"), "{name}: {e}");
        }
        let (err, _) = call_json(&t, &worker, "tools_invoke", r#"{"tool": "world_query", "arguments": {}}"#);
        assert!(!err, "within the cap, tools_invoke runs it");
        // A request goes to the board with the search's findings.
        let (err, r) = call_json(&t, &worker, "tools_request", r#"{"need": "claim a region", "why": "to build the gate"}"#);
        assert!(!err, "{r}");
        assert_eq!(r.get("task").and_then(Json::as_u64), Some(7));
        assert!(r.get("beyond_your_cap").unwrap().to_string().contains("claim_take"), "{r}");
        // The lead re-scopes the open session at once; clear puts it back to the default.
        let v = version(&t, &worker);
        let (_, s) = call_json(&t, &lead, "tools_scope", r#"{"agent": "will", "role": "builder"}"#);
        assert_eq!((s.get("sessions").and_then(Json::as_u64), s.get("capped").and_then(Json::as_bool)), (Some(1), Some(false)));
        assert_ne!(version(&t, &worker), v, "the worker is told list_changed");
        assert!(listed(&t, &worker).contains(&"claim_take".to_owned()));
        let (_, c) = call_json(&t, &lead, "tools_scope", r#"{"agent": "will", "clear": true}"#);
        assert_eq!(c.get("cleared").and_then(Json::as_bool), Some(true));
        assert_eq!(listed(&t, &worker).len(), CORE.len());
    }

    #[test]
    fn catalog_filters_and_requests_search() {
        let t = tools();
        let a = session("a");
        let (_, cat) = call_json(&t, &a, "tools_catalog", r#"{"filter": "navmesh path"}"#);
        let fams = cat.get("families").unwrap();
        assert!(fams.get("nav").unwrap().get("tools").unwrap().get("nav_path").is_some(), "{cat}");
        // (Painting vertex colours was this test's example of something no tool did, until the
        // editor's draw tools brought `model_paint`.)
        let (_, paint) = call_json(&t, &a, "tools_catalog", r#"{"filter": "paint vertex colours"}"#);
        assert_eq!(paint.get("matches").and_then(Json::as_u64), Some(1), "{paint}");
        let (_, none) = call_json(&t, &a, "tools_catalog", r#"{"filter": "fold proteins quickly"}"#);
        assert_eq!(none.get("matches").and_then(Json::as_u64), Some(0));
        assert!(none.get("closest").is_some(), "{none}");
        let (err, e) = call_json(&t, &a, "tools_catalog", r#"{"family": "wrld"}"#);
        assert!(err, "{e}");
        let (_, roles) = call_json(&t, &a, "tools_catalog", r#"{"roles": true, "family": "tools"}"#);
        assert!(roles.get("roles").unwrap().get("builder").unwrap().get("list_bytes").is_some());
    }

    /// Review probes: 80 KB of "q," took 10 s and answered 200 KB; a 1 MB tool name answered
    /// 2 MB; a lead's "world," x 500,000 made every later catalog answer carry it.
    #[test]
    fn inputs_are_bounded_in_work_and_answers() {
        let t = tools();
        let a = session("a");
        opened(&t, &a, "");
        let size = |out: &ToolOutput| match &out.content[0] {
            Content::Text(s) => s.len(),
            Content::Image { .. } => 0,
        };
        let call = |name: &str, args: Json| specula_protocol::ToolHandler::call(&t, &a, name, &args);
        let started = std::time::Instant::now();
        let out = call("tools_activate", Json::obj([("tools", Json::from("q,".repeat(1_000_000)))]));
        assert!(out.is_error && size(&out) < 400, "{} bytes", size(&out));
        let out = call("tools_activate", Json::obj([("tools", Json::from("q,".repeat(MAX_ENTRIES - 1)))]));
        assert!(out.is_error && size(&out) < 1024, "{} bytes", size(&out));
        let out = call("tools_activate", Json::obj([("tools", Json::arr([Json::from("z".repeat(100_000))]))]));
        assert!(out.is_error && size(&out) < 400, "{} bytes", size(&out));
        let out = call("tools_invoke", Json::obj([("tool", Json::from("y".repeat(1_000_000)))]));
        assert!(out.is_error && size(&out) < 400, "{} bytes", size(&out));
        assert!(started.elapsed() < std::time::Duration::from_secs(5), "{:?}", started.elapsed());
        // The lead's label comes from the deduplicated entries.
        let lead = session("L");
        opened(&t, &lead, "");
        call_json(&t, &lead, "agent_hello", r#"{"name": "lena", "role": "lead"}"#);
        let (err, _) = call_json(&t, &lead, "tools_scope", &Json::obj([("agent", Json::from("lbl")), ("tools", Json::from("world,".repeat(200)))]).to_string());
        assert!(!err);
        let w = session("W");
        opened(&t, &w, "");
        call_json(&t, &w, "agent_hello", r#"{"name": "lbl"}"#);
        let (_, cat) = call_json(&t, &w, "tools_catalog", r#"{"family": "tools"}"#);
        let from = cat.get("scope").unwrap().get("from").and_then(Json::as_str).unwrap().to_owned();
        assert!(from.starts_with("tools world, by the lead lena"), "{from}");
        let (err, e) = call_json(&t, &lead, "tools_scope", &Json::obj([("agent", Json::from("lbl")), ("tools", Json::from("world,".repeat(500_000)))]).to_string());
        assert!(err && e.to_string().len() < 400, "{e}");
    }

    /// Review probe: tools_request is core and uncapped, so a loop could fill the board. Open
    /// requests are deduplicated per agent, and at most MAX_OPEN_REQUESTS stay open per agent.
    #[test]
    fn tool_requests_are_deduplicated_and_capped_per_agent() {
        let t = tools();
        let (a, b) = (session("a"), session("b"));
        for s in [&a, &b] {
            opened(&t, s, "");
        }
        call_json(&t, &a, "agent_hello", r#"{"name": "ann"}"#);
        call_json(&t, &b, "agent_hello", r#"{"name": "bea"}"#);
        let mut ids = std::collections::BTreeSet::new();
        for i in 0..20 {
            let need = if i % 2 == 0 { "paint vertex colours" } else { "Paint  VERTEX colours" };
            let (err, r) = call_json(&t, &a, "tools_request", &Json::obj([("need", Json::from(need))]).to_string());
            assert!(!err, "{r}");
            ids.insert(r.get("task").and_then(Json::as_u64).unwrap());
            assert_eq!(r.get("duplicate").is_some(), i > 0, "{r}");
        }
        assert_eq!(ids.len(), 1, "20 identical requests are one task");
        for i in 1..MAX_OPEN_REQUESTS {
            let (err, r) = call_json(&t, &a, "tools_request", &Json::obj([("need", Json::from(format!("need number {i}")))]).to_string());
            assert!(!err, "{r}");
        }
        let (err, e) = call_json(&t, &a, "tools_request", r#"{"need": "one too many"}"#);
        assert!(err && e.get("error").and_then(Json::as_str) == Some("too_many_requests"), "{e}");
        assert_eq!(e.get("open").and_then(Json::as_array).map(<[Json]>::len), Some(MAX_OPEN_REQUESTS));
        let (err, r) = call_json(&t, &b, "tools_request", r#"{"need": "paint vertex colours"}"#);
        assert!(!err && r.get("duplicate").is_none(), "another agent's request is its own: {r}");
    }
}
