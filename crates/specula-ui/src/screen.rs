//! Screens as data (`specula.ui.screen.v1`): a tree of widgets parsed into a flat pre-order array
//! of [`Node`]s, validated strictly against the theme (unknown fields, roles, colours and styles
//! are errors with JSON paths).
//!
//! ```text
//! {
//!   "format": "specula.ui.screen.v1",
//!   "name": "hud",
//!   "input": "hud",                      // "menu" (focus + gamepad navigation), "hud" (pointer only), "none"
//!   "root": { "type": "box", "children": [
//!     { "type": "panel", "anchor": "top", "margin": 8, "layout": "row", "gap": "$m", "children": [
//!       { "type": "label", "text": "Wave {wave}/{waves}", "role": "value" },
//!       { "type": "label", "text": "Dust {dust}" } ] },
//!     { "type": "button", "id": "call", "text": "Call wave", "action": "call_wave",
//!       "shortcut": "call_wave", "anchor": "bottom_right", "margin": 12, "visible": "!paused" } ] }
//! }
//! ```

use crate::color::Rgba;
use crate::error::{bad, parse_json, wrong, ErrorCode, Obj, UiError};
use crate::state::{Cond, Key, Template};
use crate::theme::{unknown, Theme};
use specula_core::json::Json;
use std::rc::Rc;

pub const SCREEN_FORMAT: &str = "specula.ui.screen.v1";
pub const MAX_NODES: usize = 4096;
pub const MAX_DEPTH: usize = 48;
pub const MAX_REPEAT: usize = 256;

/// A length: logical px, percent of the parent's content box, or em (the theme's body size).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Len {
    Px(f32),
    Percent(f32),
    Em(f32),
}

impl Len {
    pub const ZERO: Len = Len::Px(0.0);

    /// Physical px.
    pub fn resolve(self, parent: f32, scale: f32, em: f32) -> f32 {
        match self {
            Len::Px(v) => v * scale,
            Len::Percent(p) => p * 0.01 * parent,
            Len::Em(e) => e * em * scale,
        }
    }
}

/// A widget's size along one axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Size {
    /// Fit the content.
    Auto,
    Fixed(Len),
    /// Share the parent's leftover space by weight (in a stack: the parent's full content size).
    Fill(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Children are placed by their own `anchor` inside the content box (overlapping allowed).
    Stack,
    Row,
    Column,
    Grid { columns: u16 },
    /// Radial menu items around a circle (internal: made by `radial`).
    Radial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Justify {
    Start,
    Center,
    End,
    SpaceBetween,
}

/// Where a child of a stack sits: fractions of the free space, `[0..1, 0..1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor(pub [f32; 2]);

const ANCHORS: [(&str, [f32; 2]); 9] = [
    ("top_left", [0.0, 0.0]),
    ("top", [0.5, 0.0]),
    ("top_right", [1.0, 0.0]),
    ("left", [0.0, 0.5]),
    ("center", [0.5, 0.5]),
    ("right", [1.0, 0.5]),
    ("bottom_left", [0.0, 1.0]),
    ("bottom", [0.5, 1.0]),
    ("bottom_right", [1.0, 1.0]),
];

/// Horizontal text alignment.
pub use specula_text::Align as TextAlign;

/// A colour from the screen: fixed, or a palette name resolved through the current theme.
#[derive(Clone, Debug, PartialEq)]
pub enum ColorRef {
    Hex(Rgba),
    Named(String),
}

/// Built-in vector icons (drawn as anti-aliased meshes, no textures).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Circle,
    Ring,
    Diamond,
    Triangle,
    Arrow,
    Chevron,
    Tombstone,
    Dust,
    Wave,
    Rune,
    Warding,
    Pillar,
    Brazier,
    Obelisk,
    Wall,
    Cross,
    Check,
    Pause,
    Play,
}

