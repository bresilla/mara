//! Code-editor integration — thin wrapper around
//! [`egui_code_editor`] that pipes a multiline text buffer
//! through the same maximise / restore affordance the graph
//! widget uses.
//!
//! Minimal usage (inside a [`section`](mara_core::widget::section)
//! body, since panes require containers):
//!
//! ```ignore
//! mara_code_editor(
//!     ui,
//!     "my_code",
//!     &mut state.code,
//!     Syntax::rust(),
//!     accent,
//!     mara::ui::vocab::Vec2::new(w, 300.0),
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

use mara_core::vocab::{Color32 as MaraColor32, Vec2 as MaraVec2};

// `maximizable` is no longer called directly from this file — both
// `mara_code_editor` and `mara_code_editor_with_opts` route
// through `mara_core::embed::maximizable_with_opts` so the opts path
// is always live. `pub use` re-exports the symbol callers expect
// when migrating from the older signature.
pub use mara_core::embed::OverlayOpts;

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
    accent: impl Into<MaraColor32>,
    min_size: impl Into<MaraVec2>,
) {
    mara_code_editor_with_opts(
        ui,
        id_salt,
        text,
        syntax,
        accent,
        min_size,
        mara_core::embed::OverlayOpts::default(),
    )
}

/// The maximise-state key the code-editor wrapper registers with
/// [`mara_core::embed`], computed from the caller-supplied `id_salt`
/// (the same one passed to [`mara_code_editor`]). Compare against
/// [`mara_core::ViewCtx::fullscreen_owner`] or the facade host context
/// to detect "is THIS code editor the one currently in fullscreen?".
#[must_use]
pub fn code_fullscreen_key(id_salt: impl Hash) -> mara_core::vocab::Id {
    mara_core::embed::maximize_state_key(id_salt)
}

/// Same as [`mara_code_editor`] but accepts an [`OverlayOpts`] so
/// the caller can choose where the minimize chip lands on the
/// fullscreen overlay (which edge + which cluster along that edge).
pub fn mara_code_editor_with_opts(
    ui: &mut egui::Ui,
    id_salt: impl Hash + Copy,
    text: &mut String,
    syntax: Syntax,
    accent: impl Into<MaraColor32>,
    min_size: impl Into<MaraVec2>,
    fs_opts: mara_core::embed::OverlayOpts,
) {
    let accent = accent.into();
    let min_size = min_size.into();
    let accent_egui = mara_backend_egui::color32_for_backend(accent);
    let mut backend = mara_backend_egui::__internal_backend_from_raw(ui);
    let mut mara = mara_core::MaraUi::__internal_over(&mut backend, accent);
    mara_core::embed::__internal_maximizable_with_opts_egui(
        &mut mara,
        id_salt,
        accent,
        min_size,
        fs_opts,
        |mara| {
            let code = mara_core::style::theme().code;
            let line_h = code.font_size * code.line_height_factor;
            let rows =
                ((mara.available_rect().height() / line_h).floor() as usize).max(code.min_rows);
            let editor = CodeEditor::default()
                .with_syntax(syntax)
                .with_theme(mara_code_theme(accent_egui))
                .with_fontsize(code.font_size)
                .with_rows(rows);
            show_code_text_area(mara.backend_mut(), id_salt, &editor, text, accent);
        },
    );
}

/// Render a [`CodeEditor`]'s configuration through the sealed
/// [`mara_core::MaraTextArea`] (PLAN.md WS-D2).
///
/// This is the boundary the WS-D split creates: `mara_code` supplies
/// the tokeniser and palette as plain data, and the *adapter* owns the
/// rendering. The highlighter is a closure mapping each token to a
/// [`mara_core::paint::TextRun`], so syntax colouring reaches the paint IR
/// with no backend coupling of its own.
fn show_code_text_area(
    backend: &mut dyn mara_core::layout::UiBackend,
    id_salt: impl Hash,
    editor: &CodeEditor,
    text: &mut String,
    accent: MaraColor32,
) {
    let theme = editor.theme();
    let fontsize = editor.fontsize();
    let highlight = |line: &str| -> Vec<mara_core::paint::TextRun> {
        editor
            .highlight_line(line)
            .into_iter()
            .map(|(text, ty)| {
                let c = theme.type_color(ty).to_array();
                mara_core::paint::TextRun {
                    text,
                    size: fontsize,
                    color: MaraColor32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]),
                    family: mara_core::paint::TextFamily::Monospace,
                    extra_letter_spacing: 0.0,
                    leading_space: 0.0,
                }
            })
            .collect()
    };
    let area =
        mara_core::MaraTextArea::new(mara_core::vocab::Id::new(("mara_code_editor", id_salt)))
            .rows(editor.rows())
            .font_size(fontsize)
            .accent(accent)
            .highlight(&highlight);
    let _ = area.show(backend, text);
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
/// Convert a Mara colour into `mara_code`'s palette type.
///
/// A `From` impl is impossible here: both types are foreign to the
/// other crate, and `mara_code` cannot depend on `mara_core` (the
/// dependency already runs the other way). So the adapter owns the
/// conversion — which is where a boundary conversion belongs anyway.
fn code_color(c: impl Into<mara_core::vocab::Color32>) -> mara_code::CodeColor {
    let c = c.into();
    mara_code::CodeColor::from_rgba_unmultiplied(c.r(), c.g(), c.b(), c.a())
}

