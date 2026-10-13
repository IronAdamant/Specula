//! Actions as data: [`ActionMap`] holds named actions and their bindings, loads and saves them as
//! JSON, detects binding conflicts and produces prompt glyphs.

use specula_core::hash::StableHasher;
use specula_core::Json;

use crate::error::{ErrorCode, InputError};
use crate::source::{DeviceFamily, PadStyle, Signal, Source};

/// The `"format"` tag of a full action map file.
pub const MAP_FORMAT: &str = "specula.input.v1";
/// The `"format"` tag of a bindings-only file (a player's rebinds).
pub const BINDINGS_FORMAT: &str = "specula.bindings.v1";

/// What an action reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionKind {
    /// On/off with `pressed` / `released` edges (`place`, `pause`).
    Button,
    /// One number (`zoom`).
    Axis,
    /// Two numbers (`orbit`, `cursor_move`).
    Axis2,
}

impl ActionKind {
    pub fn name(self) -> &'static str {
        match self {
            ActionKind::Button => "button",
            ActionKind::Axis => "axis",
            ActionKind::Axis2 => "axis2",
        }
    }

    pub fn from_name(name: &str) -> Option<ActionKind> {
        [ActionKind::Button, ActionKind::Axis, ActionKind::Axis2].into_iter().find(|k| k.name() == name)
    }
}

/// How an axis action combines its bindings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AxisOutput {
    /// The amount to apply **this frame**: delta sources (mouse motion, wheel, touch drag) are
    /// summed as they are; level sources (sticks, held keys, triggers) are multiplied by the
    /// frame time first, so their `scale` is "per second". Camera and cursor movement use this.
    #[default]
    Delta,
    /// The current level: level sources as they are, plus this frame's delta sources. For
    /// things like a throttle.
    Value,
}

impl AxisOutput {
    pub fn name(self) -> &'static str {
        match self {
            AxisOutput::Delta => "delta",
            AxisOutput::Value => "value",
        }
    }
}

/// One part of a composite binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompositePart {
    Up,
    Down,
    Left,
    Right,
    Negative,
    Positive,
}

impl CompositePart {
    pub fn name(self) -> &'static str {
        match self {
            CompositePart::Up => "up",
            CompositePart::Down => "down",
            CompositePart::Left => "left",
            CompositePart::Right => "right",
            CompositePart::Negative => "negative",
            CompositePart::Positive => "positive",
        }
    }
}

/// What a binding listens to: one source, or buttons combined into an axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BindingInput {
    Source(Source),
    /// 1D axis from two buttons: `positive - negative`.
    Composite1 { negative: Source, positive: Source },
    /// 2D axis from four buttons: `x = right - left`, `y = up - down` (+y up), length clamped to 1.
    Composite2 { up: Source, down: Source, left: Source, right: Source },
}

impl BindingInput {
    pub fn family(&self) -> DeviceFamily {
        match *self {
            BindingInput::Source(s) => s.family(),
            BindingInput::Composite1 { positive, .. } => positive.family(),
            BindingInput::Composite2 { up, .. } => up.family(),
        }
    }

    pub fn signal(&self) -> Signal {
        match self {
            BindingInput::Source(s) => s.signal(),
            BindingInput::Composite1 { .. } => Signal::Axis,
            BindingInput::Composite2 { .. } => Signal::Axis2,
        }
    }

    fn part_mut(&mut self, part: CompositePart) -> Option<&mut Source> {
        match (self, part) {
            (BindingInput::Composite1 { negative, .. }, CompositePart::Negative) => Some(negative),
            (BindingInput::Composite1 { positive, .. }, CompositePart::Positive) => Some(positive),
            (BindingInput::Composite2 { up, .. }, CompositePart::Up) => Some(up),
            (BindingInput::Composite2 { down, .. }, CompositePart::Down) => Some(down),
            (BindingInput::Composite2 { left, .. }, CompositePart::Left) => Some(left),
            (BindingInput::Composite2 { right, .. }, CompositePart::Right) => Some(right),
            _ => None,
        }
    }

    /// Replace one part of a composite. `false` if the input has no such part.
    pub fn set_part(&mut self, part: CompositePart, source: Source) -> bool {
        match self.part_mut(part) {
            Some(slot) => {
                *slot = source;
                true
            }
            None => false,
        }
    }
}

