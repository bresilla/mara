# NOTES.md

Gotchas worth remembering. Each entry is one numbered paragraph; if you
hit a weird bug whose root cause was far from the symptom, add it here.

1. **Pane axes are `flow` / `span`, not horizontal / vertical.** A
   pane's `flow` axis is perpendicular to its title strip — the
   direction the body extends from the title. The `span` axis runs
   parallel to the title — the title's own length and the pane's
   cross dimension. Names stay correct regardless of which rail the
   pane lives on. Surface area: `pane::user_flow` / `pane::user_span`
   (user-resized extents), `PaneResize { flow, span }` builder,
   `Normal::body_flow(...)`, and theme fields
   `section_outer_margin_flow_title` / `section_outer_margin_flow_body`
   / `section_outer_margin_span`.

2. **Pane SHAPE (horizontal / vertical) is set by the TITLE SIDE, not
   by the rail.** Corner zones (`Start` / `End`) flip the title
   perpendicular to the rail, so a single rail mixes pane shapes:
   `LeftRail::Start` and `LeftRail::End` both have a horizontal title
   (`TitleSide::Top` / `Bottom`) and are *horizontal panes*, while
   `LeftRail::Middle` has a vertical title (`TitleSide::Left`) and is
   a *vertical pane*. The right test is
   `anchor.title_side().is_horizontal_strip()`. `PaneAnchor::is_vertical_pane()`
   actually returns "lives on a vertical rail", which is NOT the same
   thing — keep that distinction in mind when reading older code.