fn mara_code_theme(accent: egui::Color32) -> ColorTheme {
    use mara_core::style::{
        accent_pressed, glass_alpha_window, glass_fill, on_panel_dim, pane_fill,
    };
    let code = mara_core::style::theme().code;
    ColorTheme {
        name: "Mara",
        dark: code.force_dark,
        // `glass_fill(pane_fill(...), …)` flows through the active
        // theme so GAME's accent panel becomes the editor bg too,
        // not a hardcoded dark.
        bg: code_color(glass_fill(pane_fill(accent), accent, glass_alpha_window())),
        cursor: code_color(accent),
        // Selection = darker accent shade derived at runtime so it
        // tracks whatever colour the user picked.
        selection: code_color(accent_pressed()),
        // `comments` / `punctuation` flip to whatever contrasts the
        // pane fill, so they stay readable on PRO's dark and GAME's
        // accent-coloured panels alike.
        comments: code_color(on_panel_dim()),
        functions: code_color(code.functions),
        keywords: code_color(accent),
        literals: code_color(code.literals),
        numerics: code_color(code.numerics),
        punctuation: code_color(on_panel_dim()),
        strs: code_color(code.strings),
        types: code_color(code.types),
        special: code_color(accent),
    }
}

// ─── Typed Pod constructor ──────────────────────────────────────────
//
// Adds `Pod::with_code_editor(text_id, syntax, default_text)` so
// pane bodies can host a code editor through the canonical pod path
// instead of reaching into custom egui-hosted pod internals. The
// text buffer is stashed in backend memory under `text_id`; the
// editor reads / writes it each frame.

/// Code-editor constructors for [`mara_core::pod::Pod`].
///
/// An extension trait rather than inherent methods: the adapters live
/// in the facade now (so the vendored crates can depend on `mara_core`
/// without a cycle), and Rust only allows inherent `impl`s in the
/// crate that defines the type. Bring it into scope to use them.
pub trait PodCodeEditorExt: Sized {
    #[must_use]
    fn with_code_editor(
        self,
        text_id: impl Into<mara_core::vocab::Id>,
        syntax: Syntax,
        default_text: impl Into<String>,
    ) -> Self;

    #[must_use]
    fn with_code_editor_opts(
        self,
        text_id: impl Into<mara_core::vocab::Id>,
        syntax: Syntax,
        default_text: impl Into<String>,
        fs_opts: OverlayOpts,
    ) -> Self;
}

impl PodCodeEditorExt for mara_core::pod::Pod {
    /// Append a mara-themed code editor to this pod. The editor's
    /// text lives in backend memory under `text_id` — pre-seed it
    /// (`mara_core::memory::MaraMemoryCtx::__internal_from_backend_ctx(ctx).set_temp(text_id, "default".to_string())`)
    /// or rely on `default_text` to seed on first render.
    ///
    /// Uses `mara_core::style::active_accent()` for the inline
    /// theme. The maximise / restore chip in the editor's top-left
    /// corner toggles fullscreen via `mara_core::embed`.
    ///
    /// Reserves 10 row-height units of pod space.
    fn with_code_editor(
        self,
        text_id: impl Into<mara_core::vocab::Id>,
        syntax: Syntax,
        default_text: impl Into<String>,
    ) -> Self {
        self.with_code_editor_opts(
            text_id,
            syntax,
            default_text,
            OverlayOpts::default().avoid_ribbons(mara_core::RibbonAvoidance::all()),
        )
    }

    /// Append a mara-themed code editor with explicit fullscreen
    /// overlay options. The fullscreen background stays full-window;
    /// these options only affect the body/chip.
    fn with_code_editor_opts(
        self,
        text_id: impl Into<mara_core::vocab::Id>,
        syntax: Syntax,
        default_text: impl Into<String>,
        fs_opts: OverlayOpts,
    ) -> Self {
        let text_id: egui::Id = text_id.into().into();
        let default = default_text.into();
        let key = mara_core::vocab::Id::from(text_id);
        self.with_custom_units(10, move |mara| {
            let mut text: String = mara
                .ctx()
                .memory()
                .get_temp::<String>(key)
                .unwrap_or_else(|| default.clone());
            let avail = mara.available_rect().size();
            let accent = mara_core::style::active_accent();
            // The vendored editor still takes a backend surface; this
            // crate is host tier, so it unwraps here.
            if let Some(ui) = mara.__internal_egui_ui_mut() {
                mara_code_editor_with_opts(
                    ui,
                    text_id,
                    &mut text,
                    syntax.clone(),
                    accent,
                    egui::Vec2::from(avail),
                    fs_opts,
                );
            }
            mara.ctx().memory().set_temp(key, text);
        })
    }
}

