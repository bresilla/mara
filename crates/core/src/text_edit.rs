//! Backend-neutral single-line text-edit model.
//!
//! This is the first slice of a Mara-owned text-edit subsystem. Today
//! the text input and command-palette query box delegate caret,
//! selection, and editing to egui's `TextEdit`. To make text editing
//! backend-agnostic, Mara needs to own that behaviour as plain data +
//! pure operations that any backend can drive from raw key/character
//! events.
//!
//! This module provides exactly the engine-independent core for a
//! single-line field:
//!
//! * [`TextEditState`] — caret + selection anchor as **char** indices
//!   into the edited `String` (char indices keep the public surface
//!   off UTF-8 byte offsets; the model converts internally for
//!   slicing).
//! * caret movement (`move_left`/`move_right`/`move_home`/`move_end`
//!   and word-wise `move_word_left`/`move_word_right`, each optionally
//!   extending the selection), `select_all`, and `clear_selection`.
//! * editing (`insert_str`, `backspace`, `delete`) that always
//!   replaces the active selection first, matching conventional
//!   single-line field behaviour.
//!
//! It is deliberately single-line (no `\n` navigation) and has no
//! clipboard/IME yet — those layer on top once a backend feeds events.
//! The egui backend will translate its key/char events into these
//! calls; a future custom backend implements the same translation and
//! reuses this logic verbatim.

/// Caret + selection state for a single-line text field, expressed in
/// char indices into the edited string.
///
/// `caret` is where new text lands; `anchor` is the other end of the
/// selection (equal to `caret` when nothing is selected).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextEditState {
    caret: usize,
    anchor: usize,
}

fn char_count(text: &str) -> usize {
    text.chars().count()
}

/// Byte offset of char index `char_idx` (clamped to `text.len()`).
fn byte_offset(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map_or(text.len(), |(b, _)| b)
}

