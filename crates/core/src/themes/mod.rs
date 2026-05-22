//! Built-in themes for the mara UI kit.
//!
//! Each theme is a single data file under
//! `crates/core/src/themes/` that exports
//! `pub const fn theme_<name>(mode: Mode) -> Theme`. The
//! [`crate::style::Theme`] struct (defined in `style.rs`) is the
//! engine — fields, helpers, global state, the de-dup cache, theme
//! apply. This module is the *catalogue*.
//!
//! Theme files are not renderer extensions. They should construct a
//! complete [`Theme`](crate::style::Theme) value and nothing more:
//! no paint closures, no alternate widget functions, and no hidden
//! control flow. When a new visual decision is needed, add a typed
//! field to the theme contract, then keep pane/container/widget code
//! generic over that field.
//!
//! The current contract is intentionally nested:
//!
//! - `palette`, `stroke`, `glass`, `shape`, `text`, `motion`
//! - `pane`, `ribbon`, `container`, `tabs`, `pod`, `widgets`
//! - `graph`, `code`, `overlay`
//!
//! Older flat fields still exist as compatibility shims while
//! call-sites migrate, but new code should prefer the nested groups.
//!
//! # Adding a new theme
//!
//! 1. **Copy** an existing theme file as a template. Use [`flat`] if
//!    you want the smallest proof theme, or [`pro`] if you want the
//!    canonical baseline. Every field has a doc comment on
//!    [`Theme`](crate::style::Theme) or its nested structs explaining
//!    what it does and what range it expects:
//!
//!    ```ignore
//!    // crates/core/src/themes/neon.rs
//!    use crate::style::{Mode, Theme, ThemeId};
//!    use super::pro::theme_pro;
//!
//!    pub const fn theme_neon(mode: Mode) -> Theme {
//!        let dark = matches!(mode, Mode::Dark);
//!        Theme {
//!            id: ThemeId {
//!                family: "NEON",
//!                variant: if dark { "DARK" } else { "LIGHT" },
//!            },
//!            name: if matches!(mode, Mode::Dark) { "NEON_DARK" }
//!                  else { "NEON_LIGHT" },
//!            // Override nested groups, for example:
//!            // tabs: TabTheme { ... },
//!            // widgets: WidgetTheme { ... },
//!            // graph: GraphTheme { ... },
//!            ..theme_pro(mode)
//!        }
//!    }
//!    ```
//!
//!    The `..theme_pro(mode)` tail is acceptable for variants that
//!    intentionally inherit PRO defaults. For a fully isolated theme,
//!    copy [`flat::theme_flat`] or [`pro::theme_pro`] and spell out
//!    every nested group explicitly so there is no accidental visual
//!    inheritance.
//!
//! 2. **Register** the file by adding one line at the bottom of
//!    this module:
//!
//!    ```ignore
//!    pub mod neon;
//!    pub use neon::theme_neon;
//!    ```
//!
//! 3. **Activate** at runtime with the same API every other theme
//!    uses:
//!
//!    ```ignore
//!    mara_core::style::set_theme(theme_neon(Mode::Dark));
//!    ```
//!
//!    The de-dup cache in `apply_theme` keys on `Theme::name`, so
//!    make sure each Mode variant returns a unique `name` string.
//!
//! # What `name` does
//!
//! `Theme::name` is the de-dup key. `apply_theme` early-returns when
//! `(name, glass_opacity, accent)` matches the cached tuple — without
//! a unique `name` for each `Mode` variant, switching from `Dark` to
//! `Light` of the SAME theme would be a silent no-op. Convention:
//! suffix with `_DARK` / `_LIGHT` (or your own mode tokens).
//!
//! Rendering code must not branch on `Theme::name`, `ThemeId::family`,
//! `"PRO"`, `"GAME"`, or any other preset identity. Identity is for
//! selectors, diagnostics, and caching only. If code wants a visual
//! dialect, use a typed field:
//!
//! ```ignore
//! match style::theme().tabs.layout {
//!     TabLayout::FolderSideStrip => { /* generic folder-tab renderer */ }
//!     TabLayout::TitleRowSegmented => { /* generic title-row renderer */ }
//! }
//! ```
//!
//! # Theme Authoring Rules
//!
//! - Keep the `Pane -> PaneBody -> Container -> Body -> Pod` hierarchy.
//! - Put visual/layout dialect choices in nested theme structs.
//! - Do not add theme-name checks in renderers.
//! - Do not put custom renderer closures in theme files.
//! - Keep domain/content icons caller-provided, but put chrome icon
//!   treatment in the theme contract when it varies by look.
//! - Add a guardrail test whenever a new category of theme leak is
//!   removed.

pub mod flat;
pub mod game;
pub mod pro;

pub use flat::{
    FLAT_DARK_BG_HOVER, FLAT_DARK_BG_INPUT, FLAT_DARK_BG_PANEL, FLAT_DARK_BG_RAISED,
    FLAT_DARK_BG_WINDOW, FLAT_LIGHT_BG_HOVER, FLAT_LIGHT_BG_INPUT, FLAT_LIGHT_BG_PANEL,
    FLAT_LIGHT_BG_RAISED, FLAT_LIGHT_BG_WINDOW, theme_flat,
};
pub use game::{
    GAME_LIGHT_BG_HOVER, GAME_LIGHT_BG_INPUT, GAME_LIGHT_BG_PANEL, GAME_LIGHT_BG_RAISED,
    GAME_LIGHT_BG_WINDOW, theme_game,
};
pub use pro::{
    PRO_LIGHT_BG_HOVER, PRO_LIGHT_BG_INPUT, PRO_LIGHT_BG_PANEL, PRO_LIGHT_BG_RAISED,
    PRO_LIGHT_BG_WINDOW, PRO_LIGHT_BORDER_INNER, PRO_LIGHT_BORDER_SUBTLE, theme_pro,
};