// ─── View + Module bridge ──────────────────────────────────────────
//
// This keeps the existing standalone editor wrapper intact while also
// exposing a PLAN.md-native surface: code can now be routed as a
// top-level L0 view or embedded as an L1-capable module.

/// A retained code editor surface that implements both [`mara_core::MaraView`]
/// and [`mara_core::MaraModule`].
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

    fn toolbar(&self, scope: mara_core::RibbonScope) -> mara_core::RibbonSlotDef {
        let format = mara_core::RibbonSlotItem::new(
            egui::Id::new(("code.format", self.id)),
            "code",
            "Format",
            "Format code document",
            mara_core::RibbonAction::Command(mara_core::vocab::Id::new((
                "code.format.command",
                self.id,
            ))),
        );
        mara_core::RibbonSlotDef::new(
            egui::Id::new(("code.ribbon", self.id)),
            scope,
            mara_core::RibbonEdge::Top,
            mara_core::RibbonCluster::Middle,
            vec![mara_core::RibbonSlot::new(
                mara_core::RibbonSlotId::new(("code.format.slot", self.id)),
                Some(format),
                mara_core::RibbonOverridePolicy::Fixed,
            )],
        )
    }

    fn show_editor(&mut self, ui: &mut egui::Ui) {
        let min_size = ui.available_size_before_wrap();
        let content_avoidance = mara_core::module::MaraModule::fullscreen_content_avoidance(self);
        mara_code_editor_with_opts(
            ui,
            self.id,
            &mut self.text,
            self.syntax.clone(),
            mara_core::style::active_accent(),
            min_size,
            OverlayOpts::default().avoid_ribbons(content_avoidance),
        );
    }
}

impl mara_core::MaraView for CodeEditorSurface {
    fn id(&self) -> mara_core::ViewId {
        mara_core::ViewId::from(self.id)
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn icon(&self) -> &'static str {
        "code"
    }

    fn ribbons(&mut self) -> Vec<mara_core::RibbonSlotDef> {
        vec![
            self.toolbar(mara_core::RibbonScope::View(mara_core::ViewId::from(
                self.id,
            ))),
        ]
    }

    fn content_avoidance(&self) -> mara_core::RibbonAvoidance {
        mara_core::RibbonAvoidance::all()
    }

    fn show(&mut self, ctx: &mut mara_core::ViewCtx<'_>) {
        let rect = ctx.content_rect();
        #[allow(deprecated)]
        {
            egui::CentralPanel::default().show(ctx.__internal_egui_ctx(), |ui| {
                let mut body = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect.into())
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                self.show_editor(&mut body);
            });
        }
    }
}

impl mara_core::MaraModule for CodeEditorSurface {
    fn id(&self) -> mara_core::vocab::Id {
        self.id.into()
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn icon(&self) -> &'static str {
        "code"
    }

    fn fullscreen_content_avoidance(&self) -> mara_core::RibbonAvoidance {
        mara_core::RibbonAvoidance::all()
    }

    fn inline(
        &mut self,
        mui: &mut mara_core::mui::MaraUi<'_>,
        ctx: mara_core::ModuleInlineCtx<'_>,
    ) -> mara_core::ModuleResponse {
        let ui = mui.__internal_raw_ui();
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Code: {}", self.title));
                ui.label(format!("{} bytes", self.text.len()));
            });
            self.show_editor(ui);
            if ctx.can_enter_workspace()
                && ui
                    .button(
                        mara_core::style::theme()
                            .modules
                            .inline_workspace_button_label,
                    )
                    .clicked()
            {
                mara_core::ModuleResponse::enter_workspace()
            } else {
                mara_core::ModuleResponse::none()
            }
        })
        .inner
    }

    fn workspace(&mut self, ws: &mut mara_core::WorkspaceCtx<'_>) {
        ws.add_bar(
            mara_core::WorkspaceBar::new(
                egui::Id::new(("code.workspace.bar", self.id)),
                mara_core::WorkspaceBarEdge::Top,
                mara_core::WorkspaceBarCluster::Middle,
            )
            .with_item(mara_core::WorkspaceBarItem::command(
                egui::Id::new(("code.workspace.format", self.id)),
                "Format",
                Some("code"),
            )),
        );
        ws.add_ribbon(self.toolbar(mara_core::RibbonScope::WorkspaceLevel(ws.level.id)));
    }
}

#[cfg(test)]
mod view_module_bridge_tests {
    use super::*;

    fn assert_view<T: mara_core::MaraView>(_value: &T) {}
    fn assert_module<T: mara_core::MaraModule>(_value: &T) {}

    #[test]
    fn code_editor_surface_is_both_view_and_module() {
        let surface =
            CodeEditorSurface::new("code-surface", "Code", "fn main() {}", Syntax::rust());
        assert_view(&surface);
        assert_module(&surface);
        assert_eq!(mara_core::MaraView::title(&surface), "Code");
        assert_eq!(mara_core::MaraModule::icon(&surface), "code");
        assert_eq!(surface.text(), "fn main() {}");
    }
}