pub const SHAPES: [(&str, Shape); 19] = [
    ("circle", Shape::Circle),
    ("ring", Shape::Ring),
    ("diamond", Shape::Diamond),
    ("triangle", Shape::Triangle),
    ("arrow", Shape::Arrow),
    ("chevron", Shape::Chevron),
    ("tombstone", Shape::Tombstone),
    ("dust", Shape::Dust),
    ("wave", Shape::Wave),
    ("rune", Shape::Rune),
    ("warding", Shape::Warding),
    ("pillar", Shape::Pillar),
    ("brazier", Shape::Brazier),
    ("obelisk", Shape::Obelisk),
    ("wall", Shape::Wall),
    ("cross", Shape::Cross),
    ("check", Shape::Check),
    ("pause", Shape::Pause),
    ("play", Shape::Play),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IconSource {
    Shape(Shape),
    /// A region of a texture the renderer knows by id; `uv` is `[u0, v0, u1, v1]`.
    Image { texture: u32, uv: [f32; 4] },
}

#[derive(Clone, Debug, PartialEq)]
pub enum TextSource {
    Template(Template),
    /// The prompt glyph of an input action for the last-used device (`"LMB"`, `"A"`).
    Prompt(String),
    /// The detail line of the radial menu's highlighted item (`radial` node index).
    RadialDetail(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Num {
    Const(f32),
    Key(Key),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// Layout only, draws nothing.
    Box,
    Panel { boxed: usize },
    Label { text: TextSource, role: usize, color: Option<ColorRef>, align: TextAlign, wrap: bool },
    Button { action: Option<Rc<str>>, style: usize, shortcut: Option<String> },
    Icon { icon: IconSource, color: Option<ColorRef>, rotation: f32 },
    Bar { value: Num, max: Num, color: Option<ColorRef> },
    /// A scrolling column; `repeat` instances follow the static children.
    List { repeat: Option<Repeat> },
    Radial { radius: f32, inner: f32 },
    RadialItem { action: Option<Rc<str>>, detail: Template, label: Template },
    /// Edge arrows for the rows of a state list (`x`, `y`, `behind`, optional `label`).
    Threats { source: Key },
    Toasts { max: u16 },
    /// A keycap with an action's glyph, then optional text.
    Prompt { action: String },
    Spacer,
}

impl Kind {
    pub fn name(&self) -> &'static str {
        match self {
            Kind::Box => "box",
            Kind::Panel { .. } => "panel",
            Kind::Label { .. } => "label",
            Kind::Button { .. } => "button",
            Kind::Icon { .. } => "icon",
            Kind::Bar { .. } => "bar",
            Kind::List { .. } => "list",
            Kind::Radial { .. } => "radial",
            Kind::RadialItem { .. } => "radial_item",
            Kind::Threats { .. } => "threats",
            Kind::Toasts { .. } => "toasts",
            Kind::Prompt { .. } => "prompt",
            Kind::Spacer => "spacer",
        }
    }

    /// Takes pointer clicks / focus.
    pub fn interactive(&self) -> bool {
        matches!(self, Kind::Button { .. } | Kind::RadialItem { .. })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Repeat {
    pub source: Key,
    pub max: usize,
}

/// One widget. Nodes are stored in pre-order: a node's subtree is `index..end`.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: Option<String>,
    pub kind: Kind,
    pub parent: Option<u32>,
    pub end: u32,
    pub width: Size,
    pub height: Size,
    pub min: [Option<Len>; 2],
    pub max: [Option<Len>; 2],
    pub layout: Layout,
    pub gap: Len,
    pub padding: [Len; 4],
    pub align: Align,
    pub justify: Justify,
    pub anchor: Anchor,
    pub margin: [Len; 4],
    pub offset: [Len; 2],
    pub visible: Cond,
    pub enabled: Cond,
    pub tooltip: Option<Template>,
    /// Explicit focus neighbours (`up`, `down`, `left`, `right`) as node indices.
    pub nav: [Option<u32>; 4],
    /// Inside a list `repeat`: (list node, row index).
    pub item: Option<(u32, usize)>,
    /// Clip descendants to this node's content box (lists always do).
    pub clip: bool,
    /// Intentional overlap with siblings (skipped by `ui_check`'s overlap rule).
    pub overlap_ok: bool,
    /// Where it came from, for findings (`root.children[1].children[0]`).
    pub path: String,
}

/// What a screen does with input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    /// Focus, gamepad / keyboard navigation and pointer.
    Menu,
    /// Pointer only: the game keeps its gamepad and keys.
    Hud,
    /// Draw only.
    None,
}

#[derive(Clone, Debug)]
pub struct Screen {
    pub name: String,
    pub input: InputMode,
    /// Draw / input order: higher is on top.
    pub layer: i32,
    /// Dims what is below and takes all input.
    pub modal: bool,
    /// Ignore the safe area (full-bleed backgrounds).
    pub full_bleed: bool,
    pub focus: Option<u32>,
    /// Action sent when "back" is pressed on this screen.
    pub back: Option<Rc<str>>,
    pub nodes: Vec<Node>,
    /// The JSON it was loaded from (re-parsed when the theme changes).
    pub source: String,
}

impl Screen {
    pub fn index_of(&self, id: &str) -> Option<u32> {
        self.nodes.iter().position(|n| n.id.as_deref() == Some(id)).map(|i| i as u32)
    }

    /// Children of `i`, in order.
    pub fn children(&self, i: u32) -> Children<'_> {
        Children { nodes: &self.nodes, next: i + 1, end: self.nodes[i as usize].end }
    }

    pub fn parse(text: &str, theme: &Theme) -> Result<Screen, UiError> {
        let json = parse_json(text)?;
        let mut o = Obj::new(&json, "")?;
        let format = o.req_str("format")?;
        if format != SCREEN_FORMAT {
            return Err(UiError::new(ErrorCode::Format, "format", format!("expected \"{SCREEN_FORMAT}\", found \"{format}\"")));
        }
        o.str("notes")?;
        let name = o.req_str("name")?.to_owned();
        if name.is_empty() {
            return Err(bad("name", "a screen name can't be empty"));
        }
        let input = match o.str("input")?.unwrap_or("hud") {
            "menu" => InputMode::Menu,
            "hud" => InputMode::Hud,
            "none" => InputMode::None,
            other => return Err(bad("input", format!("\"{other}\" is not menu, hud or none"))),
        };
        let layer = o.num("layer")?.unwrap_or(0.0) as i32;
        let modal = o.bool("modal")?.unwrap_or(false);
        let full_bleed = o.bool("full_bleed")?.unwrap_or(false);
        let focus_id = o.str("focus")?;
        let back = o.str("back")?.map(Rc::from);
        let root = o.req("root")?;
        o.finish(&["format", "name", "input", "layer", "modal", "full_bleed", "focus", "back", "root", "notes"])?;

        let mut p = Parser { theme, nodes: Vec::new(), navs: Vec::new() };
        p.node(root, "root", None, 0, None)?;
        let mut nodes = p.nodes;

        // Ids unique; explicit nav targets exist.
        for (i, n) in nodes.iter().enumerate() {
            if let Some(id) = &n.id {
                if nodes[..i].iter().any(|m| m.id.as_deref() == Some(id)) {
                    return Err(UiError::new(ErrorCode::DuplicateId, &n.path, format!("id \"{id}\" is used twice")));
                }
            }
        }
        for (node, dir, target, path) in p.navs {
            let t = nodes.iter().position(|n| n.id.as_deref() == Some(target.as_str())).ok_or_else(|| {
                bad(&path, format!("nav target \"{target}\" is not an id in this screen"))
            })?;
            nodes[node as usize].nav[dir] = Some(t as u32);
        }
        let focus = match focus_id {
            Some(id) => {
                let i = nodes.iter().position(|n| n.id.as_deref() == Some(id)).ok_or_else(|| {
                    bad("focus", format!("\"{id}\" is not an id in this screen"))
                })?;
                if !nodes[i].kind.interactive() {
                    return Err(bad("focus", format!("\"{id}\" is a {}, which can't take focus", nodes[i].kind.name())));
                }
                Some(i as u32)
            }
            None => None,
        };
        Ok(Screen { name, input, layer, modal, full_bleed, focus, back, nodes, source: text.to_owned() })
    }
}

pub struct Children<'a> {
    nodes: &'a [Node],
    next: u32,
    end: u32,
}

impl Iterator for Children<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        if self.next >= self.end {
            return None;
        }
        let i = self.next;
        self.next = self.nodes[i as usize].end;
        Some(i)
    }
}

struct Parser<'t> {
    theme: &'t Theme,
    nodes: Vec<Node>,
    /// (node, direction, target id, path) resolved after the tree is built.
    navs: Vec<(u32, usize, String, String)>,
}

