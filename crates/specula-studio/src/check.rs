//! `style.toml`'s rules on an asset's facts: budgets, scale ranges, palette distance, neutral
//! saturation, forbidden families, emissive presets and naming. Every rule that applies gives a
//! [`Finding`] (passed or not), so an agent sees what was checked as well as what failed.

use crate::colour::{chroma, hex};
use crate::facts::AssetFacts;
use crate::form::{RAZOR_DEG, SEAM_DEG};
use crate::palette::Palette;
use crate::rules::StyleRules;
use crate::{Bound, Finding, Report};
use specula_core::Json;
use specula_model::facet::By;

/// Colours a palette finding lists (the largest offenders).
const LISTED_COLOURS: usize = 5;
/// The 1.8 m reference figure scale findings compare against.
pub const FIGURE_M: f32 = 1.8;

fn at(rules: &StyleRules, rule: &str) -> String {
    format!("{} {rule}", rules.source)
}

/// What is said of the coarsest round shape that shows: which of its turns it is held to (its
/// faces', or its widest rim's seen down its axis), by how much, at what radius, and how far
/// its facets stand in (in the finished model).
fn facet_said(r: &specula_model::Round, by: By) -> String {
    let moved = if r.displaced_m > 0.0 { format!(" (under a displacement of up to {:.1} mm, which moves the facets with the curve)", r.displaced_m * 1000.0) } else { String::new() };
    if let (By::Rim, Some(rim)) = (by, r.rim) {
        return format!(
            "its {} at {}: its rim, seen along its axis, turns {:.1}° at a corner at a radius of {:.3} m, and its outline stands {:.1} mm in from the curve (its faces turn {:.1}° from one to the next){moved}",
            r.what,
            r.path,
            rim.turn_deg,
            rim.radius_m,
            rim.sag * 1000.0,
            r.turn_deg
        );
    }
    let turns = match (r.what, r.count) {
        ("capsule", "rings") => format!("its ends turn {:.1}° from one ring to the next", r.turn_deg),
        ("torus", "sides") => format!("its tube turns {:.1}° from one side to the next", r.turn_deg),
        _ => format!("its faces turn {:.1}° from one to the next", r.turn_deg),
    };
    format!("its {} at {}: {turns} at a radius of {:.3} m, and its facets stand {:.1} mm in from the curve{moved}", r.what, r.path, r.turn_radius_m, r.sag_m() * 1000.0)
}

/// How to bring a round shape's turn within `max` degrees: the number that made the turn, and
/// how much of it holds the limit, **only ever a count it can be built with** (the most is
/// the modeller's: its range, and what its part and the model can hold). What is held at
/// another count is what is held now: the faces' turn if its faces are seen (`faces`), the
/// rim's if its rim is (`rim`). `work`: what the whole check has spent on advice so far.
fn facet_fix(r: &specula_model::Round, max: f32, faces: bool, rim: bool, work: &mut u64) -> String {
    let allow = "; or exempt the asset in [form.allow] if the facets are its look";
    let flat = "; a shape meant to have flat sides is an extruded outline of that many sides, which nothing calls round";
    let part = match (r.what, r.count) {
        ("capsule", "rings") => "its ends: ",
        ("torus", "sides") => "its tube: ",
        _ => "",
    };
    let (what, count, has, most) = (r.what, r.count, r.count_value, r.most);
    // (Where the most is short of its range: what it builds with, and why no more.)
    let at_most = |noun: &str| if r.most_by == "its range" { format!("at its most{noun} ({most})") } else { format!("at the most{noun} it can be built with ({most}; past it {} would be exceeded)", r.most_by) };
    let estimated = r.most_estimated.map_or(String::new(), |why| format!(" (the most it can be built with is estimated: {why})"));
    match holding_count(r, max, faces, rim, r.least.max(has.saturating_add(1)), most, work) {
        Holds::At(k, t) => match count {
            "subdivisions" => format!("give the sphere {k} \"subdivisions\" (it has {has}; at {k} it turns {t:.1}° a face, measured as this was){estimated}{allow}"),
            "resolution" => format!("raise the sculpt's \"resolution\" to {k} (it is {has}; at {k} its cells turn the surface {t:.1}° round this primitive's radius of {:.3} m){estimated}, or give the primitive more radius; a shape meant to have flat sides is an extruded outline, not a sculpt of balls", r.radius_m),
            _ => format!("{part}give the {what} {k} \"{count}\" (it has {has}; at {k} it turns {t:.1}°, measured as this was){estimated}{flat}{allow}"),
        },
        Holds::Never(t) => match count {
            "subdivisions" => format!("{} a sphere still turns {t:.1}° a face: build it as a sculpt (\"sdf\") fine enough{estimated}{allow}", at_most(" \"subdivisions\"")),
            _ => format!("{part}the {what}'s \"{count}\" cannot hold the limit: {} it still turns {t:.1}°{estimated}{allow}", at_most("")),
        },
        Holds::Unknown => format!("{part}raise the {what}'s \"{count}\" (it has {has}; it can be built with up to {most}): what it turns at another count could not be measured here, so no count is named{allow}"),
    }
}

/// Most faces and rim corners the whole check measures while looking for counts that hold (a
/// search past it names none). At this bound the slowest recipe the limits allow takes
/// about half a second on the reference machine (`facet_advice`'s
/// `the_advice_work_is_bounded_for_the_whole_check`).
pub const MOST_ADVICE_WORK: u64 = 8_000_000;

enum Holds {
    /// This count holds the limit: the turn it has there.
    At(u32, f32),
    /// No count up to the most holds it: the turn at the most.
    Never(f32),
    /// Not measured at another count (a record made by hand, or a search past its work).
    Unknown,
}

/// A count in `from..=most` at which `r`, measured the same way in the finished model, turns
/// no more than `max` (a count is named only once it has been measured to hold): its faces'
/// turn where `faces`, its rim's where `rim` (the greater of the two). The search starts where
/// a turn that went as one over the count would hold, steps down while the count below holds
/// too (so no smaller count next to it holds) and up while it does not. Every count measured
/// is charged to `work`, the whole check's, against [`MOST_ADVICE_WORK`].
fn holding_count(r: &specula_model::Round, max: f32, faces: bool, rim: bool, from: u32, most: u32, work: &mut u64) -> Holds {
    let mut at = |k: u32| -> Option<f32> {
        // (Counted before it is done, and not done past the bound.)
        let w = work.saturating_add(r.turn_at_work(k));
        if w > MOST_ADVICE_WORK {
            return None;
        }
        *work = w;
        let (f, g) = r.turns_at(k)?;
        let held = [faces.then_some(f), if rim { g } else { None }].into_iter().flatten().fold(0.0f32, f32::max);
        Some(held)
    };
    if from > most {
        return match at(most) {
            Some(t) => Holds::Never(t),
            None => Holds::Unknown,
        };
    }
    // (What it is held to now: the faces' turn, the rim's, or the greater.)
    let now = [faces.then_some(r.turn_deg), r.rim.filter(|_| rim).map(|x| x.turn_deg)].into_iter().flatten().fold(0.0f32, f32::max);
    let guess = match r.count {
        "subdivisions" => from,
        "resolution" => (r.count_value as f32 * (0.5 * now.to_radians()).sin() / (0.5 * max.to_radians()).sin()).ceil() as u32,
        _ => (r.count_value as f32 * now / max).ceil() as u32,
    }
    .clamp(from, most);
    let Some(t) = at(guess) else { return Holds::Unknown };
    if t <= max {
        let (mut k, mut tk) = (guess, t);
        while k > from {
            match at(k - 1) {
                Some(t) if t <= max => (k, tk) = (k - 1, t),
                Some(_) => break,
                None => return Holds::Unknown,
            }
        }
        return Holds::At(k, tk);
    }
    let mut k = guess;
    let mut last = t;
    while k < most {
        k += 1;
        match at(k) {
            Some(t) if t <= max => return Holds::At(k, t),
            Some(t) => last = t,
            None => return Holds::Unknown,
        }
    }
    Holds::Never(last)
}

