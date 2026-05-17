//! Host-agnostic borderless-window chrome contracts.
//!
//! `mara_core` cannot move or resize a native window by itself:
//! Bevy, eframe, web, and embedded hosts all expose different window
//! APIs. This module owns the shared contract instead:
//!
//! * theme-owned resize hit-test metrics,
//! * published drag/exclusion regions from Mara chrome, and
//! * host-neutral hit-test results that a facade can map onto the
//!   native window operations it supports.

use egui::{Color32, Pos2, Rect, Vec2};

use crate::style::WindowChromeTheme;

/// Host-neutral native-window resize direction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WindowResizeDirection {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

/// Host-neutral borderless-window chrome hit result.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WindowChromeHit {
    Move,
    Resize(WindowResizeDirection),
}

/// Mara-owned native-window interaction regions.
///
/// `drag_regions` are areas where a primary press should start moving
/// the native window. `exclusion_rects` are interactive Mara controls
/// inside those regions (and near resize edges) that must keep their
/// normal click/drag behavior instead of being stolen by native chrome.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowChromeRegions {
    pub drag_regions: Vec<Rect>,
    pub exclusion_rects: Vec<Rect>,
}

/// Native-window chrome capabilities exposed by the current host.
///
/// Browser/web hosts should leave this at the default `false` values:
/// the browser already owns window movement and resizing. Native
/// facades such as Bevy or eframe can publish `true` values before
/// rendering Mara chrome for a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowChromeHostCapabilities {
    pub native_move: bool,
    pub native_resize: bool,
}

/// Host-neutral switches for Mara-owned native-window chrome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowChromePolicy {
    /// Whether Mara should produce native move/resize actions.
    pub enabled: bool,
    /// Allow corner resize hit-testing.
    pub resize: bool,
    /// Allow dragging published empty main-bar regions to move the
    /// native window.
    pub move_from_drag_regions: bool,
}

impl Default for WindowChromePolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            resize: true,
            move_from_drag_regions: true,
        }
    }
}

/// Host-neutral pointer snapshot for window-chrome processing.
///
/// Hosts may leave `primary_down` as `None` when their native drag API
/// can temporarily hide the held button state. If the host has an
/// authoritative UI/input pass, pass `Some(false)` there so stale
/// native claims are released.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowChromeInput {
    pub pointer_pos: Option<Pos2>,
    pub window_size: Vec2,
    pub primary_pressed: bool,
    pub primary_released: bool,
    pub primary_down: Option<bool>,
}

/// Result of one host window-chrome update.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindowChromeUpdate {
    /// Current hover target after policy/exclusion filtering.
    pub hit: Option<WindowChromeHit>,
    /// Native action the host should start this frame.
    pub start: Option<WindowChromeHit>,
    /// Whether app/view input must ignore this frame.
    pub claimed: bool,
}

/// Reusable host-neutral state machine for Mara window chrome.
///
/// Facades own the actual native calls (`start_drag_move`,
/// `start_drag_resize`, eframe viewport commands, etc.). This state
/// machine owns the common policy: hit-testing, stale-claim recovery,
/// and blocking app clicks while a native move/resize is active.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindowChromeState {
    active_claim: bool,
}

impl WindowChromeState {
    #[must_use]
    pub fn claimed(self) -> bool {
        self.active_claim
    }

    pub fn clear_claim(&mut self) {
        self.active_claim = false;
    }

    /// Release an active claim when a host has authoritative knowledge
    /// that the primary pointer is up.
    pub fn release_if_pointer_up(&mut self, primary_down: bool) {
        if !primary_down {
            self.active_claim = false;
        }
    }