const COMMON: &[&str] = &[
    "type", "id", "width", "height", "min_width", "max_width", "min_height", "max_height", "layout", "columns", "gap",
    "padding", "align", "justify", "anchor", "margin", "offset", "visible", "enabled", "tooltip", "nav", "clip",
    "overlap_ok", "children", "size", "notes",
];

const TYPES: &[&str] = &[
    "box", "panel", "label", "button", "icon", "bar", "list", "radial", "threats", "toasts", "prompt", "spacer",
];

impl Parser<'_> {
    fn blank(&self, kind: Kind, parent: Option<u32>, path: &str, item: Option<(u32, usize)>) -> Node {
        Node {
            id: None,
            kind,
            parent,
            end: 0,
            width: Size::Auto,
            height: Size::Auto,
            min: [None; 2],
            max: [None; 2],
            layout: Layout::Stack,
            gap: Len::ZERO,
            padding: [Len::ZERO; 4],
            align: Align::Start,
            justify: Justify::Start,
            anchor: Anchor([0.0, 0.0]),
            margin: [Len::ZERO; 4],
            offset: [Len::ZERO; 2],
            visible: Cond::Always,
            enabled: Cond::Always,
            tooltip: None,
            nav: [None; 4],
            item,
            clip: false,
            overlap_ok: false,
            path: path.to_owned(),
        }
    }

    fn push(&mut self, node: Node, path: &str) -> Result<u32, UiError> {
        if self.nodes.len() >= MAX_NODES {
            return Err(UiError::new(ErrorCode::TooLarge, path, format!("more than {MAX_NODES} widgets in one screen")));
        }
        self.nodes.push(node);
        Ok((self.nodes.len() - 1) as u32)
    }

    fn close(&mut self, i: u32) {
        self.nodes[i as usize].end = self.nodes.len() as u32;
    }

    fn color(&self, j: &Json, path: &str) -> Result<ColorRef, UiError> {
        let s = j.as_str().ok_or_else(|| wrong(path, "a colour (\"#rrggbb\" or a palette name)", j))?;
        if s.starts_with('#') {
            Ok(ColorRef::Hex(self.theme.resolve_color(s, path)?))
        } else {
            self.theme.resolve_color(s, path)?;
            Ok(ColorRef::Named(s.to_owned()))
        }
    }

    fn len(&self, j: &Json, path: &str) -> Result<Len, UiError> {
        let err = || wrong(path, "a length (12, \"12px\", \"50%\", \"1.5em\" or \"$name\")", j);
        match j {
            Json::Num(v) if v.is_finite() && v.abs() < 1e5 => Ok(Len::Px(*v as f32)),
            Json::Str(s) => {
                let s = s.trim();
                if let Some(name) = s.strip_prefix('$') {
                    return self.theme.spacing(name).map(Len::Px).ok_or_else(|| {
                        unknown(path, "spacing", name, self.theme.spacing.iter().map(|(n, _)| n.as_str()))
                    });
                }
                let (num, unit) = if let Some(n) = s.strip_suffix('%') {
                    (n, 1)
                } else if let Some(n) = s.strip_suffix("em") {
                    (n, 2)
                } else if let Some(n) = s.strip_suffix("px") {
                    (n, 0)
                } else {
                    (s, 0)
                };
                let v: f32 = num.trim().parse().map_err(|_| err())?;
                if !v.is_finite() || v.abs() >= 1e5 {
                    return Err(err());
                }
                Ok(match unit {
                    1 => Len::Percent(v),
                    2 => Len::Em(v),
                    _ => Len::Px(v),
                })
            }
            _ => Err(err()),
        }
    }

    fn size(&self, j: &Json, path: &str) -> Result<Size, UiError> {
        if let Some(s) = j.as_str() {
            let s = s.trim();
            if s == "auto" {
                return Ok(Size::Auto);
            }
            if s == "fill" {
                return Ok(Size::Fill(1.0));
            }
            if let Some(w) = s.strip_prefix("fill:") {
                let w: f32 = w.trim().parse().map_err(|_| bad(path, format!("bad fill weight in \"{s}\"")))?;
                if !(w > 0.0 && w < 1e4) {
                    return Err(bad(path, "fill weight must be > 0"));
                }
                return Ok(Size::Fill(w));
            }
        }
        let l = self.len(j, path).map_err(|_| wrong(path, "a size (\"auto\", \"fill\", \"fill:2\", 12, \"50%\", \"2em\")", j))?;
        match l {
            Len::Px(v) | Len::Em(v) | Len::Percent(v) if v < 0.0 => Err(bad(path, "sizes can't be negative")),
            l => Ok(Size::Fixed(l)),
        }
    }

    fn len4(&self, j: &Json, path: &str) -> Result<[Len; 4], UiError> {
        match j {
            Json::Arr(a) if a.len() == 2 => {
                let v = self.len(&a[0], &format!("{path}[0]"))?;
                let h = self.len(&a[1], &format!("{path}[1]"))?;
                Ok([v, h, v, h])
            }
            Json::Arr(a) if a.len() == 4 => {
                let mut out = [Len::ZERO; 4];
                for (k, x) in a.iter().enumerate() {
                    out[k] = self.len(x, &format!("{path}[{k}]"))?;
                }
                Ok(out)
            }
            Json::Arr(_) => Err(wrong(path, "one length, [v, h] or [top, right, bottom, left]", j)),
            _ => Ok([self.len(j, path)?; 4]),
        }
    }

    fn template(&self, j: &Json, path: &str) -> Result<Template, UiError> {
        let s = j.as_str().ok_or_else(|| wrong(path, "text", j))?;
        Template::parse(s, path)
    }

    fn cond(&self, j: &Json, path: &str) -> Result<Cond, UiError> {
        match j {
            Json::Bool(true) => Ok(Cond::Always),
            Json::Bool(false) => Ok(Cond::Any(Vec::new())),
            Json::Str(s) => Cond::parse(s, path),
            Json::Arr(a) => {
                let mut all = Vec::new();
                for (k, c) in a.iter().enumerate() {
                    all.push(self.cond(c, &format!("{path}[{k}]"))?);
                }
                Ok(Cond::All(all))
            }
            _ => Err(wrong(path, "a condition string, a list of them, or true/false", j)),
        }
    }

    fn role(&self, o: &mut Obj<'_>, default: &str) -> Result<usize, UiError> {
        match o.str("role")? {
            Some(r) => self.theme.role_ref(r, &o.field_path("role")),
            None => Ok(self.theme.role_index(default).or_else(|| self.theme.role_index("body")).unwrap_or(0)),
        }
    }

    fn text_align(&self, o: &mut Obj<'_>, key: &str) -> Result<Option<TextAlign>, UiError> {
        Ok(match o.str(key)? {
            None => None,
            Some("left") => Some(TextAlign::Left),
            Some("center") => Some(TextAlign::Center),
            Some("right") => Some(TextAlign::Right),
            Some(other) => return Err(bad(&o.field_path(key), format!("\"{other}\" is not left, center or right"))),
        })
    }

    /// Parse the common layout fields into `n`.
    fn common(&mut self, o: &mut Obj<'_>, n: &mut Node) -> Result<(), UiError> {
        o.str("notes")?;
        if let Some(id) = o.str("id")? {
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
                return Err(bad(&o.field_path("id"), format!("id \"{id}\" must be letters, digits, _ or -")));
            }
            n.id = Some(id.to_owned());
        }
        if let Some(j) = o.get("size") {
            let s = self.size(j, &o.field_path("size"))?;
            n.width = s;
            n.height = s;
        }
        if let Some(j) = o.get("width") {
            n.width = self.size(j, &o.field_path("width"))?;
        }
        if let Some(j) = o.get("height") {
            n.height = self.size(j, &o.field_path("height"))?;
        }
        for (k, key) in ["min_width", "min_height"].iter().enumerate() {
            if let Some(j) = o.get(key) {
                n.min[k] = Some(self.len(j, &o.field_path(key))?);
            }
        }
        for (k, key) in ["max_width", "max_height"].iter().enumerate() {
            if let Some(j) = o.get(key) {
                n.max[k] = Some(self.len(j, &o.field_path(key))?);
            }
        }
        if let Some(l) = o.str("layout")? {
            n.layout = match l {
                "stack" => Layout::Stack,
                "row" => Layout::Row,
                "column" => Layout::Column,
                "grid" => Layout::Grid { columns: 2 },
                other => return Err(bad(&o.field_path("layout"), format!("\"{other}\" is not stack, row, column or grid"))),
            };
        }
        if let Some(c) = o.num("columns")? {
            if !(1.0..=64.0).contains(&c) || c.fract() != 0.0 {
                return Err(bad(&o.field_path("columns"), "columns must be a whole number 1..=64"));
            }
            match n.layout {
                Layout::Grid { .. } => n.layout = Layout::Grid { columns: c as u16 },
                _ => return Err(bad(&o.field_path("columns"), "\"columns\" needs \"layout\": \"grid\"")),
            }
        }
        if let Some(j) = o.get("gap") {
            n.gap = self.len(j, &o.field_path("gap"))?;
        }
        if let Some(j) = o.get("padding") {
            n.padding = self.len4(j, &o.field_path("padding"))?;
        }
        if let Some(j) = o.get("margin") {
            n.margin = self.len4(j, &o.field_path("margin"))?;
        }
        if let Some(j) = o.get("offset") {
            let p = o.field_path("offset");
            match j.as_array() {
                Some([x, y]) => n.offset = [self.len(x, &format!("{p}[0]"))?, self.len(y, &format!("{p}[1]"))?],
                _ => return Err(wrong(&p, "[x, y]", j)),
            }
        }
        if let Some(a) = o.str("align")? {
            n.align = match a {
                "start" => Align::Start,
                "center" => Align::Center,
                "end" => Align::End,
                "stretch" => Align::Stretch,
                other => return Err(bad(&o.field_path("align"), format!("\"{other}\" is not start, center, end or stretch"))),
            };
        }
        if let Some(a) = o.str("justify")? {
            n.justify = match a {
                "start" => Justify::Start,
                "center" => Justify::Center,
                "end" => Justify::End,
                "space_between" => Justify::SpaceBetween,
                other => {
                    return Err(bad(&o.field_path("justify"), format!("\"{other}\" is not start, center, end or space_between")))
                }
            };
        }
        if let Some(a) = o.str("anchor")? {
            n.anchor = ANCHORS.iter().find(|(name, _)| *name == a).map(|(_, v)| Anchor(*v)).ok_or_else(|| {
                bad(
                    &o.field_path("anchor"),
                    format!("\"{a}\" is not an anchor{}", crate::error::suggest(a, ANCHORS.iter().map(|(n, _)| *n))),
                )
            })?;
        }
        if let Some(j) = o.get("visible") {
            n.visible = self.cond(j, &o.field_path("visible"))?;
        }
        if let Some(j) = o.get("enabled") {
            n.enabled = self.cond(j, &o.field_path("enabled"))?;
        }
        if let Some(j) = o.get("tooltip") {
            n.tooltip = Some(self.template(j, &o.field_path("tooltip"))?);
        }
        n.clip = o.bool("clip")?.unwrap_or(false);
        n.overlap_ok = o.bool("overlap_ok")?.unwrap_or(false);
        Ok(())
    }

    fn navs(&mut self, o: &mut Obj<'_>, index: u32) -> Result<(), UiError> {
        if let Some(j) = o.get("nav") {
            let p = o.field_path("nav");
            let mut nav = Obj::new(j, &p)?;
            for (d, key) in ["up", "down", "left", "right"].iter().enumerate() {
                if let Some(t) = nav.str(key)? {
                    self.navs.push((index, d, t.to_owned(), nav.field_path(key)));
                }
            }
            nav.finish(&["up", "down", "left", "right"])?;
        }
        Ok(())
    }

    fn children(&mut self, o: &mut Obj<'_>, parent: u32, depth: usize, item: Option<(u32, usize)>) -> Result<(), UiError> {
        if let Some(j) = o.get("children") {
            let p = o.field_path("children");
            let arr = j.as_array().ok_or_else(|| wrong(&p, "an array of widgets", j))?;
            for (k, c) in arr.iter().enumerate() {
                self.node(c, &format!("{p}[{k}]"), Some(parent), depth + 1, item)?;
            }
        }
        Ok(())
    }

    fn node(&mut self, j: &Json, path: &str, parent: Option<u32>, depth: usize, item: Option<(u32, usize)>) -> Result<u32, UiError> {
        if depth > MAX_DEPTH {
            return Err(UiError::new(ErrorCode::TooLarge, path, format!("widgets nested deeper than {MAX_DEPTH}")));
        }
        let mut o = Obj::new(j, path)?;
        let ty = o.req_str("type")?;
        let mut extra: Vec<&str> = Vec::new();
        let theme = self.theme;
        let index = self.nodes.len() as u32;
        // Every kind builds its node (and any synthetic children) here.
        match ty {
            "box" | "spacer" => {
                let kind = if ty == "box" { Kind::Box } else { Kind::Spacer };
                let mut n = self.blank(kind, parent, path, item);
                if ty == "spacer" {
                    n.width = Size::Fill(1.0);
                    n.height = Size::Fill(1.0);
                }
                self.common(&mut o, &mut n)?;
                let i = self.push(n, path)?;
                self.navs(&mut o, i)?;
                if ty == "box" {
                    self.children(&mut o, i, depth, item)?;
                }
                self.close(i);
            }
            "panel" => {
                extra.push("style");
                let boxed = match o.str("style")? {
                    Some(s) => theme.box_ref(s, &o.field_path("style"))?,
                    None => theme.box_index("panel").unwrap_or(0),
                };
                let mut n = self.blank(Kind::Panel { boxed }, parent, path, item);
                n.padding = theme.boxed(boxed).padding.map(Len::Px);
                self.common(&mut o, &mut n)?;
                let i = self.push(n, path)?;
                self.navs(&mut o, i)?;
                self.children(&mut o, i, depth, item)?;
                self.close(i);
            }
            "label" => {
                extra.extend(["text", "role", "color", "text_align", "wrap"]);
                let text = self.template(o.req("text")?, &o.field_path("text"))?;
                let role = self.role(&mut o, "body")?;
                let color = match o.get("color") {
                    Some(c) => Some(self.color(c, &o.field_path("color"))?),
                    None => None,
                };
                let align = self.text_align(&mut o, "text_align")?.unwrap_or(TextAlign::Left);
                let wrap = o.bool("wrap")?.unwrap_or(false);
                let mut n = self.blank(Kind::Label { text: TextSource::Template(text), role, color, align, wrap }, parent, path, item);
                self.common(&mut o, &mut n)?;
                let i = self.push(n, path)?;
                self.navs(&mut o, i)?;
                self.close(i);
            }
            "button" => {
                extra.extend(["text", "action", "style", "shortcut", "icon", "role"]);
                let style = match o.str("style")? {
                    Some(s) => theme.button_ref(s, &o.field_path("style"))?,
                    None => theme.button_index("default").unwrap_or(0),
                };
                let action = o.str("action")?.map(Rc::from);
                let shortcut = o.str("shortcut")?.map(str::to_owned);
                let bs = theme.button(style).clone();
                let role = match o.str("role")? {
                    Some(r) => theme.role_ref(r, &o.field_path("role"))?,
                    None => bs.role,
                };
                let mut n = self.blank(Kind::Button { action, style, shortcut }, parent, path, item);
                n.layout = Layout::Row;
                n.align = Align::Center;
                n.justify = Justify::Center;
                n.gap = Len::Em(0.4);
                n.padding = bs.padding.map(Len::Px);
                if bs.min_height > 0.0 {
                    n.min[1] = Some(Len::Px(bs.min_height));
                }
                if !o.has("id") {
                    return Err(UiError::new(ErrorCode::MissingField, o.field_path("id"), "buttons need an \"id\" (focus, agents and ui_check refer to it)"));
                }
                self.common(&mut o, &mut n)?;
                let i = self.push(n, path)?;
                self.navs(&mut o, i)?;
                if let Some(ic) = o.get("icon") {
                    let shape = shape(ic, &o.field_path("icon"))?;
                    let mut icon = self.blank(Kind::Icon { icon: IconSource::Shape(shape), color: None, rotation: 0.0 }, Some(i), &o.field_path("icon"), item);
                    icon.width = Size::Fixed(Len::Em(1.2));
                    icon.height = Size::Fixed(Len::Em(1.2));
                    let k = self.push(icon, path)?;
                    self.close(k);
                }
                if let Some(t) = o.get("text") {
                    let text = self.template(t, &o.field_path("text"))?;
                    let label = self.blank(
                        Kind::Label { text: TextSource::Template(text), role, color: None, align: TextAlign::Center, wrap: false },
                        Some(i),
                        &o.field_path("text"),
                        item,
                    );
                    let k = self.push(label, path)?;
                    self.close(k);
                }
                self.children(&mut o, i, depth, item)?;
                self.close(i);
            }
            "icon" => {
                extra.extend(["shape", "image", "color", "rotation"]);
                let icon = match (o.get("shape"), o.get("image")) {
                    (Some(s), None) => IconSource::Shape(shape(s, &o.field_path("shape"))?),
                    (None, Some(img)) => {
                        let p = o.field_path("image");
                        let mut io = Obj::new(img, &p)?;
                        let texture = io.num("texture")?.ok_or_else(|| UiError::new(ErrorCode::MissingField, io.field_path("texture"), "\"texture\" is required"))?;
                        if texture < 0.0 || texture.fract() != 0.0 {
                            return Err(bad(&io.field_path("texture"), "texture ids are whole numbers ≥ 0"));
                        }
                        let uv = match io.get("uv") {
                            Some(u) => {
                                let up = io.field_path("uv");
                                let a = u.as_array().filter(|a| a.len() == 4).ok_or_else(|| wrong(&up, "[u0, v0, u1, v1]", u))?;
                                let mut uv = [0.0; 4];
                                for (k, v) in a.iter().enumerate() {
                                    uv[k] = v.as_f64().ok_or_else(|| wrong(&up, "numbers", v))? as f32;
                                }
                                uv
                            }
                            None => [0.0, 0.0, 1.0, 1.0],
                        };
                        io.finish(&["texture", "uv"])?;
                        IconSource::Image { texture: texture as u32, uv }
                    }
                    (Some(_), Some(_)) => return Err(bad(path, "an icon has a \"shape\" or an \"image\", not both")),
                    (None, None) => return Err(UiError::new(ErrorCode::MissingField, o.field_path("shape"), "an icon needs a \"shape\" or an \"image\"")),
                };
                let color = match o.get("color") {
                    Some(c) => Some(self.color(c, &o.field_path("color"))?),
                    None => None,
                };
                let rotation = o.num("rotation")?.unwrap_or(0.0);
                let mut n = self.blank(Kind::Icon { icon, color, rotation }, parent, path, item);
                n.width = Size::Fixed(Len::Em(1.25));
                n.height = Size::Fixed(Len::Em(1.25));
                self.common(&mut o, &mut n)?;
                let i = self.push(n, path)?;
                self.navs(&mut o, i)?;
                self.close(i);
            }
            "bar" => {
                extra.extend(["value", "max", "color"]);
                let num = |o: &mut Obj<'_>, key: &str| -> Result<Option<Num>, UiError> {
                    let p = o.field_path(key);
                    match o.get(key) {
                        None => Ok(None),
                        Some(Json::Num(v)) if v.is_finite() => Ok(Some(Num::Const(*v as f32))),
                        Some(Json::Str(s)) => Key::parse(s).map(|k| Some(Num::Key(k))).ok_or_else(|| bad(&p, format!("bad key \"{s}\""))),
                        Some(j) => Err(wrong(&p, "a number or a state key", j)),
                    }
                };
                let value = num(&mut o, "value")?.ok_or_else(|| UiError::new(ErrorCode::MissingField, o.field_path("value"), "\"value\" is required"))?;
                let max = num(&mut o, "max")?.unwrap_or(Num::Const(1.0));
                let color = match o.get("color") {
                    Some(c) => Some(self.color(c, &o.field_path("color"))?),
                    None => None,
                };
                let mut n = self.blank(Kind::Bar { value, max, color }, parent, path, item);
                n.width = Size::Fixed(Len::Em(8.0));
                n.height = Size::Fixed(Len::Px(theme.bar.height));
                self.common(&mut o, &mut n)?;
                let i = self.push(n, path)?;
                self.navs(&mut o, i)?;
                self.close(i);
            }
            "list" => {
                extra.push("repeat");
                let mut n = self.blank(Kind::List { repeat: None }, parent, path, item);
                n.layout = Layout::Column;
                n.align = Align::Stretch;
                n.clip = true;
                self.common(&mut o, &mut n)?;
                if !matches!(n.layout, Layout::Column) {
                    return Err(bad(&o.field_path("layout"), "lists are columns"));
                }
                n.clip = true;
                let i = self.push(n, path)?;
                self.navs(&mut o, i)?;
                self.children(&mut o, i, depth, item)?;
                if let Some(r) = o.get("repeat") {
                    if item.is_some() {
                        return Err(bad(&o.field_path("repeat"), "a repeat can't be nested inside another repeat"));
                    }
                    let rp = o.field_path("repeat");
                    let mut ro = Obj::new(r, &rp)?;
                    let src = ro.req_str("source")?;
                    let source = match Key::parse(src) {
                        Some(k @ Key::Global(_)) => k,
                        _ => return Err(bad(&ro.field_path("source"), format!("\"{src}\" is not a state key"))),
                    };
                    let max = ro.num("max")?.unwrap_or(16.0);
                    if !(1.0..=MAX_REPEAT as f32).contains(&max) || max.fract() != 0.0 {
                        return Err(bad(&ro.field_path("max"), format!("max must be a whole number 1..={MAX_REPEAT}")));
                    }
                    let template = ro.req("template")?;
                    let tp = ro.field_path("template");
                    ro.finish(&["source", "max", "template"])?;
                    for row in 0..max as usize {
                        let start = self.nodes.len();
                        self.node(template, &tp, Some(i), depth + 1, Some((i, row)))?;
                        // Ids inside instances get the row: "bind" -> "bind[3]".
                        for n in &mut self.nodes[start..] {
                            if let Some(id) = &mut n.id {
                                id.push_str(&format!("[{row}]"));
                            }
                        }
                    }
                    self.nodes[i as usize].kind = Kind::List { repeat: Some(Repeat { source, max: max as usize }) };
                }
                self.close(i);
            }
            "radial" => {
                extra.extend(["radius", "inner_radius", "items"]);
                let radius = o.num("radius")?.unwrap_or(120.0);
                let inner = o.num("inner_radius")?.unwrap_or(radius * 0.4);
                if !(radius > 0.0 && inner >= 0.0 && inner < radius) {
                    return Err(bad(&o.field_path("radius"), "need radius > inner_radius ≥ 0"));
                }
                let mut n = self.blank(Kind::Radial { radius, inner }, parent, path, item);
                n.layout = Layout::Radial;
                n.width = Size::Fixed(Len::Px(radius * 2.0));
                n.height = Size::Fixed(Len::Px(radius * 2.0));
                self.common(&mut o, &mut n)?;
                n.layout = Layout::Radial;
                let i = self.push(n, path)?;
                let items_j = o.req("items")?;
                let ip = o.field_path("items");
                let items = items_j.as_array().ok_or_else(|| wrong(&ip, "an array of items", items_j))?;
                if !(2..=12).contains(&items.len()) {
                    return Err(bad(&ip, format!("a radial menu needs 2..=12 items, found {}", items.len())));
                }
                let label_role = theme.role_index("small").or_else(|| theme.role_index("body")).unwrap_or(0);
                for (k, it) in items.iter().enumerate() {
                    let p = format!("{ip}[{k}]");
                    let mut io = Obj::new(it, &p)?;
                    let id = io.req_str("id")?;
                    let label = Template::parse(io.req_str("label")?, &io.field_path("label"))?;
                    let detail = match io.get("detail") {
                        Some(d) => self.template(d, &io.field_path("detail"))?,
                        None => label.clone(),
                    };
                    let action = io.str("action")?.map(Rc::from);
                    let enabled = match io.get("enabled") {
                        Some(c) => self.cond(c, &io.field_path("enabled"))?,
                        None => Cond::Always,
                    };
                    let visible = match io.get("visible") {
                        Some(c) => self.cond(c, &io.field_path("visible"))?,
                        None => Cond::Always,
                    };
                    let tooltip = match io.get("tooltip") {
                        Some(t) => Some(self.template(t, &io.field_path("tooltip"))?),
                        None => None,
                    };
                    let icon = match io.get("icon") {
                        Some(s) => Some(shape(s, &io.field_path("icon"))?),
                        None => None,
                    };
                    io.finish(&["id", "label", "detail", "action", "enabled", "visible", "icon", "tooltip"])?;
                    if !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') || id.is_empty() {
                        return Err(bad(&format!("{p}.id"), format!("id \"{id}\" must be letters, digits, _ or -")));
                    }
                    let mut item_node = self.blank(Kind::RadialItem { action, detail, label: label.clone() }, Some(i), &p, item);
                    item_node.id = Some(id.to_owned());
                    item_node.enabled = enabled;
                    item_node.visible = visible;
                    item_node.tooltip = tooltip;
                    item_node.layout = Layout::Column;
                    item_node.align = Align::Center;
                    item_node.gap = Len::Px(2.0);
                    let ii = self.push(item_node, &p)?;
                    if let Some(s) = icon {
                        let mut ic = self.blank(Kind::Icon { icon: IconSource::Shape(s), color: None, rotation: 0.0 }, Some(ii), &format!("{p}.icon"), item);
                        ic.width = Size::Fixed(Len::Em(1.9));
                        ic.height = Size::Fixed(Len::Em(1.9));
                        let k = self.push(ic, &p)?;
                        self.close(k);
                    }
                    let lab = self.blank(
                        Kind::Label { text: TextSource::Template(label), role: label_role, color: None, align: TextAlign::Center, wrap: false },
                        Some(ii),
                        &format!("{p}.label"),
                        item,
                    );
                    let k = self.push(lab, &p)?;
                    self.close(k);
                    self.close(ii);
                }
                // The centre shows the highlighted item's detail.
                let mut centre = self.blank(
                    Kind::Label { text: TextSource::RadialDetail(i), role: self.theme.role_index("body").unwrap_or(0), color: None, align: TextAlign::Center, wrap: true },
                    Some(i),
                    &format!("{path}.centre"),
                    item,
                );
                centre.width = Size::Fixed(Len::Px(inner * 1.7));
                let k = self.push(centre, path)?;
                self.close(k);
                self.close(i);
            }
            "threats" => {
                extra.push("source");
                let src = o.str("source")?.unwrap_or("threats");
                let source = match Key::parse(src) {
                    Some(k @ Key::Global(_)) => k,
                    _ => return Err(bad(&o.field_path("source"), format!("\"{src}\" is not a state key"))),
                };
                let mut n = self.blank(Kind::Threats { source }, parent, path, item);
                n.width = Size::Fill(1.0);
                n.height = Size::Fill(1.0);
                n.overlap_ok = true;
                self.common(&mut o, &mut n)?;
                let i = self.push(n, path)?;
                self.close(i);
            }
            "toasts" => {
                extra.push("max");
                let max = o.num("max")?.unwrap_or(4.0);
                if !(1.0..=16.0).contains(&max) {
                    return Err(bad(&o.field_path("max"), "max must be 1..=16"));
                }
                let mut n = self.blank(Kind::Toasts { max: max as u16 }, parent, path, item);
                n.layout = Layout::Column;
                n.gap = Len::Px(6.0);
                n.align = Align::Center;
                n.width = Size::Fixed(Len::Em(24.0));
                n.overlap_ok = true;
                self.common(&mut o, &mut n)?;
                let i = self.push(n, path)?;
                self.close(i);
            }
            "prompt" => {
                extra.extend(["action", "text", "role"]);
                let action = o.req_str("action")?.to_owned();
                let role = self.role(&mut o, "small")?;
                let mut n = self.blank(Kind::Prompt { action: action.clone() }, parent, path, item);
                n.layout = Layout::Row;
                n.align = Align::Center;
                n.gap = Len::Px(6.0);
                self.common(&mut o, &mut n)?;
                let i = self.push(n, path)?;
                self.navs(&mut o, i)?;
                let kc = theme.keycap.clone();
                let mut cap = self.blank(Kind::Panel { boxed: kc.boxed }, Some(i), &format!("{path}.keycap"), item);
                cap.padding = theme.boxed(kc.boxed).padding.map(Len::Px);
                cap.min[0] = Some(Len::Px(kc.min_width));
                cap.layout = Layout::Row;
                cap.justify = Justify::Center;
                cap.align = Align::Center;
                let c = self.push(cap, path)?;
                let glyph = self.blank(
                    Kind::Label { text: TextSource::Prompt(action), role: kc.role, color: None, align: TextAlign::Center, wrap: false },
                    Some(c),
                    &format!("{path}.glyph"),
                    item,
                );
                let g = self.push(glyph, path)?;
                self.close(g);
                self.close(c);
                if let Some(t) = o.get("text") {
                    let text = self.template(t, &o.field_path("text"))?;
                    let label = self.blank(
                        Kind::Label { text: TextSource::Template(text), role, color: None, align: TextAlign::Left, wrap: false },
                        Some(i),
                        &o.field_path("text"),
                        item,
                    );
                    let k = self.push(label, path)?;
                    self.close(k);
                }
                self.close(i);
            }
            other => {
                return Err(bad(
                    &o.field_path("type"),
                    format!("unknown widget type \"{other}\"{}; types: {}", crate::error::suggest(other, TYPES.iter().copied()), TYPES.join(", ")),
                ))
            }
        }
        let mut allowed: Vec<&str> = COMMON.to_vec();
        allowed.extend(extra);
        o.finish(&allowed)?;
        Ok(index)
    }
}