/// One way to trigger an action.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub input: BindingInput,
    /// Buttons that must be held for this binding to count (a chord: `Ctrl+Z`, right-drag).
    /// While a binding with more modifiers on the same input is satisfied, this one is blocked,
    /// so `Ctrl+Z` doesn't also fire a plain `Z` action.
    pub modifiers: Vec<Source>,
    /// Multiplier per axis (`[s, s]` for one number). Negative flips direction.
    pub scale: [f32; 2],
    pub invert_x: bool,
    pub invert_y: bool,
    /// Analog sources: a dead zone replacing the platform default (radial for sticks).
    pub dead_zone: Option<f32>,
    /// Analog source on a button action: the level that counts as pressed (default:
    /// [`Settings::press_threshold`]).
    pub threshold: Option<f32>,
}

impl Binding {
    pub fn new(input: BindingInput) -> Binding {
        Binding {
            input,
            modifiers: Vec::new(),
            scale: [1.0, 1.0],
            invert_x: false,
            invert_y: false,
            dead_zone: None,
            threshold: None,
        }
    }

    pub fn source(source: Source) -> Binding {
        Binding::new(BindingInput::Source(source))
    }

    pub fn with_modifiers(mut self, modifiers: &[Source]) -> Binding {
        self.modifiers = modifiers.to_vec();
        self
    }

    pub fn with_scale(mut self, x: f32, y: f32) -> Binding {
        self.scale = [x, y];
        self
    }

    /// The effective per-axis multiplier (scale with inversion applied).
    pub fn gain(&self) -> [f32; 2] {
        [
            if self.invert_x { -self.scale[0] } else { self.scale[0] },
            if self.invert_y { -self.scale[1] } else { self.scale[1] },
        ]
    }

    /// Same input and the same modifier set (order-insensitive).
    pub fn same_trigger(&self, other: &Binding) -> bool {
        self.input == other.input
            && self.modifiers.len() == other.modifiers.len()
            && self.modifiers.iter().all(|m| other.modifiers.contains(m))
    }

    pub fn family(&self) -> DeviceFamily {
        self.input.family()
    }

    /// Prompt text, e.g. `"Ctrl+Z"`, `"RMB+Mouse"`, `"W/A/S/D"`, `"RB"`.
    pub fn glyph(&self, style: PadStyle) -> String {
        let main = match self.input {
            BindingInput::Source(s) => s.glyph(style),
            BindingInput::Composite1 { negative, positive } => {
                format!("{}/{}", negative.glyph(style), positive.glyph(style))
            }
            BindingInput::Composite2 { up, down, left, right } => {
                use specula_platform::GamepadButton as B;
                if [up, down, left, right] == [B::DpadUp, B::DpadDown, B::DpadLeft, B::DpadRight].map(Source::Pad) {
                    Source::Dpad.glyph(style)
                } else {
                    [up, left, down, right].map(|s| s.glyph(style)).join("/")
                }
            }
        };
        let mut parts: Vec<String> = self.modifiers.iter().map(|m| m.glyph(style)).collect();
        parts.push(main);
        parts.join("+")
    }
}

/// A named action and its bindings.
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub name: String,
    pub kind: ActionKind,
    /// Axis actions only.
    pub output: AxisOutput,
    /// Conflicts are only reported between actions of the same group (menu vs play).
    pub group: String,
    pub description: String,
    /// Stays live while text input mode is on (e.g. `cancel`, `submit`).
    pub text_mode: bool,
    pub bindings: Vec<Binding>,
}

impl Action {
    pub fn new(name: &str, kind: ActionKind) -> Action {
        Action {
            name: name.into(),
            kind,
            output: AxisOutput::Delta,
            group: DEFAULT_GROUP.into(),
            description: String::new(),
            text_mode: false,
            bindings: Vec::new(),
        }
    }

    pub fn bind(mut self, binding: Binding) -> Action {
        self.bindings.push(binding);
        self
    }
}

pub const DEFAULT_GROUP: &str = "default";

/// Tuning shared by all actions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Analog level that counts as a press on button actions (triggers, sticks).
    pub press_threshold: f32,
    /// Longest touch that still counts as a tap, seconds.
    pub tap_max_s: f32,
    /// How far (pixels) a contact may move and still be a tap; beyond it, it's a drag.
    pub tap_slop_px: f32,
    /// How long a still contact must be held for a long press, seconds.
    pub long_press_s: f32,
    /// Frame time cap for level sources, seconds (a hitch must not fling the camera).
    pub max_dt_s: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { press_threshold: 0.5, tap_max_s: 0.3, tap_slop_px: 12.0, long_press_s: 0.5, max_dt_s: 0.1 }
    }
}

