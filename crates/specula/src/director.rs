//! The director pattern's planning step: a design-doc section (Markdown) becomes a plan of board
//! tasks (`task_plan`; docs/design/REVIEW.md, "The director pattern").
//!
//! In the section, every **unchecked** checklist item (`- [ ] ...`) is a task; checked ones
//! (`- [x]`) are already done and skipped (counted). Nesting follows indentation: a checklist item
//! indented under another is its subtask, and other lines indented under an item (bullets, text)
//! are its detail. In an item's text, `#role` words give its role (`#builder`, `#artist`), and
//! `(after: <words>)` makes it depend on the earlier item whose title starts with those words.
//! Pure and deterministic: the same text gives the same plan.

use specula_core::Json;

/// Items per plan.
pub const MAX_ITEMS: usize = 200;
/// A plan document's size.
pub const MAX_DOC_BYTES: usize = 1 << 20;
const MAX_TITLE: usize = crate::board::MAX_TITLE;
const MAX_DETAIL: usize = crate::board::MAX_DETAIL;

#[derive(Clone, Debug, PartialEq)]
pub struct PlanItem {
    /// `1`, `2`, `2.1`... for parsed sections; the caller's for JSON items.
    pub key: String,
    pub title: String,
    pub detail: String,
    pub role: Option<String>,
    /// Keys of items this one waits for.
    pub after: Vec<String>,
    /// The key of the item it is part of.
    pub parent: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub heading: String,
    pub items: Vec<PlanItem>,
    /// Checked items skipped (already done).
    pub done_skipped: usize,
}

fn heading_of(line: &str) -> Option<(usize, String)> {
    let t = line.trim_start();
    let level = t.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > 6 || !t[level..].starts_with(' ') {
        return None;
    }
    Some((level, clean(t[level..].trim())))
}

/// Text without Markdown emphasis and links' targets, trimmed.
fn clean(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '`' => {}
            ']' if chars.peek() == Some(&'(') => {
                // [text](target): keep the text.
                for d in chars.by_ref() {
                    if d == ')' {
                        break;
                    }
                }
            }
            '[' => {}
            _ => out.push(c),
        }
    }
    out.trim().trim_end_matches(':').trim().to_owned()
}

fn norm(s: &str) -> String {
    s.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A checklist line: indent, checked, text.
fn checkbox(line: &str) -> Option<(usize, bool, &str)> {
    let indent = line.len() - line.trim_start().len();
    let t = line.trim_start();
    let rest = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).or_else(|| t.strip_prefix("+ "))?;
    let (checked, text) = match rest.strip_prefix("[ ] ") {
        Some(r) => (false, r),
        None => (true, rest.strip_prefix("[x] ").or_else(|| rest.strip_prefix("[X] "))?),
    };
    Some((indent, checked, text))
}

/// Takes `#role` words and `(after: ...)` out of an item's text.
fn annotations(text: &str) -> (String, Option<String>, Vec<String>) {
    let mut afters = Vec::new();
    let mut rest = text.to_owned();
    while let Some(start) = rest.find("(after:") {
        let Some(len) = rest[start..].find(')') else { break };
        afters.push(rest[start + 7..start + len].trim().to_owned());
        rest.replace_range(start..start + len + 1, "");
    }
    let mut role = None;
    let mut words = Vec::new();
    for w in rest.split_whitespace() {
        let tag = w.strip_prefix('#').map(|r| r.trim_end_matches([',', '.', ';']));
        match tag {
            Some(r) if !r.is_empty() && r.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') => {
                if role.is_none() {
                    role = Some(r.to_ascii_lowercase());
                }
            }
            _ => words.push(w),
        }
    }
    (clean(&words.join(" ")), role, afters)
}

fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(n - 1).collect();
        out.push('…');
        out
    }
}

