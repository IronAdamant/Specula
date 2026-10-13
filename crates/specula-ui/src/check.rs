//! `ui_check`: verify a UI without eyes. Lays the given screens out at several targets
//! (resolution, density, touch, safe area) and measures them against the theme's rules.
//!
//! | Rule | What it measures |
//! |---|---|
//! | `legibility` | capital height of every label in physical px, against `check.min_cap_px` scaled by density (Steam Deck: 9 px at 800p) |
//! | `contrast` | WCAG ratio of text against what is behind it (panels composited over the theme's `scene` colours); 4.5 body, 3 large |
//! | `non_text_contrast` | threat arrows, focus ring and bars against their backgrounds (WCAG 1.4.11: 3) |
//! | `overlap` | sibling widgets in a stack (and top-level groups of screens shown together) whose drawn bounds intersect |
//! | `offscreen` | drawn content outside the safe area |
//! | `overflow` | content sticking out of the panel or button that holds it |
//! | `text_overflow` | text wider / taller than its label's box; `missing_glyph` for characters the font lacks |
//! | `touch_target` | interactive widgets smaller than `check.min_touch_px` logical px on touch targets |
//! | `focus_reachability` | menu widgets the gamepad can't reach from the default focus |
//! | `gamepad_access` | HUD buttons with no `shortcut` action (pointer-only), or a shortcut with no gamepad binding |
//! | `unbound` | template keys the supplied state doesn't have |
//!
//! Resolution-independent rules (contrast, gamepad access, unbound keys) are reported once with
//! target `"all"`, so reports stay small.

use crate::color::{contrast_ratio, over, to_hex, Rgba};
use crate::draw::Rect;
use crate::error::UiError;
use crate::geom::{neighbour, Dir};
use crate::screen::{InputMode, Kind, TextSource};
use crate::state::{Key, Scope, StateMap};
use crate::ui::{focusable, scope_for, ScreenRt, Ui, Viewport};
use specula_core::json::Json;
use specula_input::{ActionMap, DeviceFamily, Input};

/// A device to check against.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub name: String,
    pub size: [u32; 2],
    /// Pixel density, if known (drives the scale rule and the legibility threshold).
    pub ppi: Option<f32>,
    /// Touch is a primary input (touch-target sizes are enforced).
    pub touch: bool,
    /// Safe-area insets, physical px `[top, right, bottom, left]`.
    pub safe_area: [f32; 4],
    /// The player's UI scale setting.
    pub user_scale: f32,
}

impl Target {
    pub fn new(name: &str, size: [u32; 2], ppi: Option<f32>) -> Target {
        Target { name: name.into(), size, ppi, touch: false, safe_area: [0.0; 4], user_scale: 1.0 }
    }

    /// Steam Deck: 1280×800, ~205 ppi, 7" screen.
    pub fn deck() -> Target {
        Target::new("deck_1280x800", [1280, 800], Some(205.0))
    }

    pub fn desktop_1080p() -> Target {
        Target::new("desktop_1920x1080", [1920, 1080], Some(92.0))
    }

    pub fn ultrawide() -> Target {
        Target::new("ultrawide_2560x1080", [2560, 1080], Some(81.0))
    }

    /// A phone in portrait with a notch and a gesture bar.
    pub fn phone_portrait() -> Target {
        Target { touch: true, safe_area: [96.0, 0.0, 64.0, 0.0], ..Target::new("phone_1080x2400", [1080, 2400], Some(420.0)) }
    }

    /// The four targets from the UI brief.
    pub fn defaults() -> Vec<Target> {
        vec![Target::deck(), Target::desktop_1080p(), Target::ultrawide(), Target::phone_portrait()]
    }