    /// Process one host-frame of window chrome.
    pub fn update(
        &mut self,
        regions: &WindowChromeRegions,
        input: WindowChromeInput,
        metrics: WindowChromeTheme,
        policy: WindowChromePolicy,
    ) -> WindowChromeUpdate {
        if input.primary_released {
            self.active_claim = false;
        }
        if let Some(primary_down) = input.primary_down {
            self.release_if_pointer_up(primary_down);
        }

        if !policy.enabled {
            self.active_claim = false;
            return WindowChromeUpdate::default();
        }

        let hit = input
            .pointer_pos
            .and_then(|pos| {
                hit_test_window_chrome_regions(regions, pos, input.window_size, metrics)
            })
            .and_then(|hit| match hit {
                WindowChromeHit::Resize(_) if !policy.resize => None,
                WindowChromeHit::Move if !policy.move_from_drag_regions => None,
                hit => Some(hit),
            });

        // If the compositor/native drag swallowed the release, the
        // next fresh non-chrome press belongs to the app. Clear the
        // stale claim before app UI reads it.
        if self.active_claim && input.primary_pressed && hit.is_none() {
            self.active_claim = false;
        }

        let mut start = None;
        if input.primary_pressed {
            if let Some(hit) = hit {
                self.active_claim = true;
                start = Some(hit);
            }
        }

        WindowChromeUpdate {
            hit,
            start,
            claimed: self.active_claim,
        }
    }
}

fn regions_key() -> egui::Id {
    egui::Id::new("mara_window_chrome_regions")
}

fn host_capabilities_key() -> egui::Id {
    egui::Id::new("mara_window_chrome_host_capabilities")
}

fn input_claimed_key() -> egui::Id {
    egui::Id::new("mara_window_chrome_input_claimed")
}

/// Mark the current input press as owned by native-window chrome.
///
/// Host facades call this when a press starts native move/resize so
/// egui-side app content can ignore the same press for this frame.
pub fn claim_window_chrome_input(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(input_claimed_key(), ctx.cumulative_frame_nr()));
}

/// Whether this frame's pointer press was claimed by native-window
/// chrome.
#[must_use]
pub fn window_chrome_input_claimed(ctx: &egui::Context) -> bool {
    ctx.data(|data| {
        data.get_temp::<u64>(input_claimed_key())
            .is_some_and(|frame| frame == ctx.cumulative_frame_nr())
    })
}

/// Replace the current pass' native-window chrome regions.
///
/// Callers should pass screen-space egui rects. The featureful ribbon
/// renderer publishes the top main-bar strip as a drag region and all
/// ribbon buttons as exclusion rects.
pub fn publish_window_chrome_regions(
    ctx: &egui::Context,
    drag_regions: impl IntoIterator<Item = Rect>,
    exclusion_rects: impl IntoIterator<Item = Rect>,
) {
    let regions = WindowChromeRegions {
        drag_regions: drag_regions.into_iter().collect(),
        exclusion_rects: exclusion_rects.into_iter().collect(),
    };
    ctx.data_mut(|data| data.insert_temp(regions_key(), regions));
}

/// Read the latest published native-window chrome regions.
#[must_use]
pub fn window_chrome_regions(ctx: &egui::Context) -> WindowChromeRegions {
    ctx.data(|data| {
        data.get_temp::<WindowChromeRegions>(regions_key())
            .unwrap_or_default()
    })
}

/// Publish the native-window capabilities for this frame.
///
/// This is intentionally host-controlled: web/browser hosts should not
/// show native resize corners or turn the main bar into a window-drag
/// region.
pub fn publish_window_chrome_host_capabilities(
    ctx: &egui::Context,
    capabilities: WindowChromeHostCapabilities,
) {
    ctx.data_mut(|data| data.insert_temp(host_capabilities_key(), capabilities));
}

/// Read the current frame's host native-window capabilities.
#[must_use]
pub fn window_chrome_host_capabilities(ctx: &egui::Context) -> WindowChromeHostCapabilities {
    ctx.data(|data| {
        data.get_temp::<WindowChromeHostCapabilities>(host_capabilities_key())
            .unwrap_or_default()
    })
}

/// Clear published native-window chrome regions.
pub fn clear_window_chrome_regions(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        data.remove::<WindowChromeRegions>(regions_key());
    });
}

