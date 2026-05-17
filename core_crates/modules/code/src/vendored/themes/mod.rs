#![allow(dead_code)]

//! Code-editor theme — palette of token colours used by the
//! syntax highlighter.
//!
//! Upstream stored every field as a 6-digit hex string
//! (`&'static str`) and re-parsed to a `Color32` on every call.
//! We vendored and rewrote to store [`egui::Color32`] directly
//! instead — that removes the per-frame parsing cost AND lets
//! theme authors pass colours with **alpha** (opaque hex strings
//! lose that information). The background colour in particular
//! picks up the global glass-opacity slider that way.
//!
//! Only one built-in theme (`GRUVBOX`) ships with the vendored
//! code; the other four that upstream shipped were removed when we
//! vendored — we build our own theme in `maracore::code` from
//! the mara palette. Add more as named `ColorTheme` constants in
//! `gruvbox.rs` (or a new sibling file) if you want a theme
//! picker later.

pub mod gruvbox;

use super::syntax::TokenType;
use egui::Color32;

pub const ERROR_COLOR: Color32 = Color32::from_rgb(255, 0, 255);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// Colour palette for a code-editor theme. Every field is a full
/// [`Color32`] (including alpha), so you can make any slot
/// translucent — the backgrounds most notably.
pub struct ColorTheme {
    pub name: &'static str,
    pub dark: bool,
    pub bg: Color32,
    pub cursor: Color32,
    pub selection: Color32,
    pub comments: Color32,
    pub functions: Color32,
    pub keywords: Color32,
    pub literals: Color32,
    pub numerics: Color32,
    pub punctuation: Color32,
    pub strs: Color32,
    pub types: Color32,
    pub special: Color32,
}

impl Default for ColorTheme {
    fn default() -> Self {
        ColorTheme::GRUVBOX
    }
}

impl ColorTheme {
    pub fn name(&self) -> &str {
        self.name
    }

    pub fn is_dark(&self) -> bool {
        self.dark
    }

    pub fn bg(&self) -> Color32 {
        self.bg
    }

    pub fn cursor(&self) -> Color32 {
        self.cursor
    }

    pub fn selection(&self) -> Color32 {
        self.selection
    }

    pub fn modify_style(&self, ui: &mut egui::Ui, fontsize: f32) {
        let style = ui.style_mut();
        style.visuals.widgets.noninteractive.bg_fill = self.bg;
        style.visuals.window_fill = self.bg;
        style.visuals.selection.stroke.color = self.cursor;
        style.visuals.selection.bg_fill = self.selection;
        style.visuals.extreme_bg_color = self.bg;
        style.override_font_id = Some(egui::FontId::monospace(fontsize));
        style.visuals.text_cursor.stroke.width = fontsize * 0.1;
    }

    pub fn type_color(&self, ty: TokenType) -> Color32 {
        match ty {
            TokenType::Comment(_) => self.comments,
            TokenType::Function => self.functions,
            TokenType::Keyword => self.keywords,
            TokenType::Literal => self.literals,
            TokenType::Hyperlink => self.special,
            TokenType::Numeric(_) => self.numerics,
            TokenType::Punctuation(_) => self.punctuation,
            TokenType::Special => self.special,
            TokenType::Str(_) => self.strs,
            TokenType::Type => self.types,
            TokenType::Whitespace(_) | TokenType::Unknown => self.comments,
        }
    }

    /// Build a theme where every token type shares `fg` — handy
    /// when you want syntax-agnostic rendering (e.g. plain text
    /// mode or a colour-blind-accessible monochrome view).
    pub const fn monocolor(
        dark: bool,
        bg: Color32,
        fg: Color32,
        cursor: Color32,
        selection: Color32,
    ) -> Self {
        ColorTheme {
            name: "monocolor",
            dark,
            bg,
            cursor,
            selection,
            literals: fg,
            numerics: fg,
            keywords: fg,
            functions: fg,
            punctuation: fg,
            types: fg,
            strs: fg,
            comments: fg,
            special: fg,
        }
    }
}
