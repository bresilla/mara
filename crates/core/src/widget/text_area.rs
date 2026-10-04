//! Multi-line text editing — PLAN.md WS-A8.
//!
//! The sealed counterpart to a code editor's text surface: a
//! [`MaraTextArea`] paints a `String` across several lines, drives the
//! caret and selection from [`crate::text_edit::TextEditState`], and
//! renders each line through the backend-neutral paint IR.
//!
//! ## Why it exists
//!
//! `mara_code` is a vendored editor whose public API hands out the
//! backend's `Ui` and text-buffer types directly. Rewriting it onto the
//! sealed surface (WS-D2) needs a Mara-owned editing surface to rewrite
//! it *onto*. This is that surface.
//!
//! ## Syntax highlighting
//!
//! A caller supplies a per-line `highlight` closure returning
//! [`TextRun`]s. That is the same shape a tokeniser already produces,
//! and it lowers to [`PaintCmd::TextRuns`] — so highlighting costs no
//! backend coupling at all. Without a closure, lines paint in one
//! colour.
//!
//! ## Scope
//!
//! Deliberately not included yet: clipboard, IME composition, undo,
//! horizontal scrolling, and click-to-position-caret (which needs
//! per-glyph hit testing the paint IR does not expose). Keyboard
//! editing, selection, and rendering are complete. Those gaps are
//! listed in PLAN.md WS-D2 rather than hidden here.

use crate::layout::{Sense, UiBackend};
use crate::memory::MaraMemory;
use crate::mui::{MaraKey, MaraResponse};
use crate::paint::{PaintCmd, TextFamily, TextRun};
use crate::style;
use crate::text_edit::TextEditState;
use crate::vocab;

/// How one line of text should be styled.
pub type HighlightFn<'a> = &'a dyn Fn(&str) -> Vec<TextRun>;

/// A multi-line editing surface.
pub struct MaraTextArea<'a> {
    id: vocab::Id,
    rows: usize,
    font_size: f32,
    monospace: bool,
    accent: vocab::Color32,
    highlight: Option<HighlightFn<'a>>,
}

/// What a [`MaraTextArea`] pass did.
#[derive(Clone, Debug)]
pub struct MaraTextAreaResponse {
    pub response: MaraResponse,
    /// The buffer was edited this pass.
    pub changed: bool,
    /// Caret position as zero-based `(line, column)`.
    pub caret: (usize, usize),
}

impl<'a> MaraTextArea<'a> {
    #[must_use]
    pub fn new(id: impl Into<vocab::Id>) -> Self {
        Self {
            id: id.into(),
            rows: 12,
            font_size: style::BODY_FONT_SIZE,
            monospace: true,
            accent: style::raw_accent().into(),
            highlight: None,
        }
    }

    /// Visible line count — the surface's height in rows.
    #[must_use]
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = rows.max(1);
        self
    }

    #[must_use]
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size.max(1.0);
        self
    }

    #[must_use]
    pub fn monospace(mut self, monospace: bool) -> Self {
        self.monospace = monospace;
        self
    }

    /// Caret and selection tint. Defaults to the app's accent.
    #[must_use]
    pub fn accent(mut self, accent: impl Into<vocab::Color32>) -> Self {
        self.accent = accent.into();
        self
    }

    /// Style each line by splitting it into runs — the seam a syntax
    /// highlighter plugs into.
    #[must_use]
    pub fn highlight(mut self, highlight: HighlightFn<'a>) -> Self {
        self.highlight = Some(highlight);
        self
    }

    /// Render and edit `text`.
    pub fn show(self, backend: &mut dyn UiBackend, text: &mut String) -> MaraTextAreaResponse {
        let theme = style::theme();
        let line_height = self.font_size * 1.35;
        let size = vocab::Vec2::new(
            backend.available_width().max(32.0),
            line_height * self.rows as f32,
        );
        let response = backend.allocate(size, Sense::Click);
        let rect = response.rect;

        let mut state: TextEditState = backend
            .memory()
            .get_temp(self.id)
            .unwrap_or_else(|| TextEditState::at_end(text));
        state.clamp(text);

        let focused = response.clicked || response.hovered;
        let changed = if focused {
            apply_input(backend, &mut state, text)
        } else {
            false
        };

        backend.paint(PaintCmd::RectFilled {
            rect,
            corner: vocab::CornerRadius::same(3),
            fill: theme.palette.bg_panel,
        });

        paint_selection(
            backend,
            state,
            text,
            rect,
            line_height,
            self.font_size,
            self.accent,
        );
        self.paint_lines(backend, text, rect, line_height, &theme);
        paint_caret(
            backend,
            state,
            text,
            rect,
            line_height,
            self.font_size,
            self.accent,
        );

        let caret = state.line_col(text);
        backend.memory().set_temp(self.id, state);
        MaraTextAreaResponse {
            response,
            changed,
            caret,
        }
    }

    fn paint_lines(
        &self,
        backend: &mut dyn UiBackend,
        text: &str,
        rect: vocab::Rect,
        line_height: f32,
        theme: &style::Theme,
    ) {
        let family = if self.monospace {
            TextFamily::Monospace
        } else {
            TextFamily::Proportional
        };
        for (index, line) in text.split('\n').enumerate().take(self.rows) {
            let y = rect.min.y + line_height * index as f32 + line_height * 0.5;
            let runs = match self.highlight {
                Some(highlight) => highlight(line),
                None => vec![TextRun {
                    text: line.to_owned(),
                    size: self.font_size,
                    color: theme.palette.text_primary,
                    family: family.clone(),
                    extra_letter_spacing: 0.0,
                    leading_space: 0.0,
                }],
            };
            if runs.iter().all(|run| run.text.is_empty()) {
                continue;
            }
            backend.paint(PaintCmd::TextRuns {
                pos: vocab::Pos2::new(rect.min.x + 4.0, y),
                anchor: vocab::Align2::LEFT_CENTER,
                angle: 0.0,
                runs,
            });
        }
    }
}