    pub fn viewport(&self, ui: &Ui) -> Viewport {
        let scale = ui.theme().scale.scale(self.size, self.ppi, self.user_scale);
        Viewport::new(self.size, scale).with_safe_area(self.safe_area)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub rule: &'static str,
    pub severity: Severity,
    /// Target name, or `"all"` for resolution-independent rules.
    pub target: String,
    pub screen: String,
    /// Widget id, or its JSON path when it has none.
    pub widget: String,
    pub message: String,
    pub measured: f64,
    pub required: f64,
}

impl Finding {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("rule", Json::Str(self.rule.into())),
            ("severity", Json::Str(if self.severity == Severity::Error { "error" } else { "warning" }.into())),
            ("target", Json::Str(self.target.clone())),
            ("screen", Json::Str(self.screen.clone())),
            ("widget", Json::Str(self.widget.clone())),
            ("message", Json::Str(self.message.clone())),
            ("measured", Json::Num((self.measured * 100.0).round() / 100.0)),
            ("required", Json::Num((self.required * 100.0).round() / 100.0)),
        ])
    }
}

/// Findings as a JSON report with counts.
pub fn report_json(findings: &[Finding]) -> Json {
    let errors = findings.iter().filter(|f| f.severity == Severity::Error).count();
    Json::obj([
        ("errors", Json::Num(errors as f64)),
        ("warnings", Json::Num((findings.len() - errors) as f64)),
        ("findings", Json::Arr(findings.iter().map(Finding::to_json).collect())),
    ])
}

fn widget_name(sr: &ScreenRt, i: usize) -> String {
    let n = &sr.screen.nodes[i];
    match &n.id {
        Some(id) => id.clone(),
        None => {
            // The nearest ancestor id helps humans find it; the path is exact.
            let mut p = n.parent;
            while let Some(k) = p {
                if let Some(id) = &sr.screen.nodes[k as usize].id {
                    return format!("{id} > {}", n.path);
                }
                p = sr.screen.nodes[k as usize].parent;
            }
            n.path.clone()
        }
    }
}

/// Draws something itself (as opposed to pure layout boxes).
fn is_visual(k: &Kind, ui: &Ui) -> bool {
    match k {
        Kind::Panel { boxed } => ui.theme.boxed(*boxed).is_visible(),
        Kind::Label { .. } | Kind::Button { .. } | Kind::Icon { .. } | Kind::Bar { .. } | Kind::Radial { .. } => true,
        _ => false,
    }
}

/// Check `screens` (shown together, in their layer order) at every target. `input_map` (the
/// game's actions, if given) lets `gamepad_access` verify shortcut bindings. The UI's own
/// visibility and focus are restored afterwards.
pub fn check(ui: &mut Ui, screens: &[&str], state: &StateMap, input_map: Option<&ActionMap>, targets: &[Target]) -> Result<Vec<Finding>, UiError> {
    let mut ids = Vec::new();
    for s in screens {
        ids.push(ui.screen_id(s).ok_or_else(|| UiError::new(crate::error::ErrorCode::UnknownScreen, "", format!("no screen \"{s}\"")))?.0);
    }
    let saved: Vec<(bool, Option<u32>)> = ui.screens.iter().map(|s| (s.visible, s.focus)).collect();
    for (i, s) in ui.screens.iter_mut().enumerate() {
        s.visible = ids.contains(&i);
        s.focus = s.screen.focus;
    }
    // Re-sort by showing / hiding through the public API (keeps the order logic in one place).
    for s in screens {
        ui.show(s)?;
    }
    let input = Input::new(input_map.cloned().unwrap_or_default());
    let mut out = Vec::new();
    if ui.fonts.is_empty() {
        out.push(Finding {
            rule: "legibility",
            severity: Severity::Error,
            target: "all".into(),
            screen: String::new(),
            widget: String::new(),
            message: "no fonts: text can't be drawn or measured".into(),
            measured: 0.0,
            required: 1.0,
        });
    }
    for (k, t) in targets.iter().enumerate() {
        let view = t.viewport(ui);
        let _ = ui.frame(&input, state, &view);
        per_target(ui, &ids, t, &view, &mut out);
        if k == 0 {
            global_rules(ui, &ids, state, input_map, &mut out);
        }
    }
    for (s, (v, f)) in ui.screens.iter_mut().zip(saved) {
        s.visible = v;
        s.focus = f;
    }
    // Restore the draw order.
    let names: Vec<String> = ui.screens.iter().filter(|s| s.visible).map(|s| s.screen.name.clone()).collect();
    for n in names {
        ui.show(&n)?;
    }
    out.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.rule.cmp(b.rule)));
    Ok(out)
}

