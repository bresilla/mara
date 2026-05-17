//! Code-editor integration — thin wrapper around
//! [`egui_code_editor`] that pipes a multiline text buffer
//! through the same maximise / restore affordance the graph
//! widget uses.
//!
//! Minimal usage (inside a [`section`](crate::widgets::section)
//! body, since panes require containers):
//!
//! ```ignore
//! mara_code_editor(
//!     ui,
//!     "my_code",
//!     &mut state.code,
//!     Syntax::rust(),
//!     accent,
//!     egui::vec2(w, 300.0),
//! );
//! ```
//!
//! The widget paints:
//!
//! * Line numbers in the gutter.
//! * Monospace text with syntax highlighting for the chosen
//!   [`Syntax`] (Rust, shell, SQL, ASM, or custom).
//! * The maximise / restore chip in the top-left corner — click
//!   once to blow the editor up to full window, click again to
//!   snap it back inline.
//!
//! Re-exports: `Syntax`, `ColorTheme`, `CodeEditor` from
//! `egui_code_editor` so callers don't need a second dep.

use std::hash::Hash;

use egui;

pub use mara_code::{CodeEditor, ColorTheme, Syntax};

// `maximizable` is no longer called directly from this file — both
// `mara_code_editor` and `mara_code_editor_with_opts` route
// through `crate::embed::maximizable_with_opts` so the opts path
// is always live. `pub use` re-exports the symbol callers expect
// when migrating from the older signature.
pub use crate::embed::OverlayOpts;

/// Render a syntax-highlighted code editor bound to `text`,
/// wrapped in the shared maximise / restore toggle. The caller
/// owns the text buffer — the widget just edits it in place.
///
/// `syntax` controls keyword / punctuation / literal highlighting.
/// Pre-built variants: `Syntax::rust()`, `Syntax::shell()`,
/// `Syntax::sql()`, `Syntax::asm()`. Build a custom one with the
/// `Syntax` struct fields directly for other languages.
pub fn mara_code_editor(
    ui: &mut egui::Ui,
    id_salt: impl Hash + Copy,
    text: &mut String,
    syntax: Syntax,
    accent: egui::Color32,
    min_size: egui::Vec2,
) {
    mara_code_editor_with_opts(
        ui,
        id_salt,
        text,
        syntax,
        accent,
        min_size,
        crate::embed::OverlayOpts::default(),
    )
}

/// The maximise-state key the code-editor wrapper registers with
/// [`crate::embed`], computed from the caller-supplied `id_salt`
/// (the same one passed to [`mara_code_editor`]). Compare against
/// [`crate::embed::fullscreen_owner`] to detect "is THIS code
/// editor the one currently in fullscreen?".
#[must_use]
pub fn code_fullscreen_key(id_salt: impl Hash) -> egui::Id {
    crate::embed::maximize_state_key(id_salt)
}

/// `true` while the code editor identified by `id_salt` is
/// currently in its fullscreen overlay. Shorthand for
/// `fullscreen_owner(ctx) == Some(code_fullscreen_key(id_salt))`.
#[must_use]
pub fn is_code_fullscreen(ctx: &egui::Context, id_salt: impl Hash) -> bool {
    crate::embed::fullscreen_owner(ctx) == Some(code_fullscreen_key(id_salt))
}

/// Same as [`mara_code_editor`] but accepts an [`OverlayOpts`] so
/// the caller can choose where the minimize chip lands on the
/// fullscreen overlay (which edge + which cluster along that edge).
pub fn mara_code_editor_with_opts(
    ui: &mut egui::Ui,
    id_salt: impl Hash + Copy,
    text: &mut String,
    syntax: Syntax,
    accent: egui::Color32,
    min_size: egui::Vec2,
    fs_opts: crate::embed::OverlayOpts,
) {
    crate::embed::maximizable_with_opts(ui, id_salt, accent, min_size, fs_opts, |ui| {
        let id = format!("mara_code_editor_{:?}", ui.id());
        let code = crate::style::theme().code;
        let line_h = code.font_size * code.line_height_factor;
        let rows = ((ui.available_height() / line_h).floor() as usize).max(code.min_rows);
        CodeEditor::default()
            .id_source(id)
            .with_syntax(syntax)
            .with_theme(mara_code_theme(accent))
            .with_fontsize(code.font_size)
            .with_rows(rows)
            .with_numlines(true)
            .show(ui, text);
    });
}

