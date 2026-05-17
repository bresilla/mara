//! Minimal ViewRouter + permanent slot-ribbon shell example.
//!
//! This is intentionally separate from the legacy widget gallery so
//! the PLAN.md app-shell path can be exercised without disturbing the
//! older draggable ribbon/pane demo while migration continues.
//!
//! Run: `cargo run -p egui_mara --example views`.

use eframe::egui;
use egui_mara::{
    AccentColor, AppShellChrome, GlassOpacity, MaraView, ViewCtx, ViewId, ViewRouter,
    apply_theme_now, permanent_view_switcher_ribbon, show_app_shell_chrome_with_slot_ribbons,
};

struct LabelView {
    id: ViewId,
    title: &'static str,
    icon: &'static str,
    message: &'static str,
}

impl LabelView {
    fn new(
        id: &'static str,
        title: &'static str,
        icon: &'static str,
        message: &'static str,
    ) -> Self {
        Self {
            id: ViewId::new(id),
            title,
            icon,
            message,
        }
    }
}

impl MaraView for LabelView {
    fn id(&self) -> ViewId {
        self.id
    }

    fn title(&self) -> &str {
        self.title
    }

    fn icon(&self) -> &'static str {
        self.icon
    }

    fn show(&mut self, ctx: &mut ViewCtx<'_>) {
        egui::CentralPanel::default().show(ctx.egui_ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading(self.title);
                ui.label(self.message);
                ui.separator();
                ui.label(format!("workspace depth: {}", ctx.workspace.depth()));
                if ui.button("Push demo L1 module workspace").clicked() {
                    ctx.workspace
                        .push_module(egui::Id::new((self.title, "demo-module")));
                }
                ui.label("At L1+ the permanent top-right X slot resolves to restore/pop.");
            });
        });
    }
}

struct ViewShellApp {
    router: ViewRouter,
    chrome: AppShellChrome,
    accent: AccentColor,
    glass: GlassOpacity,
}

impl Default for ViewShellApp {
    fn default() -> Self {
        let mut router = ViewRouter::new(LabelView::new(
            "bevy",
            "Bevy",
            "cube",
            "This stands in for the Bevy scene/root L0 view.",
        ));
        router.register(LabelView::new(
            "graph",
            "Graph",
            "node_tree",
            "This stands in for a top-level graph L0 view.",
        ));
        router.register(LabelView::new(
            "whiteboard",
            "Whiteboard",
            "draw",
            "This stands in for a top-level canvas/whiteboard L0 view.",
        ));

        let chrome = AppShellChrome::new(permanent_view_switcher_ribbon(router.entries()));

        Self {
            router,
            chrome,
            accent: AccentColor(egui::Color32::from_rgb(0x7C, 0x5C, 0xFF)),
            glass: GlassOpacity(100),
        }
    }
}

impl eframe::App for ViewShellApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_theme_now(ctx, self.accent, self.glass);
        let _ = show_app_shell_chrome_with_slot_ribbons(
            ctx,
            &mut self.router,
            &self.chrome,
            self.accent.0,
        );
    }
}

fn main() -> eframe::Result<()> {
    let opts = eframe::NativeOptions::default();
    eframe::run_native(
        "egui_mara views",
        opts,
        Box::new(|_cc| Ok(Box::<ViewShellApp>::default())),
    )
}