/// Another binding a candidate collides with.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    pub action: String,
    pub binding: usize,
}

/// Where a new binding goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingSlot {
    Append,
    Replace(usize),
}

/// All actions of a game (or a context) with their bindings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActionMap {
    pub settings: Settings,
    actions: Vec<Action>,
}

impl ActionMap {
    pub fn new() -> ActionMap {
        ActionMap::default()
    }

    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    pub fn action(&self, name: &str) -> Option<&Action> {
        self.actions.iter().find(|a| a.name == name)
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.actions.iter().position(|a| a.name == name)
    }

    /// Adds a validated action; returns its index.
    pub fn add_action(&mut self, action: Action) -> Result<usize, InputError> {
        let path = format!("actions[{}]", self.actions.len());
        validate_action(&action, &path)?;
        if self.index_of(&action.name).is_some() {
            return Err(InputError::new(
                ErrorCode::DuplicateAction,
                format!("{path}.name"),
                format!("action {:?} is defined twice", action.name),
            ));
        }
        self.actions.push(action);
        Ok(self.actions.len() - 1)
    }

    fn action_index(&self, name: &str) -> Result<usize, InputError> {
        self.index_of(name)
            .ok_or_else(|| InputError::new(ErrorCode::UnknownAction, "", format!("no action named {name:?}")))
    }

    /// Adds or replaces a binding after checking it suits the action. Returns the binding index.
    pub fn set_binding(&mut self, action: &str, slot: BindingSlot, binding: Binding) -> Result<usize, InputError> {
        let i = self.action_index(action)?;
        let a = &self.actions[i];
        let index = match slot {
            BindingSlot::Append => a.bindings.len(),
            BindingSlot::Replace(b) if b < a.bindings.len() => b,
            BindingSlot::Replace(b) => {
                return Err(InputError::new(
                    ErrorCode::UnknownBinding,
                    format!("actions[{i}].bindings[{b}]"),
                    format!("action {action:?} has {} bindings", a.bindings.len()),
                ))
            }
        };
        validate_binding(a.kind, &binding, &format!("actions[{i}].bindings[{index}]"))?;
        let bindings = &mut self.actions[i].bindings;
        if index == bindings.len() {
            bindings.push(binding);
        } else {
            bindings[index] = binding;
        }
        Ok(index)
    }

    pub fn remove_binding(&mut self, action: &str, index: usize) -> Result<Binding, InputError> {
        let i = self.action_index(action)?;
        let bindings = &mut self.actions[i].bindings;
        if index >= bindings.len() {
            return Err(InputError::new(
                ErrorCode::UnknownBinding,
                format!("actions[{i}].bindings[{index}]"),
                format!("action {action:?} has {} bindings", bindings.len()),
            ));
        }
        Ok(bindings.remove(index))
    }

    /// Bindings in `action`'s group that `binding` would collide with (same input and modifier
    /// set), skipping `action`'s own binding `ignore` (the one being replaced).
    pub fn conflicts_with(&self, action: &str, binding: &Binding, ignore: Option<usize>) -> Vec<Conflict> {
        let Some(i) = self.index_of(action) else { return Vec::new() };
        let group = &self.actions[i].group;
        let mut out = Vec::new();
        for (ai, a) in self.actions.iter().enumerate() {
            if a.group != *group {
                continue;
            }
            for (bi, b) in a.bindings.iter().enumerate() {
                if (ai, Some(bi)) == (i, ignore) {
                    continue;
                }
                if b.same_trigger(binding) {
                    out.push(Conflict { action: a.name.clone(), binding: bi });
                }
            }
        }
        out
    }