/// Build a [`ColorTheme`] whose background / text / selection
/// colours come from the mara palette, while the syntactic
/// colours reuse the existing accent / status hues — so the
/// editor belongs to the same visual family as sections and
/// widgets around it.
///
/// Now that [`ColorTheme`] stores [`Color32`] directly (the
/// vendored struct was rewritten from `&'static str` hex), the
/// background uses the same `glass_fill` recipe as the node-graph
/// canvas and the floating-pane frame — so the global
/// `GlassOpacity` slider dims the code editor in lockstep with
/// every other mara surface.
///
/// `accent` drives keyword highlighting + the cursor; status
/// colours (`SUCCESS`, `AXIS_X/Y/Z`) tint literals / types /
/// punctuation for a readable hierarchy.
fn mara_code_theme(accent: egui::Color32) -> ColorTheme {
    use crate::style::{accent_pressed, glass_alpha_window, glass_fill, on_panel_dim, pane_fill};
    let code = crate::style::theme().code;
    ColorTheme {
        name: "Mara",
        dark: code.force_dark,
        // `glass_fill(pane_fill(...), …)` flows through the active
        // theme so GAME's accent panel becomes the editor bg too,
        // not a hardcoded dark.
        bg: glass_fill(pane_fill(accent), accent, glass_alpha_window()),
        cursor: accent,
        // Selection = darker accent shade derived at runtime so it
        // tracks whatever colour the user picked.
        selection: accent_pressed(),
        // `comments` / `punctuation` flip to whatever contrasts the
        // pane fill, so they stay readable on PRO's dark and GAME's
        // accent-coloured panels alike.
        comments: on_panel_dim(),
        functions: code.functions,
        keywords: accent,
        literals: code.literals,
        numerics: code.numerics,
        punctuation: on_panel_dim(),
        strs: code.strings,
        types: code.types,
        special: accent,
    }
}

// ─── Typed Pod constructor ──────────────────────────────────────────
//
// Adds `Pod::with_code_editor(text_id, syntax, default_text)` so
// pane bodies can host a code editor through the canonical pod path
// instead of reaching into `Pod::with_custom_units` (which is the
// raw-egui escape hatch). The text buffer is stashed in egui ctx
// data under `text_id`; the editor reads / writes it each frame.

impl crate::pod::Pod {
    /// Append a mara-themed code editor to this pod. The editor's
    /// text lives in egui ctx data under `text_id` — pre-seed it
    /// (`ctx.data_mut(|d| d.insert_temp(text_id, "default".to_string()))`)
    /// or rely on `default_text` to seed on first render.
    ///
    /// Uses `mara_core::style::active_accent()` for the inline
    /// theme. The maximise / restore chip in the editor's top-left
    /// corner toggles fullscreen via `mara_core::embed`.
    ///
    /// Reserves 10 row-height units of pod space.
    #[must_use]
    pub fn with_code_editor(
        self,
        text_id: egui::Id,
        syntax: Syntax,
        default_text: impl Into<String>,
    ) -> Self {
        self.with_code_editor_opts(
            text_id,
            syntax,
            default_text,
            OverlayOpts::default().avoid_ribbons(crate::RibbonAvoidance::all()),
        )
    }

    /// Append a mara-themed code editor with explicit fullscreen
    /// overlay options. The fullscreen background stays full-window;
    /// these options only affect the body/chip.
    #[must_use]
    pub fn with_code_editor_opts(
        self,
        text_id: egui::Id,
        syntax: Syntax,
        default_text: impl Into<String>,
        fs_opts: OverlayOpts,
    ) -> Self {
        let default = default_text.into();
        self.with_custom_units(10, move |ui| {
            let mut text: String = ui
                .ctx()
                .data(|d| d.get_temp::<String>(text_id))
                .unwrap_or_else(|| default.clone());
            let avail = ui.available_size_before_wrap();
            let accent = crate::style::active_accent();
            mara_code_editor_with_opts(
                ui,
                text_id,
                &mut text,
                syntax.clone(),
                accent,
                avail,
                fs_opts,
            );
            ui.ctx().data_mut(|d| d.insert_temp(text_id, text));
        })
    }
}

// ─── View + Module bridge ──────────────────────────────────────────
//
// This keeps the existing standalone editor wrapper intact while also
// exposing a PLAN.md-native surface: code can now be routed as a
// top-level L0 view or embedded as an L1-capable module.

/// A retained code editor surface that implements both [`crate::MaraView`]
/// and [`crate::MaraModule`].
#[derive(Clone, Debug)]
pub struct CodeEditorSurface {
    id: egui::Id,
    title: String,
    text: String,
    syntax: Syntax,
    units: usize,
}