/// The check `faceting`'s answer at a limit of `max` degrees: the turn it is held to, what is
/// said of it, and its fix. All the advice one check gives is charged to `work` together.
fn faceting_answer(f: &AssetFacts, max: f32, work: &mut u64) -> (f32, String, String) {
    match f.faceting_held() {
        Some((r, turn, by)) => {
            let seen = f.width_m * crate::form::FACET_SEEN;
            let rim = r.rim.is_some_and(|x| x.sag >= seen);
            (turn, facet_said(r, by), if turn > max { facet_fix(r, max, r.sag_m() >= seen, rim, work) } else { String::new() })
        }
        None => (
            0.0,
            if f.round_total == 0 {
                "its recipe names no shape round".to_string()
            } else {
                let unmeasured = f.round_not_measured.max(f.faceting_unmeasured().count()) + f.round_not_recorded;
                format!("none of its {} measured round shapes has facets that show at its size", f.round_total.saturating_sub(unmeasured))
            },
            String::new(),
        ),
    }
}

/// The work the check `faceting`'s advice takes on `facts` at a limit of `max` degrees, in faces
/// and rim corners measured (at most [`MOST_ADVICE_WORK`]): for what bounds it.
pub fn faceting_advice_work(facts: &AssetFacts, max: f32) -> u64 {
    let mut work = 0;
    faceting_answer(facts, max, &mut work);
    work
}

/// Every rule of `rules` that applies to `facts`.
pub fn check(facts: &AssetFacts, rules: &StyleRules, palette: &Palette) -> Report {
    check_with(facts, rules, palette, &|| false)
}

/// [`check`], asking `stop` every [`STOP_EVERY`] colours whether to give up the palette search.
pub fn check_with(facts: &AssetFacts, rules: &StyleRules, palette: &Palette, stop: &dyn Fn() -> bool) -> Report {
    let mut r = Report::default();
    naming(facts, rules, &mut r);
    budget(facts, rules, &mut r);
    scale(facts, rules, &mut r);
    form(facts, rules, &mut r);
    colours(facts, rules, palette, &mut r, stop);
    r
}