    /// Every colliding pair in the map, in a stable order.
    pub fn conflicts(&self) -> Vec<(Conflict, Conflict)> {
        let all: Vec<(usize, usize, &Binding)> = self
            .actions
            .iter()
            .enumerate()
            .flat_map(|(ai, a)| a.bindings.iter().enumerate().map(move |(bi, b)| (ai, bi, b)))
            .collect();
        let mut out = Vec::new();
        for (n, &(ai, bi, b)) in all.iter().enumerate() {
            for &(aj, bj, c) in &all[n + 1..] {
                if self.actions[ai].group == self.actions[aj].group && b.same_trigger(c) {
                    out.push((
                        Conflict { action: self.actions[ai].name.clone(), binding: bi },
                        Conflict { action: self.actions[aj].name.clone(), binding: bj },
                    ));
                }
            }
        }
        out
    }

    /// Prompt glyphs for `action`'s bindings of one device family, in binding order.
    pub fn glyphs(&self, action: &str, family: DeviceFamily, style: PadStyle) -> Vec<String> {
        self.action(action)
            .map(|a| a.bindings.iter().filter(|b| b.family() == family).map(|b| b.glyph(style)).collect())
            .unwrap_or_default()
    }

    /// Stable hash of the action names and kinds, in order: recordings carry it so a replay can
    /// tell it was made for this set of actions.
    pub fn schema_hash(&self) -> u64 {
        schema_hash(self.actions.iter().map(|a| (a.name.as_str(), a.kind)))
    }

    // ---- JSON ----

    /// Parses a full map (`{"format": "specula.input.v1", "settings": {..}, "actions": [..]}`).
    pub fn from_json_str(text: &str) -> Result<ActionMap, InputError> {
        let json = Json::parse(text)
            .map_err(|e| InputError::new(ErrorCode::JsonSyntax, "", e.message.clone()).at(e.offset))?;
        ActionMap::from_json(&json)
    }

    pub fn from_json(json: &Json) -> Result<ActionMap, InputError> {
        let obj = Obj::new(json, "")?;
        obj.only(&["format", "settings", "actions"])?;
        check_format(&obj, MAP_FORMAT)?;
        let mut map = ActionMap::new();
        if let Some(s) = obj.opt("settings") {
            map.settings = parse_settings(s, "settings")?;
        }
        let actions = obj.req("actions")?;
        let list = actions.as_array().ok_or_else(|| wrong_type("actions", "an array"))?;
        for (i, a) in list.iter().enumerate() {
            map.add_action(parse_action(a, &format!("actions[{i}]"))?)?;
        }
        Ok(map)
    }

    pub fn to_json(&self) -> Json {
        let s = &self.settings;
        let settings = Json::obj([
            ("press_threshold", num(s.press_threshold)),
            ("tap_max_s", num(s.tap_max_s)),
            ("tap_slop_px", num(s.tap_slop_px)),
            ("long_press_s", num(s.long_press_s)),
            ("max_dt_s", num(s.max_dt_s)),
        ]);
        let actions = self.actions.iter().map(|a| {
            let mut o = Json::obj([("name", Json::from(a.name.as_str())), ("kind", Json::from(a.kind.name()))]);
            if a.kind != ActionKind::Button {
                o.set("output", Json::from(a.output.name()));
            }
            if a.group != DEFAULT_GROUP {
                o.set("group", Json::from(a.group.as_str()));
            }
            if !a.description.is_empty() {
                o.set("description", Json::from(a.description.as_str()));
            }
            if a.text_mode {
                o.set("text_mode", Json::Bool(true));
            }
            o.set("bindings", Json::arr(a.bindings.iter().map(binding_to_json)));
            o
        });
        Json::obj([
            ("format", Json::from(MAP_FORMAT)),
            ("settings", settings),
            ("actions", Json::arr(actions)),
        ])
    }

    /// Only the bindings (a player's rebinds), keyed by action name.
    pub fn bindings_json(&self) -> Json {
        Json::obj([
            ("format", Json::from(BINDINGS_FORMAT)),
            (
                "bindings",
                Json::obj(
                    self.actions
                        .iter()
                        .map(|a| (a.name.clone(), Json::arr(a.bindings.iter().map(binding_to_json)))),
                ),
            ),
        ])
    }