fn per_target(ui: &Ui, ids: &[usize], t: &Target, view: &Viewport, out: &mut Vec<Finding>) {
    let scale = view.scale;
    let rules = ui.theme.check;
    let density = t.ppi.map_or(1.0, |p| (p / ui.theme.scale.reference_ppi).max(1.0));
    let min_cap = rules.min_cap_px * density;
    let mut push = |rule: &'static str, sev: Severity, sr: &ScreenRt, i: usize, message: String, measured: f32, required: f32| {
        out.push(Finding {
            rule,
            severity: sev,
            target: t.name.clone(),
            screen: sr.screen.name.clone(),
            widget: if i == usize::MAX { String::new() } else { widget_name(sr, i) },
            message,
            measured: f64::from(measured),
            required: f64::from(required),
        });
    };
    // Groups for cross-screen overlap: (screen, node, ink).
    let mut groups: Vec<(usize, usize, Rect)> = Vec::new();
    for &s in ids {
        let sr = &ui.screens[s];
        let nodes = &sr.screen.nodes;
        let n = nodes.len();
        let bounds = if sr.screen.full_bleed { view.full() } else { view.safe() };
        // Ink: the union of visual descendants' rects, bottom-up.
        let mut ink: Vec<Option<Rect>> = vec![None; n];
        for i in (0..n).rev() {
            if !sr.comp[i].visible {
                continue;
            }
            let mut r = if is_visual(&nodes[i].kind, ui) { Some(sr.comp[i].rect) } else { None };
            for c in sr.screen.children(i as u32) {
                if let Some(cr) = ink[c as usize] {
                    // Content scrolled out of a list doesn't count beyond the list.
                    let cr = match nodes[i].kind {
                        Kind::List { .. } => cr.intersect(&sr.comp[i].rect),
                        _ => cr,
                    };
                    r = Some(match r {
                        Some(a) => union(a, cr),
                        None => cr,
                    });
                }
            }
            ink[i] = r;
        }
        for i in 0..n {
            let c = &sr.comp[i];
            if !c.visible {
                continue;
            }
            let node = &nodes[i];
            let visible_rect = match c.clip {
                Some(cl) => c.rect.intersect(&cl),
                None => c.rect,
            };
            // ---- legibility + text overflow ----
            if let Kind::Label { role, wrap, .. } = &node.kind {
                let rt = &sr.rt[i];
                if !rt.text.trim().is_empty() {
                    if let Some(fi) = rt.font {
                        let font = ui.fonts.font(fi);
                        let role = ui.theme.role(*role);
                        let cap_units = font.cap_height().map_or(0.7 * font.units_per_em() as f32, f32::from);
                        let cap = cap_units / font.units_per_em() as f32 * role.size * scale;
                        if cap + 1e-3 < min_cap {
                            push(
                                "legibility",
                                Severity::Error,
                                sr,
                                i,
                                format!("cap height {cap:.1} px < {min_cap:.1} px (\"{}\" at {} px, scale {scale:.2})", short(&rt.text), role.size),
                                cap,
                                min_cap,
                            );
                        }
                        let content = c.content;
                        let [tw, th] = c.text;
                        let wrap = *wrap;
                        if (!wrap && tw > content.w + 0.5) || th > content.h + 0.5 {
                            push(
                                "text_overflow",
                                Severity::Error,
                                sr,
                                i,
                                format!("text \"{}\" needs {tw:.0}×{th:.0} px, box is {:.0}×{:.0}", short(&rt.text), content.w, content.h),
                                tw.max(th),
                                if tw > content.w + 0.5 { content.w } else { content.h },
                            );
                        }
                        if !rt.laid.missing.is_empty() {
                            let m: String = rt.laid.missing.iter().collect();
                            push("missing_glyph", Severity::Error, sr, i, format!("the font has no glyph for {m:?}"), m.chars().count() as f32, 0.0);
                        }
                    }
                }
            }
            // ---- offscreen ----
            if is_visual(&node.kind, ui) && !visible_rect.is_empty() && !visible_rect.inside(&bounds) {
                let parent_out = node.parent.is_some_and(|p| {
                    let pc = &sr.comp[p as usize];
                    is_visual(&nodes[p as usize].kind, ui) && !pc.rect.inside(&bounds)
                });
                if !parent_out {
                    let r = visible_rect;
                    push(
                        "offscreen",
                        Severity::Error,
                        sr,
                        i,
                        format!(
                            "{} at [{:.0}, {:.0}, {:.0}×{:.0}] leaves the safe area [{:.0}, {:.0}, {:.0}×{:.0}]",
                            node.kind.name(), r.x, r.y, r.w, r.h, bounds.x, bounds.y, bounds.w, bounds.h
                        ),
                        overshoot(&r, &bounds),
                        0.0,
                    );
                }
            }
            // ---- overflow: content outside its panel / button ----
            if matches!(node.kind, Kind::Panel { .. } | Kind::Button { .. }) && is_visual(&node.kind, ui) {
                for ch in sr.screen.children(i as u32) {
                    let Some(cr) = ink[ch as usize] else { continue };
                    if !cr.inside(&c.rect) {
                        push(
                            "overflow",
                            Severity::Error,
                            sr,
                            ch as usize,
                            format!("content sticks out of its {} by {:.1} px", node.kind.name(), overshoot(&cr, &c.rect)),
                            overshoot(&cr, &c.rect),
                            0.0,
                        );
                    }
                }
            }
            // ---- overlap among siblings in a stack ----
            if node.layout == crate::screen::Layout::Stack && !matches!(node.kind, Kind::Radial { .. }) {
                let kids: Vec<u32> = sr.screen.children(i as u32).filter(|&k| sr.comp[k as usize].visible && !nodes[k as usize].overlap_ok).collect();
                for (a_i, &a) in kids.iter().enumerate() {
                    for &b in &kids[a_i + 1..] {
                        let (Some(ra), Some(rb)) = (ink[a as usize], ink[b as usize]) else { continue };
                        let area = ra.intersect(&rb).area();
                        if area > 1.0 {
                            push(
                                "overlap",
                                Severity::Error,
                                sr,
                                b as usize,
                                format!("overlaps {} by {area:.0} px²", widget_name(sr, a as usize)),
                                area,
                                0.0,
                            );
                        }
                    }
                }
            }
            // ---- touch targets ----
            if t.touch && node.kind.interactive() {
                let side = if let Kind::RadialItem { .. } = node.kind {
                    radial_item_size(sr, i)
                } else {
                    c.rect.w.min(c.rect.h)
                } / scale;
                if side + 1e-3 < rules.min_touch_px {
                    push(
                        "touch_target",
                        Severity::Error,
                        sr,
                        i,
                        format!("touch target {side:.0} logical px < {:.0}", rules.min_touch_px),
                        side,
                        rules.min_touch_px,
                    );
                }
            }
        }
        // ---- focus reachability ----
        if sr.screen.input == InputMode::Menu && !sr.screen.nodes.iter().any(|n| matches!(n.kind, Kind::Radial { .. })) {
            let all: Vec<usize> = (0..n).filter(|&i| focusable(sr, i)).collect();
            match sr.focus.filter(|&f| focusable(sr, f as usize)) {
                None if !all.is_empty() => push(
                    "focus_reachability",
                    Severity::Error,
                    sr,
                    usize::MAX,
                    "menu screen has no initial focus".into(),
                    0.0,
                    1.0,
                ),
                None => {}
                Some(start) => {
                    let mut seen = vec![false; n];
                    let mut queue = vec![start as usize];
                    seen[start as usize] = true;
                    while let Some(f) = queue.pop() {
                        for d in Dir::ALL {
                            let next = nodes[f].nav[d.index()].filter(|&t| focusable(sr, t as usize)).or_else(|| {
                                neighbour(f as u32, sr.comp[f].rect, d, all.iter().map(|&i| (i as u32, sr.comp[i].rect)))
                            });
                            if let Some(t) = next {
                                if !seen[t as usize] {
                                    seen[t as usize] = true;
                                    queue.push(t as usize);
                                }
                            }
                        }
                    }
                    for &i in &all {
                        if !seen[i] {
                            push(
                                "focus_reachability",
                                Severity::Error,
                                sr,
                                i,
                                format!("not reachable with the d-pad from \"{}\"", widget_name(sr, start as usize)),
                                0.0,
                                1.0,
                            );
                        }
                    }
                }
            }
        }
        if !sr.screen.modal {
            for c in sr.screen.children(0) {
                if sr.comp[c as usize].visible && !nodes[c as usize].overlap_ok {
                    if let Some(r) = ink[c as usize] {
                        groups.push((s, c as usize, r));
                    }
                }
            }
        }
    }
    // ---- overlap between screens shown together ----
    for (a_i, a) in groups.iter().enumerate() {
        for b in &groups[a_i + 1..] {
            if a.0 == b.0 {
                continue;
            }
            let area = a.2.intersect(&b.2).area();
            if area > 1.0 {
                let (sa, sb) = (&ui.screens[a.0], &ui.screens[b.0]);
                out.push(Finding {
                    rule: "overlap",
                    severity: Severity::Error,
                    target: t.name.clone(),
                    screen: sb.screen.name.clone(),
                    widget: widget_name(sb, b.1),
                    message: format!("overlaps {}:{} by {area:.0} px²", sa.screen.name, widget_name(sa, a.1)),
                    measured: f64::from(area),
                    required: 0.0,
                });
            }
        }
    }
}