/// The plan in `text`'s section `section` (matched without case or emphasis: exactly, else the
/// one heading that contains it).
pub fn parse_section(text: &str, section: &str) -> Result<Plan, String> {
    let want = norm(&clean(section));
    if want.is_empty() {
        return Err("'section' must name a heading of the document".into());
    }
    let lines: Vec<&str> = text.lines().collect();
    let heads: Vec<(usize, usize, String)> = lines.iter().enumerate().filter_map(|(i, l)| heading_of(l).map(|(lv, h)| (i, lv, h))).collect();
    let exact: Vec<&(usize, usize, String)> = heads.iter().filter(|(_, _, h)| norm(h) == want).collect();
    let found = if exact.len() == 1 {
        exact[0]
    } else {
        let partial: Vec<&(usize, usize, String)> = heads.iter().filter(|(_, _, h)| norm(h).contains(&want)).collect();
        match (exact.len(), partial.len()) {
            (0, 1) => partial[0],
            (0, 0) => {
                let list: Vec<String> = heads.iter().take(40).map(|(_, _, h)| h.clone()).collect();
                return Err(format!("no heading matches '{section}'; headings: {}", list.join(" | ")));
            }
            _ => {
                let list: Vec<String> = (if exact.len() > 1 { exact } else { partial }).iter().take(10).map(|(i, _, h)| format!("{h} (line {})", i + 1)).collect();
                return Err(format!("'{section}' matches several headings; name one exactly: {}", list.join(" | ")));
            }
        }
    };
    let (start, level, heading) = (found.0, found.1, found.2.clone());
    let end = heads.iter().find(|(i, lv, _)| *i > start && *lv <= level).map_or(lines.len(), |(i, _, _)| *i);
    let mut items: Vec<PlanItem> = Vec::new();
    // Open items by indent: (indent, index into items).
    let mut stack: Vec<(usize, usize)> = Vec::new();
    // Items' children counters for keys.
    let mut counts: Vec<usize> = Vec::new();
    let mut top = 0usize;
    let mut skipped = 0usize;
    // Indent of a checked item being skipped (its subtree goes with it).
    let mut skipping: Option<usize> = None;
    let mut pending_after: Vec<(usize, Vec<String>)> = Vec::new();
    for line in &lines[start + 1..end] {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if let Some(s) = skipping {
            if indent > s {
                continue;
            }
            skipping = None;
        }
        match checkbox(line) {
            Some((ind, checked, body)) => {
                while stack.last().is_some_and(|(i, _)| *i >= ind) {
                    stack.pop();
                }
                if checked {
                    skipped += 1;
                    skipping = Some(ind);
                    continue;
                }
                if items.len() >= MAX_ITEMS {
                    return Err(format!("the section has more than {MAX_ITEMS} open items; plan a smaller section"));
                }
                let (title, role, afters) = annotations(body);
                let (key, parent) = match stack.last() {
                    Some((_, p)) => {
                        counts[*p] += 1;
                        (format!("{}.{}", items[*p].key, counts[*p]), Some(items[*p].key.clone()))
                    }
                    None => {
                        top += 1;
                        (top.to_string(), None)
                    }
                };
                let title = if title.is_empty() { format!("item {key}") } else { cut(&title, MAX_TITLE) };
                let idx = items.len();
                items.push(PlanItem { key, title, detail: String::new(), role, after: Vec::new(), parent });
                counts.push(0);
                if !afters.is_empty() {
                    pending_after.push((idx, afters));
                }
                stack.push((ind, idx));
            }
            None => {
                // Detail of the innermost item it is indented under.
                if heading_of(line).is_some() {
                    stack.clear();
                    continue;
                }
                if let Some((_, idx)) = stack.iter().rev().find(|(i, _)| indent > *i) {
                    let d = &mut items[*idx].detail;
                    let text = clean(line.trim().trim_start_matches(['-', '*', '+']).trim());
                    if !text.is_empty() && d.chars().count() < MAX_DETAIL {
                        if !d.is_empty() {
                            d.push('\n');
                        }
                        d.push_str(&text);
                        *d = cut(d, MAX_DETAIL);
                    }
                } else {
                    stack.clear();
                }
            }
        }
    }
    for (idx, afters) in pending_after {
        for a in afters {
            let w = norm(&a);
            let dep = items[..idx].iter().rev().find(|it| norm(&it.title).starts_with(&w)).map(|it| it.key.clone());
            match dep {
                Some(k) => items[idx].after.push(k),
                None => return Err(format!("item '{}' says (after: {a}), but no earlier item's title starts with that", items[idx].title)),
            }
        }
    }
    Ok(Plan { heading, items, done_skipped: skipped })
}

