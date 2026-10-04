//! Syntax definitions, lexer, and colour themes.
//!
//! Vendored from `egui_code_editor`, with the widget half removed:
//! rendering now lives in the Mara adapter, which drives
//! `MaraTextArea` (PLAN.md WS-D2). What remains is the tokeniser and
//! the palette, and neither names a UI type.

pub mod highlighting;
mod syntax;
mod themes;

pub use highlighting::Token;
use std::hash::{Hash, Hasher};
pub use syntax::{Syntax, TokenType};
pub use themes::ColorTheme;

#[derive(Clone, Debug, PartialEq)]
/// CodeEditor struct which stores settings for highlighting.
pub struct CodeEditor {
    id: String,
    theme: ColorTheme,
    syntax: Syntax,
    numlines: bool,
    numlines_shift: isize,
    numlines_only_natural: bool,
    fontsize: f32,
    rows: usize,
    vscroll: bool,
    stick_to_bottom: bool,
    desired_width: f32,
}

impl Hash for CodeEditor {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.theme.hash(state);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        (self.fontsize as u32).hash(state);
        self.syntax.hash(state);
    }
}

impl Default for CodeEditor {
    fn default() -> CodeEditor {
        let syntax = Syntax::rust();
        CodeEditor {
            id: String::from("Code Editor"),
            theme: ColorTheme::GRUVBOX,
            syntax,
            numlines: true,
            numlines_shift: 0,
            numlines_only_natural: false,
            fontsize: 10.0,
            rows: 10,
            vscroll: true,
            stick_to_bottom: false,
            desired_width: f32::INFINITY,
        }
    }
}

impl CodeEditor {
    pub fn id_source(self, id_source: impl Into<String>) -> Self {
        CodeEditor {
            id: id_source.into(),
            ..self
        }
    }

    /// Minimum number of rows to show.
    ///
    /// **Default: 10**
    pub fn with_rows(self, rows: usize) -> Self {
        CodeEditor { rows, ..self }
    }

    /// Use custom Color Theme
    ///
    /// **Default: Gruvbox**
    pub fn with_theme(self, theme: ColorTheme) -> Self {
        CodeEditor { theme, ..self }
    }

    /// Use custom font size
    ///
    /// **Default: 10.0**
    pub fn with_fontsize(self, fontsize: f32) -> Self {
        CodeEditor { fontsize, ..self }
    }

    /// Use UI font size
    /// Show or hide lines numbering
    ///
    /// **Default: true**
    pub fn with_numlines(self, numlines: bool) -> Self {
        CodeEditor { numlines, ..self }
    }

    /// Shift lines numbering by this value
    ///
    /// **Default: 0**
    pub fn with_numlines_shift(self, numlines_shift: isize) -> Self {
        CodeEditor {
            numlines_shift,
            ..self
        }
    }

    /// Show lines numbering only above zero, useful for enabling numbering since nth row
    ///
    /// **Default: false**
    pub fn with_numlines_only_natural(self, numlines_only_natural: bool) -> Self {
        CodeEditor {
            numlines_only_natural,
            ..self
        }
    }

    /// Use custom syntax for highlighting
    ///
    /// **Default: Rust**
    pub fn with_syntax(self, syntax: Syntax) -> Self {
        CodeEditor { syntax, ..self }
    }

    /// Turn on/off scrolling on the vertical axis.
    ///
    /// **Default: true**
    pub fn vscroll(self, vscroll: bool) -> Self {
        CodeEditor { vscroll, ..self }
    }
    /// Should the containing area shrink if the content is small?
    ///
    /// **Default: false**
    pub fn auto_shrink(self, shrink: bool) -> Self {
        CodeEditor {
            desired_width: if shrink { 0.0 } else { self.desired_width },
            ..self
        }
    }

    /// Sets the desired width of the code editor
    ///
    /// **Default: `f32::INFINITY`**
    pub fn desired_width(self, width: f32) -> Self {
        CodeEditor {
            desired_width: width,
            ..self
        }
    }

    /// Stick to bottom
    /// The scroll handle will stick to the bottom position even while the content size
    /// changes dynamically. This can be useful to simulate terminal UIs or log/info scrollers.
    /// The scroll handle remains stuck until user manually changes position. Once "unstuck"
    /// it will remain focused on whatever content viewport the user left it on. If the scroll
    /// handle is dragged to the bottom it will again become stuck and remain there until manually
    /// pulled from the end position.
    ///
    /// **Default: false**
    pub fn stick_to_bottom(self, stick_to_bottom: bool) -> Self {
        CodeEditor {
            stick_to_bottom,
            ..self
        }
    }

    /// Split one line into `(text, token type)` pairs.
    ///
    /// The UI-free highlighting entry point (PLAN.md WS-D2): a host maps
    /// each pair to its own styled run using [`ColorTheme::type_color`].
    /// Concatenating the texts reproduces `line` exactly, so a renderer
    /// can rely on it for layout.
    #[must_use]
    pub fn highlight_line(&self, line: &str) -> Vec<(String, TokenType)> {
        Token::default()
            .tokens(&self.syntax, line)
            .into_iter()
            .map(|token| (token.buffer().to_owned(), token.ty()))
            .collect()
    }

    /// The palette this editor renders with.
    #[must_use]
    pub fn theme(&self) -> &ColorTheme {
        &self.theme
    }

    /// Configured font size in points.
    #[must_use]
    pub fn fontsize(&self) -> f32 {
        self.fontsize
    }

    /// Configured visible row count.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }
}
