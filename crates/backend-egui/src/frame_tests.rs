//! Tests that drive a real backend frame.
//!
//! They live here rather than in `mara_core` because they need
//! `impl UiBackend for EguiUiBackend`. A dev-dependency cycle back into
//! core would not work: the impl would target the *dependency* build of
//! `mara_core`, which the crate under test cannot see. Verified with a
//! minimal reproduction before moving these.

mod enforce {
    use mara_core::enforce::*;
    use mara_core::ribbon::chrome::{RibbonDrag, RibbonOpen, RibbonPlacement};
    use mara_core::shell::ShellBar;

    fn run_pass(ctx: &egui::Context, f: impl FnOnce()) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        };
        ctx.begin_pass(input);
        f();
        ctx.end_pass()
    }

    /// A consumer that draws Mara surfaces but never renders the bar
    /// gets the enforced fallback bar from the second pass onward
    /// (pass one is the grace pass).
    #[test]
    fn fallback_bar_kicks_in_after_grace_pass() {
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);

        let out1 = run_pass(&ctx, || crate::theme::__internal_enforce_defaults(&ctx));
        assert!(
            __internal_shell_enforced_pass(&ctx).is_none(),
            "grace pass must not draw the fallback bar"
        );

        let _ = run_pass(&ctx, || crate::theme::__internal_enforce_defaults(&ctx));
        assert!(
            __internal_shell_enforced_pass(&ctx).is_some(),
            "second pass without an app bar must enforce the fallback"
        );

        // And it keeps rendering every subsequent pass (the stamp
        // advances pass over pass). egui areas are invisible on their
        // first frame (sizing pass), so paint is asserted on this
        // settled pass, not the pass the fallback first fired.
        let after_second = __internal_shell_enforced_pass(&ctx).expect("stamped above");
        let out3 = run_pass(&ctx, || crate::theme::__internal_enforce_defaults(&ctx));
        let after_third = __internal_shell_enforced_pass(&ctx).expect("still enforced");
        assert!(
            after_third > after_second,
            "fallback must re-render every pass without an app bar"
        );
        assert!(
            out3.shapes.len() > out1.shapes.len(),
            "the enforced bar must actually paint something"
        );
    }

    /// An app that renders its own `ShellBar` each pass never triggers
    /// the fallback — even though enforcement runs every pass too.
    #[test]
    fn app_bar_suppresses_fallback() {
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);
        let mut bar = ShellBar::default();
        let mut open = RibbonOpen::default();
        let mut placement = RibbonPlacement::default();
        let mut drag = RibbonDrag::default();

        for _ in 0..4 {
            run_pass(&ctx, || {
                // Content first, bar last — the common host pattern.
                crate::theme::__internal_enforce_defaults(&ctx);
                let _ = bar.__internal_show_egui(&ctx, &mut open, &mut placement, &mut drag);
            });
        }
        assert!(
            __internal_shell_enforced_pass(&ctx).is_none(),
            "fallback must never fire while the app renders the bar"
        );
    }

    /// The explicit per-frame opt-out suppresses the fallback — but
    /// only for frames it is repeated in; going silent brings the bar
    /// back.
    #[test]
    fn explicit_opt_out_suppresses_fallback_per_frame() {
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);

        for _ in 0..3 {
            run_pass(&ctx, || {
                __internal_opt_out_shell(&ctx);
                crate::theme::__internal_enforce_defaults(&ctx);
            });
        }
        assert!(
            __internal_shell_enforced_pass(&ctx).is_none(),
            "opted-out frames must not draw the fallback bar"
        );

        // Opt-out stops being called → hysteresis covers one pass,
        // then the enforced bar returns.
        run_pass(&ctx, || crate::theme::__internal_enforce_defaults(&ctx));
        assert!(__internal_shell_enforced_pass(&ctx).is_none());
        run_pass(&ctx, || crate::theme::__internal_enforce_defaults(&ctx));
        assert!(
            __internal_shell_enforced_pass(&ctx).is_some(),
            "the bar must come back once the opt-out is no longer repeated"
        );
    }

    /// An app that stops rendering its bar loses the argument: the
    /// fallback takes over after the hysteresis window.
    #[test]
    fn fallback_takes_over_when_app_bar_stops() {
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);
        let mut bar = ShellBar::default();
        let mut open = RibbonOpen::default();
        let mut placement = RibbonPlacement::default();
        let mut drag = RibbonDrag::default();

        for _ in 0..2 {
            run_pass(&ctx, || {
                crate::theme::__internal_enforce_defaults(&ctx);
                let _ = bar.__internal_show_egui(&ctx, &mut open, &mut placement, &mut drag);
            });
        }
        // App goes silent; hysteresis covers one pass, then Mara draws.
        run_pass(&ctx, || crate::theme::__internal_enforce_defaults(&ctx));
        assert!(__internal_shell_enforced_pass(&ctx).is_none());
        run_pass(&ctx, || crate::theme::__internal_enforce_defaults(&ctx));
        assert!(__internal_shell_enforced_pass(&ctx).is_some());
    }
}

mod ribbon_chrome {
    use mara_core::context::MaraCtx;
    use mara_core::ribbon::chrome::*;
    use mara_core::ribbon::*;
    use mara_core::ribbon::chrome::{RibbonCluster, RibbonOpen, RibbonPlacement};
    use mara_core::vocab::{Pos2 as MaraPos2, Rect as MaraRect};
    use mara_core::ribbon::{RibbonAction, RibbonScope};