impl TextEditState {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            caret: 0,
            anchor: 0,
        }
    }

    /// State with the caret at the end of `text` and no selection.
    #[must_use]
    pub fn at_end(text: &str) -> Self {
        let n = char_count(text);
        Self {
            caret: n,
            anchor: n,
        }
    }

    #[must_use]
    pub fn caret(self) -> usize {
        self.caret
    }

    #[must_use]
    pub fn anchor(self) -> usize {
        self.anchor
    }

    /// Selection as an ordered `(start, end)` char-index range.
    #[must_use]
    pub fn selection_range(self) -> (usize, usize) {
        (self.caret.min(self.anchor), self.caret.max(self.anchor))
    }

    #[must_use]
    pub fn has_selection(self) -> bool {
        self.caret != self.anchor
    }

    /// Currently-selected substring of `text`.
    #[must_use]
    pub fn selected_str(self, text: &str) -> String {
        let (start, end) = self.selection_range();
        let bs = byte_offset(text, start);
        let be = byte_offset(text, end);
        text[bs..be].to_owned()
    }

    /// Clamp caret/anchor into `[0, char_count(text)]` after an
    /// external edit to `text`.
    pub fn clamp(&mut self, text: &str) {
        let n = char_count(text);
        self.caret = self.caret.min(n);
        self.anchor = self.anchor.min(n);
    }

    pub fn clear_selection(&mut self) {
        self.anchor = self.caret;
    }

    pub fn select_all(&mut self, text: &str) {
        self.anchor = 0;
        self.caret = char_count(text);
    }

    /// Move the caret one char left. `extend` keeps the anchor (growing
    /// the selection); otherwise a press collapses an existing
    /// selection to its start, matching conventional field behaviour.
    pub fn move_left(&mut self, extend: bool) {
        if !extend && self.has_selection() {
            self.caret = self.selection_range().0;
        } else {
            self.caret = self.caret.saturating_sub(1);
        }
        if !extend {
            self.anchor = self.caret;
        }
    }

    /// Move the caret one char right. See [`Self::move_left`].
    pub fn move_right(&mut self, extend: bool, text: &str) {
        if !extend && self.has_selection() {
            self.caret = self.selection_range().1;
        } else {
            self.caret = (self.caret + 1).min(char_count(text));
        }
        if !extend {
            self.anchor = self.caret;
        }
    }

    pub fn move_home(&mut self, extend: bool) {
        self.caret = 0;
        if !extend {
            self.anchor = self.caret;
        }
    }

    pub fn move_end(&mut self, extend: bool, text: &str) {
        self.caret = char_count(text);
        if !extend {
            self.anchor = self.caret;
        }
    }

    /// Replace the active selection (or nothing) with `s` and place the
    /// caret after the inserted text.
    pub fn insert_str(&mut self, text: &mut String, s: &str) {
        let (start, end) = self.selection_range();
        let bs = byte_offset(text, start);
        let be = byte_offset(text, end);
        text.replace_range(bs..be, s);
        self.caret = start + char_count(s);
        self.anchor = self.caret;
    }

    /// Delete the selection if any, else the char before the caret.
    pub fn backspace(&mut self, text: &mut String) {
        if self.has_selection() {
            self.delete_selection(text);
        } else if self.caret > 0 {
            let bs = byte_offset(text, self.caret - 1);
            let be = byte_offset(text, self.caret);
            text.replace_range(bs..be, "");
            self.caret -= 1;
            self.anchor = self.caret;
        }
    }

    /// Delete the selection if any, else the char after the caret.
    pub fn delete(&mut self, text: &mut String) {
        if self.has_selection() {
            self.delete_selection(text);
        } else if self.caret < char_count(text) {
            let bs = byte_offset(text, self.caret);
            let be = byte_offset(text, self.caret + 1);
            text.replace_range(bs..be, "");
        }
    }

    fn delete_selection(&mut self, text: &mut String) {
        let (start, end) = self.selection_range();
        let bs = byte_offset(text, start);
        let be = byte_offset(text, end);
        text.replace_range(bs..be, "");
        self.caret = start;
        self.anchor = start;
    }

    /// Move the caret to the previous word boundary (Ctrl+Left).
    pub fn move_word_left(&mut self, extend: bool, text: &str) {
        let chars = chars_vec(text);
        self.caret = prev_word_boundary(&chars, self.caret);
        if !extend {
            self.anchor = self.caret;
        }
    }

    /// Move the caret to the next word boundary (Ctrl+Right).
    pub fn move_word_right(&mut self, extend: bool, text: &str) {
        let chars = chars_vec(text);
        self.caret = next_word_boundary(&chars, self.caret);
        if !extend {
            self.anchor = self.caret;
        }
    }

    /// Delete the selection if any, else from the previous word
    /// boundary up to the caret (Ctrl+Backspace).
    pub fn delete_word_left(&mut self, text: &mut String) {
        if self.has_selection() {
            self.delete_selection(text);
            return;
        }
        let chars = chars_vec(text);
        let start = prev_word_boundary(&chars, self.caret);
        if start < self.caret {
            let bs = byte_offset(text, start);
            let be = byte_offset(text, self.caret);
            text.replace_range(bs..be, "");
            self.caret = start;
            self.anchor = start;
        }
    }

    /// Delete the selection if any, else from the caret up to the next
    /// word boundary (Ctrl+Delete).
    pub fn delete_word_right(&mut self, text: &mut String) {
        if self.has_selection() {
            self.delete_selection(text);
            return;
        }
        let chars = chars_vec(text);
        let end = next_word_boundary(&chars, self.caret);
        if end > self.caret {
            let bs = byte_offset(text, self.caret);
            let be = byte_offset(text, end);
            text.replace_range(bs..be, "");
        }
    }
}