/// Hit-test only the diagonal resize corners.
#[must_use]
pub fn resize_direction(
    pos: Pos2,
    window_size: Vec2,
    metrics: WindowChromeTheme,
) -> Option<WindowResizeDirection> {
    if !pos.x.is_finite()
        || !pos.y.is_finite()
        || !window_size.x.is_finite()
        || !window_size.y.is_finite()
        || window_size.x <= 0.0
        || window_size.y <= 0.0
    {
        return None;
    }

    let extent = metrics.resize_corner_extent.max(0.0);
    let edge = metrics.resize_corner_edge_width.max(0.0).min(extent);
    if extent <= 0.0 || edge <= 0.0 {
        return None;
    }

    let in_left_len = pos.x <= extent;
    let in_right_len = pos.x >= window_size.x - extent;
    let in_top_len = pos.y <= extent;
    let in_bottom_len = pos.y >= window_size.y - extent;
    let on_left_edge = pos.x <= edge;
    let on_right_edge = pos.x >= window_size.x - edge;
    let on_top_edge = pos.y <= edge;
    let on_bottom_edge = pos.y >= window_size.y - edge;

    if (in_left_len && on_top_edge) || (on_left_edge && in_top_len) {
        Some(WindowResizeDirection::NorthWest)
    } else if (in_right_len && on_top_edge) || (on_right_edge && in_top_len) {
        Some(WindowResizeDirection::NorthEast)
    } else if (in_left_len && on_bottom_edge) || (on_left_edge && in_bottom_len) {
        Some(WindowResizeDirection::SouthWest)
    } else if (in_right_len && on_bottom_edge) || (on_right_edge && in_bottom_len) {
        Some(WindowResizeDirection::SouthEast)
    } else {
        None
    }
}

/// Hit-test the complete Mara window-chrome contract.
///
/// Resize corners win over move regions, except where an interactive
/// exclusion rect says the pointer belongs to Mara UI controls.
#[must_use]
pub fn hit_test_window_chrome(
    ctx: &egui::Context,
    pos: Pos2,
    window_size: Vec2,
    metrics: WindowChromeTheme,
) -> Option<WindowChromeHit> {
    let regions = window_chrome_regions(ctx);
    hit_test_window_chrome_regions(&regions, pos, window_size, metrics)
}

/// Hit-test against an explicit set of Mara window-chrome regions.
///
/// Host facades use this from their native input schedules without
/// touching `egui::Context`, avoiding cross-schedule egui locks.
#[must_use]
pub fn hit_test_window_chrome_regions(
    regions: &WindowChromeRegions,
    pos: Pos2,
    window_size: Vec2,
    metrics: WindowChromeTheme,
) -> Option<WindowChromeHit> {
    if regions
        .exclusion_rects
        .iter()
        .any(|rect| rect.contains(pos))
    {
        return None;
    }
    if let Some(direction) = resize_direction(pos, window_size, metrics) {
        return Some(WindowChromeHit::Resize(direction));
    }
    if regions.drag_regions.iter().any(|rect| rect.contains(pos)) {
        return Some(WindowChromeHit::Move);
    }
    None
}

/// Hit-test the current pointer against resize corners only.
#[must_use]
pub fn hovered_resize_corner(
    ctx: &egui::Context,
    window_rect: Rect,
    metrics: WindowChromeTheme,
) -> Option<WindowResizeDirection> {
    let pos = ctx.input(|input| input.pointer.hover_pos())?;
    let regions = window_chrome_regions(ctx);
    if regions
        .exclusion_rects
        .iter()
        .any(|rect| rect.contains(pos))
    {
        return None;
    }
    let local = Pos2::new(pos.x - window_rect.min.x, pos.y - window_rect.min.y);
    resize_direction(local, window_rect.size(), metrics)
}

/// Paint the full L-shaped corner hit area while hovering a resize
/// corner.
///
/// The painted strips match the same few-pixel edge bands used by
/// [`resize_direction`], so the visible affordance is exactly the
/// clickable native-resize area.
pub fn paint_resize_corner_hover(
    ctx: &egui::Context,
    accent: Color32,
    metrics: WindowChromeTheme,
) -> Option<WindowResizeDirection> {
    let window_rect = ctx.viewport_rect();
    let direction = hovered_resize_corner(ctx, window_rect, metrics)?;
    let (horizontal, vertical) = resize_corner_paint_rects(window_rect, direction, metrics)?;

    let painter = ctx.layer_painter(crate::layer::layer_id(
        "mara_window_resize_corner_hover",
        crate::layer::z::WINDOW_CHROME,
    ));
    let fill = accent;
    painter.rect_filled(horizontal, 0.0, fill);
    painter.rect_filled(vertical, 0.0, fill);
    Some(direction)
}

