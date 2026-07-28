//! In-pane container widgets — visual blocks the caller drops into a
//! [`crate::pane::Pane`] body to organise content. A container holds
//! [`crate::pod::Pod`]s and (between consecutive pods) separators —
//! see [`SeparatorStyle`].
//!
//! Variants:
//!
//! * [`normal`] — single title bar above a single body. The default
//!   building block; equivalent to a section.
//! * [`tabbed`] — multiple labelled bodies behind a tab strip
//!   (placeholder; not yet implemented).

pub mod body;
pub mod normal;
pub mod separator;
pub mod tabbed;

pub use body::Body;
pub use normal::Normal;
pub use separator::{SeparatorOrient, SeparatorStyle};
pub(crate) use separator::{paint_separator, paint_separator_resize};
pub use tabbed::Tab;

use crate::vocab::Id;

use crate::vocab::Id as MaraId;

/// First-frame default flow-axis size, used before any content has
/// been measured AND before any drag has set an explicit value.
pub const CONTAINER_DEFAULT_FLOW: f32 = 200.0;
/// Hard minimum on a *vertically-stacked* container's flow
/// (= height in horizontal-strip panes — TM/BM). Content drives
/// height; this is just the absolute floor so the user can't
/// collapse it to nothing.
pub const CONTAINER_MIN_FLOW: f32 = crate::style::UNIT;
/// Upper bound on any container's persisted flow size.
pub const CONTAINER_MAX_FLOW: f32 = 1200.0;
/// Auto-fit cap for vertically-stacked containers. Kept public so
/// callers that want a hard cap can pass it as an
/// explicit `Normal::initial_flow` override. The default
/// auto-fit path (in [`container_flow`]) NO LONGER applies this
/// cap; intrinsic content height is honoured up to
/// [`CONTAINER_MAX_FLOW`], so widgets that expand on demand (color
/// picker, future tree, …) extend the container to fit their
/// content rather than getting clipped behind a `ScrollArea`.
pub const CONTAINER_AUTOFIT_CAP: f32 = 10.0 * crate::style::UNIT;
/// Hard minimum for *horizontally-stacked* containers (= width
/// in vertical-strip panes — LM/RM). Their content doesn't drive
/// width (the pods stack vertically inside, so width is just
/// "available width"), so we pin a deliberate floor — wide enough
/// for a search field + chrome to read without cramping. The user
/// CAN'T drag below this.
pub const CONTAINER_HORIZONTAL_MIN_FLOW: f32 = 12.0 * crate::style::UNIT;
/// First-frame default for horizontally-stacked containers — sits
/// 3U above the floor so a fresh container has room for a wider
/// dropdown / labelled-select before the user has to drag it bigger,
/// without overshooting the small accents the floor protects.
pub const CONTAINER_HORIZONTAL_DEFAULT_FLOW: f32 = 15.0 * crate::style::UNIT;

fn container_flow_key(cid: Id) -> Id {
    // Persisted ONLY when `set_container_flow` is called (= the
    // user has dragged the resize handle). Presence of this key
    // means "user has overridden the auto-fit"; absence means
    // "auto-fit from measured content".
    cid.with("mara_container_flow")
}

fn container_intrinsic_key(cid: Id) -> Id {
    // Measured content size of the container's body, written every
    // frame after the body renders (see
    // [`record_container_intrinsic`]). Read by [`container_flow`]
    // when no explicit user value is persisted, capped at
    // [`CONTAINER_AUTOFIT_CAP`] so very tall content scrolls
    // rather than ballooning the container.
    cid.with("mara_container_intrinsic")
}

fn container_initial_flow_key(cid: Id) -> Id {
    // Per-container override for the autofit cap (vertically-
    // stacked) or the fixed default (horizontally-stacked). Set
    // by `Normal::initial_flow` and read by `container_flow`. If
    // unset, the global `CONTAINER_AUTOFIT_CAP` /
    // `CONTAINER_HORIZONTAL_DEFAULT_FLOW` apply.
    cid.with("mara_container_initial_flow")
}

/// Read the per-container override for the autofit cap / default
/// flow set by [`crate::container::Normal::initial_flow`]. Returns
/// `None` when the container has no override (use the global
/// default in that case).
pub(crate) fn container_initial_flow(
    ctx: &dyn crate::context::MaraCtx,
    cid: impl Into<MaraId>,
) -> Option<f32> {
    let cid: Id = cid.into().into();
    ctx.memory()
        .get_persisted::<f32>(container_initial_flow_key(cid))
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(CONTAINER_MIN_FLOW, CONTAINER_MAX_FLOW))
}

