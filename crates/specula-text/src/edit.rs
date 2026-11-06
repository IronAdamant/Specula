//! Text input: a caret, a selection, clipboard operations, word and line movement, undo, and IME
//! composition, as a pure model (stage T3; design `docs/design/TEXT.md`).
//!
//! The same [`TextEdit`] serves a human at a keyboard (the window turns keys, `Char` and IME
//! events into [`EditOp`]s), a gamepad or touch user (an on-screen keyboard sends the same ops:
//! [`OnScreenKeyboard`]) and an agent over MCP (`text_edit` sends ops as data). Offsets are byte
//! offsets into the UTF-8 text, always on a caret stop: a character boundary that doesn't split
//! a base character from its combining marks ([`is_caret_stop`]).
//!
//! - **Selection:** `anchor..caret` (either order); typing replaces it.
//! - **Clipboard:** through the [`Clipboard`] trait: the OS clipboard for a window, a private one
//!   per agent (agents never touch the human's clipboard).
//! - **IME:** while composing, the composition text shows at the caret (underlined by the view)
//!   without entering the text; committing inserts it, cancelling drops it.
//! - **Limits:** the text never passes `max_bytes` (input past it is cut at a caret stop and
//!   reported), the undo history keeps [`MAX_UNDO`] steps.

use crate::layout::TextLayout;
use crate::linebreak::{class_of, Class};

/// Undo steps kept.
pub const MAX_UNDO: usize = 64;
/// Longest text an edit accepts by default.
pub const DEFAULT_MAX_BYTES: usize = 4096;

/// Where copied text goes.
pub trait Clipboard {
    fn get(&mut self) -> Option<String>;
    fn set(&mut self, text: &str);
}

/// A clipboard in memory (tests, and each agent's own clipboard).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryClipboard(pub Option<String>);

impl Clipboard for MemoryClipboard {
    fn get(&mut self) -> Option<String> {
        self.0.clone()
    }
    fn set(&mut self, text: &str) {
        self.0 = Some(text.to_owned());
    }
}

/// How far a movement goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// One caret stop (a character with its combining marks).
    Char,
    /// To the next word start (right) or the previous one (left).
    Word,
    /// To the start or end of the line (needs a layout; without one, the paragraph).
    Line,
    /// To the start or end of the text.
    All,
}

/// One editing operation.
#[derive(Clone, Debug, PartialEq)]
pub enum EditOp {
    /// Types text at the caret, replacing the selection.
    Insert(String),
    /// Deletes the selection, or the unit before the caret.
    Backspace(Unit),
    /// Deletes the selection, or the unit after the caret.
    Delete(Unit),
    /// Moves the caret; `select` extends the selection instead of collapsing it.
    Left { unit: Unit, select: bool },
    Right { unit: Unit, select: bool },
    /// Moves a line up or down (needs a layout; without one, to the start or end).
    Up { select: bool },
    Down { select: bool },
    /// Puts the caret at a byte offset (snapped back to a caret stop); `select` extends.
    SetCaret { byte: usize, select: bool },
    /// Selects `anchor..caret` (both snapped).
    Select { anchor: usize, caret: usize },
    SelectAll,
    Copy,
    Cut,
    Paste,
    /// IME: the composition string so far and its caret (bytes into it).
    Compose { text: String, caret: usize },
    /// IME: the final string; replaces the selection like typing.
    Commit(String),
    /// IME: the composition is abandoned.
    CancelComposition,
    Undo,
    Redo,
}

/// What an operation did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditResult {
    /// The text changed.
    pub changed: bool,
    /// Input was cut to fit `max_bytes` (bytes dropped).
    pub dropped: usize,
    /// The operation did nothing (e.g. Backspace at the start).
    pub noop: bool,
}

/// An IME composition in progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Composition {
    pub text: String,
    /// Bytes into `text`.
    pub caret: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    text: String,
    caret: usize,
    anchor: usize,
}

/// The editing state of one text field.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEdit {
    text: String,
    caret: usize,
    anchor: usize,
    composition: Option<Composition>,
    max_bytes: usize,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