fn chars_vec(text: &str) -> Vec<char> {
    text.chars().collect()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Previous word boundary at or before `pos`: skip trailing non-word
/// chars, then skip the word run.
fn prev_word_boundary(chars: &[char], pos: usize) -> usize {
    let mut i = pos.min(chars.len());
    while i > 0 && !is_word_char(chars[i - 1]) {
        i -= 1;
    }
    while i > 0 && is_word_char(chars[i - 1]) {
        i -= 1;
    }
    i
}

/// Next word boundary at or after `pos`: skip leading non-word chars,
/// then skip the word run.
fn next_word_boundary(chars: &[char], pos: usize) -> usize {
    let n = chars.len();
    let mut i = pos.min(n);
    while i < n && !is_word_char(chars[i]) {
        i += 1;
    }
    while i < n && is_word_char(chars[i]) {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_end_and_basic_accessors() {
        let s = TextEditState::at_end("héllo");
        assert_eq!(s.caret(), 5);
        assert_eq!(s.anchor(), 5);
        assert!(!s.has_selection());
    }

    #[test]
    fn insert_replaces_selection_and_advances_caret() {
        let mut text = "abcd".to_owned();
        let mut s = TextEditState::new();
        s.move_right(false, &text); // caret 1
        s.move_right(true, &text); // select char index 1..2 ("b")
        assert!(s.has_selection());
        assert_eq!(s.selected_str(&text), "b");
        s.insert_str(&mut text, "XY");
        assert_eq!(text, "aXYcd");
        assert_eq!(s.caret(), 3);
        assert!(!s.has_selection());
    }

    #[test]
    fn insert_at_caret_without_selection() {
        let mut text = "ad".to_owned();
        let mut s = TextEditState::new();
        s.move_right(false, &text); // caret 1
        s.insert_str(&mut text, "bc");
        assert_eq!(text, "abcd");
        assert_eq!(s.caret(), 3);
    }

    #[test]
    fn backspace_without_then_with_selection() {
        let mut text = "abc".to_owned();
        let mut s = TextEditState::at_end(&text); // caret 3
        s.backspace(&mut text);
        assert_eq!(text, "ab");
        assert_eq!(s.caret(), 2);

        s.move_home(true); // select "ab"
        assert!(s.has_selection());
        s.backspace(&mut text);
        assert_eq!(text, "");
        assert_eq!(s.caret(), 0);
        assert!(!s.has_selection());
    }

    #[test]
    fn delete_forward_without_then_with_selection() {
        let mut text = "abc".to_owned();
        let mut s = TextEditState::new(); // caret 0
        s.delete(&mut text);
        assert_eq!(text, "bc");
        assert_eq!(s.caret(), 0);

        s.select_all(&text);
        s.delete(&mut text);
        assert_eq!(text, "");
        assert_eq!(s.caret(), 0);
    }

    #[test]
    fn delete_at_end_is_noop() {
        let mut text = "ab".to_owned();
        let mut s = TextEditState::at_end(&text);
        s.delete(&mut text);
        assert_eq!(text, "ab");
        assert_eq!(s.caret(), 2);
    }

    #[test]
    fn move_left_collapses_selection_to_start() {
        let text = "abcd".to_owned();
        let mut s = TextEditState::new();
        s.select_all(&text); // anchor 0, caret 4
        s.move_left(false);
        assert_eq!(s.caret(), 0);
        assert!(!s.has_selection());
    }

    #[test]
    fn move_right_collapses_selection_to_end() {
        let text = "abcd".to_owned();
        let mut s = TextEditState::new();
        s.select_all(&text); // anchor 0, caret 4
        // caret already at end; collapsing keeps it at selection end.
        s.move_right(false, &text);
        assert_eq!(s.caret(), 4);
        assert!(!s.has_selection());
    }

    #[test]
    fn extend_selection_with_shift_movement() {
        let text = "abcd".to_owned();
        let mut s = TextEditState::new();
        s.move_right(true, &text);
        s.move_right(true, &text);
        assert_eq!(s.selection_range(), (0, 2));
        assert_eq!(s.selected_str(&text), "ab");
    }

    #[test]
    fn movement_clamps_at_bounds() {
        let text = "ab".to_owned();
        let mut s = TextEditState::new();
        s.move_left(false); // already at 0
        assert_eq!(s.caret(), 0);
        s.move_end(false, &text);
        s.move_right(false, &text); // already at end
        assert_eq!(s.caret(), 2);
    }

    #[test]
    fn multibyte_editing_stays_on_char_boundaries() {
        // "café" — 'é' is 2 bytes; char indices must not split it.
        let mut text = "café".to_owned();
        let mut s = TextEditState::at_end(&text); // caret 4 (chars)
        s.backspace(&mut text); // remove 'é'
        assert_eq!(text, "caf");
        assert_eq!(s.caret(), 3);

        let mut text2 = "αβγ".to_owned();
        let mut s2 = TextEditState::new();
        s2.move_right(false, &text2); // caret 1 (after α)
        s2.insert_str(&mut text2, "x");
        assert_eq!(text2, "αxβγ");
        assert_eq!(s2.caret(), 2);
    }

    #[test]
    fn clamp_after_external_shrink() {
        let mut s = TextEditState::at_end("abcdef"); // caret 6
        let text = "ab".to_owned();
        s.clamp(&text);
        assert_eq!(s.caret(), 2);
        assert_eq!(s.anchor(), 2);
    }

    #[test]
    fn move_word_left_skips_to_word_start() {
        let text = "foo bar baz".to_owned();
        let mut s = TextEditState::at_end(&text); // caret 11
        s.move_word_left(false, &text);
        assert_eq!(s.caret(), 8); // start of "baz"
        s.move_word_left(false, &text);
        assert_eq!(s.caret(), 4); // start of "bar"
    }

    #[test]
    fn move_word_right_skips_to_word_end() {
        let text = "foo bar baz".to_owned();
        let mut s = TextEditState::new(); // caret 0
        s.move_word_right(false, &text);
        assert_eq!(s.caret(), 3); // end of "foo"
        s.move_word_right(false, &text);
        assert_eq!(s.caret(), 7); // end of "bar"
    }

    #[test]
    fn move_word_extends_selection() {
        let text = "foo bar".to_owned();
        let mut s = TextEditState::new();
        s.move_word_right(true, &text);
        assert_eq!(s.selection_range(), (0, 3));
        assert_eq!(s.selected_str(&text), "foo");
    }

    #[test]
    fn delete_word_left_removes_previous_word() {
        let mut text = "foo bar baz".to_owned();
        let mut s = TextEditState::at_end(&text);
        s.delete_word_left(&mut text);
        assert_eq!(text, "foo bar ");
        assert_eq!(s.caret(), 8);
    }

    #[test]
    fn delete_word_right_removes_next_word() {
        let mut text = "foo bar baz".to_owned();
        let mut s = TextEditState::new();
        s.delete_word_right(&mut text);
        assert_eq!(text, " bar baz");
        assert_eq!(s.caret(), 0);
    }

    #[test]
    fn delete_word_with_selection_deletes_selection() {
        let mut text = "foo bar baz".to_owned();
        let mut s = TextEditState::new();
        s.select_all(&text);
        s.delete_word_left(&mut text);
        assert_eq!(text, "");
        assert_eq!(s.caret(), 0);
    }

    #[test]
    fn word_movement_is_utf8_safe() {
        let text = "αα ββ".to_owned();
        let mut s = TextEditState::at_end(&text); // 5 chars
        s.move_word_left(false, &text);
        assert_eq!(s.caret(), 3); // start of "ββ"
        // Caret is at index 3 (start of "ββ"); deleting the previous
        // word removes "αα " (indices 0..3), leaving "ββ".
        let mut text2 = text.clone();
        s.delete_word_left(&mut text2);
        assert_eq!(text2, "ββ");
    }
}
