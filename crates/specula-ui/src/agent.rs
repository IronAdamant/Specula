//! What agents need from a [`Ui`] beyond what players do (the engine's `ui_tree`, `ui_frame`,
//! `ui_act` and `ui_check` tools): the laid-out tree as paged data, outlines for captures, the
//! focus scope and d-pad routes to a widget, layout without disturbing the player, and the
//! pseudo-locale switch.
//!
//! Everything here reads the layout the UI last made ([`Ui::last_view`]); [`Ui::inspect`] lays
//! out for another view and puts everything back.

use crate::draw::Rect;
use crate::error::{ErrorCode, UiError};
use crate::geom::{neighbour, Dir};
use crate::screen::{InputMode, Kind};
use crate::state::StateMap;
use crate::ui::{focusable, radial_items, ScreenRt, Ui, UiEvents, Viewport, WidgetRef};
use specula_core::json::Json;
use specula_input::{ActionMap, Input};

/// Most widgets one [`Ui::tree_page`] lists.
pub const MAX_PAGE: usize = 500;
/// Longest text a tree entry carries (characters; longer ends in `…`).
pub const MAX_ENTRY_TEXT: usize = 160;
/// Most d-pad presses a route may take.
pub const MAX_ROUTE: usize = 64;

/// Which widgets a tree lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Only {
    /// Every laid-out widget, layout boxes included.
    All,
    /// Widgets with an id, text, or that take input (what a player deals with).
    Named,
    /// Buttons and radial items.
    Interactive,
}

impl Only {
    pub fn parse(s: &str) -> Option<Only> {
        match s {
            "all" => Some(Only::All),
            "named" => Some(Only::Named),
            "interactive" => Some(Only::Interactive),
            _ => None,
        }
    }
}

/// What [`Ui::tree_page`] lists.
#[derive(Clone, Debug)]
pub struct TreeOpts {
    /// Only this screen (`None`: every visible screen, bottom to top).
    pub screen: Option<String>,
    pub only: Only,
    pub offset: usize,
    /// At most [`MAX_PAGE`].
    pub limit: usize,
}

impl Default for TreeOpts {
    fn default() -> Self {
        TreeOpts { screen: None, only: Only::Named, offset: 0, limit: 100 }
    }
}

/// A widget's outline for a capture (`ui_frame`).
#[derive(Clone, Debug, PartialEq)]
pub struct Outline {
    pub screen: String,
    pub id: Option<String>,
    pub kind: &'static str,
    /// Border box, physical px (clipped to what shows when an ancestor clips it).
    pub rect: Rect,
    pub interactive: bool,
    pub enabled: bool,
    pub focused: bool,
    /// A modal screen above takes its input (shown, but out of reach).
    pub blocked: bool,
}

fn round1(v: f32) -> Json {
    Json::Num((f64::from(v) * 10.0).round() / 10.0)
}

fn rect_json(r: Rect) -> Json {
    Json::arr(r.to_array().map(round1))
}

fn clip_text(s: &str) -> String {
    if s.chars().count() <= MAX_ENTRY_TEXT {
        return s.to_owned();
    }
    let mut t: String = s.chars().take(MAX_ENTRY_TEXT).collect();
    t.push('…');
    t
}

/// The label's text doesn't fit its box (what `ui_check`'s `text_overflow` measures).
pub(crate) fn text_overflows(sr: &ScreenRt, i: usize) -> bool {
    let Kind::Label { wrap, .. } = &sr.screen.nodes[i].kind else { return false };
    let c = &sr.comp[i];
    if sr.rt[i].text.trim().is_empty() {
        return false;
    }
    let [tw, th] = c.text;
    (!*wrap && tw > c.content.w + 0.5) || th > c.content.h + 0.5
}

impl Ui {
    /// The view the UI was last laid out for (`None` before its first frame).
    pub fn last_view(&self) -> Option<Viewport> {
        self.last_view
    }

    /// Forget which view the UI was last laid out for (until its next frame): a capture clears
    /// it before drawing, so only a UI the capture really laid out claims that capture's layout.
    pub fn forget_view(&mut self) {
        self.last_view = None;
    }