/// Whether `byte` is a caret stop of `text`: a character boundary not followed by a combining
/// mark or joiner (so a base character and its marks move and delete together).
pub fn is_caret_stop(text: &str, byte: usize) -> bool {
    if byte == 0 || byte >= text.len() {
        return byte <= text.len();
    }
    if !text.is_char_boundary(byte) {
        return false;
    }
    let next = text[byte..].chars().next().map(class_of);
    let prev = text[..byte].chars().next_back();
    !matches!(next, Some(Class::CM | Class::ZWJ)) && prev != Some('\u{200D}') && !(prev == Some('\r') && text[byte..].starts_with('\n'))
}

fn snap_back(text: &str, mut byte: usize) -> usize {
    byte = byte.min(text.len());
    while byte > 0 && !is_caret_stop(text, byte) {
        byte -= 1;
    }
    byte
}

fn next_stop(text: &str, byte: usize) -> usize {
    let mut b = byte;
    loop {
        match text[b..].chars().next() {
            None => return text.len(),
            Some(c) => b += c.len_utf8(),
        }
        if is_caret_stop(text, b) {
            return b;
        }
    }
}

fn prev_stop(text: &str, byte: usize) -> usize {
    let mut b = byte;
    loop {
        match text[..b].chars().next_back() {
            None => return 0,
            Some(c) => b -= c.len_utf8(),
        }
        if is_caret_stop(text, b) {
            return b;
        }
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || matches!(class_of(c), Class::CM | Class::ZWJ)
}

/// The next word start after `byte` (skipping the rest of the word, then spaces/punctuation).
fn next_word(text: &str, byte: usize) -> usize {
    let rest = &text[byte..];
    let mut it = rest.char_indices().peekable();
    // Through the current word…
    while let Some(&(_, c)) = it.peek() {
        if !is_word_char(c) {
            break;
        }
        it.next();
    }
    // …then whatever separates it from the next.
    while let Some(&(_, c)) = it.peek() {
        if is_word_char(c) {
            break;
        }
        it.next();
    }
    it.peek().map_or(text.len(), |&(i, _)| byte + i)
}

/// The start of the word before `byte`.
fn prev_word(text: &str, byte: usize) -> usize {
    let before = &text[..byte];
    let mut it = before.char_indices().rev().peekable();
    while let Some(&(_, c)) = it.peek() {
        if is_word_char(c) {
            break;
        }
        it.next();
    }
    let mut start = it.peek().map_or(0, |&(i, _)| i);
    while let Some(&(i, c)) = it.peek() {
        if !is_word_char(c) {
            break;
        }
        start = i;
        it.next();
    }
    if it.peek().is_none() && before.chars().next().is_some_and(is_word_char) {
        start = 0;
    }
    start
}

impl TextEdit {
    pub fn new(text: &str) -> TextEdit {
        TextEdit::with_limit(text, DEFAULT_MAX_BYTES)
    }

    /// A field holding at most `max_bytes` bytes (the initial text is cut to fit).
    pub fn with_limit(text: &str, max_bytes: usize) -> TextEdit {
        let t = text[..snap_back(text, text.len().min(max_bytes))].to_owned();
        let end = t.len();
        TextEdit { text: t, caret: end, anchor: end, composition: None, max_bytes, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn anchor(&self) -> usize {
        self.anchor
    }

    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    /// The selected bytes (empty when the selection is collapsed).
    pub fn selection(&self) -> std::ops::Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection()]
    }

    pub fn composition(&self) -> Option<&Composition> {
        self.composition.as_ref()
    }

    /// The text as shown: the composition (if any) inserted at the caret, and where it sits.
    pub fn display(&self) -> (String, Option<std::ops::Range<usize>>) {
        match &self.composition {
            None => (self.text.clone(), None),
            Some(c) => {
                let at = self.selection().start;
                let mut s = String::with_capacity(self.text.len() + c.text.len());
                s.push_str(&self.text[..at]);
                s.push_str(&c.text);
                s.push_str(&self.text[self.selection().end..]);
                (s, Some(at..at + c.text.len()))
            }
        }
    }

    /// Restores a caret and selection (snapped to caret stops), e.g. from saved state.
    pub fn set_selection(&mut self, anchor: usize, caret: usize) {
        self.anchor = snap_back(&self.text, anchor);
        self.caret = snap_back(&self.text, caret);
    }

    fn remember(&mut self) {
        if self.undo.len() >= MAX_UNDO {
            self.undo.remove(0);
        }
        self.undo.push(Snapshot { text: self.text.clone(), caret: self.caret, anchor: self.anchor });
        self.redo.clear();
    }

    /// Replaces the selection with `s` (cut to fit `max_bytes` at a caret stop).
    fn replace(&mut self, s: &str) -> EditResult {
        let sel = self.selection();
        let room = self.max_bytes.saturating_sub(self.text.len() - sel.len());
        // Cut at a caret stop of the input: a cluster that doesn't fit is dropped whole.
        let piece = &s[..snap_back(s, s.len().min(room))];
        let dropped = s.len() - piece.len();
        if piece.is_empty() && sel.is_empty() {
            return EditResult { changed: false, dropped, noop: true };
        }
        self.remember();
        self.text.replace_range(sel.clone(), piece);
        self.caret = sel.start + piece.len();
        self.caret = snap_back(&self.text, self.caret);
        self.anchor = self.caret;
        EditResult { changed: true, dropped, noop: false }
    }

    fn delete_range(&mut self, r: std::ops::Range<usize>) -> EditResult {
        if r.is_empty() {
            return EditResult { noop: true, ..EditResult::default() };
        }
        self.remember();
        self.text.replace_range(r.clone(), "");
        self.caret = r.start;
        self.anchor = r.start;
        EditResult { changed: true, ..EditResult::default() }
    }

    fn move_to(&mut self, to: usize, select: bool) -> EditResult {
        let to = snap_back(&self.text, to);
        let noop = to == self.caret && (select || self.anchor == self.caret);
        self.caret = to;
        if !select {
            self.anchor = to;
        }
        EditResult { noop, ..EditResult::default() }
    }

    fn line_bounds(&self, layout: Option<&TextLayout>) -> (usize, usize) {
        if let Some(l) = layout.filter(|l| l.text_len == self.text.len() && !l.lines.is_empty()) {
            let line = &l.lines[l.line_of(self.caret)];
            return (line.bytes.start, line.bytes.end);
        }
        let start = self.text[..self.caret].rfind('\n').map_or(0, |i| i + 1);
        let end = self.text[self.caret..].find('\n').map_or(self.text.len(), |i| self.caret + i);
        (start, end)
    }

    /// Applies `op`. `layout` (of the current text, for line movement) may be `None`.
    pub fn apply(&mut self, op: &EditOp, clipboard: &mut dyn Clipboard, layout: Option<&TextLayout>) -> EditResult {
        // Any op but Compose ends a composition (Commit inserts it first).
        if !matches!(op, EditOp::Compose { .. }) {
            self.composition = None;
        }
        match op {
            EditOp::Insert(s) | EditOp::Commit(s) => {
                // Line breaks inside a single-line field are the caller's business; control
                // characters other than tab and newline are dropped.
                let clean: String = s.chars().filter(|c| !c.is_control() || matches!(c, '\n' | '\t')).collect();
                let mut r = self.replace(&clean);
                r.dropped += s.len() - clean.len();
                r
            }
            EditOp::Backspace(unit) | EditOp::Delete(unit) => {
                let sel = self.selection();
                if !sel.is_empty() {
                    return self.delete_range(sel);
                }
                let back = matches!(op, EditOp::Backspace(_));
                let other = match (unit, back) {
                    (Unit::Char, true) => prev_stop(&self.text, self.caret),
                    (Unit::Char, false) => next_stop(&self.text, self.caret),
                    (Unit::Word, true) => prev_word(&self.text, self.caret),
                    (Unit::Word, false) => next_word(&self.text, self.caret),
                    (Unit::Line, true) => self.line_bounds(layout).0,
                    (Unit::Line, false) => self.line_bounds(layout).1,
                    (Unit::All, true) => 0,
                    (Unit::All, false) => self.text.len(),
                };
                let r = if back { other..self.caret } else { self.caret..other };
                self.delete_range(r)
            }
            EditOp::Left { unit, select } | EditOp::Right { unit, select } => {
                let left = matches!(op, EditOp::Left { .. });
                let sel = self.selection();
                if !*select && !sel.is_empty() && *unit == Unit::Char {
                    // Collapse to the selection's edge.
                    return self.move_to(if left { sel.start } else { sel.end }, false);
                }
                let to = match (unit, left) {
                    (Unit::Char, true) => prev_stop(&self.text, self.caret),
                    (Unit::Char, false) => next_stop(&self.text, self.caret),
                    (Unit::Word, true) => prev_word(&self.text, self.caret),
                    (Unit::Word, false) => next_word(&self.text, self.caret),
                    (Unit::Line, true) => self.line_bounds(layout).0,
                    (Unit::Line, false) => self.line_bounds(layout).1,
                    (Unit::All, true) => 0,
                    (Unit::All, false) => self.text.len(),
                };
                self.move_to(to, *select)
            }
            EditOp::Up { select } | EditOp::Down { select } => {
                let up = matches!(op, EditOp::Up { .. });
                let to = match layout.filter(|l| l.text_len == self.text.len() && !l.lines.is_empty()) {
                    Some(l) => {
                        let c = l.caret(self.caret);
                        let target = if up { c.line.checked_sub(1) } else { Some(c.line + 1).filter(|&n| n < l.lines.len()) };
                        match target {
                            Some(n) => l.hit(c.x, l.lines[n].top + l.lines[n].height * 0.5),
                            None if up => 0,
                            None => self.text.len(),
                        }
                    }
                    None if up => 0,
                    None => self.text.len(),
                };
                self.move_to(to, *select)
            }
            EditOp::SetCaret { byte, select } => self.move_to(*byte, *select),
            EditOp::Select { anchor, caret } => {
                self.set_selection(*anchor, *caret);
                EditResult::default()
            }
            EditOp::SelectAll => {
                self.anchor = 0;
                self.caret = self.text.len();
                EditResult::default()
            }
            EditOp::Copy | EditOp::Cut => {
                let sel = self.selection();
                if sel.is_empty() {
                    return EditResult { noop: true, ..EditResult::default() };
                }
                clipboard.set(&self.text[sel.clone()]);
                if matches!(op, EditOp::Cut) {
                    self.delete_range(sel)
                } else {
                    EditResult::default()
                }
            }
            EditOp::Paste => match clipboard.get() {
                Some(s) => self.apply(&EditOp::Insert(s), clipboard, layout),
                None => EditResult { noop: true, ..EditResult::default() },
            },
            EditOp::Compose { text, caret } => {
                let clean: String = text.chars().filter(|c| !c.is_control()).take(256).collect();
                let caret = snap_back(&clean, (*caret).min(clean.len()));
                self.composition = (!clean.is_empty()).then_some(Composition { text: clean, caret });
                EditResult::default()
            }
            EditOp::CancelComposition => EditResult::default(),
            EditOp::Undo | EditOp::Redo => {
                let (from, to) = if matches!(op, EditOp::Undo) { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
                match from.pop() {
                    Some(s) => {
                        to.push(Snapshot { text: std::mem::take(&mut self.text), caret: self.caret, anchor: self.anchor });
                        self.text = s.text;
                        self.caret = s.caret;
                        self.anchor = s.anchor;
                        EditResult { changed: true, ..EditResult::default() }
                    }
                    None => EditResult { noop: true, ..EditResult::default() },
                }
            }
        }
    }
}

/// The on-screen keyboard seam for gamepad and touch: a grid of keys a focus cursor (or a finger)
/// moves over; activating a key yields the [`EditOp`] to apply. Layout is data (rows of keys) so
/// games and locales can swap it; the default is a Latin QWERTY with shift, space, backspace and
/// done.
#[derive(Clone, Debug, PartialEq)]
pub struct OnScreenKeyboard {
    pub rows: Vec<Vec<OskKey>>,
    /// Focused key (row, column).
    pub focus: (usize, usize),
    pub shift: bool,
}

/// One on-screen key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OskKey {
    /// Types its text (shifted: upper case).
    Text(String),
    Shift,
    Space,
    Backspace,
    Left,
    Right,
    /// Closes the keyboard (the field keeps its text).
    Done,
}

/// What activating a key did.
#[derive(Clone, Debug, PartialEq)]
pub enum OskAction {
    Edit(EditOp),
    ShiftToggled,
    Done,
}

impl Default for OnScreenKeyboard {
    fn default() -> OnScreenKeyboard {
        let row = |s: &str| s.chars().map(|c| OskKey::Text(c.to_string())).collect::<Vec<_>>();
        let mut rows = vec![row("1234567890"), row("qwertyuiop"), row("asdfghjkl'"), row("zxcvbnm,.-")];
        rows.push(vec![OskKey::Shift, OskKey::Left, OskKey::Space, OskKey::Right, OskKey::Backspace, OskKey::Done]);
        OnScreenKeyboard { rows, focus: (1, 0), shift: false }
    }
}

impl OnScreenKeyboard {
    /// Moves the focus by (`dx`, `dy`) keys, wrapping around rows and columns (a d-pad or stick).
    pub fn move_focus(&mut self, dx: i32, dy: i32) {
        if self.rows.is_empty() {
            return;
        }
        let rows = self.rows.len() as i32;
        let r = (self.focus.0 as i32 + dy).rem_euclid(rows) as usize;
        let cols = self.rows[r].len().max(1) as i32;
        let c = (self.focus.1.min(cols as usize - 1) as i32 + dx).rem_euclid(cols) as usize;
        self.focus = (r, c);
    }

    /// The key at the focus.
    pub fn focused(&self) -> Option<&OskKey> {
        self.rows.get(self.focus.0).and_then(|r| r.get(self.focus.1))
    }

    /// Activates the focused key (a face button, or a tap after [`OnScreenKeyboard::focus`] is set
    /// to the touched key).
    pub fn activate(&mut self) -> Option<OskAction> {
        let key = self.focused()?.clone();
        Some(match key {
            OskKey::Text(t) => {
                let t = if self.shift { t.to_uppercase() } else { t };
                self.shift = false;
                OskAction::Edit(EditOp::Insert(t))
            }
            OskKey::Shift => {
                self.shift = !self.shift;
                OskAction::ShiftToggled
            }
            OskKey::Space => OskAction::Edit(EditOp::Insert(" ".into())),
            OskKey::Backspace => OskAction::Edit(EditOp::Backspace(Unit::Char)),
            OskKey::Left => OskAction::Edit(EditOp::Left { unit: Unit::Char, select: false }),
            OskKey::Right => OskAction::Edit(EditOp::Right { unit: Unit::Char, select: false }),
            OskKey::Done => OskAction::Done,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(e: &mut TextEdit, ops: &[EditOp]) -> MemoryClipboard {
        let mut cb = MemoryClipboard::default();
        for op in ops {
            e.apply(op, &mut cb, None);
        }
        cb
    }

    #[test]
    fn typing_moving_selecting_and_deleting() {
        let mut e = TextEdit::new("");
        run(&mut e, &[EditOp::Insert("Here lies".into()), EditOp::Insert(" Bob".into())]);
        assert_eq!((e.text(), e.caret()), ("Here lies Bob", 13));
        run(&mut e, &[EditOp::Left { unit: Unit::Word, select: true }]);
        assert_eq!(e.selected_text(), "Bob");
        run(&mut e, &[EditOp::Insert("Ada".into())]);
        assert_eq!(e.text(), "Here lies Ada");
        run(&mut e, &[EditOp::Left { unit: Unit::All, select: false }, EditOp::Right { unit: Unit::Word, select: false }]);
        assert_eq!(e.caret(), 5);
        run(&mut e, &[EditOp::Delete(Unit::Word)]);
        assert_eq!(e.text(), "Here Ada");
        run(&mut e, &[EditOp::Backspace(Unit::Char), EditOp::Backspace(Unit::Char)]);
        assert_eq!((e.text(), e.caret()), ("HerAda", 3));
        run(&mut e, &[EditOp::Undo, EditOp::Undo]);
        assert_eq!(e.text(), "Here Ada");
        run(&mut e, &[EditOp::Redo]);
        assert_eq!(e.text(), "HereAda");
    }

    #[test]
    fn clusters_move_and_delete_together() {
        let mut e = TextEdit::new("e\u{301}x");
        assert!(!is_caret_stop(e.text(), 1), "inside é");
        run(&mut e, &[EditOp::Left { unit: Unit::Char, select: false }]);
        assert_eq!(e.caret(), 3);
        run(&mut e, &[EditOp::Left { unit: Unit::Char, select: false }]);
        assert_eq!(e.caret(), 0, "over the whole cluster");
        run(&mut e, &[EditOp::Delete(Unit::Char)]);
        assert_eq!(e.text(), "x");
        let mut e = TextEdit::new("a\r\nb");
        run(&mut e, &[EditOp::Left { unit: Unit::Char, select: false }, EditOp::Backspace(Unit::Char)]);
        assert_eq!(e.text(), "ab", "CR LF is one stop");
        e.set_selection(1, 99);
        assert_eq!((e.anchor(), e.caret()), (1, 2));
    }

    /// Mutation-test probes (probatio): the edges of caret stops, word characters, no-op inserts.
    #[test]
    fn caret_stops_words_and_noops_at_the_edges() {
        assert!(is_caret_stop("\u{301}a", 0), "the start is always a stop");
        assert!(is_caret_stop("a\u{200D}", 4), "the end is always a stop, even after a joiner");
        assert!(!is_caret_stop("a\u{200D}b", 4), "never right after a joiner");
        assert!(!is_caret_stop("a\r\nb", 2), "never inside CR LF");
        assert!(is_caret_stop("ab", 1) && !is_caret_stop("ab", 3));
        let mut e = TextEdit::new("foo_bar baz");
        e.set_selection(0, 0);
        run(&mut e, &[EditOp::Right { unit: Unit::Word, select: false }]);
        assert_eq!(e.caret(), 8, "an underscore is part of a word");
        let r = e.apply(&EditOp::Insert(String::new()), &mut MemoryClipboard::default(), None);
        assert!(r.noop && !r.changed, "{r:?}");
    }

    #[test]
    fn clipboard_cut_copy_paste() {
        let mut e = TextEdit::new("Wave 3 of 10");
        let mut cb = MemoryClipboard::default();
        e.apply(&EditOp::Select { anchor: 0, caret: 6 }, &mut cb, None);
        e.apply(&EditOp::Cut, &mut cb, None);
        assert_eq!((e.text(), cb.0.as_deref()), (" of 10", Some("Wave 3")));
        e.apply(&EditOp::Right { unit: Unit::All, select: false }, &mut cb, None);
        e.apply(&EditOp::Insert(": ".into()), &mut cb, None);
        e.apply(&EditOp::Paste, &mut cb, None);
        assert_eq!(e.text(), " of 10: Wave 3");
        assert!(e.apply(&EditOp::Copy, &mut cb, None).noop, "nothing selected");
    }

    #[test]
    fn ime_composition_shows_then_commits_or_cancels() {
        let mut e = TextEdit::new("名前:");
        let mut cb = MemoryClipboard::default();
        e.apply(&EditOp::Compose { text: "やま".into(), caret: 6 }, &mut cb, None);
        assert_eq!(e.text(), "名前:", "not in the text yet");
        let (shown, at) = e.display();
        assert_eq!(shown, "名前:やま");
        assert_eq!(at, Some(7..13));
        e.apply(&EditOp::Compose { text: "山".into(), caret: 3 }, &mut cb, None);
        e.apply(&EditOp::Commit("山".into()), &mut cb, None);
        assert_eq!((e.text(), e.composition()), ("名前:山", None));
        e.apply(&EditOp::Compose { text: "x".into(), caret: 1 }, &mut cb, None);
        e.apply(&EditOp::CancelComposition, &mut cb, None);
        assert_eq!((e.text(), e.display().1), ("名前:山", None));
    }

    #[test]
    fn limits_hold() {
        let mut e = TextEdit::with_limit("", 8);
        let r = run_one(&mut e, EditOp::Insert("abcdefghij".into()));
        assert_eq!((e.text(), r.dropped), ("abcdefgh", 2));
        let mut e = TextEdit::with_limit("", 4);
        let r = run_one(&mut e, EditOp::Insert("abe\u{301}".into()));
        assert_eq!(e.text(), "ab", "a cluster that doesn't fit isn't split: {r:?}");
        let mut e = TextEdit::new("");
        run_one(&mut e, EditOp::Insert("a\u{7}b\u{1b}c".into()));
        assert_eq!(e.text(), "abc", "control characters dropped");
        for _ in 0..(MAX_UNDO + 10) {
            run_one(&mut e, EditOp::Insert("x".into()));
        }
        let mut undone = 0;
        while !run_one(&mut e, EditOp::Undo).noop {
            undone += 1;
        }
        assert_eq!(undone, MAX_UNDO);
    }

    fn run_one(e: &mut TextEdit, op: EditOp) -> EditResult {
        e.apply(&op, &mut MemoryClipboard::default(), None)
    }

    #[test]
    fn on_screen_keyboard_types_through_the_same_ops() {
        let mut k = OnScreenKeyboard::default();
        let mut e = TextEdit::new("");
        let mut cb = MemoryClipboard::default();
        // Shift, then 'q' (row 1, col 0), then 'w'.
        k.focus = (4, 0);
        assert_eq!(k.activate(), Some(OskAction::ShiftToggled));
        k.focus = (1, 0);
        for _ in 0..2 {
            if let Some(OskAction::Edit(op)) = k.activate() {
                e.apply(&op, &mut cb, None);
            }
            k.move_focus(1, 0);
        }
        assert_eq!(e.text(), "Qw");
        k.move_focus(-3, 0);
        assert_eq!(k.focus, (1, 9), "wraps");
        k.move_focus(0, 4);
        assert_eq!(k.focus, (0, 9));
    }

    #[test]
    fn hostile_ops_never_panic() {
        let texts = ["", "é", "e\u{301}\u{302}", "a\r\n\u{200D}b", "日本語 text", "\u{FFFC}"];
        let mut seed = 99u32;
        for t in texts {
            let mut e = TextEdit::with_limit(t, 16);
            let mut cb = MemoryClipboard(Some("\u{301}zz".into()));
            for _ in 0..400 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let u = [Unit::Char, Unit::Word, Unit::Line, Unit::All][(seed >> 8) as usize % 4];
                let n = (seed >> 12) as usize % 40;
                let op = match (seed >> 20) % 14 {
                    0 => EditOp::Insert("é\u{301}x".into()),
                    1 => EditOp::Backspace(u),
                    2 => EditOp::Delete(u),
                    3 => EditOp::Left { unit: u, select: seed & 1 == 0 },
                    4 => EditOp::Right { unit: u, select: seed & 1 == 0 },
                    5 => EditOp::SetCaret { byte: n, select: seed & 2 == 0 },
                    6 => EditOp::Select { anchor: n, caret: n / 3 },
                    7 => EditOp::Cut,
                    8 => EditOp::Paste,
                    9 => EditOp::Compose { text: "かな".into(), caret: n },
                    10 => EditOp::Commit("\u{301}".into()),
                    11 => EditOp::Undo,
                    12 => EditOp::Up { select: true },
                    _ => EditOp::Redo,
                };
                e.apply(&op, &mut cb, None);
                assert!(e.text().len() <= 16 && e.text().is_char_boundary(e.caret()) && e.text().is_char_boundary(e.anchor()), "{t:?} {op:?}");
                let _ = e.display();
            }
        }
    }
}
