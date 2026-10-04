//! Gruvbox — the one built-in theme kept from upstream's palette
//! pack. Useful as a reference when you author new themes
//! against the new `Color32` fields.
//!
//! Original palette by Jakub Bartodziej
//! <kubabartodziej@gmail.com>, Pavel Pertsev (Gruvbox).

use crate::CodeColor as Color32;

use super::ColorTheme;

impl ColorTheme {
    /// Gruvbox Dark — the classic dark palette. Fully opaque
    /// background; if you want transparency, mutate `.bg` to a
    /// `Color32::from_rgba_unmultiplied(..., alpha)` or build a
    /// new theme.
    pub const GRUVBOX: ColorTheme = ColorTheme {
        name: "Gruvbox",
        dark: true,
        bg: Color32::from_rgb(0x28, 0x28, 0x28),
        cursor: Color32::from_rgb(0xa8, 0x99, 0x84), // fg4
        selection: Color32::from_rgb(0x50, 0x49, 0x45), // bg2
        comments: Color32::from_rgb(0x92, 0x83, 0x74), // gray1
        functions: Color32::from_rgb(0xb8, 0xbb, 0x26), // green1
        keywords: Color32::from_rgb(0xfb, 0x49, 0x34), // red1
        literals: Color32::from_rgb(0xeb, 0xdb, 0xb2), // fg1
        numerics: Color32::from_rgb(0xd3, 0x86, 0x9b), // purple1
        punctuation: Color32::from_rgb(0xfe, 0x80, 0x19), // orange1
        strs: Color32::from_rgb(0x8e, 0xc0, 0x7c),   // aqua1
        types: Color32::from_rgb(0xfa, 0xbd, 0x2f),  // yellow1
        special: Color32::from_rgb(0x83, 0xa5, 0x98), // blue1
    };

    pub const GRUVBOX_DARK: ColorTheme = ColorTheme::GRUVBOX;
}