    fn test_ctx_with_chrome(rect: egui::Rect) -> crate::EguiCtx {
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);
        mara_core::memory::MaraMemoryCtx::new(&ctx).set_temp(chrome_bounds_key(), MaraRect::from(rect));
        ctx
    }

    fn test_ctx_with_screen_and_chrome(
        screen: egui::Rect,
        chrome: egui::Rect,
    ) -> crate::EguiCtx {
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        });
        mara_core::memory::MaraMemoryCtx::new(&ctx)
            .set_temp(chrome_bounds_key(), MaraRect::from(chrome));
        ctx
    }

    fn ribbon(edge: RibbonEdge) -> ResolvedSlotRibbon {
        ribbon_with_id("test_ribbon", edge)
    }

    fn ribbon_with_id(id: &'static str, edge: RibbonEdge) -> ResolvedSlotRibbon {
        ResolvedSlotRibbon {
            id: mara_core::vocab::Id::new((id, edge)),
            chrome_id: Some(id),
            scope: RibbonScope::Permanent,
            edge,
            role: RibbonRole::Icon,
            mode: RibbonMode::ThreeSided,
            cluster: RibbonCluster::Middle,
            accepts: &["*"],
            items: vec![
                RibbonSlotItem::featureful("test_item", "info", "Test", "Test", RibbonAction::Noop)
                    .draggable(true),
            ],
        }
    }

    #[test]
    fn bottom_bar_spans_full_width_side_rails_inset() {
        // The bottom bar runs corner-to-corner; the side rails stop
        // short above it. Holds in BOTH declaration orders, so a
        // relocated main bar dropped to the bottom still spans fully.
        let chrome = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 480.0));
        let ctx = test_ctx_with_chrome(chrome);
        let left = ribbon_with_id("left", RibbonEdge::Left);
        let bottom = ribbon_with_id("bottom", RibbonEdge::Bottom);

        for order in [
            vec![left.clone(), bottom.clone()],
            vec![bottom.clone(), left.clone()],
        ] {
            let base = compute_side_insets(&order);
            let left_ribbon = order.iter().find(|r| r.edge == RibbonEdge::Left).unwrap();
            let bottom_ribbon = order.iter().find(|r| r.edge == RibbonEdge::Bottom).unwrap();
            let left_strip = strip_rect(
                left_ribbon,
                &ctx,
                insets_for_ribbon(&order, left_ribbon, base),
            );
            let bottom_strip = strip_rect(
                bottom_ribbon,
                &ctx,
                insets_for_ribbon(&order, bottom_ribbon, base),
            );
            // Bottom bar reaches the left edge (owns the corner).
            assert_eq!(bottom_strip.left(), chrome.left() + EDGE_GAP);
            // Side rail stops above the bottom bar.
            assert!(left_strip.bottom() < bottom_strip.top());
        }
    }

    #[test]
    fn opening_a_side_pane_closes_the_opposite_side() {
        let left = ribbon_with_id("left", RibbonEdge::Left);
        let right = ribbon_with_id("right", RibbonEdge::Right);
        let ribbons = vec![left, right];
        let mut open = RibbonOpen::default();
        open.set("left", "left_pane");
        open.set("right", "right_pane");

        // Just opened the left pane → the right side must close.
        close_opposite_side_panes(&ribbons, &mut open, "left");
        assert!(open.is_open("left", "left_pane"));
        assert!(open.get("right").is_none());

        // Now open the right pane → the left side closes.
        open.set("right", "right_pane");
        close_opposite_side_panes(&ribbons, &mut open, "right");
        assert!(open.is_open("right", "right_pane"));
        assert!(open.get("left").is_none());
    }

    #[test]
    fn fresh_chrome_bounds_track_window_resize_without_explicit_publish() {
        // No shelf layout published. The bounds must follow the live
        // window each pass — regression for the self-perpetuating
        // chrome_bounds_key that froze side ribbons at frame 1.
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);

        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(800.0, 480.0),
            )),
            ..Default::default()
        });
        // Chrome bounds reserve the (assumed-present) top bar strip, so the
        // content area starts one rail clearance below the window top.
        let cr = MaraCtx::content_rect(&ctx);
        assert_eq!(
            fresh_chrome_bounds(&ctx),
            MaraRect::from_min_max(
                MaraPos2::new(cr.min.x, cr.min.y + ribbon_clearance()),
                cr.max,
            )
        );
        // Simulate the renderer writing the key (what froze it before).
        let first = fresh_chrome_bounds(&ctx);
        mara_core::memory::MaraMemoryCtx::new(&ctx).set_temp(chrome_bounds_key(), first);
        let _ = ctx.end_pass();

        // Window grows on the next pass.
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1200.0, 700.0),
            )),
            ..Default::default()
        });
        let second = fresh_chrome_bounds(&ctx);
        let _ = ctx.end_pass();

        assert_eq!(
            second,
            MaraRect::from(egui::Rect::from_min_max(
                egui::pos2(0.0, ribbon_clearance()),
                egui::pos2(1200.0, 700.0)
            )),
            "chrome bounds must follow the resized window, not the stale write"
        );
        assert_ne!(second, first, "bounds must not freeze at the first pass");
    }

    #[test]
    fn fresh_chrome_bounds_prefer_published_shelf_viewport() {
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(800.0, 480.0),
            )),
            ..Default::default()
        });
        let reserved = egui::Rect::from_min_max(egui::pos2(60.0, 40.0), egui::pos2(740.0, 480.0));
        mara_core::shelf::__internal_publish_shelf_layout(
            &ctx,
            mara_core::shelf::ShelfLayout::full(reserved),
        );
        // The published shelf viewport is preferred, then the top-bar strip
        // is reserved on top of it.
        assert_eq!(
            fresh_chrome_bounds(&ctx),
            MaraRect::from(egui::Rect::from_min_max(
                egui::pos2(60.0, 40.0 + ribbon_clearance()),
                egui::pos2(740.0, 480.0)
            ))
        );
        let _ = ctx.end_pass();
    }

    #[test]
    fn top_ribbon_uses_full_window_even_when_chrome_bounds_are_reserved() {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 480.0));
        let chrome = egui::Rect::from_min_max(egui::pos2(220.0, 0.0), egui::pos2(620.0, 480.0));
        let ctx = test_ctx_with_screen_and_chrome(screen, chrome);
        let top = ribbon_with_id("top", RibbonEdge::Top);
        let left = ribbon_with_id("left", RibbonEdge::Left);
        let ribbons = vec![top, left];
        let base = compute_side_insets(&ribbons);

        let top_strip = strip_rect(
            &ribbons[0],
            &ctx,
            insets_for_ribbon(&ribbons, &ribbons[0], base),
        );
        assert_eq!(top_strip.left(), screen.left() + EDGE_GAP);
        assert_eq!(top_strip.right(), screen.right() - EDGE_GAP);

        let left_strip = strip_rect(
            &ribbons[1],
            &ctx,
            insets_for_ribbon(&ribbons, &ribbons[1], base),
        );
        assert_eq!(left_strip.left(), chrome.left() + EDGE_GAP);
    }

    #[test]
    fn vertical_middle_buttons_center_against_published_chrome_height() {
        let chrome = egui::Rect::from_min_size(egui::pos2(24.0, 40.0), egui::vec2(320.0, 384.0));
        let ctx = test_ctx_with_chrome(chrome);
        let insets = SideInsets::default();

        for edge in [RibbonEdge::Left, RibbonEdge::Right] {
            let ribbon = ribbon(edge);
            let rect = screen_rect(place_button(
                &ctx,
                &ribbon,
                RibbonCluster::Middle,
                0,
                1,
                insets,
            ));

            assert_eq!(rect.center().y, chrome.center().y);
        }
    }

    #[test]
    fn vertical_middle_button_group_centers_against_published_chrome_height() {
        let chrome = egui::Rect::from_min_size(egui::pos2(0.0, 96.0), egui::vec2(480.0, 512.0));
        let ctx = test_ctx_with_chrome(chrome);
        let insets = SideInsets::default();
        let ribbon = ribbon(RibbonEdge::Left);

        let first = screen_rect(place_button(
            &ctx,
            &ribbon,
            RibbonCluster::Middle,
            0,
            3,
            insets,
        ));
        let last = screen_rect(place_button(
            &ctx,
            &ribbon,
            RibbonCluster::Middle,
            2,
            3,
            insets,
        ));
        let group_center = (first.center().y + last.center().y) * 0.5;

        assert_eq!(group_center, chrome.center().y);
    }

    #[test]
    fn featureful_button_placement_uses_mara_geometry() {
        let chrome = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 480.0));
        let ctx = test_ctx_with_chrome(chrome);
        let ribbon = ribbon(RibbonEdge::Bottom);

        let rect: MaraRect = screen_rect(place_button(
            &ctx,
            &ribbon,
            RibbonCluster::End,
            0,
            1,
            SideInsets::default(),
        ));

        assert_eq!(rect.bottom(), chrome.bottom() - EDGE_GAP);
        assert_eq!(rect.right(), chrome.right());
    }

    #[test]
    fn ribbon_open_rejects_blank_chrome_ids() {
        let mut open = RibbonOpen::default();

        let blank_ribbon = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            open.set(" ", "item");
        }));
        let blank_item = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            open.toggle("ribbon", " ");
        }));

        assert!(blank_ribbon.is_err());
        assert!(blank_item.is_err());
    }

    #[test]
    fn ribbon_width_sanitizes_invalid_values() {
        let mut widths = RibbonWidth::default();

        widths.set("ribbon", RibbonCluster::Start, -12.0);
        assert_eq!(widths.get("ribbon", RibbonCluster::Start), Some(0.0));

        widths.set("ribbon", RibbonCluster::Start, f32::NAN);
        assert_eq!(widths.get("ribbon", RibbonCluster::Start), None);

        widths
            .per_cluster
            .insert(("ribbon", RibbonCluster::Middle), f32::NEG_INFINITY);
        assert_eq!(widths.get("ribbon", RibbonCluster::Middle), None);

        let blank_ribbon = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            widths.set(" ", RibbonCluster::End, 10.0);
        }));
        assert!(blank_ribbon.is_err());
    }

    #[test]
    fn ribbon_placement_rejects_blank_ids_and_ignores_invalid_direct_targets() {
        let mut placement = RibbonPlacement::default();
        placement.set("item", "target", RibbonCluster::End, 3);
        assert_eq!(
            placement.resolve_parts("item", "source", RibbonCluster::Start, 0),
            ("target", RibbonCluster::End, 3)
        );

        placement
            .overrides
            .insert("bad-target", (" ", RibbonCluster::End, 9));
        assert_eq!(
            placement.resolve_parts("bad-target", "source", RibbonCluster::Start, 0),
            ("source", RibbonCluster::Start, 0)
        );

        let blank_item = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            placement.set(" ", "target", RibbonCluster::Middle, 0);
        }));
        let blank_fallback = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = placement.resolve_parts("item", " ", RibbonCluster::Middle, 0);
        }));

        assert!(blank_item.is_err());
        assert!(blank_fallback.is_err());
    }
}