    /// Takes the player's interaction state from `live` (the same screens loaded from the same
    /// files: a capture's copy of the window's interface): which screen widgets have the focus,
    /// whether the focus shows, and every list's scroll, converted from `live`'s last scale to
    /// `scale` (the one this UI lays out at next). Screens are matched by name and copied only
    /// when their node counts agree. A capture drawn after this shows (and `ui_frame` outlines)
    /// the focus the player has, not the screens' default focus (review R1 of m5-uitools,
    /// finding 2).
    pub fn copy_interaction_from(&mut self, live: &Ui, scale: f32) {
        self.focus_visible = live.focus_visible;
        let k = match live.last_view {
            Some(v) if v.scale > 0.0 && scale.is_finite() && scale > 0.0 => scale / v.scale,
            _ => 1.0,
        };
        for sr in &mut self.screens {
            let Some(from) = live.screens.iter().find(|l| l.screen.name == sr.screen.name) else { continue };
            if from.screen.nodes.len() != sr.screen.nodes.len() || from.comp.len() != sr.comp.len() {
                continue;
            }
            sr.focus = from.focus;
            for (c, l) in sr.comp.iter_mut().zip(&from.comp) {
                c.scroll = l.scroll * k;
            }
        }
    }

    /// What the last frame sent (actions, refusals, back) and what it took.
    pub fn last_events(&self) -> &UiEvents {
        &self.events
    }

    /// Lay screens' text out in a pseudo-locale (`ui_check {pseudo: true}`): the screens' own
    /// words accented and about 35% longer, so overflow shows before anyone translates, and text
    /// the game supplies through bindings is noted per label (`untranslated`).
    pub fn set_pseudo_locale(&mut self, on: bool) {
        self.pseudo = on;
        if !on {
            for s in &mut self.screens {
                for r in &mut s.rt {
                    r.untranslated = None;
                }
            }
        }
    }

    pub fn pseudo_locale(&self) -> bool {
        self.pseudo
    }

    /// Lays the UI out for `view` with no new input, runs `f` on it, then puts the UI back as it
    /// was (layout, pointer, focus, hover, events, draw list, queued agent requests), so a
    /// player's next frame never notices.
    pub fn inspect<R>(&mut self, state: &StateMap, view: &Viewport, f: impl FnOnce(&Ui) -> R) -> R {
        let snap = self.snapshot();
        let pending = std::mem::take(&mut self.pending);
        let idle = Input::new(ActionMap::new());
        let _ = self.frame(&idle, state, view);
        let r = f(self);
        self.restore(snap);
        self.pending = pending;
        r
    }

    /// The screen that has gamepad / keyboard focus now: the topmost visible menu screen at or
    /// above the topmost modal (what the d-pad moves in).
    pub fn focus_screen(&self) -> Option<usize> {
        let lowest = self.order.iter().rposition(|&s| self.screens[s].screen.modal).unwrap_or(0);
        self.order.get(lowest..)?.iter().rev().copied().find(|&s| self.screens[s].screen.input == InputMode::Menu)
    }

    /// The focused widget of the focus screen.
    pub fn focused(&self) -> Option<WidgetRef> {
        let s = self.focus_screen()?;
        self.screens[s].focus.map(|node| WidgetRef { screen: s, node })
    }

    /// The topmost modal screen shown, if any.
    pub fn top_modal(&self) -> Option<&str> {
        self.order.iter().rev().find(|&&s| self.screens[s].screen.modal).map(|&s| self.screens[s].screen.name.as_str())
    }

    /// A widget by id on the visible screens, topmost first (what a player would see on top).
    pub fn widget_shown(&self, id: &str) -> Option<WidgetRef> {
        self.order.iter().rev().find_map(|&s| self.screens[s].screen.index_of(id).map(|node| WidgetRef { screen: s, node }))
    }

    /// Where the d-pad goes from widget `f` of screen `s` in direction `d` (the same rule as
    /// a frame's navigation: explicit `nav` overrides, else the nearest focusable neighbour;
    /// radial menus step around the ring).
    pub fn focus_step(&self, s: usize, f: u32, d: Dir) -> Option<u32> {
        let sr = self.screens.get(s)?;
        if sr.screen.nodes.iter().any(|n| matches!(n.kind, Kind::Radial { .. })) {
            let (items, count) = radial_items(sr)?;
            let items = &items[..count];
            let cur = items.iter().position(|&i| i == f).unwrap_or(0);
            let n = items.len();
            if n == 0 {
                return None;
            }
            return Some(match d {
                Dir::Right | Dir::Down => items[(cur + 1) % n],
                Dir::Left | Dir::Up => items[(cur + n - 1) % n],
            });
        }
        let node = sr.screen.nodes.get(f as usize)?;
        node.nav[d.index()].filter(|&t| focusable(sr, t as usize)).or_else(|| {
            let cands = (0..sr.screen.nodes.len()).filter(|&i| focusable(sr, i)).map(|i| (i as u32, sr.comp[i].rect));
            neighbour(f, sr.comp[f as usize].rect, d, cands)
        })
    }

