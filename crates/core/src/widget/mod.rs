//! Mara-styled widgets that go inside a
//! [`crate::container::Normal`] body — leaf nodes that paint a
//! single primitive (text input, button, slider, …).
//!
//! Inter-pod separators are NOT widgets — they're container-level
//! chrome. See [`crate::container::SeparatorStyle`].

pub mod badge;
pub mod button;
pub mod chip;
pub mod color;
pub mod color_picker;
pub(crate) mod context_menu;
pub mod drag_value;
pub mod dropdown;
pub mod foldable;
pub mod keybinding;
pub mod label;
pub mod progressbar;
pub mod readout;
pub mod select;
pub mod slider;
pub mod text_area;
pub mod text_input;
pub mod toggle;
pub mod tree;

pub use badge::{BADGE_LABEL_COL_W, BADGE_ROW_H};
pub use button::{
    ActionButton, ActionButtonResponse, BUTTON_ACTION_GAP, BUTTON_ACTION_W, BUTTON_LABEL_FONT,
    BUTTON_ROW_H, BUTTON_ROW_H_SUBTITLE, Button, CARD_BUTTON_ROW_H, FillStyle,
};
pub use chip::CHIP_H;
pub use color::COLOR_SWATCH_H;
pub use dropdown::DROPDOWN_ROW_H;
pub use keybinding::KEYBINDING_ROW_H;
pub use label::{LABEL_FONT, LABEL_ROW_H};
pub use progressbar::PROGRESSBAR_ROW_H;
pub use readout::READOUT_ROW_H;
pub use select::{HYBRID_SELECT_ROW_H, HybridSelectResponse, SELECT_ROW_H};
pub use slider::SLIDER_ROW_H;
pub use toggle::TOGGLE_ROW_H;
pub use tree::{
    TREE_ACTION_GAP, TREE_ACTION_ROW_H, TREE_ACTION_W, TREE_INDENT, TREE_ROW_H,
    TreeActionRowResponse, TreeBody, TreeBranchGuide, TreeIconKind, TreeIconSlot, TreeRowResponse,
};