mod shell {
    use mara_core::shell::*;
    use mara_core::ribbon::chrome::{RibbonCluster, RibbonDrag, RibbonOpen, RibbonPlacement};
    use mara_core::shell::ShellBar;

    /// Rendering a bar with an app-menu + views must build valid slot
    /// items (non-empty label/tooltip) and not panic. Regression for
    /// the empty-tooltip assert that crashed the native demo.
    /// A single-tab app shows no tab chrome: with nothing to switch
    /// between, the switcher ribbon must not be emitted at all.
    #[test]
    fn single_view_bar_emits_no_switcher() {
        let bar = ShellBar {
            views: vec![ShellView::new("v.only", "cube", "Only")],
            active: Some("v.only"),
            ..Default::default()
        };
        let ribbons = bar.build_ribbons();
        assert!(
            !ribbons
                .iter()
                .any(|ribbon| ribbon.chrome_id == Some(TOP_BAR_VIEWS_CHROME_ID)),
            "one view => no switcher ribbon"
        );

        let two = ShellBar {
            views: vec![
                ShellView::new("v.a", "cube", "A"),
                ShellView::new("v.b", "pen", "B"),
            ],
            ..Default::default()
        };
        assert!(
            two.build_ribbons()
                .iter()
                .any(|ribbon| ribbon.chrome_id == Some(TOP_BAR_VIEWS_CHROME_ID)),
            "two views => switcher present"
        );
    }