/// Plan items given as JSON (`task_plan {items}`): `[{key, title, detail?, role?, after?: [keys],
/// parent?: key}]`, each key unique and every reference to an earlier item.
pub fn parse_items(v: &Json) -> Result<Vec<PlanItem>, String> {
    let arr = v.as_array().ok_or("'items' must be an array of {key, title, detail?, role?, after?, parent?}")?;
    if arr.is_empty() || arr.len() > MAX_ITEMS {
        return Err(format!("'items' must hold 1..{MAX_ITEMS} items"));
    }
    let mut out: Vec<PlanItem> = Vec::with_capacity(arr.len());
    for (i, it) in arr.iter().enumerate() {
        crate::tools::check_keys(it, &format!("items[{i}]"), &[&["key", "title", "detail", "role", "after", "parent"]])?;
        let s = |k: &str| it.get(k).and_then(Json::as_str);
        let key = match it.get("key") {
            Some(Json::Num(n)) if n.fract() == 0.0 => format!("{n}"),
            _ => s("key").filter(|k| !k.trim().is_empty() && k.len() <= 64).ok_or(format!("items[{i}].key must be a short unique name or number"))?.to_owned(),
        };
        if out.iter().any(|o| o.key == key) {
            return Err(format!("items[{i}].key '{key}' is used twice"));
        }
        let title = s("title").filter(|t| !t.trim().is_empty()).ok_or(format!("items[{i}].title is required"))?;
        crate::board::check_text(&format!("items[{i}].title"), title, MAX_TITLE, true)?;
        let detail = s("detail").unwrap_or("");
        crate::board::check_text(&format!("items[{i}].detail"), detail, MAX_DETAIL, false)?;
        let role = match s("role") {
            Some(r) if !r.is_empty() && r.len() <= 32 && r.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') => Some(r.to_ascii_lowercase()),
            Some(_) => return Err(format!("items[{i}].role must be a role name like \"builder\"")),
            None => None,
        };
        let refer = |k: &Json| -> Result<String, String> {
            let k = match k {
                Json::Num(n) if n.fract() == 0.0 => format!("{n}"),
                Json::Str(s) => s.clone(),
                _ => return Err(format!("items[{i}]: references are keys")),
            };
            if out.iter().any(|o| o.key == k) {
                Ok(k)
            } else {
                Err(format!("items[{i}] refers to '{k}', which is not an earlier item's key"))
            }
        };
        let after = match it.get("after") {
            None | Some(Json::Null) => Vec::new(),
            Some(Json::Arr(a)) if a.len() <= crate::board::MAX_LINKS => a.iter().map(refer).collect::<Result<_, _>>()?,
            Some(_) => return Err(format!("items[{i}].after must be an array of earlier items' keys")),
        };
        let parent = match it.get("parent") {
            None | Some(Json::Null) => None,
            Some(p) => Some(refer(p)?),
        };
        out.push(PlanItem { key, title: title.trim().to_owned(), detail: detail.to_owned(), role, after, parent });
    }
    Ok(out)
}

