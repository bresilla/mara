//! # mara_code
//!
//! Standalone code-editor crate for egui. Vendored fork of
//! [`egui_code_editor`](https://crates.io/crates/egui_code_editor).
//!
//! The crate is theme-neutral: the bundled [`ColorTheme`] presets
//! (`GRUVBOX`, `GITHUB_LIGHT`, `SONOKAI`, …) cover the common
//! cases, and [`default_code_theme`] picks one as a starting
//! point. Mara-tinted styling lives in the `mara_core` crate
//! behind the optional `code` feature, which depends on this crate
//! and wires the embed / maximise affordance on top.
//!
//! Use it standalone:
//!
//! ```ignore
//! use mara_code::{CodeEditor, ColorTheme, Syntax};
//!
//! CodeEditor::default()
//!     .id_source("my_editor")
//!     .with_theme(ColorTheme::GRUVBOX)
//!     .with_syntax(Syntax::rust())
//!     .with_fontsize(13.0)
//!     .with_rows(20)
//!     .with_numlines(true)
//!     .show(ui, &mut text);
//! ```

mod vendored;

pub use vendored::{CodeEditor, ColorTheme, Syntax, Token, TokenType};

/// A default [`ColorTheme`] — currently [`ColorTheme::GRUVBOX`].
/// Use this when you want a sane starting palette without
/// committing to a specific scheme; later swap in any of the
/// other bundled presets or build one with `ColorTheme::default()`.
#[must_use]
pub fn default_code_theme() -> ColorTheme {
    ColorTheme::GRUVBOX
}