    /// The switcher rides `views_cluster` — Middle by default, movable
    /// to Start/End by the app.
    #[test]
    fn switcher_cluster_is_configurable_default_middle() {
        let mut bar = ShellBar {
            views: vec![
                ShellView::new("v.a", "cube", "A"),
                ShellView::new("v.b", "pen", "B"),
            ],
            ..Default::default()
        };
        let cluster_of = |bar: &ShellBar| {
            bar.build_ribbons()
                .into_iter()
                .find(|ribbon| ribbon.chrome_id == Some(TOP_BAR_VIEWS_CHROME_ID))
                .map(|ribbon| ribbon.cluster)
        };
        assert_eq!(cluster_of(&bar), Some(RibbonCluster::Middle));
        bar.views_cluster = RibbonCluster::End;
        assert_eq!(cluster_of(&bar), Some(RibbonCluster::End));
    }

    #[test]
    fn shell_bar_renders_without_panicking() {
        let bar = ShellBar {
            views: vec![
                ShellView::new("v.scene", "cube", "Scene"),
                ShellView::new("v.graph", "pen", "Graph"),
            ],
            active: Some("v.scene"),
            ..Default::default()
        };
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);
        let mut open = RibbonOpen::default();
        let mut placement = RibbonPlacement::default();
        let mut drag = RibbonDrag::default();
        ctx.begin_pass(egui::RawInput::default());
        let mut bar = bar;
        let events = bar.__internal_show_egui(&ctx, &mut open, &mut placement, &mut drag);
        let _ = ctx.end_pass();
        // No interaction in a headless pass → no events.
        assert!(events.is_empty());
    }

    /// The bar render always paints — the bar has no disable flag.
    /// (The explicit per-frame opt-out lives in `crate::enforce` and is
    /// tested there.)
    #[test]
    fn shell_bar_show_always_renders() {
        let mut bar = ShellBar {
            views: vec![ShellView::new("v", "cube", "V")],
            ..Default::default()
        };
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);
        let mut open = RibbonOpen::default();
        let mut placement = RibbonPlacement::default();
        let mut drag = RibbonDrag::default();
        ctx.begin_pass(egui::RawInput::default());
        let events = bar.__internal_show_egui(&ctx, &mut open, &mut placement, &mut drag);
        let output = ctx.end_pass();
        assert!(events.is_empty());
        assert!(
            !output.shapes.is_empty(),
            "the bar must render unconditionally"
        );
    }
}

mod ribbon_slot_paint {
pub fn augment_shelf_buttons(
    ribbons: &[ResolvedSlotRibbon],
    presence: mara_core::shelf::ShelfPresence,
    left_visible: bool,
    right_visible: bool,
    bottom_visible: bool,
    order: ShelfButtonOrder,
) -> Option<Vec<ResolvedSlotRibbon>> {
    augment_shelf_buttons_with_chrome(
        ribbons,
        mara_core::window_chrome::WindowChromeHostCapabilities {
            system_maximize: false,
            system_close: false,
            ..Default::default()
        },
        presence,
        left_visible,
        right_visible,
        bottom_visible,
        order,
        false,
        false,
    )
}

    use mara_core::ribbon::slot_paint::*;
    use mara_core::ribbon::*;
    use mara_core::ribbon::slot_paint::ShelfButtonOrder;
    use mara_core::ribbon::chrome::{RibbonCluster};
    use mara_core::vocab::{Color32 as MaraColor32, Id as MaraId, Pos2 as MaraPos2, Rect as MaraRect, Vec2 as MaraVec2};

    #[test]
    fn resolve_leaf_ribbon_none_when_no_items() {
        use mara_core::ribbon::{
            RibbonOverridePolicy, RibbonScope, RibbonSlot, RibbonSlotDef, RibbonSlotId,
        };
        use mara_core::vocab::Id;

        // A slot with no default item resolves to nothing → no drawable
        // ribbon (so an empty leaf ribbon set draws nothing).
        let empty_slot = RibbonSlot::new(
            RibbonSlotId::new("empty.slot"),
            None,
            RibbonOverridePolicy::Fixed,
        );
        let def = RibbonSlotDef::new(
            Id::new("empty"),
            RibbonScope::Permanent,
            RibbonEdge::Right,
            RibbonCluster::Middle,
            vec![empty_slot],
        );
        assert!(resolve_leaf_ribbon(&def).is_none());
    }