/// The form checks: each measure of [`crate::form`] against the limit `[form]` gives it (none
/// given: not checked). A failure `[form.allow]` names for this subject is exempt.
fn form(f: &AssetFacts, rules: &StyleRules, r: &mut Report) {
    let (m, limits, s) = (&f.form, &rules.form, f.subject.as_str());
    if m.triangles == 0 {
        return;
    }
    if let Some(max) = limits.max_facet_deg {
        // From what the recipe names round, where its facets show at the asset's size: a mesh
        // alone cannot say what was meant to be round (three tries at it: docs/design/FORM.md).
        // An asset with nothing named round, or none whose facets show, has nothing to fail.
        // (Checked whether or not the mesh's own form was measured: it is the recipe's to say.)
        let (turn, said, fix) = faceting_answer(f, max, &mut 0);
        let mut finding = Finding::measure("faceting", s, f64::from(turn), f64::from(max), Bound::Max, "deg", at(rules, "form.max_facet_deg"), said, fix);
        if !finding.passed && limits.allows(s, "faceting") {
            finding.exempt = true;
        }
        r.findings.push(finding);
        // A round shape whose facets could not be measured is said, and fails: it is never
        // passed on what was not looked at. It is said in the same answer as a measured one
        // that fails, so that following that one's advice is not taken to be enough.
        // Round shapes made past what one build records are not measured either: any of them
        // could be the answer.
        let kept: Vec<&specula_model::Round> = f.faceting_unmeasured().collect();
        let dropped = f.round_not_recorded;
        if !kept.is_empty() || dropped > 0 {
            let n = f.round_not_measured.max(kept.len());
            let named: Vec<String> = kept.iter().map(|u| format!("its {} at {} was {}", u.what, u.path, u.unmeasured.unwrap_or("not measured"))).collect();
            let more = if n > kept.len() { format!(" (and {} more round shapes not measured)", n - kept.len()) } else { String::new() };
            let past = if dropped > 0 { format!("{}{dropped} round shape{} made past what one build records {} not recorded, so not measured", if n > 0 { "; and " } else { "" }, if dropped == 1 { "" } else { "s" }, if dropped == 1 { "was" } else { "were" }) } else { String::new() };
            let said = if n > 0 { format!("the facets of {n} round shape{} were not measured: {}{more}", if n == 1 { "" } else { "s" }, named.join("; ")) } else { String::new() };
            let fix = match kept.first() {
                Some(u) => format!("as built the first turns {:.1}° a face, which what was done to it changes by an amount the model does not know; no count can be checked to hold for it, so this stays until it is not stretched so or the asset is exempt in [form.allow], if its look is right", u.turn_deg),
                None => "build the model of fewer shapes (every shape of an include counts too), or exempt the asset in [form.allow] if its look is right".to_string(),
            };
            let mut finding = Finding::flag("faceting", s, true, at(rules, "form.max_facet_deg"), format!("{said}{past}"), fix);
            if limits.allows(s, "faceting") {
                finding.exempt = true;
            }
            r.findings.push(finding);
        }
    }
    if let (Some(why), Some(_)) = (m.skipped, limits.max_loose_pieces) {
        // Not measured at all (too many triangles): the loose check fails saying so. It never
        // passes on what it did not look at.
        let mut finding = Finding::flag(
            "loose_pieces",
            s,
            true,
            at(rules, "form.max_loose_pieces"),
            format!("its form was not measured ({why}: {} triangles), so loose pieces could not be ruled out", m.triangles),
            "give it a coarser finest level, or fewer triangles; if it is right as it is, exempt the asset in [form.allow]".to_string(),
        );
        if limits.allows(s, "loose_pieces") {
            finding.exempt = true;
        }
        r.findings.push(finding);
    }
    if m.skipped.is_some() {
        return;
    }
    if let (Some(_), true) = (limits.max_loose_pieces, m.unread > 0) {
        // Triangles that could not be read are no part of what was measured: a mesh with any
        // was not all looked at, and is not passed.
        let mut finding = Finding::flag(
            "loose_pieces",
            s,
            true,
            at(rules, "form.max_loose_pieces"),
            format!("{} of its {} triangles could not be read (an index past its vertices, or a corner whose place is not a finite number): loose pieces could not be ruled out", m.unread, m.triangles),
            "export the mesh again: every index must name a vertex, and every vertex a finite place".to_string(),
        );
        if limits.allows(s, "loose_pieces") {
            finding.exempt = true;
        }
        r.findings.push(finding);
    } else if let (Some(_), None) = (limits.max_loose_pieces, m.loose_count) {
        // Too many pieces hang clear of the ground to compare each with every other: that is
        // said as a failure, never passed over.
        let mut finding = Finding::flag(
            "loose_pieces",
            s,
            true,
            at(rules, "form.max_loose_pieces"),
            format!("{} of its {} pieces hang clear of the ground: too many to compare, so loose pieces could not be ruled out", m.clear_of_ground, m.pieces),
            "join what floats to the rest, or build it as fewer pieces; if they are meant to hover, exempt the asset in [form.allow]".to_string(),
        );
        if limits.allows(s, "loose_pieces") {
            finding.exempt = true;
        }
        r.findings.push(finding);
    }
    // Nothing of it could be measured (every triangle without an area), or most of its edges
    // are shared by more than two triangles (its faces written twice, or back to back, so
    // that no edge is a crease between two faces): the measures of its creases are of nothing,
    // and the checks that limit them fail saying so. (A box written double-sided passed
    // "0% of its creases are razors". Fifth review.)
    let nothing = m.pieces == 0 && m.skipped.is_none();
    let tangled = m.tangled_m > m.edge_m;
    if nothing && limits.max_loose_pieces.is_some() && m.unread == 0 {
        let mut finding = Finding::flag(
            "loose_pieces",
            s,
            true,
            at(rules, "form.max_loose_pieces"),
            format!("none of its {} triangles has an area: there is nothing of it to measure", m.triangles),
            "export the mesh again: its triangles have no size".to_string(),
        );
        if limits.allows(s, "loose_pieces") {
            finding.exempt = true;
        }
        r.findings.push(finding);
    }
    for (check, key, limit) in [("razor_edges", "max_razor_share", limits.max_razor_share), ("smeared_creases", "max_smeared_share", limits.max_smeared_share), ("shading_seams", "max_seam_share", limits.max_seam_share)] {
        if tangled && limit.is_some() {
            let mut finding = Finding::flag(
                check,
                s,
                true,
                at(rules, &format!("form.{key}")),
                format!("its creases could not be measured: {:.2} m of its edges are shared by more than two triangles, against {:.2} m shared by two (faces written twice, or back to back)", m.tangled_m, m.edge_m),
                "write each face once (a sheet seen from both sides is one face drawn double-sided, not two faces); or exempt the asset in [form.allow]".to_string(),
            );
            if limits.allows(s, check) {
                finding.exempt = true;
            }
            r.findings.push(finding);
        }
    }
    // Shading that was not measured is said as a failure of the checks that limit it, never
    // passed as a share of nothing (a mesh whose every crease is smeared read "0% of its
    // creases", with one vertex's normal missing: fourth review).
    let unshaded = !tangled && !m.shaded && m.edge_m + m.open_m + m.tangled_m > 0.0;
    for (check, key, limit) in [("shading_seams", "max_seam_share", limits.max_seam_share), ("smeared_creases", "max_smeared_share", limits.max_smeared_share)] {
        if unshaded && limit.is_some() {
            let mut finding = Finding::flag(
                check,
                s,
                true,
                at(rules, &format!("form.{key}")),
                "its shading could not be measured: a vertex in use has no normal, or one of no length".to_string(),
                "give every vertex a normal (the modeller does; an imported mesh may need its normals made again)".to_string(),
            );
            if limits.allows(s, check) {
                finding.exempt = true;
            }
            r.findings.push(finding);
        }
    }
    let mut push = |check: &'static str, value: f64, limit: f64, unit: &'static str, key: &str, message: String, fix: &str| {
        let mut finding = Finding::measure(check, s, value, limit, Bound::Max, unit, at(rules, &format!("form.{key}")), message, fix.to_string());
        if !finding.passed && limits.allows(s, check) {
            finding.exempt = true;
        }
        r.findings.push(finding);
    };
    if let (Some(max), Some(loose), false) = (limits.max_loose_pieces, m.loose_count, m.unread > 0 || nothing) {
        let first = m.loose.first().map(|l| format!(": the largest, {} triangles and {:.2} m across, is {:.2} m from anything else, at [{:.2}, {:.2}, {:.2}]", l.triangles, l.size_m, l.gap_m, l.at[0], l.at[1], l.at[2])).unwrap_or_default();
        push("loose_pieces", loose as f64, f64::from(max), "pieces", "max_loose_pieces", format!("{loose} of its {} pieces touch nothing and hang clear of the ground{first}", m.pieces), "join each loose piece to the rest (move it until it overlaps, or bridge the gap), or delete it; if it is meant to hover, exempt it in [form.allow]");
    }
    if let Some(max) = limits.max_razor_share.filter(|_| !tangled) {
        push("razor_edges", f64::from(m.razor_share()), f64::from(max), "share", "max_razor_share", format!("{:.0}% of its {:.2} m of creases turn more than {RAZOR_DEG}° with no bevel", m.razor_share() * 100.0, m.crease_m), "bevel the hard edges (a \"bevel\" on the outline, a \"corner\" or \"roundness\" on the box): no made thing has an edge that sharp");
    }
    let size = f.size_m[0].max(f.size_m[1]).max(f.size_m[2]).max(1e-6);
    if let Some(max) = limits.max_seam_share.filter(|_| !unshaded && !tangled) {
        push("shading_seams", f64::from(m.seam_m / size), f64::from(max), "of its size", "max_seam_share", format!("{:.2} m of edges that are not creases are shaded more than {SEAM_DEG}° apart on their two sides: a line in the light where the surface has none", m.seam_m), "let the two sides share their normals there (one part, or a smooth step), or make the edge a real crease");
    }
    if let Some(max) = limits.max_smeared_share.filter(|_| !unshaded && !tangled) {
        let share = if m.crease_m > 0.0 { m.smeared_m / m.crease_m } else { 0.0 };
        push("smeared_creases", f64::from(share), f64::from(max), "share", "max_smeared_share", format!("{:.0}% of its creases are shaded as if they were round: a dark smear along each", share * 100.0), "shade hard edges flat (split the normals there), or round the edge with real faces");
    }
    if let Some(max) = limits.max_small_relief {
        push("small_relief", f64::from(m.relief[2]), f64::from(max), "share", "max_small_relief", format!("{:.0}% of its relief is small form ({:.0}% big, {:.0}% medium)", m.relief[2] * 100.0, m.relief[0] * 100.0, m.relief[1] * 100.0), "fewer, bigger shapes: say the form in its large masses first and keep detail for where the eye should rest");
    }
}

/// How many surface colours are searched between asking whether to stop.
pub const STOP_EVERY: usize = 32;

fn is_snake(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !s.contains("__")
        && !s.ends_with('_')
}