/// Advance the width of `text` at `size`, in the surface's font.
fn advance(backend: &dyn UiBackend, text: &str, size: f32, mono: bool) -> f32 {
    backend.measure_text(text, size, mono).x
}

fn line_of(text: &str, line: usize) -> &str {
    text.split('\n').nth(line).unwrap_or("")
}

fn paint_caret(
    backend: &mut dyn UiBackend,
    state: TextEditState,
    text: &str,
    rect: vocab::Rect,
    line_height: f32,
    font_size: f32,
    accent: vocab::Color32,
) {
    let (line, column) = state.line_col(text);
    let prefix: String = line_of(text, line).chars().take(column).collect();
    let x = rect.min.x + 4.0 + advance(backend, &prefix, font_size, true);
    let top = rect.min.y + line_height * line as f32 + line_height * 0.15;
    backend.paint(PaintCmd::RectFilled {
        rect: vocab::Rect::from_min_max(
            vocab::Pos2::new(x, top),
            vocab::Pos2::new(x + 1.5, top + line_height * 0.7),
        ),
        corner: vocab::CornerRadius::ZERO,
        fill: accent,
    });
}

/// Paint the selection as one rect per covered line.
fn paint_selection(
    backend: &mut dyn UiBackend,
    state: TextEditState,
    text: &str,
    rect: vocab::Rect,
    line_height: f32,
    font_size: f32,
    accent: vocab::Color32,
) {
    if !state.has_selection() {
        return;
    }
    let (start, end) = state.selection_range();
    let fill = vocab::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 70);

    let mut char_index = 0usize;
    for (line_index, line) in text.split('\n').enumerate() {
        let line_len = line.chars().count();
        let line_start = char_index;
        let line_end = line_start + line_len;
        // `+ 1` accounts for the newline this line is followed by.
        char_index = line_end + 1;

        let from = start.max(line_start);
        let to = end.min(line_end);
        if from >= to {
            continue;
        }
        let before: String = line.chars().take(from - line_start).collect();
        let inside: String = line
            .chars()
            .skip(from - line_start)
            .take(to - from)
            .collect();
        let x0 = rect.min.x + 4.0 + advance(backend, &before, font_size, true);
        let x1 = x0 + advance(backend, &inside, font_size, true);
        let top = rect.min.y + line_height * line_index as f32 + line_height * 0.1;
        backend.paint(PaintCmd::RectFilled {
            rect: vocab::Rect::from_min_max(
                vocab::Pos2::new(x0, top),
                vocab::Pos2::new(x1, top + line_height * 0.8),
            ),
            corner: vocab::CornerRadius::ZERO,
            fill,
        });
    }
}

/// Translate this frame's keys and typed text into edits. Returns
/// whether the buffer changed.
fn apply_input(backend: &mut dyn UiBackend, state: &mut TextEditState, text: &mut String) -> bool {
    let input = backend.input();
    let extend = input.modifiers_shift;
    let word = input.modifiers_ctrl;
    let before = text.len();

    for key in input.keys_pressed.iter() {
        match key {
            MaraKey::ArrowLeft if word => state.move_word_left(extend, text),
            MaraKey::ArrowRight if word => state.move_word_right(extend, text),
            MaraKey::ArrowLeft => state.move_left(extend),
            MaraKey::ArrowRight => state.move_right(extend, text),
            MaraKey::ArrowUp => state.move_line_up(extend, text),
            MaraKey::ArrowDown => state.move_line_down(extend, text),
            MaraKey::Home => state.move_line_home(extend, text),
            MaraKey::End => state.move_line_end(extend, text),
            MaraKey::Backspace if word => state.delete_word_left(text),
            MaraKey::Backspace => state.backspace(text),
            MaraKey::Delete if word => state.delete_word_right(text),
            MaraKey::Delete => state.delete(text),
            MaraKey::Enter => state.insert_newline_with_indent(text),
            MaraKey::Tab => state.insert_str(text, "    "),
            MaraKey::A if word => state.select_all(text),
            _ => {}
        }
    }

    let typed = backend.text_typed();
    if !typed.is_empty() {
        state.insert_str(text, &typed);
    }
    text.len() != before || !typed.is_empty()
}