    /// PROOF: a leaf's ribbon renders INSIDE the node's region — the
    /// area egui actually places must be contained by the cell rect,
    /// nowhere near the window edges.
    #[test]
    #[allow(deprecated)]
    fn view_ribbons_land_inside_the_node_region() {
        use mara_core::ribbon::{RibbonOverridePolicy, RibbonSlot, RibbonSlotDef, RibbonSlotId};
        use mara_core::vocab::Id;

        let raw = egui::Context::default();
        // Cell in the middle-right of a 1600x900 window.
        let region =
            MaraRect::from_min_size(MaraPos2::new(600.0, 100.0), MaraVec2::new(500.0, 600.0));

        let pen = mara_core::ribbon::RibbonSlotItem::new(
            Id::new("pen"),
            "pen",
            "Pen",
            "tip",
            RibbonAction::Command(Id::new("pen.cmd")),
        );
        let def = RibbonSlotDef::new(
            Id::new("test.view.ribbon"),
            RibbonScope::Permanent,
            RibbonEdge::Left,
            RibbonCluster::Middle,
            vec![RibbonSlot::new(
                RibbonSlotId::new("pen.slot"),
                Some(pen),
                RibbonOverridePolicy::Fixed,
            )],
        );

        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1600.0, 900.0),
        ));
        let _ = raw.run(input, |ctx| {
            let _ = __internal_draw_view_ribbons(
                &crate::EguiCtx::new(ctx),
                region,
                MaraId::new("test.view.salt"),
                MaraColor32::WHITE,
                std::slice::from_ref(&def),
            );
        });

        let ribbon_id = MaraId::new((def.id, def.cluster));
        let area_id: egui::Id = MaraId::new(("mara_slot_ribbon", ribbon_id, Id::new("pen"))).into();
        let rect = raw
            .memory(|m| m.area_rect(area_id))
            .expect("leaf ribbon area must exist after the pass");
        let rect: MaraRect = rect.into();
        assert!(
            rect.min.x >= region.min.x
                && rect.min.y >= region.min.y
                && rect.max.x <= region.max.x
                && rect.max.y <= region.max.y,
            "leaf ribbon rendered at {rect:?}, OUTSIDE its region {region:?}"
        );
        // And specifically hugging the region's LEFT edge, not the window's.
        assert!(
            rect.min.x < region.min.x + 40.0,
            "left-edge ribbon should hug the cell's left edge, got {rect:?}"
        );
    }

    fn presence(left: bool, right: bool, bottom: bool) -> mara_core::shelf::ShelfPresence {
        mara_core::shelf::ShelfPresence {
            left,
            right,
            bottom,
        }
    }

    fn item(id: &'static str, icon: &'static str, action: RibbonAction) -> RibbonSlotItem {
        RibbonSlotItem::featureful(id, icon, id, id, action)
            .with_role(mara_core::ribbon::RibbonRole::Icon)
    }

    fn top_ribbon(cluster: RibbonCluster, items: Vec<RibbonSlotItem>) -> ResolvedSlotRibbon {
        ResolvedSlotRibbon {
            id: MaraId::new(("top", cluster)),
            chrome_id: Some("top"),
            scope: RibbonScope::Permanent,
            edge: RibbonEdge::Top,
            role: mara_core::ribbon::RibbonRole::Icon,
            mode: mara_core::ribbon::RibbonMode::ThreeSided,
            cluster,
            accepts: &[],
            items,
        }
    }

    fn window_caps(
        system_maximize: bool,
        system_close: bool,
    ) -> mara_core::window_chrome::WindowChromeHostCapabilities {
        mara_core::window_chrome::WindowChromeHostCapabilities {
            system_maximize,
            system_close,
            ..Default::default()
        }
    }

    #[test]
    fn window_controls_inject_maximize_and_close_when_shown() {
        let ribbons = vec![top_ribbon(RibbonCluster::Start, Vec::new())];
        let augmented = augment_shelf_buttons_with_chrome(
            &ribbons,
            window_caps(true, true),
            presence(false, false, false),
            false,
            false,
            false,
            ShelfButtonOrder::Featureful,
            false,
            false, // not hidden
        )
        .expect("window controls should be injected");
        assert!(contains_item(&augmented, maximize_item_id()));
        assert!(contains_item(&augmented, close_item_id()));
    }

    #[test]
    fn window_controls_hidden_completely_on_phone() {
        let ribbons = vec![top_ribbon(RibbonCluster::Start, Vec::new())];
        let augmented = augment_shelf_buttons_with_chrome(
            &ribbons,
            window_caps(true, true),
            presence(false, false, false),
            false,
            false,
            false,
            ShelfButtonOrder::Featureful,
            false,
            true, // phone: hide both
        );
        assert!(
            augmented.is_none(),
            "phone-class hides both maximize and close completely"
        );
    }

    #[test]
    fn open_side_panel_hides_only_that_side_rail() {
        let mut left = top_ribbon(RibbonCluster::Start, Vec::new());
        left.edge = RibbonEdge::Left;
        let mut right = top_ribbon(RibbonCluster::Start, Vec::new());
        right.edge = RibbonEdge::Right;
        let mut bottom = top_ribbon(RibbonCluster::Middle, Vec::new());
        bottom.edge = RibbonEdge::Bottom;
        let set = vec![left, right, bottom];

        // Left panel open → only the left rail is dropped.
        let kept = hide_side_rails_under_open_panels(set.clone(), true, false);
        assert!(!kept.iter().any(|r| r.edge == RibbonEdge::Left));
        assert!(kept.iter().any(|r| r.edge == RibbonEdge::Right));
        assert!(kept.iter().any(|r| r.edge == RibbonEdge::Bottom));

        // Both panels open → both side rails gone, bottom bar stays.
        let kept = hide_side_rails_under_open_panels(set.clone(), true, true);
        assert!(!kept.iter().any(|r| r.edge.is_vertical()));
        assert!(kept.iter().any(|r| r.edge == RibbonEdge::Bottom));

        // No panel open → nothing removed.
        assert_eq!(
            hide_side_rails_under_open_panels(set.clone(), false, false).len(),
            3
        );
    }

    #[test]
    fn maximize_glyph_reflects_state() {
        assert_eq!(maximize_item(false).icon, "maximize");
        assert_eq!(maximize_item(true).icon, "arrow-minimize");
        assert_eq!(maximize_item(false).action, RibbonAction::ToggleMaximize);
    }

    #[test]
    fn shelf_buttons_are_absent_without_side_shelves() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::Start,
            vec![item("system.maximize.item", "maximize", RibbonAction::Noop)],
        )];

        assert!(
            augment_shelf_buttons(
                &ribbons,
                presence(false, false, false),
                false,
                false,
                false,
                ShelfButtonOrder::Featureful
            )
            .is_none(),
            "no published side shelves should not alter the top bar"
        );
    }

    #[test]
    fn shelf_buttons_need_an_existing_top_bar() {
        let ribbons = vec![ResolvedSlotRibbon {
            id: MaraId::new("left.rail"),
            chrome_id: Some("left.rail"),
            scope: RibbonScope::View(mara_core::ViewId::new("test.view")),
            edge: RibbonEdge::Left,
            role: mara_core::ribbon::RibbonRole::Icon,
            mode: mara_core::ribbon::RibbonMode::ThreeSided,
            cluster: RibbonCluster::Start,
            accepts: &[],
            items: vec![item("tool", "cube", RibbonAction::Noop)],
        }];

        assert!(
            augment_shelf_buttons(
                &ribbons,
                presence(true, true, true),
                true,
                true,
                true,
                ShelfButtonOrder::Featureful
            )
            .is_none(),
            "shelf buttons attach to an existing permanent top bar, not to side rails alone"
        );
    }

    #[test]
    fn left_shelf_button_is_inserted_after_maximize_button() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::Start,
            vec![
                item("system.maximize.item", "maximize", RibbonAction::Noop),
                item("view.switch.item", "cube", RibbonAction::Noop),
            ],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(true, false, false),
            true,
            false,
            false,
            ShelfButtonOrder::Featureful,
        )
        .expect("left shelf should add a button");
        let ids: Vec<_> = augmented[0].items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![
                MaraId::new("system.maximize.item"),
                left_shelf_item_id(),
                MaraId::new("view.switch.item"),
            ]
        );
        assert_eq!(augmented[0].items[1].icon, "panel-left");
        assert!(augmented[0].items[1].active);
    }

    #[test]
    fn hidden_declared_shelf_keeps_inactive_top_bar_button() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::Start,
            vec![item("system.maximize.item", "maximize", RibbonAction::Noop)],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(true, false, false),
            false,
            false,
            false,
            ShelfButtonOrder::Featureful,
        )
        .expect("declared hidden left shelf should keep a button for re-opening");
        assert_eq!(augmented[0].items[1].id, left_shelf_item_id());
        assert!(!augmented[0].items[1].active);
    }

    #[test]
    fn right_shelf_button_keeps_close_at_outer_edge_for_featureful_chrome() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::End,
            vec![item("system.close_app", "dismiss", RibbonAction::CloseApp)],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(false, true, false),
            false,
            true,
            false,
            ShelfButtonOrder::Featureful,
        )
        .expect("right shelf should add a button");
        let ids: Vec<_> = augmented[0].items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![MaraId::new("system.close_app"), right_shelf_item_id()]
        );
        assert_eq!(augmented[0].items[1].icon, "panel-right");
    }

    #[test]
    fn right_shelf_button_keeps_close_at_outer_edge_for_simple_painter() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::End,
            vec![item("system.close_app", "dismiss", RibbonAction::CloseApp)],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(false, true, false),
            false,
            true,
            false,
            ShelfButtonOrder::Simple,
        )
        .expect("right shelf should add a button");
        let ids: Vec<_> = augmented[0].items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![right_shelf_item_id(), MaraId::new("system.close_app")]
        );
        assert_eq!(augmented[0].items[0].icon, "panel-right");
    }

    #[test]
    fn bottom_shelf_button_uses_right_side_of_permanent_bar() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::End,
            vec![item("system.close_app", "dismiss", RibbonAction::CloseApp)],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(false, false, true),
            false,
            false,
            true,
            ShelfButtonOrder::Featureful,
        )
        .expect("bottom shelf should add a right-side top-bar button");
        let ids: Vec<_> = augmented[0].items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![MaraId::new("system.close_app"), bottom_shelf_item_id()]
        );
        assert_eq!(augmented[0].items[1].icon, "panel-bottom");
    }

    #[test]
    fn bottom_shelf_button_stays_left_of_right_shelf_button_when_both_exist() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::End,
            vec![item("system.close_app", "dismiss", RibbonAction::CloseApp)],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(false, true, true),
            false,
            true,
            true,
            ShelfButtonOrder::Featureful,
        )
        .expect("right and bottom shelves should add right-side top-bar buttons");
        let ids: Vec<_> = augmented[0].items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![
                MaraId::new("system.close_app"),
                right_shelf_item_id(),
                bottom_shelf_item_id(),
            ]
        );
    }
}

