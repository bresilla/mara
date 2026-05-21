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
pub mod context_menu;
pub mod drag_value;
pub mod dropdown;
pub mod foldable;
pub mod keybinding;
pub mod legacy;
pub mod progressbar;
pub mod readout;
pub mod select;
pub mod slider;
pub mod text_input;
pub mod toggle;
pub mod tree;

pub use badge::{BADGE_LABEL_COL_W, BADGE_ROW_H, badge_row, badge_row_colored};
pub use button::{
    ActionButton, ActionButtonResponse, BUTTON_ACTION_GAP, BUTTON_ACTION_W, BUTTON_LABEL_FONT,
    BUTTON_ROW_H, BUTTON_ROW_H_SUBTITLE, Button, CARD_BUTTON_ROW_H, FillStyle, button, button_h,
    card_action_button, card_button,
};
pub use chip::{CHIP_H, chip, chip_colored};
pub use color::{COLOR_SWATCH_H, color_rgb, color_rgba};
pub use context_menu::context_menu_mara;
pub use drag_value::{axis_drag, axis_drag_h, drag_value, drag_value_h};
pub use dropdown::{DROPDOWN_ROW_H, dropdown, dropdown_h};
pub use foldable::section;
pub use keybinding::{KEYBINDING_ROW_H, keybinding_row, keybinding_row_h};
pub use legacy::{
    LABEL_COL_WIDTH, dropdown_control, key_chip, labelled_row, labelled_row_custom_left,
    pretty_slider, readout_row, row_separator, search_field, sub_caption, wide_button,
};
pub use progressbar::{progressbar, progressbar_h};
pub use readout::{READOUT_ROW_H, readout, readout_h};
pub use select::{
    HYBRID_SELECT_ROW_H, HybridSelectResponse, SELECT_ROW_H, hybrid_select_row,
    hybrid_select_row_h, select_row, select_row_h,
};
pub use slider::{slider, slider_h};
pub use text_input::{text_input, text_input_h};
pub use toggle::{toggle, toggle_h, toggle_track_only};
pub use tree::{
    TREE_ACTION_GAP, TREE_ACTION_ROW_H, TREE_ACTION_W, TREE_INDENT, TREE_ROW_H,
    TreeActionRowResponse, TreeBody, TreeBranchGuide, TreeIconKind, TreeIconSlot, TreeRowResponse,
    tree_action_row, tree_action_row_with_guide, tree_row,
};