/// Write the per-container default-flow override. Called by
/// [`crate::container::Normal::show`] when the builder set
/// `initial_flow`. Subsequent calls overwrite — the most recent
/// value wins.
pub(crate) fn set_container_initial_flow(
    ctx: &dyn crate::context::MaraCtx,
    cid: impl Into<MaraId>,
    value: f32,
) {
    let cid: Id = cid.into().into();
    let v = if value.is_finite() {
        value.clamp(CONTAINER_MIN_FLOW, CONTAINER_MAX_FLOW)
    } else {
        f32::NAN
    };
    ctx.memory()
        .set_persisted(container_initial_flow_key(cid), v);
}

/// Read the flow-axis size the container should render at. The
/// resolution depends on the parent pane's orientation —
/// `is_horizontal_strip == true` means the container lives in a
/// horizontal-strip pane (Top/Bottom title), where containers
/// stack VERTICALLY and flow = height; `false` means a vertical-
/// strip pane (Left/Right title), where containers stack
/// HORIZONTALLY and flow = width.
///
/// **Vertically-stacked (`is_horizontal_strip == true`)** —
/// content height varies, so the size is content-driven:
///
/// 1. Explicit user drag (persisted via [`set_container_flow`])
///    if any, clamped `[CONTAINER_MIN_FLOW, CONTAINER_MAX_FLOW]`.
/// 2. Otherwise auto-fit: previous frame's measured content height,
///    capped at [`CONTAINER_AUTOFIT_CAP`] (`8U`). Past the cap,
///    the body's `ScrollArea` takes over.
/// 3. First frame, no measurement yet: [`CONTAINER_DEFAULT_FLOW`].
///
/// **Horizontally-stacked (`is_horizontal_strip == false`)** —
/// content doesn't drive width (pods stack vertically inside, so
/// width is just available-width), so the size is fixed:
///
/// 1. Explicit user drag, clamped
///    `[CONTAINER_HORIZONTAL_MIN_FLOW, CONTAINER_MAX_FLOW]`.
/// 2. Default [`CONTAINER_HORIZONTAL_DEFAULT_FLOW`] (= 12U).
///    The user cannot drag below 12U.
pub(crate) fn container_flow(
    ctx: &dyn crate::context::MaraCtx,
    cid: impl Into<MaraId>,
    is_horizontal_strip: bool,
) -> f32 {
    let cid: Id = cid.into().into();
    let (min_v, max_v) = container_flow_bounds(is_horizontal_strip);
    if let Some(user) = ctx.memory().get_persisted::<f32>(container_flow_key(cid)) {
        let fallback = if is_horizontal_strip {
            CONTAINER_DEFAULT_FLOW
        } else {
            CONTAINER_HORIZONTAL_DEFAULT_FLOW
        };
        let repaired = sanitize_flow(user, fallback, min_v, max_v);
        if repaired != user {
            ctx.memory()
                .set_persisted(container_flow_key(cid), repaired);
        }
        return repaired;
    }
    // Per-container override set via `Normal::initial_flow` —
    // replaces the static "no measurement yet" default with a
    // caller-chosen value. Once intrinsic content has been recorded,
    // the container tracks that directly so widgets that expand on
    // demand (color picker, future tree, …) extend the container to
    // fit their content rather than getting clipped behind a
    // ScrollArea. `MAX_FLOW` is still the upper backstop so a
    // runaway body doesn't dominate the whole pane.
    let override_default = container_initial_flow(ctx, cid);
    if is_horizontal_strip {
        if let Some(intrinsic) = ctx
            .memory()
            .get_persisted::<f32>(container_intrinsic_key(cid))
        {
            let fallback = override_default.unwrap_or(CONTAINER_DEFAULT_FLOW);
            let repaired = sanitize_flow(intrinsic, fallback, min_v, max_v);
            if repaired != intrinsic {
                ctx.memory()
                    .set_persisted(container_intrinsic_key(cid), repaired);
            }
            return repaired;
        }
        // No measurement yet → fall back to the override (if any)
        // or the static `CONTAINER_DEFAULT_FLOW`. This way the
        // very first frame already shows the caller's chosen size
        // rather than 200 px of empty space.
        override_default
            .unwrap_or(CONTAINER_DEFAULT_FLOW)
            .clamp(min_v, max_v)
    } else {
        override_default
            .unwrap_or(CONTAINER_HORIZONTAL_DEFAULT_FLOW)
            .clamp(min_v, max_v)
    }
}

