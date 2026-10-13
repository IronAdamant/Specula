//! Specula UI: the toolkit agents and humans build HUDs and menus with (design: `docs/design/UI.md`).
//!
//! - **Screens are data** ([`screen`], `specula.ui.screen.v1` JSON): a widget tree with anchored
//!   stacks, rows, columns and grids; px / % / em sizes; bindings to game state (`"Dust {dust}"`),
//!   visibility and enabled conditions (`"dust >= 50"`), and named actions. Strict validation with
//!   JSON paths ([`UiError`]).
//! - **Themes are data** ([`theme`], `specula.ui.theme.v1`): palette, text roles, box and button
//!   styles, scale rule and the `ui_check` thresholds.
//! - **Widgets:** box, panel, label, button, icon (vector shapes or texture regions), bar, list
//!   (scrolling, `repeat` over state rows), radial menu, edge threat arrows, toasts, tooltips,
//!   modal screens, input prompt glyphs.
//! - **Input:** pointer hover / press / drag, touch, and gamepad / keyboard focus navigation
//!   (spatial, with explicit overrides), reading [`specula_input::Input`]; [`Consumed`] tells the
//!   game what the UI took.
//! - **Output:** a [`DrawList`] in physical px (rounded rects, images, glyph quads, AA meshes, clip
//!   rects) plus a hit-test map, allocation-free once warm.
//! - **[`check`]** (`ui_check`): legibility, contrast, overlap, off-screen, overflow, touch-target
//!   and focus-reachability findings at several resolutions, as data.
//! - **[`preview`]**: a software rasteriser for draw lists → RGBA / PNG, so agents and CI can see
//!   UI without a GPU.
//!
//! Conventions: physical pixels, origin top-left, y down; colours sRGB with straight alpha.

pub mod check;
pub mod color;
pub mod draw;
pub mod error;
pub mod geom;
pub mod layout;
pub mod preview;
pub mod screen;
pub mod shapes;
pub mod state;
pub mod testfont;
pub mod theme;
mod ui;

pub use check::{check, Finding, Severity, Target};
pub use draw::{DrawCmd, DrawList, DrawStats, GlyphQuad, Rect, RectCmd, Vertex};
pub use error::{ErrorCode, UiError};
pub use screen::{Screen, SCREEN_FORMAT};
pub use state::{StateList, StateMap, Value};
pub use theme::{Theme, THEME_FORMAT};
pub use ui::{
    add_ui_actions, Consumed, FontSet, Frame, ScreenId, ToastKind, Ui, UiEvent, UiEvents, Viewport, WidgetRef, UI_ACTIONS,
    UI_ACTIONS_JSON,
};