    /// The shortest d-pad route from the current focus to `target` on the last layout. Refused
    /// with `not_visible` / `modal_blocks` when input can't reach the widget at all,
    /// `not_reachable` when it isn't focusable (disabled, hidden, not a button) or no route
    /// leads there.
    pub fn focus_route(&self, target: WidgetRef) -> Result<Vec<Dir>, UiError> {
        self.reachable(target)?;
        let s = target.screen;
        let name = &self.screens[s].screen.name;
        if self.focus_screen() != Some(s) {
            let why = match self.focus_screen() {
                Some(f) => format!("the d-pad moves in \"{}\", not \"{name}\"", self.screens[f].screen.name),
                None => format!("no menu screen has the focus (\"{name}\" takes no d-pad input)"),
            };
            return Err(UiError::new(ErrorCode::NotReachable, "", why));
        }
        let sr = &self.screens[s];
        if !focusable(sr, target.node as usize) {
            let n = &sr.screen.nodes[target.node as usize];
            let why = if !n.kind.interactive() {
                format!("a {} can't take focus", n.kind.name())
            } else if !sr.comp[target.node as usize].visible {
                "it isn't showing".to_owned()
            } else {
                "it is disabled".to_owned()
            };
            return Err(UiError::new(ErrorCode::NotReachable, "", why));
        }
        let start = match sr.focus.filter(|&f| focusable(sr, f as usize)) {
            Some(f) => f,
            None => return Err(UiError::new(ErrorCode::NotReachable, "", format!("\"{name}\" has no focused widget to move from"))),
        };
        let n = sr.screen.nodes.len();
        let mut prev: Vec<Option<(u32, Dir)>> = vec![None; n];
        let mut seen = vec![false; n];
        seen[start as usize] = true;
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(f) = queue.pop_front() {
            if f == target.node {
                break;
            }
            for d in Dir::ALL {
                if let Some(t) = self.focus_step(s, f, d) {
                    if !seen[t as usize] {
                        seen[t as usize] = true;
                        prev[t as usize] = Some((f, d));
                        queue.push_back(t);
                    }
                }
            }
        }
        if !seen[target.node as usize] {
            return Err(UiError::new(ErrorCode::NotReachable, "", "no d-pad route leads there from the focused widget".to_owned()));
        }
        let mut route = Vec::new();
        let mut at = target.node;
        while let Some((p, d)) = prev[at as usize] {
            route.push(d);
            at = p;
            if route.len() > MAX_ROUTE {
                return Err(UiError::new(ErrorCode::NotReachable, "", format!("the route is longer than {MAX_ROUTE} presses")));
            }
        }
        route.reverse();
        Ok(route)
    }