mod pane_body {
    #![allow(deprecated)]
    use mara_core::pane::body::*;
    use mara_core::vocab::Id;
    use mara_core::container::Tab;
    use mara_core::pane::{ContainerSpec, PaneAnchor, TabRoutingScope};
    use mara_core::vocab::{Color32 as MaraColor32, Id as MaraId};

    use mara_core::pane::{RailZone, tab_drag};

    #[test]
    fn tabbed_container_requires_at_least_one_tab() {
        let result = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::tabbed("empty", "Empty", "settings", Vec::new());
        });

        assert!(result.is_err());
    }

    #[test]
    fn containers_require_non_empty_icons() {
        let normal = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::normal("no-icon", "No Icon", "  ", Vec::new());
        });
        let tabbed = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::tabbed(
                "tabs-no-icon",
                "Tabs",
                "",
                vec![Tab::new("main", "Main", "settings")],
            );
        });
        let raw = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::raw_internal("raw-no-icon", "Raw", "", |_| {});
        });

        assert!(normal.is_err());
        assert!(tabbed.is_err());
        assert!(raw.is_err());
    }

    #[test]
    fn containers_require_non_empty_titles() {
        let normal = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::normal("no-title", " ", "settings", Vec::new());
        });
        let tabbed = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::tabbed(
                "tabs-no-title",
                "",
                "settings",
                vec![Tab::new("main", "Main", "settings")],
            );
        });
        let raw = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::raw_internal("raw-no-title", " ", "settings", |_| {});
        });

        assert!(normal.is_err());
        assert!(tabbed.is_err());
        assert!(raw.is_err());
    }

    #[test]
    fn tabbed_container_accepts_tabs_with_icons() {
        let spec = ContainerSpec::tabbed(
            "tabs",
            "Tabs",
            "settings",
            vec![Tab::new("main", "Main", "settings")],
        );

        assert_eq!(spec.container_id(), Id::new("tabs"));
    }

    #[test]
    fn tabbed_container_rejects_duplicate_tab_ids() {
        let result = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::tabbed(
                "tabs",
                "Tabs",
                "settings",
                vec![
                    Tab::new("duplicate", "First", "settings"),
                    Tab::new("duplicate", "Second", "info"),
                ],
            );
        });

        assert!(result.is_err());
    }

    #[test]
    fn pane_rejects_duplicate_tab_ids_across_containers() {
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);
        let pane_id = Id::new("pane");
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            ..Default::default()
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            egui::CentralPanel::default().show(&ctx, |ui| {
                mara_core::memory::MaraMemoryCtx::new(&crate::store_for_ui(ui))
                    .set_temp(mara_core::pane::__internal_active_pane_key(), pane_id);
                let mut backend = crate::EguiUiBackend::new(ui);
                let mut mara = mara_core::MaraUi::over(&mut backend, mara_core::vocab::Color32::WHITE);
                let _ = render_containers(
                    &mut mara,
                    pane_id,
                    PaneAnchor::LeftRail(RailZone::Middle),
                    MaraColor32::from_rgb(120, 160, 220),
                    vec![
                        ContainerSpec::tabbed(
                            "first",
                            "First",
                            "settings",
                            vec![Tab::new("shared-tab", "Shared A", "settings")],
                        ),
                        ContainerSpec::tabbed(
                            "second",
                            "Second",
                            "info",
                            vec![Tab::new("shared-tab", "Shared B", "info")],
                        ),
                    ],
                );
            });
        }));
        let _ = ctx.end_pass();

        assert!(
            result.is_err(),
            "tab ids route per pane, so two containers in one pane must not reuse the same tab id"
        );
    }

    #[test]
    fn pane_rejects_duplicate_container_ids() {
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);
        let pane_id = Id::new("pane");
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            ..Default::default()
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            egui::CentralPanel::default().show(&ctx, |ui| {
                mara_core::memory::MaraMemoryCtx::new(&crate::store_for_ui(ui))
                    .set_temp(mara_core::pane::__internal_active_pane_key(), pane_id);
                let mut backend = crate::EguiUiBackend::new(ui);
                let mut mara = mara_core::MaraUi::over(&mut backend, mara_core::vocab::Color32::WHITE);
                let _ = render_containers(
                    &mut mara,
                    pane_id,
                    PaneAnchor::LeftRail(RailZone::Middle),
                    MaraColor32::from_rgb(120, 160, 220),
                    vec![
                        ContainerSpec::normal("duplicate", "First", "settings", Vec::new()),
                        ContainerSpec::normal("duplicate", "Second", "info", Vec::new()),
                    ],
                );
            });
        }));
        let _ = ctx.end_pass();

        assert!(
            result.is_err(),
            "duplicate container ids would silently overwrite routing/render state"
        );
    }

    #[test]
    fn single_tabbed_container_still_registers_tab_strip() {
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);
        let pane_id = Id::new("pane");
        let container_id = Id::new("single-tab-container");
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            ..Default::default()
        });
        egui::CentralPanel::default().show(&ctx, |ui| {
            mara_core::memory::MaraMemoryCtx::new(&crate::store_for_ui(ui))
                .set_temp(mara_core::pane::__internal_active_pane_key(), pane_id);
            let mut backend = crate::EguiUiBackend::new(ui);
            let mut mara = mara_core::MaraUi::over(&mut backend, mara_core::vocab::Color32::WHITE);
            let responses = render_containers(
                &mut mara,
                pane_id,
                PaneAnchor::LeftRail(RailZone::Middle),
                MaraColor32::from_rgb(120, 160, 220),
                vec![ContainerSpec::tabbed(
                    container_id,
                    "One Tab",
                    "settings",
                    vec![Tab::new("only", "Only", "settings")],
                )],
            );
            assert!(responses.contains_key(&container_id));
            let strips =
                tab_drag::strip_cache(&crate::store_for_ui(ui), pane_id.into());
            let buttons =
                tab_drag::button_cache(&crate::store_for_ui(ui), pane_id.into());
            assert_eq!(
                strips
                    .iter()
                    .filter(|strip| strip.container_id == container_id)
                    .count(),
                1,
                "single-tab tabbed containers must still paint/register their tab strip"
            );
            assert_eq!(
                buttons
                    .iter()
                    .filter(|button| button.container_id == container_id)
                    .count(),
                1,
                "single-tab tabbed containers must still expose one tab button"
            );
        });
        let _ = ctx.end_pass();
    }

    #[test]
    fn shared_tab_scope_renders_moved_tab_with_container_after_pane_change() {
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);
        let routing_id = Id::new("shelf-tab-routing");
        let target_pane = Id::new("target-shelf-pane");
        let source = Id::new("source-container");
        let target = Id::new("target-container");
        let moved_tab = Id::new("moved-tab");
        let source_stay = Id::new("source-stay");
        let target_own = Id::new("target-own");

        let mut source_specs = vec![ContainerSpec::tabbed(
            source,
            "Source",
            "box",
            vec![
                Tab::new(moved_tab, "Moved", "settings"),
                Tab::new(source_stay, "Stay", "info"),
            ],
        )];
        let mut target_specs = vec![ContainerSpec::tabbed(
            target,
            "Target",
            "box",
            vec![Tab::new(target_own, "Own", "settings")],
        )];
        let mut scope = TabRoutingScope::new();
        scope.absorb_specs(&mut source_specs);
        scope.absorb_specs(&mut target_specs);

        tab_drag::commit_drop(
            &ctx,
            routing_id.into(),
            moved_tab.into(),
            source.into(),
            target.into(),
            0,
        );
        assert_eq!(
            tab_drag::route(
                &ctx,
                routing_id.into(),
                target.into(),
                scope.declared_tabs(MaraId::from(target)),
                scope.all_tabs(),
            ),
            vec![moved_tab, target_own],
            "shared routing scope should keep moved tabs attached to their new owner before rendering"
        );

        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            ..Default::default()
        });
        egui::CentralPanel::default().show(&ctx, |ui| {
            mara_core::memory::MaraMemoryCtx::new(&crate::store_for_ui(ui)).set_temp(mara_core::pane::__internal_active_pane_key(), target_pane);
            let mut backend =
                crate::EguiUiBackend::new(ui);
            let mut mara = mara_core::MaraUi::over(&mut backend, mara_core::vocab::Color32::WHITE);
            let responses = render_containers_with_tab_scope(
                &mut mara,
                target_pane,
                routing_id,
                PaneAnchor::LeftRail(RailZone::Middle),
                MaraColor32::from_rgb(120, 160, 220),
                target_specs,
                &mut scope,
                None,
            );

            assert!(responses.contains_key(&target));
            let mut target_buttons: Vec<MaraId> =
                tab_drag::button_cache(&crate::store_for_ui(ui), target_pane.into())
                .into_iter()
                .filter(|button| button.container_id == MaraId::from(target))
                .map(|button| button.tab_id)
                .collect();
            target_buttons.sort_by_key(|id| format!("{id:?}"));
            assert_eq!(
                target_buttons,
                {
                    let mut expected = vec![moved_tab, target_own];
                    expected.sort_by_key(|id| format!("{id:?}"));
                    expected
                },
                "a tab dropped into a container must render with that container after the container moves to a different Shelf pane"
            );
        });
        let _ = ctx.end_pass();
    }
}

