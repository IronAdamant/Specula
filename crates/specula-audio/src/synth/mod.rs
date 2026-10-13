//! The sound synthesiser: JSON recipes → deterministic sample buffers.

pub mod recipe;
pub mod render;

pub use recipe::Recipe;
pub use render::{render, render_text, RenderOptions, Rendered};

#[cfg(test)]
mod tests;