impl CodeEditorSurface {
    #[must_use]
    pub fn new(
        id: impl Hash,
        title: impl Into<String>,
        text: impl Into<String>,
        syntax: Syntax,
    ) -> Self {
        Self {
            id: egui::Id::new(id),
            title: title.into(),
            text: text.into(),
            syntax,
            units: 12,
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn text_mut(&mut self) -> &mut String {
        &mut self.text
    }

    #[must_use]
    pub fn with_units(mut self, units: usize) -> Self {
        self.units = units.max(1);
        self
    }

    fn toolbar(&self, scope: crate::RibbonScope) -> crate::RibbonSlotDef {
        let format = crate::RibbonSlotItem::new(
            egui::Id::new(("code.format", self.id)),
            "code",
            "Format",
            "Format code document",
            crate::RibbonAction::Command(egui::Id::new(("code.format.command", self.id))),
        );
        crate::RibbonSlotDef::new(
            egui::Id::new(("code.ribbon", self.id)),
            scope,
            crate::RibbonEdge::Top,
            crate::RibbonCluster::Middle,
            vec![crate::RibbonSlot::new(
                crate::RibbonSlotId::new(("code.format.slot", self.id)),
                Some(format),
                crate::RibbonOverridePolicy::Fixed,
            )],
        )
    }

    fn show_editor(&mut self, ui: &mut egui::Ui) {
        let min_size = ui.available_size_before_wrap();
        let content_avoidance = crate::module::MaraModule::fullscreen_content_avoidance(self);
        mara_code_editor_with_opts(
            ui,
            self.id,
            &mut self.text,
            self.syntax.clone(),
            crate::style::active_accent(),
            min_size,
            OverlayOpts::default().avoid_ribbons(content_avoidance),
        );
    }
}

impl crate::MaraView for CodeEditorSurface {
    fn id(&self) -> crate::ViewId {
        crate::ViewId(self.id)
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn icon(&self) -> &'static str {
        "code"
    }

    fn ribbons(&mut self) -> Vec<crate::RibbonSlotDef> {
        vec![self.toolbar(crate::RibbonScope::View(crate::ViewId(self.id)))]
    }

    fn content_avoidance(&self) -> crate::RibbonAvoidance {
        crate::RibbonAvoidance::all()
    }

    fn show(&mut self, ctx: &mut crate::ViewCtx<'_>) {
        let rect = ctx.content_rect();
        egui::CentralPanel::default().show(ctx.egui_ctx, |ui| {
            let mut body = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            self.show_editor(&mut body);
        });
    }
}

impl crate::MaraModule for CodeEditorSurface {
    fn id(&self) -> egui::Id {
        self.id
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn icon(&self) -> &'static str {
        "code"
    }

    fn fullscreen_content_avoidance(&self) -> crate::RibbonAvoidance {
        crate::RibbonAvoidance::all()
    }

    fn inline(
        &mut self,
        ui: &mut egui::Ui,
        ctx: crate::ModuleInlineCtx<'_>,
    ) -> crate::ModuleResponse {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Code: {}", self.title));
                ui.label(format!("{} bytes", self.text.len()));
            });
            self.show_editor(ui);
            if ctx.can_enter_workspace()
                && ui
                    .button(crate::style::theme().modules.inline_workspace_button_label)
                    .clicked()
            {
                crate::ModuleResponse::enter_workspace()
            } else {
                crate::ModuleResponse::none()
            }
        })
        .inner
    }

    fn workspace(&mut self, ws: &mut crate::WorkspaceCtx<'_>) {
        ws.add_bar(
            crate::WorkspaceBar::new(
                egui::Id::new(("code.workspace.bar", self.id)),
                crate::WorkspaceBarEdge::Top,
                crate::WorkspaceBarCluster::Middle,
            )
            .with_item(crate::WorkspaceBarItem::command(
                egui::Id::new(("code.workspace.format", self.id)),
                "Format",
                Some("code"),
            )),
        );
        ws.add_ribbon(self.toolbar(crate::RibbonScope::WorkspaceLevel(ws.level.id)));
    }
}

#[cfg(test)]
mod view_module_bridge_tests {
    use super::*;

    fn assert_view<T: crate::MaraView>(_value: &T) {}
    fn assert_module<T: crate::MaraModule>(_value: &T) {}

    #[test]
    fn code_editor_surface_is_both_view_and_module() {
        let surface =
            CodeEditorSurface::new("code-surface", "Code", "fn main() {}", Syntax::rust());
        assert_view(&surface);
        assert_module(&surface);
        assert_eq!(crate::MaraView::title(&surface), "Code");
        assert_eq!(crate::MaraModule::icon(&surface), "code");
        assert_eq!(surface.text(), "fn main() {}");
    }
}
