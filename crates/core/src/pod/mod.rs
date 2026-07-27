//! # `Pod` — a configurable widget host that lives inside a container
//!
//! A pod is the *only* thing a container's body accepts. It hosts
//! one or more widgets (`text_input`, button, toggle, …) and
//! arranges them into a single visual unit.
//!
//! Widgets stack vertically in declaration order. Each widget's
//! per-frame response is collected into [`PodResponse`].
//!
//! ## Building one
//!
//! ```ignore
//! let pod = Pod::new(cid.with("settings"))
//!     .with_search("type something…", accent)
//!     .with_toggle("enabled", &mut toggle_state, accent)
//!     .with_button("apply", accent);
//! let resp = Normal::new(title, anchor, accent, cid).show(ui, vec![pod]);
//! if resp[0].buttons.first().map_or(false, |b| b.clicked) { ... }
//! ```

use crate::vocab::Id;
use egui::Ui;

use crate::container::SeparatorStyle;
use crate::memory::MaraMemory;
use crate::module::{MaraModule, ModuleInlineCtx, ModuleInlineOptions};
use crate::style::{UNIT, theme};
use crate::vocab::{Color32, Id as MaraId};
use crate::widget::{
    badge::badge_row_backend,
    button::{Button, FillStyle},
    chip::{chip_colored_backend, chip_fill},
    color::{color_rgb, color_rgba},
    drag_value::drag_value_backend,
    dropdown::dropdown,
    keybinding::keybinding_row_backend,
    progressbar::progressbar_backend,
    readout::readout_backend,
    select::{hybrid_select_row_backend, select_row_backend},
    slider::slider_backend,
    text_input::text_input,
    toggle::toggle_backend,
};

// ─── Per-widget responses ─────────────────────────────────────────

/// What a [`Pod`] surfaces to the caller per frame. One vec per
/// widget kind, in declaration order within that kind.
#[derive(Clone, Debug, Default)]
pub struct PodResponse {
    pub searches: Vec<SearchResponse>,
    pub buttons: Vec<ButtonResponse>,
    pub card_buttons: Vec<ButtonResponse>,
    pub action_buttons: Vec<ActionButtonPodResponse>,
    pub toggles: Vec<ToggleResponse>,
    pub progress: Vec<ProgressResponse>,
    pub sliders: Vec<SliderResponse>,
    pub drag_values: Vec<DragValueResponse>,
    pub dropdowns: Vec<DropdownResponse>,
    pub selects: Vec<SelectResponse>,
    pub hybrid_selects: Vec<HybridSelectPodResponse>,
    pub colors: Vec<ColorResponse>,
    pub readouts: Vec<ReadoutResponse>,
    pub select_lists: Vec<SelectListResponse>,
    pub hybrid_select_lists: Vec<HybridSelectListResponse>,
    pub tags: Vec<TagsResponse>,
    pub keybindings: Vec<KeybindingsResponse>,
    pub badges: Vec<BadgesResponse>,
    pub modules: Vec<ModulePodResponse>,
}