    /// The laid-out widgets as data, a page at a time: `{screens: [{name, layer, modal, input}],
    /// widgets: [...], total, offset, next_offset?}`. Each widget: `screen`, `id` (when it has
    /// one), `type`, `rect` (physical px, border box), `text` (labels), `enabled` / `focused` /
    /// `action` (buttons), `clipped` (an ancestor hides part of it), `overflow` (its text doesn't
    /// fit), `untranslated` (pseudo-locale: the key that supplied words), `blocked` (a modal
    /// above takes its input), `path`.
    pub fn tree_page(&self, opts: &TreeOpts) -> Json {
        let limit = opts.limit.min(MAX_PAGE);
        let modal_at = self.order.iter().rposition(|&s| self.screens[s].screen.modal);
        let focus_screen = self.focus_screen();
        let mut screens = Vec::new();
        let mut widgets = Vec::new();
        let mut total = 0usize;
        for (k, &s) in self.order.iter().enumerate() {
            let sr = &self.screens[s];
            if opts.screen.as_deref().is_some_and(|n| n != sr.screen.name) {
                continue;
            }
            let blocked = modal_at.is_some_and(|m| k < m);
            let mut sj = vec![
                ("name", Json::Str(sr.screen.name.clone())),
                ("layer", Json::Num(f64::from(sr.screen.layer))),
                ("input", Json::Str(match sr.screen.input {
                    InputMode::Menu => "menu",
                    InputMode::Hud => "hud",
                    InputMode::None => "none",
                }.into())),
            ];
            if sr.screen.modal {
                sj.push(("modal", Json::Bool(true)));
            }
            if blocked {
                sj.push(("blocked", Json::Bool(true)));
            }
            if focus_screen == Some(s) {
                sj.push(("has_focus", Json::Bool(true)));
            }
            screens.push(Json::obj(sj));
            for (i, n) in sr.screen.nodes.iter().enumerate() {
                let c = &sr.comp[i];
                if !c.visible {
                    continue;
                }
                let is_label = matches!(n.kind, Kind::Label { .. });
                let keep = match opts.only {
                    Only::All => true,
                    Only::Interactive => n.kind.interactive(),
                    Only::Named => n.id.is_some() || n.kind.interactive() || (is_label && !sr.rt[i].text.trim().is_empty()),
                };
                if !keep {
                    continue;
                }
                total += 1;
                if total <= opts.offset || widgets.len() >= limit {
                    continue;
                }
                let mut pairs = vec![("screen", Json::Str(sr.screen.name.clone()))];
                if let Some(id) = &n.id {
                    pairs.push(("id", Json::Str(id.clone())));
                }
                pairs.push(("type", Json::Str(n.kind.name().into())));
                pairs.push(("rect", rect_json(c.rect)));
                if is_label {
                    pairs.push(("text", Json::Str(clip_text(&sr.rt[i].text))));
                }
                if n.kind.interactive() {
                    pairs.push(("enabled", Json::Bool(sr.rt[i].enabled)));
                    pairs.push(("focused", Json::Bool(focus_screen == Some(s) && sr.focus == Some(i as u32))));
                    if let Kind::Button { action: Some(a), .. } | Kind::RadialItem { action: Some(a), .. } = &n.kind {
                        pairs.push(("action", Json::Str(a.to_string())));
                    }
                    if blocked {
                        pairs.push(("blocked", Json::Bool(true)));
                    }
                }
                if let Some(cl) = c.clip {
                    if !c.rect.inside(&cl) {
                        pairs.push(("clipped", Json::Bool(true)));
                        pairs.push(("visible_rect", rect_json(c.rect.intersect(&cl))));
                    }
                }
                if is_label && text_overflows(sr, i) {
                    pairs.push(("overflow", Json::Bool(true)));
                }
                if let Some(k) = &sr.rt[i].untranslated {
                    pairs.push(("untranslated", Json::Str(k.clone())));
                }
                pairs.push(("path", Json::Str(n.path.clone())));
                widgets.push(Json::obj(pairs));
            }
        }
        let mut out = vec![
            ("screens", Json::Arr(screens)),
            ("widgets", Json::Arr(widgets)),
            ("total", Json::Num(total as f64)),
            ("offset", Json::Num(opts.offset as f64)),
        ];
        let shown = total.saturating_sub(opts.offset).min(limit);
        if opts.offset + shown < total {
            out.push(("next_offset", Json::Num((opts.offset + shown) as f64)));
        }
        if let Some(v) = self.last_view {
            out.push(("view", Json::obj([
                ("size", Json::arr(v.size.map(|x| Json::Num(f64::from(x))))),
                ("scale", Json::Num(f64::from((v.scale * 1000.0).round() / 1000.0))),
                ("safe", rect_json(v.safe())),
            ])));
        }
        Json::obj(out)
    }

    /// Outlines of the widgets a player deals with (an id, or takes input), topmost screen last,
    /// at most `max`; and how many were left out.
    pub fn outlines(&self, max: usize) -> (Vec<Outline>, usize) {
        let focus = self.focused();
        let modal_at = self.order.iter().rposition(|&s| self.screens[s].screen.modal);
        let mut out = Vec::new();
        let mut left_out = 0;
        for (k, &s) in self.order.iter().enumerate() {
            let sr = &self.screens[s];
            let blocked = modal_at.is_some_and(|m| k < m);
            for (i, n) in sr.screen.nodes.iter().enumerate() {
                let c = &sr.comp[i];
                if !c.visible || (n.id.is_none() && !n.kind.interactive()) || i == 0 && n.id.is_none() {
                    continue;
                }
                let rect = match c.clip {
                    Some(cl) => c.rect.intersect(&cl),
                    None => c.rect,
                };
                if rect.is_empty() {
                    continue;
                }
                if out.len() >= max {
                    left_out += 1;
                    continue;
                }
                out.push(Outline {
                    screen: sr.screen.name.clone(),
                    id: n.id.clone(),
                    kind: n.kind.name(),
                    rect,
                    interactive: n.kind.interactive(),
                    enabled: sr.rt[i].enabled,
                    focused: focus == Some(WidgetRef { screen: s, node: i as u32 }),
                    blocked,
                });
            }
        }
        (out, left_out)
    }
}