/// Who gets each item with a role: agents of that role, round-robin in name order (`roster` is
/// `(name, role)` of connected agents). Items without a role, or whose role nobody has, get none.
pub fn assign_by_role(items: &[PlanItem], roster: &[(String, String)]) -> Vec<Option<String>> {
    let mut by_role: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
    for (name, role) in roster {
        by_role.entry(role.as_str()).or_default().push(name.as_str());
    }
    for v in by_role.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    let mut next: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    items
        .iter()
        .map(|it| {
            let r = it.role.as_deref()?;
            let names = by_role.get(r)?;
            let n = next.entry(r).or_insert(0);
            let name = names[*n % names.len()].to_owned();
            *n += 1;
            Some(name)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# Game\n\nIntro.\n\n## Crypt props\n\nWhat we need:\n\n- [ ] Carve **the gate** #builder\n  - hinges: iron, two\n  - [ ] Gate hinges #artist\n- [x] Floor tiles #builder\n  - [ ] never planned\n- [ ] Light the crypt #artist (after: carve the gate)\n- [ ] Playtest the crypt (after: light) #playtester\n  Check the wraith path.\n\n### Notes\n\n- [ ] not in the section? It is: a subheading is inside.\n\n## Next section\n\n- [ ] elsewhere\n";

    #[test]
    fn a_section_becomes_a_plan() {
        let p = parse_section(DOC, "crypt props").unwrap();
        assert_eq!(p.heading, "Crypt props");
        assert_eq!(p.done_skipped, 1);
        let keys: Vec<(&str, &str)> = p.items.iter().map(|i| (i.key.as_str(), i.title.as_str())).collect();
        assert_eq!(
            keys,
            [("1", "Carve the gate"), ("1.1", "Gate hinges"), ("2", "Light the crypt"), ("3", "Playtest the crypt"), ("4", "not in the section? It is: a subheading is inside.")]
        );
        assert_eq!(p.items[0].role.as_deref(), Some("builder"));
        assert_eq!(p.items[0].detail, "hinges: iron, two");
        assert_eq!(p.items[1].parent.as_deref(), Some("1"));
        assert_eq!(p.items[2].after, ["1"]);
        assert_eq!(p.items[3].after, ["2"]);
        assert_eq!(p.items[3].detail, "Check the wraith path.");
        assert_eq!(p.items[3].role.as_deref(), Some("playtester"));
        // Headings match without case; a missing one lists what there is.
        assert!(parse_section(DOC, "CRYPT").is_ok());
        let e = parse_section(DOC, "kitchen").unwrap_err();
        assert!(e.contains("Crypt props") && e.contains("Next section"), "{e}");
        assert!(parse_section("## A\n- [ ] x (after: nothing)\n", "A").unwrap_err().contains("after: nothing"));
    }

    #[test]
    fn json_items_and_role_assignment() {
        let items = parse_items(&Json::parse(r#"[{"key": "walls", "title": "Walls", "role": "builder"}, {"key": 2, "title": "Roof", "role": "builder", "after": ["walls"]}, {"key": "lamps", "title": "Lamps", "role": "artist", "parent": 2}, {"key": "x", "title": "Free"}]"#).unwrap()).unwrap();
        assert_eq!(items[1].after, ["walls"]);
        assert_eq!(items[2].parent.as_deref(), Some("2"));
        assert!(parse_items(&Json::parse(r#"[{"key": "a", "title": "A", "after": ["b"]}]"#).unwrap()).unwrap_err().contains("earlier"));
        assert!(parse_items(&Json::parse(r#"[{"key": "a", "title": "A"}, {"key": "a", "title": "B"}]"#).unwrap()).is_err());
        assert!(parse_items(&Json::parse(r#"[{"key": "a", "titel": "A"}]"#).unwrap()).is_err());
        let roster = vec![("cat".to_owned(), "builder".to_owned()), ("ann".to_owned(), "builder".to_owned()), ("eve".to_owned(), "reviewer".to_owned())];
        let who = assign_by_role(&items, &roster);
        assert_eq!(who, [Some("ann".to_owned()), Some("cat".to_owned()), None, None], "round-robin in name order; nobody is an artist");
    }
}