#[derive(Clone, Debug, Default)]
pub struct SearchResponse {
    pub query: String,
    pub changed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ButtonResponse {
    pub clicked: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ActionButtonPodResponse {
    pub body_clicked: bool,
    pub body_double_clicked: bool,
    pub action_clicked: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ToggleResponse {
    pub on: bool,
    pub changed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ProgressResponse;

#[derive(Clone, Debug, Default)]
pub struct SliderResponse {
    pub value: f64,
    pub changed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct DragValueResponse {
    pub value: f64,
    pub changed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct DropdownResponse {
    pub selected: usize,
    pub changed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SelectResponse {
    pub clicked: bool,
    pub double_clicked: bool,
    pub selected: bool,
}

#[derive(Clone, Debug, Default)]
pub struct HybridSelectPodResponse {
    pub body_clicked: bool,
    pub body_double_clicked: bool,
    pub radio_clicked: bool,
    pub selected: bool,
    pub radio_on: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ColorResponse {
    /// RGBA in 0.0..=1.0. For `with_color_rgb`, alpha is always 1.0.
    pub rgba: [f32; 4],
    pub changed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ReadoutResponse;

#[derive(Clone, Debug, Default)]
pub struct SelectListResponse {
    /// Index of the row that was clicked this frame, if any.
    pub clicked: Option<usize>,
    /// Index of the row that was double-clicked this frame, if any.
    pub double_clicked: Option<usize>,
    /// Persisted "currently selected" index for the list.
    pub selected: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct HybridSelectListResponse {
    /// Body click target — same as `SelectListResponse::clicked`.
    pub body_clicked: Option<usize>,
    pub body_double_clicked: Option<usize>,
    /// Right-edge radio click — independent from body.
    pub radio_clicked: Option<usize>,
    pub selected: Option<usize>,
    /// Persisted "pinned" radio index — at most one row pinned at a
    /// time (the radio is single-select, like a real radio group).
    pub pinned: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct TagsResponse {
    /// Index of the chip that was clicked this frame, if any.
    pub clicked: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct KeybindingsResponse;

#[derive(Clone, Debug, Default)]
pub struct BadgesResponse;

#[derive(Clone, Debug)]
pub struct ModulePodResponse {
    pub id: MaraId,
    pub title: String,
    pub icon: &'static str,
    pub enter_workspace_requested: bool,
}

// ─── Widget specs ─────────────────────────────────────────────────

#[derive(Clone)]
struct SearchConfig {
    placeholder: String,
    accent: Color32,
}

#[derive(Clone)]
struct ButtonConfig {
    label: String,
    accent: Color32,
    /// Optional second-row caption beneath the label (small dim text).
    /// When `Some`, the button paints in the 2-row "card" shape and
    /// reports its result in `PodResponse::card_buttons` instead of
    /// `buttons`, so callers that rely on the index split keep
    /// matching the right wire.
    subtitle: Option<String>,
    /// Optional leading icon glyph painted in `accent`.
    glyph: Option<String>,
    /// Optional CSS-style hover-fill animation. `None` falls back to
    /// the standard hover/press tint.
    animation: Option<FillStyle>,
}

#[derive(Clone)]
struct ActionButtonConfig {
    label: String,
    subtitle: Option<String>,
    glyph: Option<String>,
    action_glyph: String,
    action_tooltip: Option<String>,
    action_armed: bool,
    accent: Color32,
}

#[derive(Clone)]
struct ToggleConfig {
    label: String,
    accent: Color32,
    /// If `Some`, override the persisted state with this value.
    /// Caller can use this to drive the toggle from external state
    /// instead of relying on the ctx-data cache.
    initial: Option<bool>,
}

#[derive(Clone)]
struct ProgressConfig {
    label: String,
    fraction: f32,
    text: String,
    accent: Color32,
}

#[derive(Clone)]
struct SliderConfig {
    label: String,
    value: f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    suffix: String,
    accent: Color32,
}

#[derive(Clone)]
struct DragValueConfig {
    label: String,
    value: f64,
    speed: f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    suffix: String,
}

#[derive(Clone)]
struct DropdownConfig {
    options: Vec<String>,
    initial: usize,
    accent: Color32,
}

#[derive(Clone)]
struct SelectConfig {
    label: String,
    trailing: Option<String>,
    selected_initial: bool,
    accent: Color32,
}

#[derive(Clone)]
struct HybridSelectConfig {
    label: String,
    trailing: Option<String>,
    selected_initial: bool,
    radio_initial: bool,
    accent: Color32,
}

#[derive(Clone)]
struct ColorConfig {
    label: String,
    initial: [f32; 4],
    /// `true` shows the alpha slider in the picker (RGBA);
    /// `false` keeps it opaque (RGB).
    alpha: bool,
    accent: Color32,
}

#[derive(Clone)]
struct ReadoutConfig {
    label: String,
    value: String,
}

#[derive(Clone)]
struct SelectListConfig {
    items: Vec<String>,
    /// Optional trailing text per item (e.g. `#3`, `(2.4 MB)`).
    /// When `Some`, length must match `items.len()`. When `None`,
    /// rows render with no trailing column.
    trailing: Option<Vec<String>>,
    accent: Color32,
}

#[derive(Clone)]
struct HybridSelectListConfig {
    items: Vec<String>,
    trailing: Option<Vec<String>>,
    accent: Color32,
}

/// One chip in a [`Pod::with_tags`] cluster.
#[derive(Clone, Debug)]
pub struct TagItem {
    pub label: String,
    /// `None` → faint accent-tinted glass fill (default chip look).
    /// `Some(c)` → solid fill with `c` (e.g. `style::WARNING` for
    /// status / severity chips).
    pub fill: Option<Color32>,
}

impl TagItem {
    pub fn new(label: impl Into<String>) -> Self {
        let label = label.into();
        assert_non_empty("tag chips", "label", &label);
        Self { label, fill: None }
    }
    pub fn colored(label: impl Into<String>, fill: impl Into<Color32>) -> Self {
        let label = label.into();
        let fill = fill.into();
        assert_non_empty("tag chips", "label", &label);
        Self {
            label,
            fill: Some(fill),
        }
    }
}

#[derive(Clone)]
struct TagsConfig {
    items: Vec<TagItem>,
    accent: Color32,
}

#[derive(Clone)]
struct KeybindingsConfig {
    rows: Vec<(String, String)>,
}

/// One row of a [`Pod::with_badges`] cluster: a label on the left
/// and a list of tag chips on the right (e.g.
/// `lights  [12 dir] [4 pt] [2 spot]`).
#[derive(Clone, Debug)]
pub struct BadgeRowSpec {
    pub label: String,
    pub badges: Vec<TagItem>,
}

impl BadgeRowSpec {
    pub fn new(label: impl Into<String>, badges: Vec<TagItem>) -> Self {
        let label = label.into();
        assert_non_empty("badge rows", "label", &label);
        assert!(!badges.is_empty(), "badge rows require at least one badge");
        Self { label, badges }
    }

    /// Convenience: build a row from plain strings — every chip uses
    /// the default accent-tinted glass fill.
    pub fn from_strs(
        label: impl Into<String>,
        badges: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::new(label, badges.into_iter().map(TagItem::new).collect())
    }
}

fn assert_non_empty(component: &str, field: &str, value: &str) {
    assert!(
        !value.trim().is_empty(),
        "{component} require a non-empty {field}"
    );
}

fn assert_optional_non_empty(component: &str, field: &str, value: Option<&str>) {
    if let Some(value) = value {
        assert_non_empty(component, field, value);
    }
}

fn assert_non_empty_items(component: &str, items: &[String]) {
    assert!(!items.is_empty(), "{component} require at least one item");
    assert!(
        items.iter().all(|item| !item.trim().is_empty()),
        "{component} require every item to be non-empty"
    );
}

fn assert_finite(component: &str, field: &str, value: f64) {
    assert!(value.is_finite(), "{component} require a finite {field}");
}

fn assert_fraction(component: &str, field: &str, value: f32) {
    assert!(
        value.is_finite() && (0.0..=1.0).contains(&value),
        "{component} require {field} to be finite and in 0.0..=1.0"
    );
}

fn assert_finite_range(component: &str, range: &std::ops::RangeInclusive<f64>) {
    let start = *range.start();
    let end = *range.end();
    assert_finite(component, "range start", start);
    assert_finite(component, "range end", end);
    assert!(
        start <= end,
        "{component} require range start to be <= range end"
    );
}

fn assert_value_in_range(component: &str, value: f64, range: &std::ops::RangeInclusive<f64>) {
    assert_finite(component, "value", value);
    assert_finite_range(component, range);
    assert!(
        range.contains(&value),
        "{component} require value to be inside the configured range"
    );
}

fn assert_color_channels(component: &str, channels: &[f32]) {
    assert!(
        channels
            .iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel)),
        "{component} require every channel to be finite and in 0.0..=1.0"
    );
}

#[derive(Clone)]
struct BadgesConfig {
    rows: Vec<BadgeRowSpec>,
    accent: Color32,
}

struct ModuleConfig {
    options: ModuleInlineOptions,
    module: Box<dyn MaraModule + Send + Sync>,
}

/// One ordered slot in the pod's widget stack. Painted in
/// declaration order; response indices match the order each widget
/// kind was added (e.g. the third `with_button` shows up at
/// `response.buttons[2]`).
///
/// Not `Clone` — the [`WidgetSpec::Custom`] variant carries a move-only
/// closure (`Box<dyn FnOnce>`). Pod consumes its widget vec on
/// `show(self, ui)`, so cloning was never needed in the first place;
/// removing the derive lets the custom variant exist without
/// special-casing.
enum WidgetSpec {
    Search(SearchConfig),
    Button(ButtonConfig),
    ActionButton(ActionButtonConfig),
    Toggle(ToggleConfig),
    Progress(ProgressConfig),
    Slider(SliderConfig),
    DragValue(DragValueConfig),
    Dropdown(DropdownConfig),
    Select(SelectConfig),
    HybridSelect(HybridSelectConfig),
    Color(ColorConfig),
    Readout(ReadoutConfig),
    /// Multi-row select list — ONE widget that paints N
    /// `select_row`s. Use this instead of stacking N
    /// [`WidgetSpec::Select`] entries when "the list IS the widget"
    /// (the conceptual unit is the whole roster).
    SelectList(SelectListConfig),
    /// Multi-row hybrid select list — body click + right-edge radio
    /// pin per row. Body selection is independent per row; the radio
    /// is single-select across the list (only one row pinned at a
    /// time), matching the "active layer / current camera target"
    /// pattern.
    HybridSelectList(HybridSelectListConfig),
    /// Wrapping chip cluster — N tags laid out via
    /// `horizontal_wrapped`, growing the pod's row-count as more
    /// chips are added. The widget IS the whole cluster, so the
    /// resize-handle math treats it as one entry whose unit count
    /// scales with row count.
    Tags(TagsConfig),
    /// N keybinding rows (key chip + action label), bundled as ONE
    /// widget. Pod height = N × `KEYBINDING_ROW_H`. Mirrors the
    /// `with_select_list` shape — the list IS the widget.
    Keybindings(KeybindingsConfig),
    /// N labelled chip rows — each row has a fixed-width label cell
    /// on the left and a wrapping chip cluster on the right
    /// (`name: tag1 tag2 …`). Pod height grows both with row count
    /// AND with how many chips wrap inside any single row.
    Badges(BadgesConfig),
    /// Typed module slot. Modules render a compact inline surface in
    /// the pod and can request entry into a full workspace level.
    Module(ModuleConfig),
    /// Internal typed-widget paint closure. This is intentionally
    /// private: public pod/module APIs should stay typed instead of
    /// exposing arbitrary egui hooks.
    Custom {
        units: usize,
        paint: Box<dyn FnOnce(&mut Ui) + Send + Sync>,
    },
}

impl WidgetSpec {
    /// Number of 1U row-heights this widget consumes (for
    /// proportional resize accounting). Single-row widgets (search,
    /// 1U button, toggle, drag-value, dropdown, select) → 1; the
    /// chunky button-with-subtitle → 2 (32 px ≈ 1.7U at default
    /// heights, rounded up); 2-row widgets (progressbar, slider) → 2.
    /// `Custom` returns its caller-supplied hint so the resize-handle
    /// math still adds up.
    fn unit_count(&self) -> usize {
        match self {
            WidgetSpec::Search(_) => 1,
            WidgetSpec::Button(cfg) => {
                if cfg.subtitle.is_some() {
                    2
                } else {
                    1
                }
            }
            WidgetSpec::ActionButton(cfg) => {
                if cfg.subtitle.is_some() {
                    2
                } else {
                    1
                }
            }
            WidgetSpec::Toggle(_) => 1,
            WidgetSpec::Progress(_) => 2,
            WidgetSpec::Slider(_) => 2,
            WidgetSpec::DragValue(_) => 1,
            WidgetSpec::Dropdown(_) => 1,
            WidgetSpec::Select(_) => 1,
            WidgetSpec::HybridSelect(_) => 1,
            WidgetSpec::Color(_) => 1,
            WidgetSpec::Readout(_) => 1,
            WidgetSpec::SelectList(cfg) => cfg.items.len().max(1),
            WidgetSpec::HybridSelectList(cfg) => cfg.items.len().max(1),
            WidgetSpec::Tags(cfg) => tags_estimated_rows(cfg.items.len()),
            WidgetSpec::Keybindings(cfg) => cfg.rows.len().max(1),
            WidgetSpec::Badges(cfg) => cfg.rows.len().max(1),
            WidgetSpec::Module(cfg) => cfg.options.units.max(1),
            WidgetSpec::Custom { units, .. } => *units,
        }
    }

    /// Exact pixel height the widget will paint at when rendered by
    /// [`Pod::show`]'s arms. Used by [`Pod::natural_h`] for the
    /// fill-pod allocation math — must match what each arm in
    /// [`paint_widgets`] actually produces, otherwise the fill
    /// height drifts off by the rounding error per widget and the
    /// bottom pod gets pushed past the body's clip.
    fn natural_height_px(&self) -> f32 {
        match self {
            WidgetSpec::Search(_) => UNIT,
            WidgetSpec::Button(cfg) => {
                if cfg.subtitle.is_some() {
                    theme().widgets.button.subtitle_row_h
                } else {
                    theme().widgets.button.row_h
                }
            }
            WidgetSpec::ActionButton(cfg) => {
                if cfg.subtitle.is_some() {
                    theme().widgets.button.subtitle_row_h
                } else {
                    theme().widgets.button.row_h
                }
            }
            WidgetSpec::Toggle(_) => theme().widgets.toggle.row_h,
            WidgetSpec::Progress(_) => 2.0 * theme().widgets.progress.row_h,
            WidgetSpec::Slider(_) => 2.0 * theme().widgets.slider.row_h,
            WidgetSpec::DragValue(_) => theme().widgets.drag_value.row_h,
            WidgetSpec::Dropdown(_) => theme().widgets.dropdown.row_h,
            WidgetSpec::Select(_) => theme().widgets.select.row_h,
            WidgetSpec::HybridSelect(_) => theme().widgets.select.row_h,
            WidgetSpec::Color(_) => theme().widgets.color.row_h,
            WidgetSpec::Readout(_) => theme().widgets.readout.row_h,
            WidgetSpec::SelectList(cfg) => {
                cfg.items.len().max(1) as f32 * theme().widgets.select.row_h
            }
            WidgetSpec::HybridSelectList(cfg) => {
                cfg.items.len().max(1) as f32 * theme().widgets.select.row_h
            }
            WidgetSpec::Tags(cfg) => {
                tags_estimated_rows(cfg.items.len()) as f32 * theme().pod.tag_row_pitch
            }
            WidgetSpec::Keybindings(cfg) => {
                cfg.rows.len().max(1) as f32 * theme().widgets.keybinding.row_h
            }
            WidgetSpec::Badges(cfg) => cfg.rows.len().max(1) as f32 * theme().widgets.badge.row_h,
            WidgetSpec::Module(cfg) => cfg.options.units.max(1) as f32 * UNIT,
            WidgetSpec::Custom { units, .. } => (*units as f32) * UNIT,
        }
    }
}

/// Per-row pitch for `Pod::with_tags`: chip height + a touch of
/// vertical spacing that `horizontal_wrapped` introduces between
/// wrapped rows. Used by both `unit_count` (rounded up to whole U)
/// and `natural_height_px`.
pub const TAG_ROW_PITCH: f32 = crate::widget::CHIP_H + 4.0;

/// Estimated row count for a wrapping chip cluster of `n` chips.
/// Width-agnostic: assumes ~3 chips per row at typical pod widths
/// (260–300 px) so the pod's flow accounting reserves enough
/// vertical space for the cluster on the first paint. Actual wrap
/// might use fewer rows; that's fine — the pod's layout grows
/// around whatever the cluster paints, this is just the natural-h
/// hint.
fn tags_estimated_rows(n: usize) -> usize {
    n.div_ceil(3).max(1)
}

// ─── Pod ──────────────────────────────────────────────────────────

/// A widget host that goes inside a container's body. Build with
/// [`Pod::new`], add widgets via the builder methods (`with_*`),
/// then hand to a container's `show`. Builder calls accumulate;
/// every widget paints in declaration order, top to bottom.
pub struct Pod {
    id: Id,
    widgets: Vec<WidgetSpec>,
    separator: SeparatorStyle,
    resizable: bool,
    /// `true` for pods that should auto-fill remaining vertical space
    /// in their container. Caller marks at most one pod per container
    /// with [`Pod::fill`]; the container computes its height as
    /// `body_avail - sum(other_pods_natural)` and stashes it in ctx
    /// data, so the fill pod ends up at exactly the right size to
    /// soak up the leftover slack.
    fill: bool,
}

/// Lower bound on the per-widget height of a [`Pod::resizable`]
/// pod. Pinned to [`crate::style::UNIT`] — a widget can never
/// shrink below 1U regardless of how aggressively the user drags
/// the resize handle.
pub const POD_MIN_WIDGET_H: f32 = UNIT;
/// Upper bound on the per-widget height of a [`Pod::resizable`]
/// pod. Roughly 11U.
pub const POD_MAX_WIDGET_H: f32 = 240.0;

impl Pod {
    /// `id` scopes the per-widget persisted state and the debug-
    /// inspector label. Pass a stable value (e.g. derived from the
    /// container's id) so widget state survives across frames.
    pub fn new(id: impl Into<MaraId>) -> Self {
        Self {
            id: id.into().into(),
            widgets: Vec::new(),
            separator: SeparatorStyle::Line,
            resizable: false,
            fill: false,
        }
    }

    /// Mark this pod resizable. The inter-pod separator below it
    /// becomes a vertical drag handle that grows / shrinks every
    /// widget inside the pod uniformly.
    pub fn resizable(mut self) -> Self {
        self.resizable = true;
        self
    }

    pub fn is_resizable(&self) -> bool {
        self.resizable
    }

    /// Mark this pod as the container's "fill" pod — it auto-expands
    /// to occupy `container body height − sum(other pods' natural
    /// heights)`. The user can't drag it; sizing is purely a function
    /// of container size. Use for the pod that holds the variable-
    /// content widget (tree, code editor, log tail) when other pods
    /// in the container are fixed-size headers / footers.
    ///
    /// ONE pod per container should be `fill`; if multiple are marked,
    /// only the first one in the pod list takes effect (others
    /// behave as plain pods). Combined with `resizable()`, `fill`
    /// wins (the user can't drag a fill pod).
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }

    pub fn is_fill(&self) -> bool {
        self.fill
    }

    /// Pod's natural total pixel height — sum of every widget's
    /// EXACT painted row height plus inter-widget spacing. Used by
    /// the container's fill-height math; must equal what
    /// [`paint_widgets`] actually produces so the fill pod's slot
    /// fits the remaining space without pixel drift.
    ///
    /// Rounding to `unit_count * UNIT` (the cheaper coarse formula)
    /// undercounts widgets like Button (24 vs 19.5), Select (20 vs
    /// 19.5) and overcounts Toggle / Slider / Progress (18 vs 19.5).
    /// The errors compound across the non-fill pods above the fill
    /// pod and push the bottom pod past the body's clip — so we
    /// need exact pixels here.
    pub fn natural_h(&self) -> f32 {
        let widget_h: f32 = self.widgets.iter().map(|w| w.natural_height_px()).sum();
        let spacing = if self.widgets.len() > 1 {
            (self.widgets.len() - 1) as f32 * theme().pod.widget_spacing
        } else {
            0.0
        };
        widget_h + spacing
    }

    /// Persistence key for the resizable per-widget height. Also
    /// reused for the `fill` pod's externally-supplied viewport (set
    /// by the container as `forced_height_key`); fill and resizable
    /// share the same render path, just differ in where the height
    /// comes from.
    pub fn widget_height_key(id: impl Into<MaraId>) -> MaraId {
        let id: Id = id.into().into();
        id.with("mara_pod_widget_height").into()
    }

    /// Read the current text in a `with_search` widget without
    /// having to thread the `PodResponse` through (useful when an
    /// adjacent custom widget needs to filter against the query
    /// before the pod containing the search has even been
    /// rendered this frame). `search_idx` is the 0-based index of
    /// the search slot within `pod_id` — `0` for the first
    /// `with_search`, `1` for the second, etc.
    pub fn search_query(
        ctx: &egui::Context,
        pod_id: impl Into<MaraId>,
        search_idx: usize,
    ) -> String {
        let pod_id: Id = pod_id.into().into();
        let key = pod_id.with(("mara_pod_search_buf", search_idx));
        crate::memory::MaraMemoryCtx::new(ctx)
            .get_temp::<String>(key)
            .unwrap_or_default()
    }

    /// Ctx-data key the container writes the fill pod's computed
    /// height under. Pod::show reads this when `self.fill` is set.
    pub fn forced_height_key(id: impl Into<MaraId>) -> MaraId {
        let id: Id = id.into().into();
        id.with("mara_pod_forced_height").into()
    }

    /// Number of widgets the pod will paint.
    pub fn widget_count(&self) -> usize {
        self.widgets.len()
    }

    /// Total number of 1U row-heights the pod's widgets occupy.
    /// Single-row widgets (search / button / toggle) contribute 1
    /// each; multi-row widgets (`progressbar` = caption + bar)
    /// contribute their row count. Used by the inter-pod
    /// drag-resize handler to divide drag delta proportionally.
    pub fn unit_count(&self) -> usize {
        self.widgets.iter().map(|w| w.unit_count()).sum()
    }

    /// Override the separator painted AFTER this pod.
    pub fn with_separator(mut self, style: SeparatorStyle) -> Self {
        self.separator = style;
        self
    }

    pub fn separator_style(&self) -> SeparatorStyle {
        self.separator
    }

    pub fn id(&self) -> MaraId {
        self.id.into()
    }

    pub(crate) fn egui_id(&self) -> Id {
        self.id
    }

    /// Add a search widget (single-line [`crate::widget::text_input`]).
    /// Each search's query buffer is keyed off the pod's id + its
    /// index across the search slots, so multiple searches in the
    /// same pod persist independently.
    pub fn with_search(
        mut self,
        placeholder: impl Into<String>,
        accent: impl Into<Color32>,
    ) -> Self {
        let placeholder = placeholder.into();
        let accent = accent.into();
        assert_non_empty("search widgets", "placeholder", &placeholder);
        self.widgets.push(WidgetSpec::Search(SearchConfig {
            placeholder,
            accent,
        }));
        self
    }

    /// Add a plain button widget. `label` is the centred caption.
    /// Click status is reported in `PodResponse::buttons[i]`.
    pub fn with_button(mut self, label: impl Into<String>, accent: impl Into<Color32>) -> Self {
        let label = label.into();
        let accent = accent.into();
        assert_non_empty("buttons", "label", &label);
        self.widgets.push(WidgetSpec::Button(ButtonConfig {
            label,
            accent,
            subtitle: None,
            glyph: None,
            animation: None,
        }));
        self
    }

    /// Add a button with a small dim caption underneath the primary
    /// label (chunky 2-row look). Click status is reported in
    /// `PodResponse::card_buttons[i]` so callers can split the wires
    /// from plain buttons.
    pub fn with_button_subtitle(
        mut self,
        label: impl Into<String>,
        subtitle: impl Into<String>,
        accent: impl Into<Color32>,
    ) -> Self {
        let label = label.into();
        let subtitle = subtitle.into();
        let accent = accent.into();
        assert_non_empty("buttons", "label", &label);
        assert_non_empty("buttons", "subtitle", &subtitle);
        self.widgets.push(WidgetSpec::Button(ButtonConfig {
            label,
            accent,
            subtitle: Some(subtitle),
            glyph: None,
            animation: None,
        }));
        self
    }

    /// Add a button with a CSS-style hover-fill animation overlay.
    /// At rest the button paints the same as `with_button`; on hover
    /// it paints `style` over a darker-accent fill.
    pub fn with_button_animated(
        mut self,
        label: impl Into<String>,
        accent: impl Into<Color32>,
        style: FillStyle,
    ) -> Self {
        let label = label.into();
        let accent = accent.into();
        assert_non_empty("buttons", "label", &label);
        self.widgets.push(WidgetSpec::Button(ButtonConfig {
            label,
            accent,
            subtitle: None,
            glyph: None,
            animation: Some(style),
        }));
        self
    }

    /// Add a fully-configured button — combine any of subtitle,
    /// glyph (Fluent icon name or literal), and animation in one
    /// call. The simpler `with_button*` shortcuts cover the common
    /// cases; reach for this when you need (e.g.) "icon + 2-row
    /// label + animated hover" all together. Subtitle bumps the
    /// height to 2U automatically.
    pub fn with_button_styled(
        mut self,
        label: impl Into<String>,
        accent: impl Into<Color32>,
        subtitle: Option<impl Into<String>>,
        glyph: Option<impl Into<String>>,
        animation: Option<FillStyle>,
    ) -> Self {
        let label = label.into();
        let accent = accent.into();
        let subtitle = subtitle.map(Into::into);
        let glyph = glyph.map(Into::into);
        assert_non_empty("buttons", "label", &label);
        assert_optional_non_empty("buttons", "subtitle", subtitle.as_deref());
        assert_optional_non_empty("buttons", "glyph", glyph.as_deref());
        self.widgets.push(WidgetSpec::Button(ButtonConfig {
            label,
            accent,
            subtitle,
            glyph,
            animation,
        }));
        self
    }

    /// Add a labelled toggle widget. Label sits left, pill track +
    /// knob sit right on the same row (1U). State persists in
    /// `ctx().data` keyed off the pod's id + toggle slot index.
    pub fn with_toggle(mut self, label: impl Into<String>, accent: impl Into<Color32>) -> Self {
        let label = label.into();
        let accent = accent.into();
        assert_non_empty("toggles", "label", &label);
        self.widgets.push(WidgetSpec::Toggle(ToggleConfig {
            label,
            accent,
            initial: None,
        }));
        self
    }

    /// Add a toggle initialised to `initial` if no persisted state
    /// exists for that slot yet. Once the user clicks, the
    /// persisted value takes over.
    pub fn with_toggle_initial(
        mut self,
        label: impl Into<String>,
        accent: impl Into<Color32>,
        initial: bool,
    ) -> Self {
        let label = label.into();
        let accent = accent.into();
        assert_non_empty("toggles", "label", &label);
        self.widgets.push(WidgetSpec::Toggle(ToggleConfig {
            label,
            accent,
            initial: Some(initial),
        }));
        self
    }

    /// Add a labelled progress bar (read-only). 2 rows.
    pub fn with_progress(
        mut self,
        label: impl Into<String>,
        fraction: f32,
        text: impl Into<String>,
        accent: impl Into<Color32>,
    ) -> Self {
        let label = label.into();
        let accent = accent.into();
        assert_non_empty("progress bars", "label", &label);
        assert_fraction("progress bars", "fraction", fraction);
        self.widgets.push(WidgetSpec::Progress(ProgressConfig {
            label,
            fraction,
            text: text.into(),
            accent,
        }));
        self
    }

    /// Add a labelled slider. 2 rows (caption + interactive bar).
    /// Initial value is read from `value`; user drags update the
    /// pod's persisted slot. Read the resolved value back from
    /// `PodResponse::sliders[i].value`.
    pub fn with_slider(
        mut self,
        label: impl Into<String>,
        value: f64,
        range: std::ops::RangeInclusive<f64>,
        decimals: usize,
        suffix: impl Into<String>,
        accent: impl Into<Color32>,
    ) -> Self {
        let label = label.into();
        let accent = accent.into();
        assert_non_empty("sliders", "label", &label);
        assert_value_in_range("sliders", value, &range);
        self.widgets.push(WidgetSpec::Slider(SliderConfig {
            label,
            value,
            range,
            decimals,
            suffix: suffix.into(),
            accent,
        }));
        self
    }

    /// Add a labelled `egui::DragValue` numeric input. 1 row.
    pub fn with_drag_value(
        mut self,
        label: impl Into<String>,
        value: f64,
        speed: f64,
        range: std::ops::RangeInclusive<f64>,
        decimals: usize,
        suffix: impl Into<String>,
    ) -> Self {
        let label = label.into();
        assert_non_empty("drag values", "label", &label);
        assert_value_in_range("drag values", value, &range);
        assert_finite("drag values", "speed", speed);
        assert!(speed >= 0.0, "drag values require a non-negative speed");
        self.widgets.push(WidgetSpec::DragValue(DragValueConfig {
            label,
            value,
            speed,
            range,
            decimals,
            suffix: suffix.into(),
        }));
        self
    }

    /// Add a "card" button — leading glyph + primary `name` + small
    /// `subtitle`. Click status is reported in
    /// `PodResponse::card_buttons[i]`.
    pub fn with_card_button(
        mut self,
        glyph: impl Into<String>,
        name: impl Into<String>,
        subtitle: impl Into<String>,
        accent: impl Into<Color32>,
    ) -> Self {
        let glyph = glyph.into();
        let name = name.into();
        let subtitle = subtitle.into();
        let accent = accent.into();
        assert_non_empty("card buttons", "glyph", &glyph);
        assert_non_empty("card buttons", "name", &name);
        assert_non_empty("card buttons", "subtitle", &subtitle);
        self.widgets.push(WidgetSpec::Button(ButtonConfig {
            label: name,
            accent,
            subtitle: Some(subtitle),
            glyph: Some(glyph),
            animation: None,
        }));
        self
    }

    /// Add a two-layer button with an independent embedded tail
    /// action. This is the "row body + plus button inside the same
    /// chrome" shape used by hierarchy UIs: body click selects /
    /// opens the row, tail click performs the secondary action
    /// without also firing the body click.
    #[allow(clippy::too_many_arguments)]
    pub fn with_card_action_button(
        mut self,
        glyph: impl Into<String>,
        name: impl Into<String>,
        subtitle: impl Into<String>,
        action_glyph: impl Into<String>,
        action_tooltip: impl Into<String>,
        action_armed: bool,
        accent: impl Into<Color32>,
    ) -> Self {
        let glyph = glyph.into();
        let name = name.into();
        let subtitle = subtitle.into();
        let action_glyph = action_glyph.into();
        let action_tooltip = action_tooltip.into();
        let accent = accent.into();
        assert_non_empty("action card buttons", "glyph", &glyph);
        assert_non_empty("action card buttons", "name", &name);
        assert_non_empty("action card buttons", "subtitle", &subtitle);
        assert_non_empty("action card buttons", "action glyph", &action_glyph);
        assert_non_empty("action card buttons", "action tooltip", &action_tooltip);
        self.widgets
            .push(WidgetSpec::ActionButton(ActionButtonConfig {
                label: name,
                subtitle: Some(subtitle),
                glyph: Some(glyph),
                action_glyph,
                action_tooltip: Some(action_tooltip),
                action_armed,
                accent,
            }));
        self
    }

    /// Add a single-select dropdown. `options` is the menu list;
    /// `initial` is the default index until the user picks something
    /// (subsequent selections persist in the pod's ctx-data slot).
    /// Result lands in `PodResponse::dropdowns[i]`.
    pub fn with_dropdown(
        mut self,
        options: impl IntoIterator<Item = impl Into<String>>,
        initial: usize,
        accent: impl Into<Color32>,
    ) -> Self {
        let accent = accent.into();
        let options: Vec<String> = options.into_iter().map(Into::into).collect();
        assert_non_empty_items("dropdowns", &options);
        assert!(
            initial < options.len(),
            "dropdown initial index must be within the option list"
        );
        self.widgets.push(WidgetSpec::Dropdown(DropdownConfig {
            options,
            initial,
            accent,
        }));
        self
    }

    /// Add a select row (single click target on the body). The
    /// `selected` paint state persists in the pod's ctx-data slot —
    /// each click toggles it. `trailing` is rendered dim-right.
    /// Result lands in `PodResponse::selects[i]`.
    pub fn with_select(
        mut self,
        label: impl Into<String>,
        trailing: Option<impl Into<String>>,
        selected_initial: bool,
        accent: impl Into<Color32>,
    ) -> Self {
        let label = label.into();
        let trailing = trailing.map(Into::into);
        let accent = accent.into();
        assert_non_empty("select rows", "label", &label);
        assert_optional_non_empty("select rows", "trailing", trailing.as_deref());
        self.widgets.push(WidgetSpec::Select(SelectConfig {
            label,
            trailing,
            selected_initial,
            accent,
        }));
        self
    }

    /// Add a hybrid-select row (body click + right-edge radio
    /// pin). The radio's `radio_on` state persists in its own
    /// ctx-data slot. Result lands in `PodResponse::hybrid_selects[i]`.
    pub fn with_hybrid_select(
        mut self,
        label: impl Into<String>,
        trailing: Option<impl Into<String>>,
        selected_initial: bool,
        radio_initial: bool,
        accent: impl Into<Color32>,
    ) -> Self {
        let label = label.into();
        let trailing = trailing.map(Into::into);
        let accent = accent.into();
        assert_non_empty("hybrid select rows", "label", &label);
        assert_optional_non_empty("hybrid select rows", "trailing", trailing.as_deref());
        self.widgets
            .push(WidgetSpec::HybridSelect(HybridSelectConfig {
                label,
                trailing,
                selected_initial,
                radio_initial,
                accent,
            }));
        self
    }

    /// Add an opaque sRGB colour swatch. Click expands the picker
    /// inline below the row. Result lands in `PodResponse::colors[i]`
    /// (alpha is fixed at 1.0 in the result).
    pub fn with_color_rgb(
        mut self,
        label: impl Into<String>,
        initial_rgb: [f32; 3],
        accent: impl Into<Color32>,
    ) -> Self {
        let label = label.into();
        let accent = accent.into();
        assert_non_empty("RGB color widgets", "label", &label);
        assert_color_channels("RGB color widgets", &initial_rgb);
        self.widgets.push(WidgetSpec::Color(ColorConfig {
            label,
            initial: [initial_rgb[0], initial_rgb[1], initial_rgb[2], 1.0],
            alpha: false,
            accent,
        }));
        self
    }

    /// Add an sRGBA colour swatch (alpha slider in the picker).
    /// Result lands in `PodResponse::colors[i]`.
    pub fn with_color_rgba(
        mut self,
        label: impl Into<String>,
        initial_rgba: [f32; 4],
        accent: impl Into<Color32>,
    ) -> Self {
        let label = label.into();
        let accent = accent.into();
        assert_non_empty("RGBA color widgets", "label", &label);
        assert_color_channels("RGBA color widgets", &initial_rgba);
        self.widgets.push(WidgetSpec::Color(ColorConfig {
            label,
            initial: initial_rgba,
            alpha: true,
            accent,
        }));
        self
    }

    /// Add a read-only readout row — label on the left, monospace
    /// value on the right. Use for surfaces that just *display* a
    /// piece of data (selected node path, current speed, active
    /// tool, …). Result is reported in `PodResponse::readouts[i]`,
    /// though the response carries no state — re-render the pod with
    /// a new `value` to update what's shown.
    pub fn with_readout(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        let label = label.into();
        let value = value.into();
        assert_non_empty("readouts", "label", &label);
        assert_non_empty("readouts", "value", &value);
        self.widgets
            .push(WidgetSpec::Readout(ReadoutConfig { label, value }));
        self
    }

    /// Add a multi-row select list as ONE widget. Each item becomes a
    /// `select_row`; selection state persists per-list (a single
    /// "current row" index, like a single-select listbox) in
    /// `PodResponse::select_lists[i].selected`. Pass `trailing` to
    /// add a dim-right column per row; length must equal `items` or
    /// the list is rendered without trailing.
    pub fn with_select_list(
        mut self,
        items: impl IntoIterator<Item = impl Into<String>>,
        trailing: Option<Vec<String>>,
        accent: impl Into<Color32>,
    ) -> Self {
        let accent = accent.into();
        let items: Vec<String> = items.into_iter().map(Into::into).collect();
        assert_non_empty_items("select lists", &items);
        let trailing = trailing.filter(|t| t.len() == items.len());
        self.widgets.push(WidgetSpec::SelectList(SelectListConfig {
            items,
            trailing,
            accent,
        }));
        self
    }

    /// Add a multi-row hybrid select list — body click + radio pin
    /// per row, all bundled as ONE widget. Body select is single-row
    /// (current selection); radio pin is also single-row (only one
    /// row pinned at a time, like a real radio group). Result
    /// indices land in `PodResponse::hybrid_select_lists[i]`.
    pub fn with_hybrid_select_list(
        mut self,
        items: impl IntoIterator<Item = impl Into<String>>,
        trailing: Option<Vec<String>>,
        accent: impl Into<Color32>,
    ) -> Self {
        let accent = accent.into();
        let items: Vec<String> = items.into_iter().map(Into::into).collect();
        assert_non_empty_items("hybrid select lists", &items);
        let trailing = trailing.filter(|t| t.len() == items.len());
        self.widgets
            .push(WidgetSpec::HybridSelectList(HybridSelectListConfig {
                items,
                trailing,
                accent,
            }));
        self
    }

    /// Add a wrapping chip cluster — N tags rendered via
    /// `horizontal_wrapped`. The pod's natural height grows with the
    /// number of chips (≈ 3 chips per row at default pod width), so
    /// adding more tags extends the pod automatically.
    ///
    /// Convenience overload that takes plain labels — every chip
    /// uses the default faint accent-tinted glass fill. For mixed
    /// colours (e.g. a `WARNING`-fill status chip alongside neutral
    /// labels) use [`Pod::with_tag_items`].
    pub fn with_tags(
        mut self,
        items: impl IntoIterator<Item = impl Into<String>>,
        accent: impl Into<Color32>,
    ) -> Self {
        let accent = accent.into();
        let items: Vec<TagItem> = items.into_iter().map(|s| TagItem::new(s)).collect();
        self.widgets
            .push(WidgetSpec::Tags(TagsConfig { items, accent }));
        self
    }

    /// Like [`Pod::with_tags`] but accepts pre-built [`TagItem`]s so
    /// individual chips can override the fill colour for status /
    /// severity categorisation.
    pub fn with_tag_items(mut self, items: Vec<TagItem>, accent: impl Into<Color32>) -> Self {
        let accent = accent.into();
        self.widgets
            .push(WidgetSpec::Tags(TagsConfig { items, accent }));
        self
    }

    /// Add a keybinding list — N rows of `[keys]  action`. Pod
    /// height = N × `KEYBINDING_ROW_H`, so adding more rows extends
    /// the pod proportionally.
    pub fn with_keybindings(
        mut self,
        rows: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        let rows: Vec<(String, String)> = rows
            .into_iter()
            .map(|(k, a)| (k.into(), a.into()))
            .collect();
        assert!(!rows.is_empty(), "keybindings require at least one row");
        assert!(
            rows.iter()
                .all(|(keys, action)| !keys.trim().is_empty() && !action.trim().is_empty()),
            "keybindings require every key and action label to be non-empty"
        );
        self.widgets
            .push(WidgetSpec::Keybindings(KeybindingsConfig { rows }));
        self
    }

    /// Add a single labelled chip row — `name: tag1 tag2 …` on one
    /// 1U-tall line. Idiomatic for "one fact per pod" layouts where
    /// each row gets its own pod (and therefore its own separator,
    /// resize handle, and reorder slot). Plain-string badges all
    /// share the default accent-tinted fill.
    pub fn with_badge_row(
        mut self,
        label: impl Into<String>,
        badges: impl IntoIterator<Item = impl Into<String>>,
        accent: impl Into<Color32>,
    ) -> Self {
        let accent = accent.into();
        let row = BadgeRowSpec::from_strs(label, badges);
        self.widgets.push(WidgetSpec::Badges(BadgesConfig {
            rows: vec![row],
            accent,
        }));
        self
    }

    /// Add a single labelled chip row with per-chip fill overrides
    /// — same shape as [`Pod::with_badge_row`] but each chip is a
    /// [`TagItem`] so individual badges can be tinted with status
    /// colours (e.g. `WARNING` for a broken-state counter).
    pub fn with_badge_row_items(
        mut self,
        label: impl Into<String>,
        badges: Vec<TagItem>,
        accent: impl Into<Color32>,
    ) -> Self {
        let accent = accent.into();
        let row = BadgeRowSpec::new(label, badges);
        self.widgets.push(WidgetSpec::Badges(BadgesConfig {
            rows: vec![row],
            accent,
        }));
        self
    }

    /// Multi-row variant — packs N labelled chip rows into ONE pod
    /// (so the whole stack reads as a single unit, no separator
    /// between rows). Pod height = N × `BADGE_ROW_H`. For the
    /// "one row per pod" layout — where each row carries its own
    /// separator and resize handle — call [`Pod::with_badge_row`]
    /// per pod instead.
    pub fn with_badges(mut self, rows: Vec<BadgeRowSpec>, accent: impl Into<Color32>) -> Self {
        let accent = accent.into();
        self.widgets
            .push(WidgetSpec::Badges(BadgesConfig { rows, accent }));
        self
    }

    /// Add a typed Mara module to this pod.
    ///
    /// The module paints its inline representation in the pod flow.
    /// By default modules are allowed to request entry into a full
    /// module workspace; the workspace stack wiring lands in the
    /// next implementation phase, and the request is surfaced through
    /// [`PodResponse::modules`] for now.
    #[must_use]
    pub fn with_module<M>(self, module: M) -> Self
    where
        M: MaraModule + Send + Sync + 'static,
    {
        self.with_module_options(module, ModuleInlineOptions::default())
    }

    /// Like [`Pod::with_module`] with explicit inline options.
    #[must_use]
    pub fn with_module_options<M>(mut self, module: M, options: ModuleInlineOptions) -> Self
    where
        M: MaraModule + Send + Sync + 'static,
    {
        assert!(
            !module.title().trim().is_empty(),
            "pod modules require a non-empty title"
        );
        assert!(
            !module.icon().trim().is_empty(),
            "pod modules require a non-empty icon"
        );
        self.widgets.push(WidgetSpec::Module(ModuleConfig {
            options,
            module: Box::new(module),
        }));
        self
    }

    /// Typed pod constructor for hosting a recursive tree built
    /// from mara [`tree_row`](crate::widget::tree_row)s. The
    /// closure receives a [`TreeBody`](crate::widget::TreeBody)
    /// wrapper that exposes only `row(...)` (forwarding to
    /// `tree_row`) and ctx-data access — no raw [`egui::Ui`]
    /// leaks. `units` is the inter-pod resize hint (same shape
    /// as the internal custom slot units).
    #[must_use]
    pub fn with_tree<F>(self, units: usize, body: F) -> Self
    where
        F: FnOnce(&mut crate::widget::TreeBody) + Send + Sync + 'static,
    {
        self.with_custom_units(units, move |ui| {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            let mut tb = crate::widget::TreeBody::new(&mut backend);
            body(&mut tb);
        })
    }

    /// Internal custom paint slot with an explicit "this slot
    /// occupies N units of 1U row height" hint, used by the
    /// inter-pod resize-handle to share drag delta across pods
    /// proportionally to their content size. Public APIs should
    /// expose typed wrappers instead of raw egui closures.
    #[doc(hidden)]
    pub fn with_custom_units(
        mut self,
        units: usize,
        paint: impl FnOnce(&mut Ui) + Send + Sync + 'static,
    ) -> Self {
        self.widgets.push(WidgetSpec::Custom {
            units: units.max(1),
            paint: Box::new(paint),
        });
        self
    }

    /// Render the pod into the current egui backend. Public app
    /// code reaches this through [`crate::mui::MaraUi::pod`] or a
    /// Mara container, not by passing around a raw `egui::Ui`.
    pub(crate) fn show(self, ui: &mut Ui) -> PodResponse {
        let pod_id = self.id;
        let mut response = PodResponse::default();
        // Two paths share the same ScrollArea-clipped viewport
        // implementation: `fill` (height supplied by the container)
        // and `resizable` (height from the persisted user-drag handle).
        // `fill` takes precedence when both flags are set so the
        // container's calculation always wins over the drag handle.
        let viewport_h: Option<f32> = if self.fill {
            // Container writes the computed remaining-space height
            // here BEFORE iterating the pods (see
            // `Normal::show`'s pre-pass over the pod list). If
            // missing — e.g. caller marked a pod `fill` but the
            // container didn't pre-compute — fall back to natural
            // so the pod still renders.
            let key: Id = Self::forced_height_key(pod_id).into();
            Some(
                crate::memory::MaraMemoryCtx::new(ui.ctx())
                    .get_temp::<f32>(key)
                    .unwrap_or_else(|| self.natural_h())
                    .max(theme().pod.min_widget_h),
            )
        } else if self.resizable {
            let natural_h = self.natural_h();
            let key: Id = Self::widget_height_key(pod_id).into();
            Some(
                crate::memory::MaraMemoryCtx::new(ui.ctx())
                    .get_persisted::<f32>(key)
                    .unwrap_or(natural_h)
                    .clamp(theme().pod.min_widget_h, theme().pod.max_widget_h),
            )
        } else {
            None
        };
        if let Some(viewport_h) = viewport_h {
            let avail_w = ui.available_width().max(1.0);
            let slot_rect = {
                let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
                crate::layout::UiBackend::allocate(
                    &mut backend,
                    crate::vocab::Vec2::new(avail_w, viewport_h),
                    crate::layout::Sense::Hover,
                )
                .rect
            };
            let mut child = crate::backend::egui::child_ui_for_region(
                ui,
                crate::layout::ChildRegion::top_down(slot_rect, crate::layout::StackAlign::Min),
            );
            // `shrink_clip_rect` (= intersect with current clip) so a
            // pod inside an already-clipped container can never grow
            // its own clip. Hierarchy stays intact:
            //   widget rect ⊆ pod slot ⊆ container body ⊆ pane.
            child.shrink_clip_rect(slot_rect.into());
            let widgets = self.widgets;
            if self.resizable && !self.fill {
                // Resizable pods get a vertical `ScrollArea` so when
                // content exceeds the user-dragged viewport, the bar
                // appears and rows scroll. `auto_shrink([false,
                // false])` keeps the area filling the slot;
                // `min_scrolled_height(0.0)` disables egui's default
                // 64-px floor.
                egui::ScrollArea::vertical()
                    .id_salt(pod_id.with("mara_pod_scroll"))
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .show(&mut child, |inner| {
                        let mut backend = crate::backend::egui::EguiUiBackend::new(inner);
                        paint_widgets(widgets, &mut backend, &mut response, pod_id);
                    });
            } else {
                // Fill pods skip the ScrollArea — the slot is already
                // exactly the size the container computed, and
                // embedded scrollable widgets (node graph, code
                // editor) own their internal pan/zoom. Nesting a
                // ScrollArea here makes those widgets fight the bar:
                // their reported size oscillates as the bar appears /
                // disappears, triggering their own re-layout (e.g.
                // graph's `initial placing` request_discard spam),
                // which causes the pod to re-measure → loop. Plain
                // `paint_widgets` inside the clipped `child` is
                // exactly what fill pods want: hard clip, no
                // outer-scroll feedback.
                let mut backend = crate::backend::egui::EguiUiBackend::new(&mut child);
                paint_widgets(widgets, &mut backend, &mut response, pod_id);
            }
        } else {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            paint_widgets(self.widgets, &mut backend, &mut response, pod_id);
        }
        response
    }
}

/// Inter-widget vertical breathing space inside a pod. Used both
/// when laying out widgets in [`paint_widgets`] and when computing a
/// resizable pod's natural height in [`Pod::show`].
pub const POD_WIDGET_SPACING: f32 = 4.0;

/// Paint every widget in `widgets` into `ui`, accumulating responses
/// into `response`. Shared between [`Pod::show`]'s plain (parent ui)
/// and resizable (clipped child + ScrollArea) paths so the per-widget
/// rendering logic lives in exactly one place.
fn paint_widgets(
    widgets: Vec<WidgetSpec>,
    backend: &mut dyn crate::layout::UiBackend,
    response: &mut PodResponse,
    pod_id: Id,
) {
    let widget_spacing = theme().pod.widget_spacing;
    // Per-kind stable indices: the Nth `with_search` keeps its
    // own state key independent of any buttons / toggles /
    // progress bars declared between them.
    let mut search_idx = 0usize;
    let mut button_idx = 0usize;
    let mut card_button_idx = 0usize;
    let mut action_button_idx = 0usize;
    let mut toggle_idx = 0usize;
    let mut progress_idx = 0usize;
    let mut slider_idx = 0usize;
    let mut drag_value_idx = 0usize;
    let mut dropdown_idx = 0usize;
    let mut select_idx = 0usize;
    let mut hybrid_select_idx = 0usize;
    let mut color_idx = 0usize;
    let mut readout_idx = 0usize;
    let mut select_list_idx = 0usize;
    let mut hybrid_select_list_idx = 0usize;
    let mut tags_idx = 0usize;
    let mut keybindings_idx = 0usize;
    let mut module_idx = 0usize;
    for (slot_idx, spec) in widgets.into_iter().enumerate() {
        if slot_idx > 0 {
            backend.add_space(crate::layout::SpaceSpec::vertical(widget_spacing));
        }
        // Each widget slot gets its own pushed id chain. This
        // is what keeps an explicit id derivation like
        // `id.with(("mara_toggle", label))` from colliding across
        // pods that happen to share the same label — the pushed
        // id (= pod_id ⊕ slot_idx) is unique per (pod, widget
        // slot), so every child id inherits uniqueness.
        let salt: MaraId = egui::Id::new((pod_id, slot_idx)).into();
        // `spec` is moved into the id-scope body; `in_id_scope`
        // takes an `FnMut`, so bridge the one-shot move through an
        // `Option::take`.
        let mut spec_slot = Some(spec);
        backend.in_id_scope(salt, &mut |mut backend| {
            let spec = spec_slot
                .take()
                .expect("in_id_scope body runs exactly once");
            match spec {
                WidgetSpec::Search(cfg) => {
                    if let Some(ui) = backend.__internal_egui_ui_mut() {
                        let buf_key = pod_id.with(("mara_pod_search_buf", search_idx));
                        let mut buf: String = crate::memory::MaraMemoryCtx::new(ui.ctx()).get_temp::<String>(buf_key)
                            .unwrap_or_default();
                        let resp = text_input(ui, &mut buf, &cfg.placeholder, cfg.accent);
                        let changed = resp.changed();
                        if changed {
                            crate::memory::MaraMemoryCtx::new(ui.ctx()).set_temp(buf_key, buf.clone());
                        }
                        crate::debug::tag(
                            ui,
                            resp.rect.into(),
                            format!("widget[text_input/search #{}]", search_idx),
                        );
                        response.searches.push(SearchResponse {
                            query: buf,
                            changed,
                        });
                    } else {
                        response.searches.push(SearchResponse {
                            query: String::new(),
                            changed: false,
                        });
                    }
                    search_idx += 1;
                }
                WidgetSpec::Button(cfg) => {
                    let has_subtitle = cfg.subtitle.is_some();
                    // Card-shaped button (subtitle and/or glyph) gets
                    // its own height + result wire so callers can
                    // index them independently of plain buttons.
                    let mut builder = Button::new(&cfg.label);
                    if let Some(s) = &cfg.subtitle {
                        builder = builder.subtitle(s);
                    }
                    if let Some(g) = &cfg.glyph {
                        builder = builder.glyph(g);
                    }
                    if let Some(a) = cfg.animation {
                        builder = builder.animation(a);
                    }
                    let resp = builder.show_backend(&mut backend, cfg.accent);
                    if has_subtitle {
                        crate::debug::tag_backend(
                            &mut *backend,
                            resp.rect,
                            format!("widget[card_button #{}]", card_button_idx),
                        );
                        response.card_buttons.push(ButtonResponse {
                            clicked: resp.clicked(),
                        });
                        card_button_idx += 1;
                    } else {
                        crate::debug::tag_backend(
                            &mut *backend,
                            resp.rect,
                            format!("widget[button #{}]", button_idx),
                        );
                        response.buttons.push(ButtonResponse {
                            clicked: resp.clicked(),
                        });
                        button_idx += 1;
                    }
                }
                WidgetSpec::ActionButton(cfg) => {
                    let mut builder =
                        crate::widget::ActionButton::new(&cfg.label, &cfg.action_glyph)
                            .action_armed(cfg.action_armed);
                    if let Some(s) = &cfg.subtitle {
                        builder = builder.subtitle(s);
                    }
                    if let Some(g) = &cfg.glyph {
                        builder = builder.glyph(g);
                    }
                    if let Some(tip) = &cfg.action_tooltip {
                        builder = builder.action_tooltip(tip);
                    }
                    let resp = builder.show_backend(&mut backend, cfg.accent);
                    crate::debug::tag_backend(
                        &mut *backend,
                        resp.body.rect,
                        format!("widget[action_button #{}]", action_button_idx),
                    );
                    response.action_buttons.push(ActionButtonPodResponse {
                        body_clicked: resp.body.clicked,
                        body_double_clicked: resp.body.double_clicked,
                        action_clicked: resp.action.clicked,
                    });
                    action_button_idx += 1;
                }
                WidgetSpec::Toggle(cfg) => {
                    let state_key: MaraId =
                        pod_id.with(("mara_pod_toggle_state", toggle_idx)).into();
                    let mut on: bool = {
                        let mut memory = backend.memory();
                        if let Some(stored) = memory.get_persisted::<bool>(state_key) {
                            stored
                        } else {
                            let v = cfg.initial.unwrap_or(false);
                            memory.set_persisted(state_key, v);
                            v
                        }
                    };
                    let resp = toggle_backend(
                        &mut backend,
                        &cfg.label,
                        &mut on,
                        cfg.accent,
                        theme().widgets.toggle.row_h,
                    );
                    let changed = resp.changed();
                    if changed {
                        backend.memory().set_persisted(state_key, on);
                    }
                    crate::debug::tag_backend(
                        &mut *backend,
                        resp.rect,
                        format!(
                            "widget[toggle #{}{}]",
                            toggle_idx,
                            if cfg.label.is_empty() {
                                String::new()
                            } else {
                                format!(" \"{}\"", cfg.label)
                            }
                        ),
                    );
                    response.toggles.push(ToggleResponse { on, changed });
                    toggle_idx += 1;
                }
                WidgetSpec::Progress(cfg) => {
                    let resp = progressbar_backend(
                        &mut backend,
                        &cfg.label,
                        cfg.fraction,
                        &cfg.text,
                        cfg.accent,
                        theme().widgets.progress.row_h,
                    );
                    crate::debug::tag_backend(
                        &mut *backend,
                        resp.rect,
                        format!("widget[progress #{}]", progress_idx),
                    );
                    response.progress.push(ProgressResponse);
                    progress_idx += 1;
                }
                WidgetSpec::Slider(cfg) => {
                    // Persist the current value so user drags
                    // accumulate across frames without the caller
                    // having to thread state.
                    let val_key: MaraId = pod_id.with(("mara_pod_slider_val", slider_idx)).into();
                    let mut val: f64 = backend
                        .memory()
                        .get_persisted::<f64>(val_key)
                        .unwrap_or(cfg.value);
                    let resp = slider_backend(
                        &mut backend,
                        &cfg.label,
                        &mut val,
                        cfg.range.clone(),
                        cfg.decimals,
                        &cfg.suffix,
                        cfg.accent,
                        theme().widgets.slider.row_h,
                    );
                    let changed = resp.changed();
                    if changed {
                        backend.memory().set_persisted(val_key, val);
                    }
                    crate::debug::tag_backend(
                        &mut *backend,
                        resp.rect,
                        format!("widget[slider #{}]", slider_idx),
                    );
                    response.sliders.push(SliderResponse {
                        value: val,
                        changed,
                    });
                    slider_idx += 1;
                }
                WidgetSpec::DragValue(cfg) => {
                    let val_key: MaraId = pod_id
                        .with(("mara_pod_drag_value_val", drag_value_idx))
                        .into();
                    let mut val: f64 = backend
                        .memory()
                        .get_persisted::<f64>(val_key)
                        .unwrap_or(cfg.value);
                    let resp = drag_value_backend(
                        &mut backend,
                        &cfg.label,
                        &mut val,
                        cfg.speed,
                        cfg.range.clone(),
                        cfg.decimals,
                        &cfg.suffix,
                        theme().widgets.drag_value.row_h,
                    );
                    let changed = resp.changed();
                    if changed {
                        backend.memory().set_persisted(val_key, val);
                    }
                    crate::debug::tag_backend(
                        &mut *backend,
                        resp.rect,
                        format!("widget[drag_value #{}]", drag_value_idx),
                    );
                    response.drag_values.push(DragValueResponse {
                        value: val,
                        changed,
                    });
                    drag_value_idx += 1;
                }
                WidgetSpec::Dropdown(cfg) => {
                    if let Some(ui) = backend.__internal_egui_ui_mut() {
                        let val_key = pod_id.with(("mara_pod_dropdown_idx", dropdown_idx));
                        let mut sel: usize = crate::memory::MaraMemoryCtx::new(ui.ctx()).get_persisted::<usize>(val_key)
                            .unwrap_or(cfg.initial)
                            .min(cfg.options.len().saturating_sub(1));
                        let opts: Vec<&str> = cfg.options.iter().map(String::as_str).collect();
                        let resp = dropdown(
                            ui,
                            ("mara_pod_dropdown", dropdown_idx),
                            &mut sel,
                            &opts,
                            cfg.accent,
                        );
                        let changed = resp.changed();
                        if changed {
                            crate::memory::MaraMemoryCtx::new(ui.ctx()).set_persisted(val_key, sel);
                        }
                        crate::debug::tag(
                            ui,
                            resp.rect.into(),
                            format!("widget[dropdown #{}]", dropdown_idx),
                        );
                        response.dropdowns.push(DropdownResponse {
                            selected: sel,
                            changed,
                        });
                    } else {
                        response.dropdowns.push(DropdownResponse {
                            selected: cfg.initial,
                            changed: false,
                        });
                    }
                    dropdown_idx += 1;
                }
                WidgetSpec::Select(cfg) => {
                    let sel_key: MaraId = pod_id.with(("mara_pod_select_sel", select_idx)).into();
                    let mut selected: bool = backend
                        .memory()
                        .get_persisted::<bool>(sel_key)
                        .unwrap_or(cfg.selected_initial);
                    let resp = select_row_backend(
                        &mut backend,
                        ("mara_pod_select", select_idx),
                        &cfg.label,
                        cfg.trailing.as_deref(),
                        selected,
                        cfg.accent,
                        theme().widgets.select.row_h,
                    );
                    if resp.clicked() {
                        selected = !selected;
                        backend.memory().set_persisted(sel_key, selected);
                    }
                    crate::debug::tag_backend(
                        &mut *backend,
                        resp.rect,
                        format!("widget[select #{}]", select_idx),
                    );
                    response.selects.push(SelectResponse {
                        clicked: resp.clicked(),
                        double_clicked: resp.double_clicked(),
                        selected,
                    });
                    select_idx += 1;
                }
                WidgetSpec::HybridSelect(cfg) => {
                    let sel_key: MaraId = pod_id
                        .with(("mara_pod_hybrid_sel", hybrid_select_idx))
                        .into();
                    let radio_key: MaraId = pod_id
                        .with(("mara_pod_hybrid_radio", hybrid_select_idx))
                        .into();
                    let mut selected: bool = backend
                        .memory()
                        .get_persisted::<bool>(sel_key)
                        .unwrap_or(cfg.selected_initial);
                    let mut radio_on: bool = backend
                        .memory()
                        .get_persisted::<bool>(radio_key)
                        .unwrap_or(cfg.radio_initial);
                    let resp = hybrid_select_row_backend(
                        &mut backend,
                        ("mara_pod_hybrid", hybrid_select_idx),
                        &cfg.label,
                        cfg.trailing.as_deref(),
                        selected,
                        radio_on,
                        cfg.accent,
                        theme().widgets.select.row_h,
                    );
                    if resp.body.clicked {
                        selected = !selected;
                        backend.memory().set_persisted(sel_key, selected);
                    }
                    if resp.radio.clicked {
                        radio_on = !radio_on;
                        backend.memory().set_persisted(radio_key, radio_on);
                    }
                    crate::debug::tag_backend(
                        &mut *backend,
                        resp.body.rect,
                        format!("widget[hybrid_select #{}]", hybrid_select_idx),
                    );
                    response.hybrid_selects.push(HybridSelectPodResponse {
                        body_clicked: resp.body.clicked,
                        body_double_clicked: resp.body.double_clicked,
                        radio_clicked: resp.radio.clicked,
                        selected,
                        radio_on,
                    });
                    hybrid_select_idx += 1;
                }
                WidgetSpec::Color(cfg) => {
                    if let Some(ui) = backend.__internal_egui_ui_mut() {
                        let val_key = pod_id.with(("mara_pod_color_val", color_idx));
                        let mut rgba: [f32; 4] = crate::memory::MaraMemoryCtx::new(ui.ctx()).get_persisted::<[f32; 4]>(val_key)
                            .unwrap_or(cfg.initial);
                        let changed = if cfg.alpha {
                            let resp = {
                                let mut raw = crate::MaraUi::__internal_backend_from_raw(ui);
                                let mut mara =
                                    crate::MaraUi::__internal_over(&mut raw, cfg.accent);
                                color_rgba(&mut mara, &cfg.label, &mut rgba, cfg.accent)
                            };
                            crate::debug::tag(
                                ui,
                                resp.rect.into(),
                                format!("widget[color_rgba #{}]", color_idx),
                            );
                            resp.changed()
                        } else {
                            let mut rgb = [rgba[0], rgba[1], rgba[2]];
                            let resp = {
                                let mut raw = crate::MaraUi::__internal_backend_from_raw(ui);
                                let mut mara =
                                    crate::MaraUi::__internal_over(&mut raw, cfg.accent);
                                color_rgb(&mut mara, &cfg.label, &mut rgb, cfg.accent)
                            };
                            rgba[0] = rgb[0];
                            rgba[1] = rgb[1];
                            rgba[2] = rgb[2];
                            rgba[3] = 1.0;
                            crate::debug::tag(
                                ui,
                                resp.rect.into(),
                                format!("widget[color_rgb #{}]", color_idx),
                            );
                            resp.changed()
                        };
                        if changed {
                            crate::memory::MaraMemoryCtx::new(ui.ctx()).set_persisted(val_key, rgba);
                        }
                        response.colors.push(ColorResponse { rgba, changed });
                    } else {
                        response.colors.push(ColorResponse {
                            rgba: cfg.initial,
                            changed: false,
                        });
                    }
                    color_idx += 1;
                }
                WidgetSpec::Readout(cfg) => {
                    let resp = readout_backend(
                        &mut backend,
                        &cfg.label,
                        &cfg.value,
                        theme().widgets.readout.row_h,
                    );
                    crate::debug::tag_backend(
                        &mut *backend,
                        resp.rect,
                        format!("widget[readout #{}]", readout_idx),
                    );
                    response.readouts.push(ReadoutResponse);
                    readout_idx += 1;
                }
                WidgetSpec::SelectList(cfg) => {
                    let sel_key: MaraId = pod_id
                        .with(("mara_pod_select_list_sel", select_list_idx))
                        .into();
                    let mut selected: Option<usize> = backend
                        .memory()
                        .get_persisted::<Option<usize>>(sel_key)
                        .unwrap_or(None);
                    let mut clicked: Option<usize> = None;
                    let mut double_clicked: Option<usize> = None;
                    for (i, label) in cfg.items.iter().enumerate() {
                        let trailing = cfg.trailing.as_ref().map(|t| t[i].as_str());
                        let resp = select_row_backend(
                            &mut backend,
                            ("mara_pod_select_list", select_list_idx, i),
                            label,
                            trailing,
                            selected == Some(i),
                            cfg.accent,
                            theme().widgets.select.row_h,
                        );
                        if resp.clicked() {
                            clicked = Some(i);
                            selected = Some(i);
                        }
                        if resp.double_clicked() {
                            double_clicked = Some(i);
                        }
                    }
                    if clicked.is_some() {
                        backend.memory().set_persisted(sel_key, selected);
                    }
                    response.select_lists.push(SelectListResponse {
                        clicked,
                        double_clicked,
                        selected,
                    });
                    select_list_idx += 1;
                }
                WidgetSpec::HybridSelectList(cfg) => {
                    let sel_key: MaraId = pod_id
                        .with(("mara_pod_hybrid_select_list_sel", hybrid_select_list_idx))
                        .into();
                    let pin_key: MaraId = pod_id
                        .with(("mara_pod_hybrid_select_list_pin", hybrid_select_list_idx))
                        .into();
                    let mut selected: Option<usize> = backend
                        .memory()
                        .get_persisted::<Option<usize>>(sel_key)
                        .unwrap_or(None);
                    let mut pinned: Option<usize> = backend
                        .memory()
                        .get_persisted::<Option<usize>>(pin_key)
                        .unwrap_or(None);
                    let mut body_clicked: Option<usize> = None;
                    let mut body_double_clicked: Option<usize> = None;
                    let mut radio_clicked: Option<usize> = None;
                    for (i, label) in cfg.items.iter().enumerate() {
                        let trailing = cfg.trailing.as_ref().map(|t| t[i].as_str());
                        let resp = hybrid_select_row_backend(
                            &mut backend,
                            ("mara_pod_hybrid_select_list", hybrid_select_list_idx, i),
                            label,
                            trailing,
                            selected == Some(i),
                            pinned == Some(i),
                            cfg.accent,
                            theme().widgets.select.row_h,
                        );
                        if resp.body.clicked {
                            body_clicked = Some(i);
                            selected = Some(i);
                        }
                        if resp.body.double_clicked {
                            body_double_clicked = Some(i);
                        }
                        if resp.radio.clicked {
                            radio_clicked = Some(i);
                            // Single-select radio: clicking an
                            // unpinned row pins it; clicking the
                            // currently-pinned row unpins.
                            pinned = if pinned == Some(i) { None } else { Some(i) };
                        }
                    }
                    if body_clicked.is_some() {
                        backend.memory().set_persisted(sel_key, selected);
                    }
                    if radio_clicked.is_some() {
                        backend.memory().set_persisted(pin_key, pinned);
                    }
                    response.hybrid_select_lists.push(HybridSelectListResponse {
                        body_clicked,
                        body_double_clicked,
                        radio_clicked,
                        selected,
                        pinned,
                    });
                    hybrid_select_list_idx += 1;
                }
                WidgetSpec::Tags(cfg) => {
                    let mut clicked: Option<usize> = None;
                    if let Some(ui) = backend.__internal_egui_ui_mut() {
                        ui.horizontal_wrapped(|ui| {
                            crate::backend::egui::apply_item_spacing_spec(
                                ui,
                                crate::layout::ItemSpacingSpec::new(crate::vocab::Vec2::new(
                                    3.0, 3.0,
                                )),
                            );
                            for (i, item) in cfg.items.iter().enumerate() {
                                let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
                                let fill = item.fill.unwrap_or_else(|| chip_fill(cfg.accent));
                                let resp = chip_colored_backend(
                                    &mut backend,
                                    &item.label,
                                    fill,
                                    cfg.accent,
                                );
                                if resp.clicked() {
                                    clicked = Some(i);
                                }
                            }
                        });
                    }
                    response.tags.push(TagsResponse { clicked });
                    tags_idx += 1;
                    let _ = tags_idx; // silence unused warning when no further widgets follow
                }
                WidgetSpec::Keybindings(cfg) => {
                    for (k, a) in cfg.rows.iter() {
                        keybinding_row_backend(
                            &mut backend,
                            k,
                            a,
                            theme().widgets.keybinding.row_h,
                            crate::style::active_accent(),
                        );
                    }
                    response.keybindings.push(KeybindingsResponse);
                    keybindings_idx += 1;
                    let _ = keybindings_idx;
                }
                WidgetSpec::Badges(cfg) => {
                    for row in cfg.rows.iter() {
                        let labels: Vec<&str> =
                            row.badges.iter().map(|t| t.label.as_str()).collect();
                        let fills: Vec<Option<Color32>> =
                            row.badges.iter().map(|t| t.fill).collect();
                        badge_row_backend(
                            &mut backend,
                            &row.label,
                            &labels,
                            Some(&fills),
                            cfg.accent,
                        );
                    }
                    response.badges.push(BadgesResponse);
                }
                WidgetSpec::Module(mut cfg) => {
                    let module_id = cfg.module.id();
                    let title = cfg.module.title().to_owned();
                    let icon = cfg.module.icon();
                    let ctx = ModuleInlineCtx {
                        pod_id: pod_id.into(),
                        slot_index: slot_idx,
                        accent: crate::style::active_accent(),
                        options: cfg.options,
                        workspace: None,
                    };
                    let accent = ctx.accent;
                    let tag_rect = backend.available_rect();
                    let module_response = cfg
                        .module
                        .inline(&mut crate::mui::MaraUi::over(&mut *backend, accent), ctx);
                    crate::debug::tag_backend(
                        &mut *backend,
                        tag_rect,
                        format!("widget[module #{}]", module_idx),
                    );
                    response.modules.push(ModulePodResponse {
                        id: module_id,
                        title,
                        icon,
                        enter_workspace_requested: module_response.enter_workspace,
                    });
                    module_idx += 1;
                }
                WidgetSpec::Custom { paint, .. } => {
                    if let Some(ui) = backend.__internal_egui_ui_mut() {
                        paint(ui);
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::{ModuleResponse, WorkspaceCtx};

    struct MockModule {
        title: &'static str,
        icon: &'static str,
    }

    impl MockModule {
        fn new(title: &'static str, icon: &'static str) -> Self {
            Self { title, icon }
        }
    }

    impl MaraModule for MockModule {
        fn id(&self) -> crate::vocab::Id {
            crate::vocab::Id::new(("mock-module", self.title, self.icon))
        }

        fn title(&self) -> &str {
            self.title
        }

        fn icon(&self) -> &'static str {
            self.icon
        }

        fn inline(
            &mut self,
            _mui: &mut crate::mui::MaraUi<'_>,
            _ctx: ModuleInlineCtx<'_>,
        ) -> ModuleResponse {
            ModuleResponse::none()
        }

        fn workspace(&mut self, _ws: &mut WorkspaceCtx<'_>) {}
    }

    #[test]
    fn pod_modules_require_title_and_icon() {
        let missing_title = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_module(MockModule::new(" ", "box"));
        });
        let missing_icon = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_module(MockModule::new("Module", ""));
        });
        let valid = Pod::new("pod").with_module(MockModule::new("Module", "box"));

        assert!(missing_title.is_err());
        assert!(missing_icon.is_err());
        assert_eq!(valid.widgets.len(), 1);
    }

    #[test]
    fn pod_dropdowns_require_options_and_valid_initial_index() {
        let empty = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_dropdown(Vec::<String>::new(), 0, Color32::WHITE);
        });
        let blank = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_dropdown(["valid", " "], 0, Color32::WHITE);
        });
        let out_of_bounds = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_dropdown(["one"], 1, Color32::WHITE);
        });
        let valid = Pod::new("pod").with_dropdown(["one"], 0, Color32::WHITE);

        assert!(empty.is_err());
        assert!(blank.is_err());
        assert!(out_of_bounds.is_err());
        assert_eq!(valid.widgets.len(), 1);
    }

    #[test]
    fn pod_select_lists_require_non_empty_items() {
        let empty = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_select_list(Vec::<String>::new(), None, Color32::WHITE);
        });
        let blank = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_select_list(["one", " "], None, Color32::WHITE);
        });
        let hybrid_empty = std::panic::catch_unwind(|| {
            let _ =
                Pod::new("pod").with_hybrid_select_list(Vec::<String>::new(), None, Color32::WHITE);
        });
        let valid = Pod::new("pod").with_select_list(["one"], None, Color32::WHITE);

        assert!(empty.is_err());
        assert!(blank.is_err());
        assert!(hybrid_empty.is_err());
        assert_eq!(valid.widgets.len(), 1);
    }

    #[test]
    fn tag_and_badge_rows_require_visible_labels() {
        let blank_tag = std::panic::catch_unwind(|| {
            let _ = TagItem::new(" ");
        });
        let blank_colored_tag = std::panic::catch_unwind(|| {
            let _ = TagItem::colored("", Color32::WHITE);
        });
        let blank_row_label = std::panic::catch_unwind(|| {
            let _ = BadgeRowSpec::from_strs(" ", ["one"]);
        });
        let empty_badges = std::panic::catch_unwind(|| {
            let _ = BadgeRowSpec::from_strs("row", Vec::<String>::new());
        });
        let valid = BadgeRowSpec::from_strs("row", ["one"]);

        assert!(blank_tag.is_err());
        assert!(blank_colored_tag.is_err());
        assert!(blank_row_label.is_err());
        assert!(empty_badges.is_err());
        assert_eq!(valid.badges.len(), 1);
    }

    #[test]
    fn pod_numeric_widgets_require_finite_in_range_values() {
        let progress_nan = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_progress("loading", f32::NAN, "nan", Color32::WHITE);
        });
        let progress_oob = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_progress("loading", 1.1, "110%", Color32::WHITE);
        });
        let slider_nan = std::panic::catch_unwind(|| {
            let _ =
                Pod::new("pod").with_slider("speed", f64::NAN, 0.0..=1.0, 2, "", Color32::WHITE);
        });
        let slider_bad_range = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_slider("speed", 0.5, 1.0..=0.0, 2, "", Color32::WHITE);
        });
        let slider_oob = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_slider("speed", 2.0, 0.0..=1.0, 2, "", Color32::WHITE);
        });
        let drag_bad_speed = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_drag_value("size", 1.0, -1.0, 0.0..=2.0, 1, "");
        });
        let valid = Pod::new("pod")
            .with_progress("loading", 0.5, "50%", Color32::WHITE)
            .with_slider("speed", 0.5, 0.0..=1.0, 2, "", Color32::WHITE)
            .with_drag_value("size", 1.0, 0.1, 0.0..=2.0, 1, "");

        assert!(progress_nan.is_err());
        assert!(progress_oob.is_err());
        assert!(slider_nan.is_err());
        assert!(slider_bad_range.is_err());
        assert!(slider_oob.is_err());
        assert!(drag_bad_speed.is_err());
        assert_eq!(valid.widgets.len(), 3);
    }

    #[test]
    fn pod_color_widgets_require_finite_unit_channels() {
        let blank_label = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_color_rgb(" ", [0.1, 0.2, 0.3], Color32::WHITE);
        });
        let rgb_nan = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_color_rgb("color", [0.1, f32::NAN, 0.3], Color32::WHITE);
        });
        let rgba_oob = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_color_rgba("color", [0.1, 0.2, 0.3, 2.0], Color32::WHITE);
        });
        let valid = Pod::new("pod")
            .with_color_rgb("rgb", [0.1, 0.2, 0.3], Color32::WHITE)
            .with_color_rgba("rgba", [0.1, 0.2, 0.3, 0.4], Color32::WHITE);

        assert!(blank_label.is_err());
        assert!(rgb_nan.is_err());
        assert!(rgba_oob.is_err());
        assert_eq!(valid.widgets.len(), 2);
    }

    #[test]
    fn pod_text_widgets_require_visible_labels() {
        let blank_search = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_search(" ", Color32::WHITE);
        });
        let blank_button = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_button("", Color32::WHITE);
        });
        let blank_subtitle = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_button_subtitle("Button", " ", Color32::WHITE);
        });
        let blank_styled_glyph = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_button_styled(
                "Button",
                Color32::WHITE,
                None::<String>,
                Some(" "),
                None,
            );
        });
        let blank_toggle = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_toggle(" ", Color32::WHITE);
        });
        let blank_card_glyph = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_card_button("", "Card", "Sub", Color32::WHITE);
        });
        let valid = Pod::new("pod")
            .with_search("search", Color32::WHITE)
            .with_button("Button", Color32::WHITE)
            .with_button_subtitle("Button", "Sub", Color32::WHITE)
            .with_button_styled("Button", Color32::WHITE, Some("Sub"), Some("info"), None)
            .with_toggle("Toggle", Color32::WHITE)
            .with_toggle_initial("Toggle 2", Color32::WHITE, true)
            .with_card_button("info", "Card", "Sub", Color32::WHITE);

        assert!(blank_search.is_err());
        assert!(blank_button.is_err());
        assert!(blank_subtitle.is_err());
        assert!(blank_styled_glyph.is_err());
        assert!(blank_toggle.is_err());
        assert!(blank_card_glyph.is_err());
        assert_eq!(valid.widgets.len(), 7);
    }

    #[test]
    fn pod_select_readout_and_keybinding_widgets_require_visible_text() {
        let blank_select = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_select(" ", None::<String>, false, Color32::WHITE);
        });
        let blank_select_trailing = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_select("Row", Some(" "), false, Color32::WHITE);
        });
        let blank_hybrid = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_hybrid_select(
                "",
                None::<String>,
                false,
                false,
                Color32::WHITE,
            );
        });
        let blank_readout_value = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_readout("Version", " ");
        });
        let empty_keybindings = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_keybindings(Vec::<(String, String)>::new());
        });
        let blank_keybinding = std::panic::catch_unwind(|| {
            let _ = Pod::new("pod").with_keybindings([("Ctrl+K", " ")]);
        });
        let valid = Pod::new("pod")
            .with_select("Row", Some("tail"), false, Color32::WHITE)
            .with_hybrid_select("Hybrid", Some("tail"), false, false, Color32::WHITE)
            .with_readout("Version", "1")
            .with_keybindings([("Ctrl+K", "Command")]);

        assert!(blank_select.is_err());
        assert!(blank_select_trailing.is_err());
        assert!(blank_hybrid.is_err());
        assert!(blank_readout_value.is_err());
        assert!(empty_keybindings.is_err());
        assert!(blank_keybinding.is_err());
        assert_eq!(valid.widgets.len(), 4);
    }

    /// Phase-3 exit gate: pod widget content renders through a pure
    /// `UiBackend` with zero egui in the call path. `paint_widgets`
    /// drives a headless `RecordingBackend` — no `egui::Ui`, no
    /// `egui::Context` — and still emits paint commands and wires
    /// per-widget responses.
    #[test]
    fn paint_widgets_renders_on_recording_backend() {
        use crate::backend::record::RecordingBackend;
        use crate::vocab::{Pos2, Rect, Vec2};

        let mut backend = RecordingBackend::at(Rect::from_min_size(
            Pos2::new(0.0, 0.0),
            Vec2::new(240.0, 96.0),
        ));
        let widgets = vec![
            WidgetSpec::Progress(ProgressConfig {
                label: "loading".into(),
                fraction: 0.5,
                text: "50%".into(),
                accent: Color32::from_rgb(120, 180, 255),
            }),
            WidgetSpec::Readout(ReadoutConfig {
                label: "status".into(),
                value: "ready".into(),
            }),
        ];
        let mut response = PodResponse::default();
        paint_widgets(
            widgets,
            &mut backend,
            &mut response,
            Id::new("headless_pod"),
        );

        assert!(
            !backend.paints.is_empty(),
            "pod content should emit paint commands on a pure backend"
        );
        assert_eq!(response.progress.len(), 1);
        assert_eq!(response.readouts.len(), 1);
    }
}