mod shelf {
    use mara_core::shelf::*;
    use mara_core::style;
    use mara_core::vocab::{Color32, Id, Rect, pos2, vec2};

    fn test_tabs() -> Vec<mara_core::container::Tab> {
        vec![mara_core::container::Tab::new("test.tab", "Tab", "box")]
    }

    #[test]
    fn show_shelves_sets_public_active_container_for_default_visible_container() {
        // Drives a real frame — `show_shelves` renders, so this one
        // needs the backend rather than a state-only context.
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);
        let shelf_id = Id::new("active-shelf");
        let container_id = Id::new("visible-container");
        let theme = *style::theme().shelf();
        let available = Rect::from_min_size(pos2(0.0, 0.0), vec2(640.0, 480.0));
        let shelves = vec![
            ShelfDef::new(shelf_id, ShelfEdge::Left, Color32::WHITE)
                .default_size(220.0)
                .container(ShelfContainer::tabbed(
                    container_id,
                    "Visible",
                    "box",
                    test_tabs(),
                )),
        ];
        let mut state = ShelfState::default();
        let layout = layout_shelves(available, &shelves, &mut state, &theme);

        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(available.into()),
            ..Default::default()
        });
        __internal_show_shelves_egui(&ctx, layout, shelves, &mut state);
        let _ = ctx.end_pass();

        assert_eq!(
            state.active_container(shelf_id),
            Some(container_id),
            "the public shelf state should mirror the visible default active container"
        );
    }

    #[test]
    fn show_shelves_repairs_stale_public_active_container_from_rendered_group() {
        // Drives a real frame — `show_shelves` renders, so this one
        // needs the backend rather than a state-only context.
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);
        let shelf_id = Id::new("active-shelf");
        let visible_container = Id::new("visible-container");
        let stale_container = Id::new("removed-container");
        let edge = ShelfEdge::Left;
        let theme = *style::theme().shelf();
        let available = Rect::from_min_size(pos2(0.0, 0.0), vec2(640.0, 480.0));
        let shelves = vec![
            ShelfDef::new(shelf_id, edge, Color32::WHITE)
                .default_size(220.0)
                .container(ShelfContainer::tabbed(
                    visible_container,
                    "Visible",
                    "box",
                    test_tabs(),
                )),
        ];
        let mut state = ShelfState::default();
        state.set_active_container(shelf_id, stale_container);
        state.set_active_container_for_group(
            shelf_active_container_key_for(shelf_id, edge),
            visible_container,
        );
        let layout = layout_shelves(available, &shelves, &mut state, &theme);

        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(available.into()),
            ..Default::default()
        });
        __internal_show_shelves_egui(&ctx, layout, shelves, &mut state);
        let _ = ctx.end_pass();

        assert_eq!(
            state.active_container(shelf_id),
            Some(visible_container),
            "public shelf active state should be repaired from the visible rendered group"
        );
    }

    #[test]
    fn show_shelves_clears_active_container_when_no_container_is_visible() {
        // Drives a real frame — `show_shelves` renders, so this one
        // needs the backend rather than a state-only context.
        let raw = egui::Context::default();
        let ctx = crate::EguiCtx::new(&raw);
        let shelf_id = Id::new("empty-shelf");
        let stale_container = Id::new("removed-container");
        let edge = ShelfEdge::Left;
        let theme = *style::theme().shelf();
        let available = Rect::from_min_size(pos2(0.0, 0.0), vec2(640.0, 480.0));
        let shelves = vec![ShelfDef::new(shelf_id, edge, Color32::WHITE).default_size(220.0)];
        let mut state = ShelfState::default();
        state.set_active_container(shelf_id, stale_container);
        state.set_active_container_for_group(
            shelf_active_container_key_for(shelf_id, edge),
            stale_container,
        );
        let layout = layout_shelves(available, &shelves, &mut state, &theme);

        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(available.into()),
            ..Default::default()
        });
        __internal_show_shelves_egui(&ctx, layout, shelves, &mut state);
        let _ = ctx.end_pass();

        assert_eq!(
            state.active_container(shelf_id),
            None,
            "empty shelves must not keep stale public active-container state"
        );
        assert_eq!(
            state.active_container_for_group(shelf_active_container_key_for(shelf_id, edge)),
            None,
            "empty rendered shelf groups must not keep stale active-container state"
        );
    }
}

mod icons {

    #[test]
    fn iconflow_families_keep_proportional_fallbacks() {
        let mut fonts = egui::FontDefinitions::default();
        let proportional = fonts
            .families
            .get(&egui::FontFamily::Proportional)
            .cloned()
            .expect("egui default fonts should expose a proportional fallback chain");

        crate::theme::install_iconflow_fonts(&mut fonts);

        let (_, family) = mara_core::icons::icon_glyph("search").expect("search icon should be bundled");
        let icon_family = egui::FontFamily::Name(family.into());
        let icon_chain = fonts
            .families
            .get(&icon_family)
            .expect("install_iconflow_fonts should bind the icon family");

        assert!(
            icon_chain.len() > 1,
            "icon families need normal text fallback fonts so replacement glyph lookup cannot warn or fail"
        );
        assert_eq!(
            &icon_chain[1..],
            proportional.as_slice(),
            "icon font should be first, followed by the normal proportional fallback chain"
        );
    }
}