    /// Applies a bindings file made by [`ActionMap::bindings_json`]. All-or-nothing: on error the
    /// map is unchanged. Actions missing from the file keep their bindings; unknown actions are
    /// an error (`unknown_action`).
    pub fn load_bindings_json(&mut self, json: &Json) -> Result<(), InputError> {
        let obj = Obj::new(json, "")?;
        obj.only(&["format", "bindings"])?;
        check_format(&obj, BINDINGS_FORMAT)?;
        let entries = obj.req("bindings")?.as_object().ok_or_else(|| wrong_type("bindings", "an object"))?;
        let mut staged = Vec::new();
        for (name, list) in entries {
            let path = format!("bindings.{name}");
            let i = self.index_of(name).ok_or_else(|| {
                InputError::new(ErrorCode::UnknownAction, path.clone(), format!("no action named {name:?}"))
            })?;
            let items = list.as_array().ok_or_else(|| wrong_type(&path, "an array"))?;
            let mut bindings = Vec::new();
            for (j, b) in items.iter().enumerate() {
                let bpath = format!("{path}[{j}]");
                let binding = parse_binding(b, &bpath)?;
                validate_binding(self.actions[i].kind, &binding, &bpath)?;
                bindings.push(binding);
            }
            staged.push((i, bindings));
        }
        for (i, bindings) in staged {
            self.actions[i].bindings = bindings;
        }
        Ok(())
    }

    pub fn load_bindings_str(&mut self, text: &str) -> Result<(), InputError> {
        let json = Json::parse(text)
            .map_err(|e| InputError::new(ErrorCode::JsonSyntax, "", e.message.clone()).at(e.offset))?;
        self.load_bindings_json(&json)
    }
}

pub(crate) fn schema_hash<'a>(actions: impl Iterator<Item = (&'a str, ActionKind)>) -> u64 {
    let mut h = StableHasher::with_seed(0x5350_4952); // "SPIR"
    for (name, kind) in actions {
        h.write_str(name);
        h.write_u8(kind as u8);
    }
    h.finish()
}

// ---- validation ----

fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn validate_action(a: &Action, path: &str) -> Result<(), InputError> {
    if !valid_name(&a.name) {
        return Err(InputError::new(
            ErrorCode::BadValue,
            format!("{path}.name"),
            format!("action name {:?} must be 1-64 characters of A-Z a-z 0-9 _ -", a.name),
        ));
    }
    if !valid_name(&a.group) {
        return Err(InputError::new(
            ErrorCode::BadValue,
            format!("{path}.group"),
            format!("group {:?} must be 1-64 characters of A-Z a-z 0-9 _ -", a.group),
        ));
    }
    for (i, b) in a.bindings.iter().enumerate() {
        validate_binding(a.kind, b, &format!("{path}.bindings[{i}]"))?;
    }
    Ok(())
}

/// Checks a binding can drive an action of `kind`.
pub fn validate_binding(kind: ActionKind, b: &Binding, path: &str) -> Result<(), InputError> {
    let incompatible = |why: String| Err(InputError::new(ErrorCode::IncompatibleSource, path, why));
    for m in &b.modifiers {
        if !m.is_holdable_button() {
            return incompatible(format!("modifier {} must be a key, mouse button or pad button", m.name()));
        }
    }
    match b.input {
        BindingInput::Source(s) => match (kind, s.signal()) {
            (ActionKind::Button, Signal::Button) => {}
            (ActionKind::Button, _) if !s.is_delta() => {} // analog level past a threshold
            (ActionKind::Button, _) => {
                return incompatible(format!(
                    "{} reports motion, not a press; use Mouse.WheelUp/WheelDown or a gesture",
                    s.name()
                ))
            }
            (ActionKind::Axis, Signal::Button | Signal::Axis) => {}
            (ActionKind::Axis2, Signal::Axis2) => {}
            (ActionKind::Axis2, Signal::Button) => {
                return incompatible(format!(
                    "{} is a button; a 2D axis needs a composite {{\"up\",\"down\",\"left\",\"right\"}}",
                    s.name()
                ))
            }
            (kind, signal) => {
                return incompatible(format!("{} ({signal:?}) can't drive a {} action", s.name(), kind.name()))
            }
        },
        BindingInput::Composite1 { negative, positive } => {
            if kind != ActionKind::Axis {
                return incompatible(format!("a negative/positive composite drives axis actions, not {}", kind.name()));
            }
            for s in [negative, positive] {
                if s.signal() != Signal::Button {
                    return incompatible(format!("composite part {} must be a button", s.name()));
                }
            }
        }
        BindingInput::Composite2 { up, down, left, right } => {
            if kind != ActionKind::Axis2 {
                return incompatible(format!("an up/down/left/right composite drives axis2 actions, not {}", kind.name()));
            }
            for s in [up, down, left, right] {
                if s.signal() != Signal::Button {
                    return incompatible(format!("composite part {} must be a button", s.name()));
                }
            }
        }
    }
    let bad = |field: &str, why: &str| Err(InputError::new(ErrorCode::BadValue, format!("{path}.{field}"), why));
    if !b.scale.iter().all(|s| s.is_finite()) {
        return bad("scale", "scale must be finite");
    }
    if let Some(dz) = b.dead_zone {
        if !(0.0..1.0).contains(&dz) {
            return bad("dead_zone", "dead_zone must be in 0..1");
        }
    }
    if let Some(t) = b.threshold {
        if !(t > 0.0 && t <= 1.0) {
            return bad("threshold", "threshold must be in (0, 1]");
        }
    }
    Ok(())
}

