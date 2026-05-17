use eframe::egui;
use egui_mara::{
    AccentColor, GlassOpacity, ResolvedSlotRibbon, RibbonAction, RibbonCluster, RibbonEdge,
    RibbonOpen, RibbonScope, RibbonSlotClick, RibbonSlotItem, apply_theme, draw_slot_ribbons,
};

#[derive(Default)]
struct DemoApp {
    open: RibbonOpen,
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let accent = AccentColor(egui::Color32::from_rgb(76, 153, 242));
        apply_theme(ctx, accent, GlassOpacity(82));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("egui_mara demo");
            ui.label("This demo uses the single RibbonSlot API.");
        });

        if self.open.is_open("demo_top", "demo_about") {
            egui::Window::new("About").show(ctx, |ui| {
                ui.label("Panel state comes from RibbonOpen.");
            });
        }

        let mut about =
            RibbonSlotItem::featureful("demo_about", "info", "About", "About", RibbonAction::Noop)
                .as_panel_button();
        about.active = self.open.is_open("demo_top", "demo_about");

        let ribbons = [ResolvedSlotRibbon {
            id: egui::Id::new("demo_top_start"),
            chrome_id: None,
            scope: RibbonScope::Permanent,
            edge: RibbonEdge::Top,
            role: egui_mara::RibbonRole::Panel,
            mode: egui_mara::RibbonMode::ThreeSided,
            cluster: RibbonCluster::Start,
            accepts: &[],
            items: vec![about],
        }];

        let clicks: Vec<RibbonSlotClick> = draw_slot_ribbons(ctx, accent.0, &ribbons);
        for click in clicks {
            if click.item == egui::Id::new("demo_about") {
                self.open.toggle("demo_top", "demo_about");
            }
        }
    }
}

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "egui_mara demo",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::<DemoApp>::default())),
    )
}
