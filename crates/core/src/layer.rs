//! Numeric tier system for paint z-ordering. Tier numbers run
//! `1..=1000`. Higher tier ⇒ paints on top.
//!
//! Tier-to-egui-`Order` mapping:
//!
//! | tier      | egui `Order`        |
//! | --------- | ------------------- |
//! | 1–40      | `Order::Middle`     |
//! | 41–70     | `Order::Foreground` |
//! | 71–95     | `Order::Tooltip`    |
//! | 96–1000   | `Order::Debug`      |
//!
//! Within a single egui `Order`, layers paint in egui's internal
//! registration order — typically the order they're first
//! encountered in the frame. The tier number is also folded into
//! each layer's id so different tiers always resolve to DIFFERENT
//! sublayers, never collide.
//!
//! ## Reserved tier ranges
//!
//! * `1..=100` — built-in pane / container chrome stack (this
//!   crate). See [`z`] for named constants.
//! * `101..=997` — RESERVED for future built-in widgets and
//!   third-party extensions.
//! * `998` — INSPECTOR overlay (F10 debug paint).
//! * `999..=1000` — RESERVED for future top-of-stack overlays.

use std::hash::Hash;

use egui::{Id, LayerId, Order};

/// Pre-defined tiers used by the pane / container / chrome stack.
/// Callers can pass any `u16` directly; these are just the
/// canonical values the built-in widgets use, exposed so external
/// code can place itself in the same z-stack without picking
/// conflicting numbers.
pub mod z {
    /// Pane background fill (panel surface). Lowest paintable tier.
    pub const PANE_BG: u16 = 5;
    /// Pane body content — pods, widgets, body text. The default
    /// tier when nothing custom is requested.
    pub const PANE_CONTENT: u16 = 15;
    /// Container chrome — borders, separators, drag handles, etc.
    pub const CONTAINER_CHROME: u16 = 25;
    /// Container corner ticks (GAME-theme L-brackets at corners).
    /// Sits ABOVE container chrome so tab-cell fills don't cover it.
    pub const CONTAINER_TICKS: u16 = 30;
    /// GAME-theme floating section icon — overflows the title strip
    /// but stays within the container's z-range.
    pub const CONTAINER_FLOATING_ICON: u16 = 35;
    /// Pane-level overlays — drag-reorder ghost rect, dragged
    /// container preview, etc.
    pub const PANE_OVERLAY: u16 = 38;

    /// Fullscreen / maximized container overlay (eclipses every
    /// other pane content).
    pub const FULLSCREEN: u16 = 50;
    /// Fullscreen overlay's own chrome (close-chip, header).
    pub const FULLSCREEN_UI: u16 = 55;

    /// Command palette / global pickers — above fullscreen.
    pub const COMMAND_PALETTE: u16 = 75;
    /// Tooltips, hover popups.
    pub const TOOLTIP: u16 = 85;

    /// F10 debug inspector — sits above everything except tiers
    /// 999–1000 reserved for future top-of-stack overlays.
    pub const INSPECTOR: u16 = 998;
    /// Native window resize/move affordances. These must stay above
    /// every Mara view because they describe host-level interaction
    /// zones, not normal app content.
    pub const WINDOW_CHROME: u16 = 1000;
}

/// Map a tier number to its egui [`Order`] tier. Tier `0` reads
/// as 1; tiers above 1000 clamp to 1000 (Debug).
#[inline]
pub(crate) fn order_for(tier: u16) -> Order {
    match tier {
        0..=40 => Order::Middle,
        41..=70 => Order::Foreground,
        71..=95 => Order::Tooltip,
        _ => Order::Debug,
    }
}

/// Build a [`LayerId`] for the given tier, salted with `salt`. The
/// tier number is folded into the id so two callers at different
/// tiers always resolve to distinct sublayers even if they share
/// `salt`.
pub(crate) fn layer_id(salt: impl Hash, tier: u16) -> LayerId {
    LayerId::new(order_for(tier), Id::new(("mara_z", tier, salt)))
}