/// Persist an explicit user override for the container's flow
/// size — called from the inter-container drag handler. Clamped to
/// the orientation-specific bounds before writing, so the user can't
/// drag below 12U on horizontally-stacked containers.
pub(crate) fn set_container_flow(
    ctx: &dyn crate::context::MaraCtx,
    cid: impl Into<MaraId>,
    value: f32,
    is_horizontal_strip: bool,
) {
    let cid: Id = cid.into().into();
    let (min_v, max_v) = container_flow_bounds(is_horizontal_strip);
    let fallback = if is_horizontal_strip {
        CONTAINER_DEFAULT_FLOW
    } else {
        CONTAINER_HORIZONTAL_DEFAULT_FLOW
    };
    let v = sanitize_flow(value, fallback, min_v, max_v);
    ctx.memory().set_persisted(container_flow_key(cid), v);
}

/// `(min, max)` bounds for a container's flow size based on the
/// parent pane's orientation. Vertically-stacked containers can
/// shrink to `UNIT`; horizontally-stacked containers have a hard
/// `12U` floor.
pub(crate) fn container_flow_bounds(is_horizontal_strip: bool) -> (f32, f32) {
    if is_horizontal_strip {
        (CONTAINER_MIN_FLOW, CONTAINER_MAX_FLOW)
    } else {
        (CONTAINER_HORIZONTAL_MIN_FLOW, CONTAINER_MAX_FLOW)
    }
}

/// Persist the measured intrinsic body content size for `cid`.
/// Called by [`Normal::show`] every frame after the body renders.
/// Read by [`container_flow`]'s auto-fit path on subsequent frames.
pub(crate) fn record_container_intrinsic(
    ctx: &dyn crate::context::MaraCtx,
    cid: impl Into<MaraId>,
    height: f32,
) {
    let cid: Id = cid.into().into();
    let v = if height.is_finite() {
        height.max(0.0)
    } else {
        CONTAINER_DEFAULT_FLOW
    };
    ctx.memory().set_persisted(container_intrinsic_key(cid), v);
}

fn sanitize_flow(value: f32, fallback: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback.clamp(min, max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_flow_sanitizes_non_finite_user_values() {
        let ctx = headless_ctx();
        let vertical = Id::new("vertical-container");
        let horizontal = Id::new("horizontal-container");

        set_container_flow(&ctx, vertical, f32::NAN, true);
        set_container_flow(&ctx, horizontal, f32::INFINITY, false);

        assert_eq!(container_flow(&ctx, vertical, true), CONTAINER_DEFAULT_FLOW);
        assert_eq!(
            container_flow(&ctx, horizontal, false),
            CONTAINER_HORIZONTAL_DEFAULT_FLOW
        );
    }

    #[test]
    fn container_initial_flow_ignores_non_finite_overrides() {
        let ctx = headless_ctx();
        let cid = Id::new("initial-flow-container");

        set_container_initial_flow(&ctx, cid, f32::NAN);

        assert_eq!(container_initial_flow(&ctx, cid), None);
        assert_eq!(container_flow(&ctx, cid, true), CONTAINER_DEFAULT_FLOW);
        assert_eq!(
            container_flow(&ctx, cid, false),
            CONTAINER_HORIZONTAL_DEFAULT_FLOW
        );
    }

    #[test]
    fn intrinsic_flow_sanitizes_non_finite_measurements() {
        let ctx = headless_ctx();
        let cid = Id::new("intrinsic-container");

        record_container_intrinsic(&ctx, cid, f32::NEG_INFINITY);

        assert_eq!(container_flow(&ctx, cid, true), CONTAINER_DEFAULT_FLOW);
    }

    #[test]
    fn container_flow_clamps_to_orientation_bounds() {
        let ctx = headless_ctx();
        let vertical = Id::new("vertical-clamp-container");
        let horizontal = Id::new("horizontal-clamp-container");

        set_container_flow(&ctx, vertical, -100.0, true);
        set_container_flow(&ctx, horizontal, -100.0, false);

        assert_eq!(container_flow(&ctx, vertical, true), CONTAINER_MIN_FLOW);
        assert_eq!(
            container_flow(&ctx, horizontal, false),
            CONTAINER_HORIZONTAL_MIN_FLOW
        );
    }
}

/// A context for state-only assertions — see the note in
/// `shelf::tests`. The recording backend is a `MaraCtx`, so tests that
/// only exercise Mara's own bookkeeping need no backend.
#[cfg(test)]
fn headless_ctx() -> crate::backend::record::RecordingBackend {
    crate::backend::record::RecordingBackend::at(crate::vocab::Rect::from_min_size(
        crate::vocab::Pos2::ZERO,
        crate::vocab::Vec2::new(1280.0, 800.0),
    ))
}