// ---- JSON helpers ----

fn wrong_type(path: &str, expected: &str) -> InputError {
    InputError::new(ErrorCode::WrongType, path, format!("expected {expected}"))
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

/// An object being read, with its path for errors.
struct Obj<'a> {
    pairs: &'a [(String, Json)],
    path: String,
}

impl<'a> Obj<'a> {
    fn new(json: &'a Json, path: &str) -> Result<Obj<'a>, InputError> {
        let pairs = json.as_object().ok_or_else(|| wrong_type(if path.is_empty() { "$" } else { path }, "an object"))?;
        Ok(Obj { pairs, path: path.to_owned() })
    }

    fn only(&self, allowed: &[&str]) -> Result<(), InputError> {
        for (k, _) in self.pairs {
            if !allowed.contains(&k.as_str()) {
                return Err(InputError::new(
                    ErrorCode::BadValue,
                    join(&self.path, k),
                    format!("unknown field {k:?} (allowed: {})", allowed.join(", ")),
                ));
            }
        }
        Ok(())
    }

    fn opt(&self, key: &str) -> Option<&'a Json> {
        self.pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    fn req(&self, key: &str) -> Result<&'a Json, InputError> {
        self.opt(key).ok_or_else(|| {
            InputError::new(ErrorCode::MissingField, join(&self.path, key), format!("missing field {key:?}"))
        })
    }

    fn str(&self, key: &str) -> Result<Option<&'a str>, InputError> {
        self.opt(key).map(|v| v.as_str().ok_or_else(|| wrong_type(&join(&self.path, key), "a string"))).transpose()
    }

    fn bool(&self, key: &str) -> Result<Option<bool>, InputError> {
        self.opt(key).map(|v| v.as_bool().ok_or_else(|| wrong_type(&join(&self.path, key), "true or false"))).transpose()
    }

    fn num(&self, key: &str) -> Result<Option<f32>, InputError> {
        self.opt(key)
            .map(|v| {
                v.as_f64()
                    .filter(|n| n.is_finite())
                    .map(|n| n as f32)
                    .ok_or_else(|| wrong_type(&join(&self.path, key), "a number"))
            })
            .transpose()
    }
}

fn check_format(obj: &Obj, expected: &str) -> Result<(), InputError> {
    match obj.str("format")? {
        Some(f) if f == expected => Ok(()),
        Some(f) => Err(InputError::new(
            ErrorCode::Format,
            "format",
            format!("unsupported format {f:?} (expected {expected:?})"),
        )),
        None => Err(InputError::new(ErrorCode::Format, "format", format!("missing \"format\": {expected:?}"))),
    }
}

fn parse_settings(json: &Json, path: &str) -> Result<Settings, InputError> {
    let obj = Obj::new(json, path)?;
    obj.only(&["press_threshold", "tap_max_s", "tap_slop_px", "long_press_s", "max_dt_s"])?;
    let mut s = Settings::default();
    let field = |key: &str, slot: &mut f32, ok: fn(f32) -> bool, why: &str| -> Result<(), InputError> {
        if let Some(v) = obj.num(key)? {
            if !ok(v) {
                return Err(InputError::new(ErrorCode::BadValue, join(path, key), why));
            }
            *slot = v;
        }
        Ok(())
    };
    field("press_threshold", &mut s.press_threshold, |v| v > 0.0 && v <= 1.0, "must be in (0, 1]")?;
    field("tap_max_s", &mut s.tap_max_s, |v| v > 0.0, "must be positive")?;
    field("tap_slop_px", &mut s.tap_slop_px, |v| v >= 0.0, "must not be negative")?;
    field("long_press_s", &mut s.long_press_s, |v| v > 0.0, "must be positive")?;
    field("max_dt_s", &mut s.max_dt_s, |v| v > 0.0, "must be positive")?;
    Ok(s)
}