fn resize_corner_paint_rects(
    window_rect: Rect,
    direction: WindowResizeDirection,
    metrics: WindowChromeTheme,
) -> Option<(Rect, Rect)> {
    let extent = metrics.resize_corner_extent.max(0.0);
    let edge = metrics.resize_corner_edge_width.max(0.0).min(extent);
    if extent <= 0.0 || edge <= 0.0 {
        return None;
    }

    let rects = match direction {
        WindowResizeDirection::NorthWest => (
            Rect::from_min_size(window_rect.min, Vec2::new(extent, edge)),
            Rect::from_min_size(window_rect.min, Vec2::new(edge, extent)),
        ),
        WindowResizeDirection::NorthEast => (
            Rect::from_min_size(
                Pos2::new(window_rect.max.x - extent, window_rect.min.y),
                Vec2::new(extent, edge),
            ),
            Rect::from_min_size(
                Pos2::new(window_rect.max.x - edge, window_rect.min.y),
                Vec2::new(edge, extent),
            ),
        ),
        WindowResizeDirection::SouthEast => (
            Rect::from_min_size(
                Pos2::new(window_rect.max.x - extent, window_rect.max.y - edge),
                Vec2::new(extent, edge),
            ),
            Rect::from_min_size(
                Pos2::new(window_rect.max.x - edge, window_rect.max.y - extent),
                Vec2::new(edge, extent),
            ),
        ),
        WindowResizeDirection::SouthWest => (
            Rect::from_min_size(
                Pos2::new(window_rect.min.x, window_rect.max.y - edge),
                Vec2::new(extent, edge),
            ),
            Rect::from_min_size(
                Pos2::new(window_rect.min.x, window_rect.max.y - extent),
                Vec2::new(edge, extent),
            ),
        ),
        // Cardinal directions are not currently produced by
        // `resize_direction`, but keep this exhaustive for callers
        // that may pass future/custom directions.
        WindowResizeDirection::North
        | WindowResizeDirection::East
        | WindowResizeDirection::South
        | WindowResizeDirection::West => return None,
    };
    Some(rects)
}

#[cfg(test)]
mod tests {
    use super::*;

    const METRICS: WindowChromeTheme = WindowChromeTheme {
        resize_corner_extent: 28.0,
        resize_corner_edge_width: 4.8,
    };

    fn vec2_nearly_eq(a: Vec2, b: Vec2) -> bool {
        (a.x - b.x).abs() <= 0.001 && (a.y - b.y).abs() <= 0.001
    }

    #[test]
    fn only_resize_corners_are_hit_tested() {
        let size = Vec2::new(800.0, 600.0);
        assert_eq!(resize_direction(Pos2::new(3.0, 300.0), size, METRICS), None);
        assert_eq!(resize_direction(Pos2::new(300.0, 3.0), size, METRICS), None);
        assert_eq!(
            resize_direction(Pos2::new(797.0, 3.0), size, METRICS),
            Some(WindowResizeDirection::NorthEast)
        );
        assert_eq!(
            resize_direction(Pos2::new(797.0, 597.0), size, METRICS),
            Some(WindowResizeDirection::SouthEast)
        );
        assert_eq!(
            resize_direction(Pos2::new(3.0, 597.0), size, METRICS),
            Some(WindowResizeDirection::SouthWest)
        );
        assert_eq!(
            resize_direction(Pos2::new(20.0, 4.0), size, METRICS),
            Some(WindowResizeDirection::NorthWest)
        );
        assert_eq!(
            resize_direction(Pos2::new(4.0, 20.0), size, METRICS),
            Some(WindowResizeDirection::NorthWest)
        );
        assert_eq!(resize_direction(Pos2::new(20.0, 5.0), size, METRICS), None);
        assert_eq!(resize_direction(Pos2::new(5.0, 20.0), size, METRICS), None);
        assert_eq!(resize_direction(Pos2::new(20.0, 20.0), size, METRICS), None);
        assert_eq!(resize_direction(Pos2::new(80.0, 80.0), size, METRICS), None);
    }