fn radial_item_size(sr: &ScreenRt, i: usize) -> f32 {
    let Some(p) = sr.screen.nodes[i].parent else { return 0.0 };
    let Kind::Radial { radius, inner } = sr.screen.nodes[p as usize].kind else { return 0.0 };
    let pr = sr.comp[p as usize].rect;
    let scale = pr.w / (2.0 * radius);
    let count = sr.screen.children(p).filter(|&c| sr.comp[c as usize].visible && matches!(sr.screen.nodes[c as usize].kind, Kind::RadialItem { .. })).count().max(1);
    let thick = (radius - inner) * scale;
    let arc = std::f32::consts::TAU * (radius + inner) * 0.5 * scale / count as f32;
    thick.min(arc)
}

fn union(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = a.right().max(b.right());
    let y1 = a.bottom().max(b.bottom());
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

fn overshoot(r: &Rect, b: &Rect) -> f32 {
    (b.x - r.x).max(b.y - r.y).max(r.right() - b.right()).max(r.bottom() - b.bottom()).max(0.0)
}

fn short(s: &str) -> String {
    let mut t: String = s.chars().take(32).collect();
    if s.chars().count() > 32 {
        t.push('…');
    }
    t
}

/// What a label's text sits on: candidate opaque backgrounds (one per scene colour when the UI
/// behind it isn't fully opaque).
fn backgrounds(ui: &Ui, sr: &ScreenRt, i: usize, modal_below: bool) -> Vec<Rgba> {
    let theme = &ui.theme;
    // Layers from the node up to the root.
    let mut layers: Vec<Rgba> = Vec::new();
    let mut p = sr.screen.nodes[i].parent;
    while let Some(k) = p {
        let k = k as usize;
        match &sr.screen.nodes[k].kind {
            Kind::Panel { boxed } => layers.push(theme.boxed(*boxed).mid_fill()),
            Kind::Button { style, .. } => layers.push(theme.boxed(theme.button(*style).normal.boxed).mid_fill()),
            Kind::RadialItem { .. } => layers.push(theme.radial.wedge),
            _ => {}
        }
        p = sr.screen.nodes[k].parent;
    }
    let scene: Vec<Rgba> = if theme.scene.is_empty() { vec![[0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]] } else { theme.scene.clone() };
    scene
        .iter()
        .map(|&s| {
            let mut bg = [s[0], s[1], s[2], 1.0];
            if modal_below || sr.screen.modal {
                bg = over(theme.dim, bg);
            }
            for l in layers.iter().rev() {
                bg = over(*l, bg);
            }
            bg
        })
        .collect()
}

fn global_rules(ui: &Ui, ids: &[usize], state: &StateMap, input_map: Option<&ActionMap>, out: &mut Vec<Finding>) {
    let rules = ui.theme.check;
    let mut push = |rule: &'static str, sev: Severity, screen: &str, widget: String, message: String, measured: f32, required: f32| {
        out.push(Finding {
            rule,
            severity: sev,
            target: "all".into(),
            screen: screen.into(),
            widget,
            message,
            measured: f64::from(measured),
            required: f64::from(required),
        });
    };
    let modal_pos = ids.iter().position(|&s| ui.screens[s].screen.modal);
    let theme = &ui.theme;
    for (k, &s) in ids.iter().enumerate() {
        let sr = &ui.screens[s];
        let modal_below = modal_pos.is_some_and(|m| m < k);
        for (i, node) in sr.screen.nodes.iter().enumerate() {
            // ---- unbound keys (also in hidden widgets: they may show later) ----
            let scope = scope_for(&sr.screen, node, state);
            let mut keys: Vec<Key> = Vec::new();
            node.visible.keys(&mut keys);
            node.enabled.keys(&mut keys);
            if let Kind::Label { text: TextSource::Template(t), .. } = &node.kind {
                keys.extend(t.keys().cloned());
            }
            if let Some(t) = &node.tooltip {
                keys.extend(t.keys().cloned());
            }
            if let Kind::RadialItem { detail, label, .. } = &node.kind {
                keys.extend(detail.keys().cloned());
                keys.extend(label.keys().cloned());
            }
            let probe = scope.unwrap_or(Scope::new(state));
            for key in keys {
                let missing = match &key {
                    Key::Global(_) => probe.get(&key).is_none(),
                    // Row fields are only checked when a row exists.
                    Key::Item(_) => scope.is_some() && probe.item.is_some() && probe.get(&key).is_none(),
                    Key::Index { .. } => false,
                };
                if missing {
                    push("unbound", Severity::Warning, &sr.screen.name, widget_name(sr, i), format!("state has no \"{}\"", key.name()), 0.0, 1.0);
                }
            }
            // ---- contrast ----
            let c = &sr.comp[i];
            if c.visible {
                if let Kind::Label { role, color, .. } = &node.kind {
                    let parent_disabled = node.parent.is_some_and(|p| !sr.rt[p as usize].enabled);
                    if !sr.rt[i].text.trim().is_empty() && !parent_disabled {
                        let role = theme.role(*role);
                        let parent_kind = node.parent.map(|p| &sr.screen.nodes[p as usize].kind);
                        let fg = match color {
                            Some(crate::screen::ColorRef::Hex(c)) => *c,
                            Some(crate::screen::ColorRef::Named(n)) => theme.color(n).unwrap_or(role.color),
                            None => match parent_kind {
                                Some(Kind::Button { style, .. }) => theme.button(*style).normal.text,
                                Some(Kind::RadialItem { .. }) => theme.radial.text,
                                _ => role.color,
                            },
                        };
                        let need = if role.size >= rules.large_text_px { rules.contrast_large } else { rules.contrast_body };
                        let worst = backgrounds(ui, sr, i, modal_below)
                            .into_iter()
                            .map(|bg| (contrast_ratio(over(fg, bg), bg), bg))
                            .min_by(|a, b| a.0.total_cmp(&b.0));
                        if let Some((ratio, bg)) = worst {
                            if ratio + 1e-3 < need {
                                push(
                                    "contrast",
                                    Severity::Error,
                                    &sr.screen.name,
                                    widget_name(sr, i),
                                    format!("text {} on {} is {ratio:.2}:1 (\"{}\")", to_hex(fg), to_hex(bg), short(&sr.rt[i].text)),
                                    ratio,
                                    need,
                                );
                            }
                        }
                    }
                }
            }
            // ---- gamepad access for pointer-only HUD buttons ----
            if sr.screen.input == InputMode::Hud {
                if let Kind::Button { shortcut, .. } = &node.kind {
                    match shortcut {
                        None => push(
                            "gamepad_access",
                            Severity::Error,
                            &sr.screen.name,
                            widget_name(sr, i),
                            "a HUD button only the pointer can press: give it a \"shortcut\" input action".into(),
                            0.0,
                            1.0,
                        ),
                        Some(a) => {
                            if let Some(map) = input_map {
                                let pads = map.glyphs(a, DeviceFamily::Gamepad, specula_input::PadStyle::Xbox);
                                if map.action(a).is_none() || pads.is_empty() {
                                    push(
                                        "gamepad_access",
                                        Severity::Error,
                                        &sr.screen.name,
                                        widget_name(sr, i),
                                        format!("shortcut \"{a}\" has no gamepad binding in the input map"),
                                        0.0,
                                        1.0,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    // ---- theme-level: dynamic widgets ----
    let scene: Vec<Rgba> = if theme.scene.is_empty() { vec![[0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]] } else { theme.scene.clone() };
    let mut theme_pair = |rule: &'static str, what: &str, fg: Rgba, bgs: &[Rgba], need: f32| {
        if let Some((ratio, bg)) = bgs.iter().map(|&bg| (contrast_ratio(over(fg, bg), bg), bg)).min_by(|a, b| a.0.total_cmp(&b.0)) {
            if ratio + 1e-3 < need {
                out.push(Finding {
                    rule,
                    severity: Severity::Error,
                    target: "all".into(),
                    screen: "(theme)".into(),
                    widget: what.into(),
                    message: format!("{what}: {} on {} is {ratio:.2}:1", to_hex(fg), to_hex(bg)),
                    measured: f64::from(ratio),
                    required: f64::from(need),
                });
            }
        }
    };
    let on = |b: usize| -> Vec<Rgba> { scene.iter().map(|&s| over(theme.boxed(b).mid_fill(), [s[0], s[1], s[2], 1.0])).collect() };
    let tt = &theme.tooltip;
    theme_pair("contrast", "tooltip text", theme.role(tt.role).color, &on(tt.boxed), rules.contrast_body);
    let to = &theme.toast;
    theme_pair("contrast", "toast text", theme.role(to.role).color, &on(to.boxed), rules.contrast_body);
    theme_pair("contrast", "threat label", theme.threat.color, &on(theme.threat.label_box), rules.contrast_body);
    theme_pair("non_text_contrast", "threat arrow", theme.threat.color, &scene, 3.0);
    theme_pair("non_text_contrast", "focus ring", theme.focus.color, &scene, 3.0);
    let _ = Input::new;
}