fn parse_action(json: &Json, path: &str) -> Result<Action, InputError> {
    let obj = Obj::new(json, path)?;
    obj.only(&["name", "kind", "output", "group", "description", "text_mode", "bindings"])?;
    let name = obj.str("name")?.ok_or_else(|| obj.req("name").unwrap_err())?;
    let kind_name = obj.str("kind")?.ok_or_else(|| obj.req("kind").unwrap_err())?;
    let kind = ActionKind::from_name(kind_name).ok_or_else(|| {
        InputError::new(
            ErrorCode::BadValue,
            join(path, "kind"),
            format!("unknown kind {kind_name:?} (button, axis, axis2)"),
        )
    })?;
    let mut action = Action::new(name, kind);
    if let Some(o) = obj.str("output")? {
        action.output = match o {
            "delta" => AxisOutput::Delta,
            "value" => AxisOutput::Value,
            _ => {
                return Err(InputError::new(
                    ErrorCode::BadValue,
                    join(path, "output"),
                    format!("unknown output {o:?} (delta, value)"),
                ))
            }
        };
        if kind == ActionKind::Button {
            return Err(InputError::new(ErrorCode::BadValue, join(path, "output"), "button actions have no output mode"));
        }
    }
    if let Some(g) = obj.str("group")? {
        action.group = g.to_owned();
    }
    if let Some(d) = obj.str("description")? {
        action.description = d.to_owned();
    }
    action.text_mode = obj.bool("text_mode")?.unwrap_or(false);
    if let Some(list) = obj.opt("bindings") {
        let bpath = join(path, "bindings");
        let items = list.as_array().ok_or_else(|| wrong_type(&bpath, "an array"))?;
        for (i, b) in items.iter().enumerate() {
            action.bindings.push(parse_binding(b, &format!("{bpath}[{i}]"))?);
        }
    }
    Ok(action)
}

fn parse_source(text: &str, path: &str) -> Result<Source, InputError> {
    Source::parse(text).ok_or_else(|| {
        InputError::new(
            ErrorCode::UnknownSource,
            path,
            format!("unknown source {text:?} (expected Device.Name, e.g. Key.Space, Mouse.Left, Pad.South, Touch.Tap)"),
        )
    })
}

fn parse_source_json(json: &Json, path: &str) -> Result<Source, InputError> {
    parse_source(json.as_str().ok_or_else(|| wrong_type(path, "a source name string"))?, path)
}

/// `"Key.Ctrl+Key.Z"`: modifiers, then the main source.
fn parse_chord(text: &str, path: &str) -> Result<Binding, InputError> {
    let mut parts: Vec<Source> = Vec::new();
    for part in text.split('+') {
        parts.push(parse_source(part, path)?);
    }
    let main = parts.pop().expect("split yields at least one part");
    Ok(Binding::source(main).with_modifiers(&parts))
}