fn naming(f: &AssetFacts, rules: &StyleRules, r: &mut Report) {
    let n = &rules.naming;
    let s = f.subject.as_str();
    if n.snake_case {
        let bad = !is_snake(&f.name);
        r.findings.push(Finding::flag("name_case", s, bad, at(rules, "naming.case"), format!("'{}' is not snake_case (lower case letters, digits and single underscores, starting with a letter)", f.name), format!("rename it, e.g. '{}'", snake_of(&f.name))));
    }
    if let Some(max) = n.max_len {
        let len = f.name.chars().count();
        r.findings.push(Finding::measure("name_length", s, len as f64, max as f64, Bound::Max, "characters", at(rules, "naming.max_len"), format!("'{}' is {len} characters long", f.name), "shorten the name".into()));
    }
    if n.file_matches_name {
        if let Some(stem) = &f.file_stem {
            let bad = *stem != f.name;
            r.findings.push(Finding::flag("name_file", s, bad, at(rules, "naming.file_matches_name"), format!("the recipe '{}' is in {stem}.json", f.name), format!("rename the file to {}.json or the recipe to '{stem}'", f.name)));
        }
    }
    if let Some(cats) = &n.categories {
        let bad = f.category.as_ref().is_none_or(|c| !cats.contains(c));
        let msg = match &f.category {
            Some(c) => format!("category '{c}' is not one of {}", cats.join(", ")),
            None => format!("no category (one of {})", cats.join(", ")),
        };
        r.findings.push(Finding::flag("category", s, bad, at(rules, "naming.categories"), msg, "set the recipe's \"category\" to one of the list".into()));
    }
    if n.parts_named && !f.part_names.is_empty() {
        let unnamed = f.part_names.iter().filter(|p| p.is_none()).count();
        r.findings.push(Finding::measure("parts_named", s, unnamed as f64, 0.0, Bound::Max, "unnamed parts", at(rules, "naming.parts_named"), format!("{unnamed} of {} parts have no name", f.part_names.len()), "give every part a \"name\" (agents and critiques refer to parts by name)".into()));
    }
}

fn snake_of(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() && !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
        match c {
            c if c.is_ascii_alphanumeric() => out.push(c.to_ascii_lowercase()),
            _ if !out.ends_with('_') => out.push('_'),
            _ => {}
        }
    }
    out.trim_matches('_').to_string()
}

fn budget(f: &AssetFacts, rules: &StyleRules, r: &mut Report) {
    let b = rules.budget_for(&f.name, f.category.as_deref(), &f.params);
    let s = f.subject.as_str();
    let tris = f.lod_triangles.first().copied().unwrap_or(0) as f64;
    if let Some(l) = &b.max_triangles {
        r.findings.push(Finding::measure("triangles", s, tris, l.value as f64, Bound::Max, "triangles", at(rules, &l.rule), format!("{tris} triangles at LOD 0, budget {}", l.value), "fewer segments or subdivisions, smaller bevels, or merge small parts".into()));
    }
    if let Some(l) = &b.min_triangles {
        r.findings.push(Finding::measure("triangles_min", s, tris, l.value as f64, Bound::Min, "triangles", at(rules, &l.rule), format!("{tris} triangles at LOD 0, at least {} expected", l.value), "more segments or bevels: the silhouette needs the detail at this size".into()));
    }
    if let Some(l) = &b.max_lod_triangles {
        for (i, max) in l.value.iter().enumerate() {
            if let Some(n) = f.lod_triangles.get(i + 1) {
                r.findings.push(Finding::measure("lod_triangles", s, *n as f64, *max as f64, Bound::Max, "triangles", at(rules, &format!("{}[{i}]", l.rule)), format!("LOD {} has {n} triangles, budget {max}", i + 1), "lower the recipe's lods ratios".into()));
            }
        }
    }
    if let Some(l) = &b.max_texture_kb {
        let kb = f.texture_bytes as f64 / 1024.0;
        r.findings.push(Finding::measure("texture_kb", s, kb, l.value, Bound::Max, "KiB", at(rules, &l.rule), format!("textures take {kb:.0} KiB with mips, budget {:.0}", l.value), "smaller or fewer textures, or vertex colour".into()));
    }
    if let Some(l) = &b.max_materials {
        let n = f.materials.len() as f64;
        r.findings.push(Finding::measure("materials", s, n, l.value as f64, Bound::Max, "materials", at(rules, &l.rule), format!("{n} material presets, budget {}", l.value), "reuse presets (each one is a draw per LOD)".into()));
    }
}

fn scale(f: &AssetFacts, rules: &StyleRules, r: &mut Report) {
    let sc = rules.scale_for(&f.name, f.category.as_deref(), &f.params);
    let s = f.subject.as_str();
    let figure = format!("{:.2}x the {FIGURE_M} m figure", f.size_m[1] / FIGURE_M);
    let ranges = [("height", 1usize, &sc.height_m), ("width", 0, &sc.width_m), ("depth", 2, &sc.depth_m)];
    for (what, axis, lim) in ranges {
        let Some(l) = lim else { continue };
        let v = f.size_m[axis] as f64;
        let [lo, hi] = l.value;
        let (bound, limit) = if v < lo as f64 { (Bound::Min, lo) } else { (Bound::Max, hi) };
        let check = match what {
            "height" => "height",
            "width" => "width",
            _ => "depth",
        };
        let mut fd = Finding::measure(check, s, v, limit as f64, bound, "m", at(rules, &l.rule), format!("{what} {v:.2} m, the style says {lo}..{hi} m{}", if axis == 1 { format!(" ({figure})") } else { String::new() }), format!("scale the recipe's {what} into {lo}..{hi} m"));
        fd.detail = Some(Json::obj([("range_m", Json::arr([Json::from(lo), Json::from(hi)])), ("figure_ratio", Json::from(((f.size_m[1] / FIGURE_M) as f64 * 1000.0).round() / 1000.0))]));
        r.findings.push(fd);
    }
    if let Some(l) = &sc.aspect {
        let v = f.aspect() as f64;
        let [lo, hi] = l.value;
        let (bound, limit) = if v < lo as f64 { (Bound::Min, lo) } else { (Bound::Max, hi) };
        let fix = if v < lo as f64 { "make it taller or its footprint narrower".to_string() } else { "make it lower or its footprint wider".to_string() };
        let bible = rules.bible_for(f.category.as_deref()).iter().find(|(k, _)| k == "shape").map(|(_, t)| format!(" (bible: {t})")).unwrap_or_default();
        r.findings.push(Finding::measure("aspect", s, v, limit as f64, bound, "height/footprint", at(rules, &l.rule), format!("height over footprint {v:.2}, the style says {lo}..{hi}{bible}"), fix));
    }
    // Floating: things stand on the ground unless their [[scale]] entry gives a band (flyers).
    let v = f.float_m as f64;
    let (band, rule) = match &sc.float_m {
        Some(l) => (l.value, at(rules, &l.rule)),
        None => ([0.0, crate::rules::DEFAULT_MAX_FLOAT_M], format!("{} scale.float_m (default 0..{} m)", rules.source, crate::rules::DEFAULT_MAX_FLOAT_M)),
    };
    let (bound, limit) = if v < band[0] as f64 { (Bound::Min, band[0]) } else { (Bound::Max, band[1]) };
    r.findings.push(Finding::measure(
        "float",
        s,
        v,
        limit as f64,
        bound,
        "m",
        rule,
        format!("its lowest point is {v:.2} m above the ground; the style allows {}..{} m", band[0], band[1]),
        "stand it on the ground (y = 0), or give its [[scale]] entry a float_m band if it really hovers".into(),
    ));
    // Sinking: the same the other way (most of a stone below the slab still counts as its size).
    let v = f.sink_m as f64;
    let (band, rule) = match &sc.sink_m {
        Some(l) => (l.value, at(rules, &l.rule)),
        None => ([0.0, crate::rules::DEFAULT_MAX_SINK_M], format!("{} scale.sink_m (default 0..{} m)", rules.source, crate::rules::DEFAULT_MAX_SINK_M)),
    };
    let (bound, limit) = if v < band[0] as f64 { (Bound::Min, band[0]) } else { (Bound::Max, band[1]) };
    r.findings.push(Finding::measure(
        "sink",
        s,
        v,
        limit as f64,
        bound,
        "m",
        rule,
        format!("its lowest point is {v:.2} m below the ground; the style allows {}..{} m", band[0], band[1]),
        "stand it on the ground (y = 0), or give its [[scale]] entry a sink_m band if it is meant to reach below".into(),
    ));
}

