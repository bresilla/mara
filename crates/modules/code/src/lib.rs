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

/// An sRGB colour, as the theme presets store them.
///
/// This crate deliberately has **no UI dependency at all** — not egui,
/// not `mara_core`. It cannot depend on `mara_core` because
/// `mara_core` optionally depends on *it* (the `code` feature), and
/// Cargo forbids the cycle. So the palette speaks its own tiny colour
/// type and the Mara adapter converts it to `vocab::Color32` at the
/// boundary. See PLAN.md WS-D2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CodeColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl CodeColor {
    #[must_use]
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    #[must_use]
    pub const fn from_rgba_unmultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// `[r, g, b, a]`, for a host converting to its own colour type.
    #[must_use]
    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// Bridge for the parts of this crate that still render through egui —
/// the `CodeEditor` widget and its completion popup. Those move to the
/// Mara adapter in WS-D2; when they do, this impl and the `egui`
/// dependency go with them, and the crate becomes UI-free.
impl From<CodeColor> for egui::Color32 {
    fn from(c: CodeColor) -> Self {
        egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
    }
}

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

#[cfg(test)]
mod seal_tests {
    use super::*;
    use vendored::Token;

    /// The WS-D2 path, proven without any UI in scope: tokenise a line,
    /// colour each token from the theme, and get back plain
    /// `(text, [r,g,b,a])` pairs. That is exactly the shape
    /// `MaraTextArea`'s highlight closure wants, so the Mara adapter is
    /// a mapping step with no backend coupling of its own.
    #[test]
    fn tokenising_and_colouring_needs_no_ui_types() {
        let theme = ColorTheme::GRUVBOX;
        let syntax = Syntax::rust();
        let tokens = Token::default().tokens(&syntax, "fn main() {}");

        let runs: Vec<(String, [u8; 4])> = tokens
            .iter()
            .map(|token| {
                (
                    token.buffer().to_owned(),
                    theme.type_color(token.ty()).to_array(),
                )
            })
            .collect();

        assert!(!runs.is_empty(), "the lexer produced tokens");
        assert_eq!(
            runs.iter()
                .map(|(text, _)| text.as_str())
                .collect::<String>(),
            "fn main() {}",
            "tokens reassemble into the original line exactly"
        );

        let keyword = runs
            .iter()
            .find(|(text, _)| text == "fn")
            .expect("`fn` is tokenised");
        assert_eq!(
            keyword.1,
            theme.type_color(TokenType::Keyword).to_array(),
            "`fn` takes the keyword colour"
        );
    }

    /// The palette layer must not name a UI type at all — that is what
    /// lets `mara_code` stay free of both egui and `mara_core` (which
    /// it cannot depend on: `mara_core` optionally depends on *it*).
    #[test]
    fn theme_colours_are_plain_data() {
        let c = ColorTheme::GRUVBOX.bg();
        assert_eq!(c.to_array().len(), 4);
        assert_eq!(CodeColor::from_rgb(1, 2, 3).to_array(), [1, 2, 3, 255]);
    }
}