    #[test]
    fn resize_corner_paint_rects_are_symmetric_l_shapes() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(800.0, 600.0));
        for direction in [
            WindowResizeDirection::NorthWest,
            WindowResizeDirection::NorthEast,
            WindowResizeDirection::SouthEast,
            WindowResizeDirection::SouthWest,
        ] {
            let (horizontal, vertical) = resize_corner_paint_rects(rect, direction, METRICS)
                .expect("diagonal resize corner should paint an L affordance");
            assert!(
                vec2_nearly_eq(
                    horizontal.size(),
                    Vec2::new(
                        METRICS.resize_corner_extent,
                        METRICS.resize_corner_edge_width
                    )
                ),
                "{direction:?} horizontal strip size"
            );
            assert!(
                vec2_nearly_eq(
                    vertical.size(),
                    Vec2::new(
                        METRICS.resize_corner_edge_width,
                        METRICS.resize_corner_extent
                    )
                ),
                "{direction:?} vertical strip size"
            );
        }
    }

    #[test]
    fn interactive_exclusions_win_over_resize_and_move() {
        let ctx = egui::Context::default();
        publish_window_chrome_regions(
            &ctx,
            [Rect::from_min_max(
                Pos2::new(0.0, 0.0),
                Pos2::new(800.0, 34.0),
            )],
            [Rect::from_min_max(
                Pos2::new(4.0, 4.0),
                Pos2::new(34.0, 34.0),
            )],
        );
        assert_eq!(
            hit_test_window_chrome(&ctx, Pos2::new(5.0, 5.0), Vec2::new(800.0, 600.0), METRICS),
            None
        );
        assert_eq!(
            hit_test_window_chrome(
                &ctx,
                Pos2::new(300.0, 20.0),
                Vec2::new(800.0, 600.0),
                METRICS
            ),
            Some(WindowChromeHit::Move)
        );
    }

    #[test]
    fn host_chrome_capabilities_default_to_web_safe_disabled() {
        let ctx = egui::Context::default();
        assert_eq!(
            window_chrome_host_capabilities(&ctx),
            WindowChromeHostCapabilities::default()
        );

        let native_caps = WindowChromeHostCapabilities {
            native_move: true,
            native_resize: true,
        };
        publish_window_chrome_host_capabilities(&ctx, native_caps);
        assert_eq!(window_chrome_host_capabilities(&ctx), native_caps);
    }

    #[test]
    fn chrome_state_claims_until_authoritative_release() {
        let regions = WindowChromeRegions {
            drag_regions: vec![Rect::from_min_max(
                Pos2::new(0.0, 0.0),
                Pos2::new(800.0, 34.0),
            )],
            exclusion_rects: Vec::new(),
        };
        let mut state = WindowChromeState::default();
        let press = state.update(
            &regions,
            WindowChromeInput {
                pointer_pos: Some(Pos2::new(300.0, 20.0)),
                window_size: Vec2::new(800.0, 600.0),
                primary_pressed: true,
                primary_released: false,
                primary_down: None,
            },
            METRICS,
            WindowChromePolicy::default(),
        );
        assert_eq!(press.start, Some(WindowChromeHit::Move));
        assert!(press.claimed);

        let held_by_native_drag = state.update(
            &regions,
            WindowChromeInput {
                pointer_pos: Some(Pos2::new(300.0, 120.0)),
                window_size: Vec2::new(800.0, 600.0),
                primary_pressed: false,
                primary_released: false,
                primary_down: None,
            },
            METRICS,
            WindowChromePolicy::default(),
        );
        assert!(held_by_native_drag.claimed);

        state.release_if_pointer_up(false);
        assert!(!state.claimed());
    }

    #[test]
    fn stale_chrome_claim_clears_on_next_non_chrome_press() {
        let regions = WindowChromeRegions {
            drag_regions: vec![Rect::from_min_max(
                Pos2::new(0.0, 0.0),
                Pos2::new(800.0, 34.0),
            )],
            exclusion_rects: Vec::new(),
        };
        let mut state = WindowChromeState::default();
        state.update(
            &regions,
            WindowChromeInput {
                pointer_pos: Some(Pos2::new(300.0, 20.0)),
                window_size: Vec2::new(800.0, 600.0),
                primary_pressed: true,
                primary_released: false,
                primary_down: None,
            },
            METRICS,
            WindowChromePolicy::default(),
        );
        assert!(state.claimed());

        let next_app_press = state.update(
            &regions,
            WindowChromeInput {
                pointer_pos: Some(Pos2::new(300.0, 120.0)),
                window_size: Vec2::new(800.0, 600.0),
                primary_pressed: true,
                primary_released: false,
                primary_down: None,
            },
            METRICS,
            WindowChromePolicy::default(),
        );
        assert_eq!(next_app_press.start, None);
        assert!(!next_app_press.claimed);
        assert!(!state.claimed());
    }
}