fn colours(f: &AssetFacts, rules: &StyleRules, palette: &Palette, r: &mut Report, stop: &dyn Fn() -> bool) {
    let p = &rules.palette;
    let s = f.subject.as_str();
    if f.total_area <= 0.0 {
        return;
    }
    // Emissive presets: only those the style lets glow.
    if !p.emissive.is_empty() {
        let glowing: Vec<&str> = f.materials.iter().filter(|m| m.emissive > 0.0 && !p.emissive.contains(&m.name)).map(|m| m.name.as_str()).collect();
        r.findings.push(Finding::flag(
            "emissive_preset",
            s,
            !glowing.is_empty(),
            at(rules, "palette.emissive"),
            format!("glowing presets the style doesn't allow: {}", glowing.join(", ")),
            format!("emission is for gameplay meaning only; use one of {}", p.emissive.join(", ")),
        ));
    }
    if palette.is_empty() {
        return;
    }
    if f.textured {
        // Vertex colours don't say what a base-colour texture paints: not measured, said so.
        r.notes.push(format!("{s}: palette distance, neutral chroma and families not measured (a base-colour texture colours the surface)"));
        return;
    }
    let total = f.total_area as f64;
    let (mut off, mut sat, mut forbidden) = (0.0f64, 0.0f64, 0.0f64);
    let mut off_list = Vec::new();
    let mut sat_list = Vec::new();
    let mut forb_list = Vec::new();
    // Families this asset must not use: its category's, and any reserved for other assets.
    let mut forbid: Vec<&String> = f.category.as_ref().and_then(|c| p.forbid.iter().find(|(k, _)| k == c)).map(|(_, v)| v.iter().collect()).unwrap_or_default();
    let mut reserved = Vec::new();
    for (fam, globs) in &p.reserve {
        if !globs.iter().any(|g| crate::rules::glob(g, &f.name)) && !forbid.contains(&fam) {
            forbid.push(fam);
            reserved.push(fam.as_str());
        }
    }
    for (i, c) in f.colours.iter().enumerate() {
        if i % STOP_EVERY == STOP_EVERY - 1 && stop() {
            r.cut_short = true;
            r.notes.push(format!("{s}: palette checks stopped after {i} of {} colours (out of time)", f.colours.len()));
            return;
        }
        let rgb = c.rgb();
        let Some(n) = palette.nearest(rgb, &f.named_colours) else { continue };
        let share = c.area as f64 / total;
        let row = |extra: (&str, Json)| Json::obj([("hex", Json::from(hex(rgb))), ("share", Json::from((share * 10_000.0).round() / 10_000.0)), ("nearest", Json::from(n.blend.as_str())), ("delta_e", Json::from((n.delta_e as f64 * 1000.0).round() / 1000.0)), extra]);
        if n.delta_e > p.max_delta_e {
            off += share;
            if off_list.len() < LISTED_COLOURS {
                off_list.push(row(("material", f.materials.get(c.material).map_or(Json::Null, |m| Json::from(m.name.as_str())))));
            }
            continue;
        }
        // The family it reads as: its nearer end's; a neutral tinted towards a family colour
        // reads as that family once it is colourful (not as an over-saturated neutral).
        let sv = chroma(rgb);
        let colourful = sv > p.neutral_max_chroma;
        let reads_as = n.family.as_ref().or(if colourful { n.touches.as_ref() } else { None });
        match reads_as {
            None => {
                let exempt = n.names.iter().any(|nm| p.exempt.iter().any(|(e, _)| e == nm));
                if colourful && !exempt {
                    sat += share;
                    if sat_list.len() < LISTED_COLOURS {
                        sat_list.push(row(("chroma", Json::from((sv as f64 * 1000.0).round() / 1000.0))));
                    }
                }
            }
            Some(fam) if forbid.contains(&fam) => {
                forbidden += share;
                if forb_list.len() < LISTED_COLOURS {
                    forb_list.push(row(("family", Json::from(fam.as_str()))));
                }
            }
            Some(_) => {}
        }
    }
    let pct = |x: f64| format!("{:.1}%", x * 100.0);
    r.findings.push(
        Finding::measure(
            "palette_distance",
            s,
            off,
            p.max_off_palette as f64,
            Bound::Max,
            "share of surface",
            at(rules, "palette.max_off_palette"),
            format!("{} of the surface is more than {} (Oklab) from every palette blend", pct(off), p.max_delta_e),
            "colour those parts with palette names (style.json), not raw colours; blend only within a family".into(),
        )
        .with_detail(Json::obj([("max_delta_e", Json::from(p.max_delta_e)), ("colours", Json::Arr(off_list))])),
    );
    r.findings.push(
        Finding::measure(
            "neutral_saturation",
            s,
            sat,
            p.max_off_palette as f64,
            Bound::Max,
            "share of surface",
            at(rules, "palette.neutral_max_chroma"),
            format!("{} of the surface is a neutral more colourful than chroma {} (saturation belongs to the families)", pct(sat), p.neutral_max_chroma),
            "pull those neutrals towards grey (a less saturated palette entry)".into(),
        )
        .with_detail(Json::obj([("max_chroma", Json::from(p.neutral_max_chroma)), ("colours", Json::Arr(sat_list))])),
    );
    if !forbid.is_empty() {
        r.findings.push(
            Finding::measure(
                "family_forbidden",
                s,
                forbidden,
                0.0,
                Bound::Max,
                "share of surface",
                at(rules, &if reserved.is_empty() { format!("palette.forbid.{}", f.category.as_deref().unwrap_or("")) } else { format!("palette.forbid.{} / palette.reserve", f.category.as_deref().unwrap_or("")) }),
                format!("{} of the surface uses a family this asset must not ({}{})", pct(forbidden), forbid.iter().map(|x| x.as_str()).collect::<Vec<_>>().join(", "), if reserved.is_empty() { String::new() } else { format!("; reserved for other assets: {}", reserved.join(", ")) }),
                if reserved.is_empty() {
                    "use this side's own family: you can tell whose a thing is from its colour alone".to_string()
                } else {
                    let keep: Vec<String> = p.reserve.iter().filter(|(fam, _)| reserved.contains(&fam.as_str())).map(|(fam, g)| format!("{fam} for {}", g.join(", "))).collect();
                    format!("the style keeps {}: colour it with its own palette names (style.json) instead", keep.join("; "))
                },
            )
            .with_detail(Json::obj([("colours", Json::Arr(forb_list))])),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::parse_hex;
    use crate::facts::{ColourArea, MaterialFact};

    fn pal(n: &str) -> Option<[f32; 3]> {
        match n {
            "carved_stone" => parse_hex("#8c96a3"),
            "rune_glow" => parse_hex("#3fe0d0"),
            "grave_glow" => parse_hex("#9cff4f"),
            "moss" => parse_hex("#4f6b3a"),
            _ => None,
        }
    }

    fn rules() -> StyleRules {
        StyleRules::parse(
            r#"
[[budget]]
category = "stone"
max_triangles = 1500
[[scale]]
category = "stone"
height_m = [2.2, 2.6]
aspect = [1.5, 4]
[palette]
families = { player = ["rune_glow"], enemy = ["grave_glow"] }
forbid = { stone = ["enemy"] }
reserve = { enemy = ["shambler*"] }
emissive = ["rune_glow"]
exempt = { moss = "the bible calls moss neutral" }
[naming]
case = "snake"
file_matches_name = true
categories = ["stone", "enemy"]
"#,
            "style.toml",
            &pal,
        )
        .unwrap()
    }

    fn palette(r: &StyleRules) -> Palette {
        Palette::new(["carved_stone", "rune_glow", "grave_glow", "moss"].map(|n| (n, pal(n).unwrap())), r)
    }

    fn stone(colours: &[(&str, f32)]) -> AssetFacts {
        AssetFacts {
            subject: "warding_stone".into(),
            name: "warding_stone".into(),
            file_stem: Some("warding_stone".into()),
            category: Some("stone".into()),
            lod_triangles: vec![1200, 600],
            size_m: [1.0, 2.4, 1.0],
            materials: vec![MaterialFact { name: "carved_stone".into(), emissive: 0.0, glass: false }, MaterialFact { name: "rune_glow".into(), emissive: 2.5, glass: false }],
            colours: colours.iter().map(|(h, a)| ColourArea { rgb: parse_hex(h).unwrap().map(|c| (c * 255.0).round() as u8), material: 0, area: *a }).collect(),
            total_area: colours.iter().map(|c| c.1).sum(),
            ..AssetFacts::default()
        }
    }

    /// The form checks: loose pieces are checked in any style, the rest only when `[form]`
    /// gives a limit; a failure the style allows for the subject is exempt; and a mesh that was
    /// not measured is not checked.
    #[test]
    fn form_limits_are_checked_when_given() {
        use crate::form::{FormFacts, Loose, CHECKS};
        let mut r = rules();
        let mut f = stone(&[("#8c96a3", 9.0)]);
        let of_form = |rep: &Report| rep.findings.iter().filter(|x| CHECKS.contains(&x.check)).cloned().collect::<Vec<Finding>>();
        assert!(of_form(&check(&f, &r, &palette(&r))).is_empty(), "nothing measured, nothing checked");
        f.form = FormFacts {
            triangles: 100,
            pieces: 3,
            loose_count: Some(1),
            loose: vec![Loose { triangles: 12, at: [0.5, 2.0, 0.5], size_m: 0.1, gap_m: 0.95 }],
            edge_m: 9.0,
            crease_m: 4.0,
            razor_m: 3.0,
            shaded: true,
            seam_m: 0.6,
            smeared_m: 1.0,
            relief: [0.2, 0.2, 0.6],
            ..FormFacts::default()
        };
        let found = of_form(&check(&f, &r, &palette(&r)));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].check == "loose_pieces" && found[0].is_error() && found[0].message.contains("1 of its 3 pieces touch nothing and hang clear of the ground") && found[0].message.contains("0.95 m"), "{:?}", found[0]);
        assert!(found[0].rule.ends_with("form.max_loose_pieces"));

        r.form = crate::rules::FormRules {
            max_loose_pieces: Some(1),
            max_razor_share: Some(0.5),
            max_seam_share: Some(0.1),
            max_smeared_share: Some(0.5),
            max_small_relief: Some(0.5),
            max_facet_deg: None,
            allow: vec![("warding_*".into(), vec!["small_relief".into()])],
        };
        let found = of_form(&check(&f, &r, &palette(&r)));
        let by = |name: &str| found.iter().find(|x| x.check == name).unwrap_or_else(|| panic!("{name} was checked"));
        assert_eq!(found.len(), 5);
        assert!(by("loose_pieces").passed && by("smeared_creases").passed);
        // 3 of 4 m of creases are razors; 0.6 m of seams on a shape 2.4 m tall is a quarter of its size.
        assert!(by("razor_edges").is_error() && (by("razor_edges").value - 0.75).abs() < 1e-6);
        assert!(by("shading_seams").is_error() && (by("shading_seams").value - 0.25).abs() < 1e-6);
        assert!(!by("small_relief").passed && by("small_relief").exempt && !by("small_relief").is_error());

        // Shading that was not measured (a vertex in use without a normal) fails the two checks
        // that limit it, saying so: never "0% of its creases", a pass on nothing (fourth review).
        let measured = f.form.clone();
        f.form = FormFacts { shaded: false, seam_m: 0.0, smeared_m: 0.0, ..measured.clone() };
        let found = of_form(&check(&f, &r, &palette(&r)));
        let by = |name: &str| found.iter().find(|x| x.check == name).unwrap_or_else(|| panic!("{name} was checked"));
        assert_eq!(found.len(), 5, "{found:?}");
        for name in ["shading_seams", "smeared_creases"] {
            assert!(by(name).is_error() && by(name).message.contains("could not be measured") && by(name).rule.contains("form.max_s"), "{:?}", by(name));
        }
        assert!(by("razor_edges").is_error() && by("loose_pieces").passed);
        // (With no limit on them, nothing is said of them.)
        let mut loose_only = r.clone();
        loose_only.form = crate::rules::FormRules { max_loose_pieces: Some(1), ..crate::rules::FormRules::default() };
        assert_eq!(of_form(&check(&f, &loose_only, &palette(&r))).len(), 1);
        // Triangles that could not be read: the loose check fails saying so, whatever it counted.
        f.form = FormFacts { unread: 3, ..measured };
        let found = of_form(&check(&f, &r, &palette(&r)));
        let loose: Vec<_> = found.iter().filter(|x| x.check == "loose_pieces").collect();
        assert!(loose.len() == 1 && loose[0].is_error() && loose[0].message.contains("3 of its") && loose[0].message.contains("could not be read"), "{loose:?}");

        // Faces written twice (more edge shared by more than two triangles than by two): the
        // three checks of its creases fail saying they could not be measured (fifth review).
        f.form = FormFacts { tangled_m: 20.5, edge_m: 0.0, crease_m: 0.0, razor_m: 0.0, ..f.form.clone() };
        let found = of_form(&check(&f, &r, &palette(&r)));
        for name in ["razor_edges", "smeared_creases", "shading_seams"] {
            let x: Vec<_> = found.iter().filter(|x| x.check == name).collect();
            assert!(x.len() == 1 && x[0].is_error() && x[0].message.contains("shared by more than two triangles"), "{name}: {x:?}");
        }
        // Triangles, and none with an area: the loose check fails on nothing to measure.
        f.form = FormFacts { triangles: 2, loose_count: Some(0), ..FormFacts::default() };
        let found = of_form(&check(&f, &StyleRules::defaults(), &palette(&r)));
        assert!(found.len() == 1 && found[0].check == "loose_pieces" && found[0].is_error() && found[0].message.contains("none of its 2 triangles has an area"), "{found:?}");

        // (Faceting has a test of its own: `faceting_says_what_shows_and_what_holds_it`.)

        // A mesh too large to measure: nothing but the loose check, which fails saying so.
        f.form = FormFacts { triangles: 9_000_000, skipped: Some("more triangles than the form measures take"), ..FormFacts::default() };
        let found = of_form(&check(&f, &r, &palette(&r)));
        assert!(found.len() == 1 && found[0].check == "loose_pieces" && found[0].is_error() && found[0].message.contains("9000000 triangles"), "{found:?}");

        // Too many floating pieces to compare: a failure that says so, not a pass.
        f.form = FormFacts { triangles: 30_000, pieces: 10_000, clear_of_ground: 9_000, loose_count: None, ..FormFacts::default() };
        let found = of_form(&check(&f, &StyleRules::defaults(), &palette(&r)));
        assert!(found.len() == 1 && found[0].is_error() && found[0].message.contains("9000 of its 10000 pieces"), "{found:?}");
    }

    /// A round shape as the model gives it, measured, from its count's value (a ring's turn is
    /// 360° over it).
    /// What measures such a record again: a turn of 360° or 90° over its count, a sphere, a
    /// sculpt's cell.
    fn again(count: &str, value: u32, turn_deg: f32, radius_m: f32) -> specula_model::facet::Recount {
        use specula_model::facet::Recount;
        match count {
            "subdivisions" => Recount::sphere(f64::from(radius_m)),
            "resolution" => Recount::cell(f64::from((0.5 * turn_deg.to_radians()).sin()) * f64::from(value)),
            "rings" => Recount::even(90.0),
            _ => Recount::even(360.0),
        }
    }

    fn round(path: &str, what: &'static str, count: &'static str, value: u32, turn_deg: f32, radius_m: f32) -> specula_model::Round {
        // (The counts each builder takes, as the model would give them.)
        let (least, most) = match (what, count) {
            (_, "subdivisions") => (0, 5),
            (_, "resolution") => (4, 160),
            (_, "rings") => (1, 32),
            ("torus", "sides") => (3, 64),
            ("torus", _) => (3, 512),
            _ => (3, 256),
        };
        specula_model::Round {
            path: path.into(),
            what,
            count,
            count_value: value,
            turn_deg,
            radius_m,
            turn_radius_m: radius_m,
            sag: specula_model::facet::sag_of(f64::from(radius_m), f64::from(turn_deg)) as f32,
            rim: None,
            least,
            most,
            most_by: "its range",
            most_estimated: None,
            displaced_m: 0.0,
            unmeasured: None,
            recount: again(count, value, turn_deg, radius_m),
        }
    }

    fn fix_at(x: &specula_model::Round, max: f32) -> String {
        facet_fix(x, max, true, true, &mut 0)
    }

    /// Faceting: from what the recipe names round, measured in the finished model, where it
    /// shows at the asset's greatest width (2.6 m here: a facet shows from 5.1 mm). The answer
    /// is the largest turn among what shows (not the furthest standing in); the fix names the
    /// number that made the turn and how much of it holds the limit; what was not measured
    /// fails saying so; nothing named round passes saying so; a style may allow it by name.
    #[test]
    fn faceting_says_what_shows_and_what_holds_it() {
        let r = rules();
        let mut f = stone(&[("#8c96a3", 9.0)]);
        f.form = crate::form::FormFacts { triangles: 12, loose_count: Some(0), ..crate::form::FormFacts::default() };
        f.width_m = 2.6;
        let mut faceted = r.clone();
        faceted.form = crate::rules::FormRules { max_facet_deg: Some(20.0), allow: vec![("lantern_*".into(), vec!["faceting".into()])], ..crate::rules::FormRules::default() };
        let all = |f: &AssetFacts| check(f, &faceted, &palette(&r)).findings.into_iter().filter(|x| x.check == "faceting").collect::<Vec<_>>();
        let only = |f: &AssetFacts| {
            let a = all(f);
            assert_eq!(a.len(), 1, "{a:?}");
            a[0].clone()
        };
        let none = only(&f);
        assert!(none.passed && none.value == 0.0 && none.message.contains("names no shape round"), "{none:?}");
        // A column of six sides at half a metre stands 67 mm in: seen, 60° a side where the
        // style allows 20, and 18 sides hold it. A spike of three sides at 4 cm stands 20 mm in
        // (seen) and turns 120°: it is the answer, though the column stands in further. A
        // candle of six sides at 2 cm (2.7 mm in) is not seen.
        f.round = vec![round("parts[0].shape", "cylinder", "sides", 6, 60.0, 0.5), round("parts[4].shape", "cone", "sides", 3, 120.0, 0.04), round("parts[3].shape", "cylinder", "sides", 6, 60.0, 0.02)];
        f.round_total = 3;
        let bad = only(&f);
        assert!(bad.is_error() && (bad.value - 120.0).abs() < 1e-3 && bad.message.contains("cone at parts[4].shape: its faces turn 120.0° from one to the next at a radius of 0.040 m") && bad.fix.contains("give the cone 18 \"sides\" (it has 3;"), "{bad:?}");
        f.round.remove(1);
        let bad = only(&f);
        assert!(bad.is_error() && (bad.value - 60.0).abs() < 1e-3 && bad.limit == 20.0, "{bad:?}");
        assert!(bad.message.contains("cylinder at parts[0].shape: its faces turn 60.0°") && bad.message.contains("67.0 mm") && bad.fix.contains("give the cylinder 18 \"sides\" (it has 6;") && bad.fix.contains("extruded outline") && bad.rule.ends_with("form.max_facet_deg"), "{bad:?}");
        // The number that made the turn is named: a capsule's rings (90° over one ring: five
        // rings hold 20°), not its segments; a torus's tube; a sphere's subdivisions (two from
        // none hold 20°; past five, said), a sculpt's resolution.
        let fix = |x: specula_model::Round| fix_at(&x, 20.0);
        assert!(fix(round("p", "capsule", "rings", 1, 90.0, 0.2)).contains("its ends: give the capsule 5 \"rings\" (it has 1;"));
        assert!(fix(round("p", "capsule", "segments", 6, 60.0, 0.2)).contains("give the capsule 18 \"segments\""));
        assert!(fix(round("p", "torus", "sides", 3, 120.0, 0.05)).contains("its tube: give the torus 18 \"sides\""));
        assert!(fix(round("p", "sphere", "subdivisions", 0, 41.81, 0.4)).contains("give the sphere 2 \"subdivisions\" (it has 0"));
        assert!(fix(round("p", "sphere", "subdivisions", 1, 20.905, 0.4)).contains("give the sphere 2 \"subdivisions\" (it has 1"));
        assert!(fix_at(&round("p", "sphere", "subdivisions", 0, 41.81, 0.4), 1.0).contains("at its most \"subdivisions\" (5) a sphere still turns 1.4°"));
        assert!(fix_at(&round("p", "capsule", "rings", 1, 90.0, 0.2), 2.0).contains("cannot hold the limit: at its most (32) it still turns 2.8°"));
        // Where its part can be built with fewer than its range allows, the search stops there
        // and says why.
        let mut short = round("p", "lathe", "segments", 8, 45.0, 0.5);
        (short.most, short.most_by) = (40, "the triangles one part may have");
        assert!(fix_at(&short, 2.0).contains("cannot hold the limit: at the most it can be built with (40; past it the triangles one part may have would be exceeded) it still turns 9.0°"), "{}", fix_at(&short, 2.0));
        assert!(fix_at(&short, 9.0).contains("give the lathe 40 \"segments\""), "{}", fix_at(&short, 9.0));
        // The whole check's advice is bounded: past the bound no count is named.
        let mut spent = MOST_ADVICE_WORK;
        assert!(facet_fix(&round("p", "sphere", "subdivisions", 0, 41.81, 0.4), 20.0, true, true, &mut spent).contains("no count is named"));
        // (A cell of the sculpt turns 35° at 0.3 m at resolution 24: sin 17.5° / sin 10° = 1.73.)
        assert!(fix(round("p", "sdf", "resolution", 24, 35.0, 0.3)).contains("\"resolution\" to 42 (it is 24;"), "{}", fix(round("p", "sdf", "resolution", 24, 35.0, 0.3)));
        // Nothing shows: passes, saying so.
        f.round = vec![round("parts[3].shape", "cylinder", "sides", 6, 60.0, 0.02), round("parts[1].shape", "sphere", "subdivisions", 2, 10.45, 0.4)];
        f.round_total = 2;
        let fine = only(&f);
        assert!(fine.passed && fine.value == 0.0 && fine.message.contains("none of its 2 measured round shapes"), "{fine:?}");
        // One not measured: a failure that says so (never a pass on what was not looked at).
        let mut tapered = round("parts[5].shape", "cylinder", "sides", 12, 30.0, 0.3);
        tapered.unmeasured = Some("tapered (not affine)");
        f.round.insert(0, tapered);
        f.round_total = 3;
        let unknown = all(&f);
        assert!(unknown.len() == 2 && unknown[0].passed && unknown[1].is_error() && unknown[1].message.contains("the facets of 1 round shape were not measured: its cylinder at parts[5].shape was tapered"), "{unknown:?}");
        // Said beside a measured one that fails, in the same answer (following that one's
        // advice is not enough); and every one counted, past those kept.
        let mut both = f.clone();
        both.round.push(round("parts[0].shape", "cylinder", "sides", 6, 60.0, 0.5));
        both.round_not_measured = 12;
        let said = all(&both);
        assert!(said.len() == 2 && said[0].is_error() && said[0].value == 60.0 && said[1].is_error() && said[1].message.contains("the facets of 12 round shapes were not measured") && said[1].message.contains("(and 11 more round shapes not measured)"), "{said:?}");
        // The facts say it, and a style may allow it by name.
        f.round.push(round("parts[2].shape.tree#3", "sdf", "resolution", 24, 35.0, 0.3));
        let j = f.to_json(4);
        let fj = j.get("faceting").unwrap();
        assert_eq!(fj.get("turn_deg").and_then(Json::as_f64), Some(35.0));
        assert_eq!(fj.get("worst").and_then(|w| w.get("path")).and_then(Json::as_str), Some("parts[2].shape.tree#3"));
        assert_eq!((fj.get("round_shapes").and_then(Json::as_f64), fj.get("round_kept").and_then(Json::as_f64), fj.get("not_measured").and_then(Json::as_f64)), (Some(3.0), Some(4.0), Some(1.0)));
        let mut lantern = f.clone();
        lantern.subject = "lantern_a".into();
        let allowed = all(&lantern);
        assert!(allowed.iter().all(|a| a.passed || (a.exempt && !a.is_error())), "{allowed:?}");
    }

    #[test]
    fn a_clean_stone_passes_everything() {
        let r = rules();
        let rep = check(&stone(&[("#8c96a3", 9.0), ("#3fe0d0", 1.0)]), &r, &palette(&r));
        assert!(rep.passed(), "{:?}", rep.errors().collect::<Vec<_>>());
        // naming (case, file, category), triangles, height, aspect, float, sink, emissive,
        // palette, saturation, family
        assert_eq!(rep.findings.len(), 12);
    }

    #[test]
    fn violations_are_errors_with_numbers() {
        let r = rules();
        let mut f = stone(&[("#8c96a3", 8.0), ("#9cff4f", 1.0), ("#d02020", 1.0)]);
        f.lod_triangles = vec![2100];
        f.size_m = [1.6, 1.8, 1.6];
        f.name = "WardingStone".into();
        f.materials.push(MaterialFact { name: "fire".into(), emissive: 3.0, glass: false });
        let rep = check(&f, &r, &palette(&r));
        let failed: Vec<&str> = rep.errors().map(|e| e.check).collect();
        assert_eq!(failed, vec!["name_case", "name_file", "triangles", "height", "aspect", "emissive_preset", "palette_distance", "family_forbidden"]);
        let tri = rep.errors().find(|e| e.check == "triangles").unwrap();
        assert_eq!((tri.value, tri.limit, tri.rule.as_str()), (2100.0, 1500.0, "style.toml budget[0].max_triangles"));
        let off = rep.errors().find(|e| e.check == "palette_distance").unwrap();
        assert!((off.value - 0.1).abs() < 1e-6);
        assert!(rep.errors().find(|e| e.check == "name_case").unwrap().fix.contains("warding_stone"));
    }

    #[test]
    fn reserved_families_belong_to_their_assets() {
        let r = rules();
        // A tombstone (no forbid for its category) with grave green: reserved for shamblers here.
        let mut f = stone(&[("#a7a39a", 9.0), ("#9cff4f", 1.0)]);
        f.name = "tombstone".into();
        f.category = Some("graveyard".into());
        let e = check(&f, &r, &palette(&r)).findings.into_iter().find(|x| x.check == "family_forbidden").expect("checked");
        assert!(!e.passed && e.rule.ends_with("palette.reserve") && e.message.contains("reserved"), "{e:?}");
        f.name = "shambler".into();
        assert!(check(&f, &r, &palette(&r)).findings.iter().all(|x| x.check != "family_forbidden"));
    }

    #[test]
    fn saturated_neutrals_fail_unless_exempt() {
        let r = rules();
        // Moss is a neutral (no family) at 46% saturation: allowed only by its exemption.
        let rep = check(&stone(&[("#8c96a3", 5.0), ("#4f6b3a", 5.0)]), &r, &palette(&r));
        assert!(rep.passed(), "moss is exempt: {:?}", rep.errors().collect::<Vec<_>>());
        let mut r2 = rules();
        r2.palette.exempt.clear();
        let rep = check(&stone(&[("#8c96a3", 5.0), ("#4f6b3a", 5.0)]), &r2, &palette(&r2));
        let e = rep.errors().find(|e| e.check == "neutral_saturation").expect("moss over 25%");
        assert!((e.value - 0.5).abs() < 1e-6);
    }
}
