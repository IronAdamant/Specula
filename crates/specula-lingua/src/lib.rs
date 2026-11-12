//! Specula Lingua (*lingua*, "language"): localisation as data (docs/design/LOCALISATION.md).
//!
//! - [`table`]: string tables (`specula.strings.v1`), one file per locale: keys to texts, plural
//!   forms, review status and the source text a translation was made from; parsed with
//!   structured errors ([`LinguaError`]), bounded everywhere.
//! - [`locale`]: locale ids and the fallback chain (locale, language, source).
//! - [`plural`]: CLDR plural rules as data (built in for the first languages, overridable).
//! - [`number`]: number formats per locale (decimal and grouping separators, percent).
//! - [`message`]: placeholders in `specula-ui`'s template syntax, formatting for code outside a
//!   UI, and the pseudo-locale's text.
//! - [`catalog`]: what one locale shows, resolved through its chain.
//! - [`check`]: checks across tables (missing, unknown, stale, placeholders, plural forms).
//!
//! **Deterministic** (F2): integer arithmetic, `std`'s exact number formatting and
//! `specula_core::dmath` only, ordered containers, no clocks: game logic may format text from it
//! (dialogue lines in modules) and replays read the same words.

#![forbid(unsafe_code)]

pub mod catalog;
pub mod check;
pub mod error;
pub mod locale;
pub mod message;
pub mod number;
pub mod plural;
pub mod table;

#[cfg(test)]
mod fuzz_tests;

pub use catalog::{Catalog, Resolved};
pub use error::LinguaError;
pub use locale::{Locale, PSEUDO};
pub use message::{Arg, Fmt, Placeholder};
pub use number::NumberFormat;
pub use plural::{Category, Operands, PluralRules};
pub use table::{Entry, FontSpec, Status, Table, Text};