pub(crate) fn parse_binding(json: &Json, path: &str) -> Result<Binding, InputError> {
    if let Some(text) = json.as_str() {
        return parse_chord(text, path);
    }
    let obj = Obj::new(json, path)?;
    obj.only(&["source", "composite", "modifiers", "scale", "invert", "invert_x", "invert_y", "dead_zone", "threshold"])?;
    let input = match (obj.opt("source"), obj.opt("composite")) {
        (Some(s), None) => {
            let spath = join(path, "source");
            let b = parse_chord(s.as_str().ok_or_else(|| wrong_type(&spath, "a source name string"))?, &spath)?;
            if !b.modifiers.is_empty() && obj.opt("modifiers").is_some() {
                return Err(InputError::new(
                    ErrorCode::BadValue,
                    spath,
                    "give modifiers either in \"source\" (A+B) or in \"modifiers\", not both",
                ));
            }
            (b.input, b.modifiers)
        }
        (None, Some(c)) => {
            let cpath = join(path, "composite");
            let co = Obj::new(c, &cpath)?;
            let part = |k: &str| -> Result<Source, InputError> { parse_source_json(co.req(k)?, &join(&cpath, k)) };
            let input = if co.opt("negative").is_some() || co.opt("positive").is_some() {
                co.only(&["negative", "positive"])?;
                BindingInput::Composite1 { negative: part("negative")?, positive: part("positive")? }
            } else {
                co.only(&["up", "down", "left", "right"])?;
                BindingInput::Composite2 { up: part("up")?, down: part("down")?, left: part("left")?, right: part("right")? }
            };
            (input, Vec::new())
        }
        (Some(_), Some(_)) => {
            return Err(InputError::new(ErrorCode::BadValue, path, "a binding has \"source\" or \"composite\", not both"))
        }
        (None, None) => {
            return Err(InputError::new(ErrorCode::MissingField, join(path, "source"), "a binding needs \"source\" or \"composite\""))
        }
    };
    let mut b = Binding::new(input.0);
    b.modifiers = input.1;
    if let Some(m) = obj.opt("modifiers") {
        let mpath = join(path, "modifiers");
        let items = m.as_array().ok_or_else(|| wrong_type(&mpath, "an array of source names"))?;
        for (i, item) in items.iter().enumerate() {
            b.modifiers.push(parse_source_json(item, &format!("{mpath}[{i}]"))?);
        }
    }
    if let Some(scale) = obj.opt("scale") {
        let spath = join(path, "scale");
        b.scale = match (scale.as_f64(), scale.as_array()) {
            (Some(s), _) => [s as f32; 2],
            (None, Some([x, y])) => match (x.as_f64(), y.as_f64()) {
                (Some(x), Some(y)) => [x as f32, y as f32],
                _ => return Err(wrong_type(&spath, "a number or [x, y]")),
            },
            _ => return Err(wrong_type(&spath, "a number or [x, y]")),
        };
    }
    let invert = obj.bool("invert")?.unwrap_or(false);
    b.invert_x = invert || obj.bool("invert_x")?.unwrap_or(false);
    b.invert_y = invert || obj.bool("invert_y")?.unwrap_or(false);
    b.dead_zone = obj.num("dead_zone")?;
    b.threshold = obj.num("threshold")?;
    Ok(b)
}

/// An `f32` as a JSON number with its shortest decimal spelling (`0.3`, not `0.30000001192…`).
fn num(v: f32) -> Json {
    Json::Num(v.to_string().parse::<f64>().unwrap_or(f64::from(v)))
}

pub(crate) fn binding_to_json(b: &Binding) -> Json {
    let chord = |main: Source| {
        let mut parts: Vec<String> = b.modifiers.iter().map(|m| m.name()).collect();
        parts.push(main.name());
        parts.join("+")
    };
    let plain = b.scale == [1.0, 1.0] && !b.invert_x && !b.invert_y && b.dead_zone.is_none() && b.threshold.is_none();
    if let (BindingInput::Source(s), true) = (b.input, plain) {
        return Json::from(chord(s));
    }
    let mut o = match b.input {
        BindingInput::Source(s) => Json::obj([("source", Json::from(chord(s)))]),
        BindingInput::Composite1 { negative, positive } => Json::obj([(
            "composite",
            Json::obj([("negative", Json::from(negative.name())), ("positive", Json::from(positive.name()))]),
        )]),
        BindingInput::Composite2 { up, down, left, right } => Json::obj([(
            "composite",
            Json::obj([
                ("up", Json::from(up.name())),
                ("down", Json::from(down.name())),
                ("left", Json::from(left.name())),
                ("right", Json::from(right.name())),
            ]),
        )]),
    };
    if !matches!(b.input, BindingInput::Source(_)) && !b.modifiers.is_empty() {
        o.set("modifiers", Json::arr(b.modifiers.iter().map(|m| Json::from(m.name()))));
    }
    if b.scale != [1.0, 1.0] {
        o.set(
            "scale",
            if b.scale[0] == b.scale[1] {
                num(b.scale[0])
            } else {
                Json::arr([num(b.scale[0]), num(b.scale[1])])
            },
        );
    }
    if b.invert_x {
        o.set("invert_x", Json::Bool(true));
    }
    if b.invert_y {
        o.set("invert_y", Json::Bool(true));
    }
    if let Some(dz) = b.dead_zone {
        o.set("dead_zone", num(dz));
    }
    if let Some(t) = b.threshold {
        o.set("threshold", num(t));
    }
    o
}