fn shape(j: &Json, path: &str) -> Result<Shape, UiError> {
    let s = j.as_str().ok_or_else(|| wrong(path, "a shape name", j))?;
    SHAPES.iter().find(|(n, _)| *n == s).map(|(_, v)| *v).ok_or_else(|| {
        let names: Vec<&str> = SHAPES.iter().map(|(n, _)| *n).collect();
        bad(path, format!("unknown shape \"{s}\"{}; shapes: {}", crate::error::suggest(s, names.iter().copied()), names.join(", ")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::tests::MINI;

    fn theme() -> Theme {
        Theme::from_json_str(MINI).unwrap()
    }

    fn screen(root: &str) -> Result<Screen, UiError> {
        Screen::parse(&format!(r#"{{"format": "specula.ui.screen.v1", "name": "t", "root": {root}}}"#), &theme())
    }

    #[test]
    fn parses_a_tree_in_preorder() {
        let s = screen(
            r#"{"type": "box", "children": [
                {"type": "panel", "style": "stone", "layout": "row", "children": [{"type": "label", "text": "a"}, {"type": "label", "text": "b"}]},
                {"type": "button", "id": "go", "text": "Go", "action": "go"}
            ]}"#,
        )
        .unwrap();
        // box, panel, label, label, button, button's label
        assert_eq!(s.nodes.len(), 6);
        assert_eq!(s.nodes[0].end, 6);
        assert_eq!(s.children(0).collect::<Vec<_>>(), vec![1, 4]);
        assert_eq!(s.children(1).collect::<Vec<_>>(), vec![2, 3]);
        assert_eq!(s.nodes[5].parent, Some(4));
        assert_eq!(s.index_of("go"), Some(4));
        // Panels take their box's padding.
        assert_eq!(s.nodes[1].padding[1], Len::Px(10.0));
    }

    #[test]
    fn units_parse() {
        let s = screen(r#"{"type": "box", "width": "50%", "height": "2em", "min_width": "12px", "max_width": 300, "padding": [4, "1em"]}"#).unwrap();
        let n = &s.nodes[0];
        assert_eq!(n.width, Size::Fixed(Len::Percent(50.0)));
        assert_eq!(n.height, Size::Fixed(Len::Em(2.0)));
        assert_eq!(n.min[0], Some(Len::Px(12.0)));
        assert_eq!(n.max[0], Some(Len::Px(300.0)));
        assert_eq!(n.padding, [Len::Px(4.0), Len::Em(1.0), Len::Px(4.0), Len::Em(1.0)]);
        let s = screen(r#"{"type": "box", "width": "fill:2", "height": "fill"}"#).unwrap();
        assert_eq!((s.nodes[0].width, s.nodes[0].height), (Size::Fill(2.0), Size::Fill(1.0)));
    }

    #[test]
    fn validation_errors_carry_paths() {
        let cases: &[(&str, ErrorCode, &str)] = &[
            (r#"{"type": "lable", "text": "x"}"#, ErrorCode::BadValue, "root.type"),
            (r#"{"type": "box", "children": [{"type": "label"}]}"#, ErrorCode::MissingField, "root.children[0].text"),
            (r#"{"type": "box", "children": [{"type": "label", "text": "{dust"}]}"#, ErrorCode::BadTemplate, "root.children[0].text"),
            (r#"{"type": "box", "widht": 3}"#, ErrorCode::UnknownField, "root.widht"),
            (r#"{"type": "box", "width": "wide"}"#, ErrorCode::WrongType, "root.width"),
            (r#"{"type": "box", "anchor": "topleft"}"#, ErrorCode::BadValue, "root.anchor"),
            (r#"{"type": "box", "visible": "dust >"}"#, ErrorCode::BadCondition, "root.visible"),
            (r#"{"type": "label", "text": "x", "role": "huge"}"#, ErrorCode::UnknownName, "root.role"),
            (r#"{"type": "label", "text": "x", "color": "pink"}"#, ErrorCode::UnknownName, "root.color"),
            (r#"{"type": "panel", "style": "glass"}"#, ErrorCode::UnknownName, "root.style"),
            (r#"{"type": "button", "text": "x"}"#, ErrorCode::MissingField, "root.id"),
            (r#"{"type": "box", "children": [{"type": "button", "id": "a"}, {"type": "button", "id": "a"}]}"#, ErrorCode::DuplicateId, "root.children[1]"),
            (r#"{"type": "box", "nav": {"up": "nowhere"}}"#, ErrorCode::BadValue, "root.nav.up"),
            (r#"{"type": "icon", "shape": "circel"}"#, ErrorCode::BadValue, "root.shape"),
            (r#"{"type": "box", "columns": 3}"#, ErrorCode::BadValue, "root.columns"),
            (r#"{"type": "radial", "items": [{"id": "a", "label": "A"}]}"#, ErrorCode::BadValue, "root.items"),
        ];
        for (root, code, path) in cases {
            let e = screen(root).unwrap_err();
            assert_eq!((e.code, e.path.as_str()), (*code, *path), "{root}: {e}");
        }
        let e = screen(r#"{"type": "icon", "shape": "circel"}"#).unwrap_err();
        assert!(e.message.contains("did you mean \"circle\""), "{e}");
        let e = Screen::parse("{\"format\": \"specula.ui.screen.v1\", \"name\": \"t\", \"root\": {\"type\": \"box\"},}", &theme()).unwrap_err();
        assert_eq!(e.code, ErrorCode::JsonSyntax);
        assert!(e.offset.is_some());
    }

    #[test]
    fn repeat_instantiates_rows_with_suffixed_ids() {
        let s = screen(
            r#"{"type": "list", "repeat": {"source": "rows", "max": 3, "template":
                {"type": "button", "id": "row", "text": "{item.label}", "action": "pick"}}}"#,
        )
        .unwrap();
        assert!(matches!(&s.nodes[0].kind, Kind::List { repeat: Some(Repeat { max: 3, .. }) }));
        let ids: Vec<&str> = s.nodes.iter().filter_map(|n| n.id.as_deref()).collect();
        assert_eq!(ids, vec!["row[0]", "row[1]", "row[2]"]);
        let rows: Vec<Option<(u32, usize)>> = s.nodes.iter().map(|n| n.item).collect();
        assert_eq!(rows, vec![None, Some((0, 0)), Some((0, 0)), Some((0, 1)), Some((0, 1)), Some((0, 2)), Some((0, 2))]);
    }

    #[test]
    fn radial_expands_items_and_centre() {
        let s = screen(
            r#"{"type": "radial", "id": "build", "radius": 100, "items": [
                {"id": "a", "label": "A", "icon": "warding", "action": "build_a"},
                {"id": "b", "label": "B", "detail": "B costs {cost}", "enabled": "dust >= 60"}]}"#,
        )
        .unwrap();
        let kids: Vec<&str> = s.children(0).map(|c| s.nodes[c as usize].kind.name()).collect();
        assert_eq!(kids, vec!["radial_item", "radial_item", "label"]);
        assert_eq!(s.nodes[0].width, Size::Fixed(Len::Px(200.0)));
    }
}
