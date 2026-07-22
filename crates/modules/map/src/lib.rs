//! `mara_map` — a Mara map component with retained f64 annotations.
//!
//! Public callers use [`MaraMap`], [`MapSurface`], and typed annotation
//! data. egui painting/allocation is internal to this crate.

#![allow(clippy::too_many_arguments)]

mod mvt;

use std::f64::consts::PI;
use std::sync::atomic::{AtomicU64, Ordering};

use mara_core::{
    MaraModule, MaraView, ModuleInlineCtx, ModuleResponse, RibbonAvoidance, ViewCtx, ViewId,
    WorkspaceCtx,
    paint::{PaintCmd, TextFamily},
    vocab::{
        Align2 as MaraAlign2, Color32 as MaraColor32, Id as MaraId, Pos2 as MaraPos2,
        Rect as MaraRect, Stroke as MaraStroke, Vec2 as MaraVec2,
    },
};

const TILE_SIZE: f64 = 256.0;
const MIN_ZOOM: f64 = 0.0;
const MAX_ZOOM: f64 = 22.0;
const MAX_MERCATOR_LAT: f64 = 85.051_128_779_806_6;
pub const DEFAULT_SVG_MARKER: &str = r#"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M12 2 22 12 12 22 2 12Z" fill="currentColor"/></svg>"#;
static NEXT_UUID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn default_annotation_color() -> MaraColor32 {
    if mara_core::style::theme().is_light {
        MaraColor32::BLACK
    } else {
        MaraColor32::WHITE
    }
}

fn default_annotation_fill() -> MaraColor32 {
    let accent = default_annotation_color();
    MaraColor32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 58)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoPosition {
    pub lon: f64,
    pub lat: f64,
}

impl GeoPosition {
    #[must_use]
    pub fn lon_lat(lon: f64, lat: f64) -> Self {
        assert!(lon.is_finite(), "longitude must be finite");
        assert!(lat.is_finite(), "latitude must be finite");
        Self { lon, lat }
    }

    #[must_use]
    pub fn mercator_clamped(self) -> Self {
        Self {
            lon: self.lon,
            lat: self.lat.clamp(-MAX_MERCATOR_LAT, MAX_MERCATOR_LAT),
        }
    }
}

#[must_use]
pub fn lon_lat(lon: f64, lat: f64) -> GeoPosition {
    GeoPosition::lon_lat(lon, lat)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapViewport {
    pub center: GeoPosition,
    pub zoom: f64,
}

impl Default for MapViewport {
    fn default() -> Self {
        Self {
            center: lon_lat(0.0, 0.0),
            zoom: 13.0,
        }
    }
}

impl MapViewport {
    #[must_use]
    pub fn new(center: GeoPosition, zoom: f64) -> Self {
        Self {
            center: center.mercator_clamped(),
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MapAnnotationId {
    pub uuid: u128,
}

impl MapAnnotationId {
    #[must_use]
    pub fn new(source: impl std::hash::Hash) -> Self {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut hasher);
        Self::from_u128(hasher.finish() as u128)
    }

    #[must_use]
    pub fn from_u128(uuid: u128) -> Self {
        assert!(uuid != 0, "map annotation uuid must be non-zero");
        Self { uuid }
    }

    #[must_use]
    pub fn mara_id(self) -> MaraId {
        MaraId::new(("map.annotation", self.uuid))
    }

    #[must_use]
    pub fn new_uuid() -> Self {
        let counter = NEXT_UUID_COUNTER.fetch_add(1, Ordering::Relaxed) as u128;
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        Self::from_u128((time << 32) ^ counter)
    }

    #[must_use]
    pub fn hyphenated(self) -> String {
        let value = self.uuid;
        format!(
            "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
            (value >> 96) as u32,
            (value >> 80) as u16,
            (value >> 64) as u16,
            (value >> 48) as u16,
            value & 0x0000_ffff_ffff_ffff_ffff
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapDocument {
    pub title: String,
    pub annotations: Vec<MapAnnotation>,
}

impl MapDocument {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            annotations: Vec::new(),
        }
    }

    pub fn add(&mut self, annotation: impl Into<MapAnnotation>) {
        self.annotations.push(annotation.into());
    }

    pub fn remove(&mut self, id: MapAnnotationId) -> Option<MapAnnotation> {
        let index = self
            .annotations
            .iter()
            .position(|annotation| annotation.id() == id)?;
        Some(self.annotations.remove(index))
    }

    #[must_use]
    pub fn get(&self, id: MapAnnotationId) -> Option<&MapAnnotation> {
        self.annotations
            .iter()
            .find(|annotation| annotation.id() == id)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MapAnnotation {
    Point(MapPoint),
    Line(MapLine),
    Polygon(MapPolygon),
    Icon(MapIcon),
}

impl MapAnnotation {
    #[must_use]
    pub fn id(&self) -> MapAnnotationId {
        match self {
            Self::Point(v) => v.id,
            Self::Line(v) => v.id,
            Self::Polygon(v) => v.id,
            Self::Icon(v) => v.id,
        }
    }

    #[must_use]
    pub fn kind(&self) -> MapAnnotationKind {
        match self {
            Self::Point(_) => MapAnnotationKind::Point,
            Self::Line(_) => MapAnnotationKind::Line,
            Self::Polygon(_) => MapAnnotationKind::Polygon,
            Self::Icon(_) => MapAnnotationKind::Icon,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapAnnotationKind {
    Point,
    Line,
    Polygon,
    Icon,
}

impl From<MapPoint> for MapAnnotation {
    fn from(value: MapPoint) -> Self {
        Self::Point(value)
    }
}

impl From<MapLine> for MapAnnotation {
    fn from(value: MapLine) -> Self {
        Self::Line(value)
    }
}

impl From<MapPolygon> for MapAnnotation {
    fn from(value: MapPolygon) -> Self {
        Self::Polygon(value)
    }
}

impl From<MapIcon> for MapAnnotation {
    fn from(value: MapIcon) -> Self {
        Self::Icon(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapPoint {
    pub id: MapAnnotationId,
    pub position: GeoPosition,
    pub label: Option<String>,
    pub color: MaraColor32,
}

impl MapPoint {
    #[must_use]
    pub fn new(id: impl std::hash::Hash, position: GeoPosition) -> Self {
        Self {
            id: MapAnnotationId::new(id),
            position,
            label: None,
            color: default_annotation_color(),
        }
    }

    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapLine {
    pub id: MapAnnotationId,
    pub points: Vec<GeoPosition>,
    pub label: Option<String>,
    pub color: MaraColor32,
}

impl MapLine {
    #[must_use]
    pub fn new(id: impl std::hash::Hash, points: Vec<GeoPosition>) -> Self {
        assert!(points.len() >= 2, "lines require at least two points");
        Self {
            id: MapAnnotationId::new(id),
            points,
            label: None,
            color: default_annotation_color(),
        }
    }

    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapPolygon {
    pub id: MapAnnotationId,
    pub points: Vec<GeoPosition>,
    pub label: Option<String>,
    pub fill: MaraColor32,
    pub stroke: MaraStroke,
}

impl MapPolygon {
    #[must_use]
    pub fn new(id: impl std::hash::Hash, points: Vec<GeoPosition>) -> Self {
        assert!(points.len() >= 3, "polygons require at least three points");
        Self {
            id: MapAnnotationId::new(id),
            points,
            label: None,
            fill: default_annotation_fill(),
            stroke: MaraStroke::new(1.5, default_annotation_color()),
        }
    }

    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapIcon {
    pub id: MapAnnotationId,
    pub position: GeoPosition,
    pub glyph: MapIconGlyph,
    pub label: Option<String>,
    pub color: MaraColor32,
    pub size: f32,
}

impl MapIcon {
    #[must_use]
    pub fn fluent(
        id: impl std::hash::Hash,
        position: GeoPosition,
        icon: impl Into<String>,
    ) -> Self {
        Self::new(id, position, MapIconGlyph::Fluent(icon.into()))
    }

    #[must_use]
    pub fn svg(id: impl std::hash::Hash, position: GeoPosition, svg: impl Into<String>) -> Self {
        Self::new(id, position, MapIconGlyph::Svg(svg.into()))
    }

    #[must_use]
    pub fn text(id: impl std::hash::Hash, position: GeoPosition, text: impl Into<String>) -> Self {
        Self::new(id, position, MapIconGlyph::Text(text.into()))
    }

    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    fn new(id: impl std::hash::Hash, position: GeoPosition, glyph: MapIconGlyph) -> Self {
        Self {
            id: MapAnnotationId::new(id),
            position,
            glyph,
            label: None,
            color: default_annotation_color(),
            size: 22.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapIconGlyph {
    Fluent(String),
    Svg(String),
    Text(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MapTool {
    #[default]
    Select,
    Point,
    Line,
    Polygon,
    Icon,
    Svg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapFeatureGeometry {
    Point,
    Line,
    Polygon,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapFeatureInfo {
    pub layer: String,
    pub class: String,
    pub geometry: MapFeatureGeometry,
    pub name: Option<String>,
    pub properties: Vec<(String, String)>,
    pub paths: Vec<Vec<GeoPosition>>,
}

impl MapFeatureInfo {
    #[must_use]
    pub fn type_label(&self) -> String {
        if self.class.is_empty() {
            self.layer.clone()
        } else {
            format!("{} / {}", self.layer, self.class)
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapInteraction {
    pub tool: MapTool,
    pub selected: Option<MapAnnotationId>,
    pub selected_kind: Option<MapAnnotationKind>,
    pub selected_uuid: Option<u128>,
    pub selected_feature: Option<MapFeatureInfo>,
    pub basemap_selection_enabled: bool,
    draft: Vec<GeoPosition>,
}

impl MapInteraction {
    pub fn set_tool(&mut self, tool: MapTool) {
        if self.tool != tool {
            self.draft.clear();
        }
        self.tool = tool;
    }

    pub fn clear_draft(&mut self) {
        self.draft.clear();
    }

    fn pop_draft(&mut self) {
        self.draft.pop();
    }

    pub fn select(&mut self, annotation: &MapAnnotation) {
        let id = annotation.id();
        self.selected = Some(id);
        self.selected_kind = Some(annotation.kind());
        self.selected_uuid = Some(id.uuid);
        self.selected_feature = None;
    }

    pub fn select_feature(&mut self, feature: MapFeatureInfo) {
        self.selected = None;
        self.selected_kind = None;
        self.selected_uuid = None;
        self.selected_feature = Some(feature);
    }

    pub fn clear_selection(&mut self) {
        self.selected = None;
        self.selected_kind = None;
        self.selected_uuid = None;
        self.selected_feature = None;
    }

    #[must_use]
    pub fn draft_len(&self) -> usize {
        self.draft.len()
    }

    fn next_annotation_id(&mut self) -> MapAnnotationId {
        MapAnnotationId::new_uuid()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaraMapResponse {
    pub hovered_position: Option<GeoPosition>,
    pub clicked_position: Option<GeoPosition>,
    pub selected: Option<MapAnnotationId>,
    pub selected_kind: Option<MapAnnotationKind>,
    pub selected_uuid: Option<u128>,
    pub selected_feature: Option<MapFeatureInfo>,
    pub deleted: Option<MapAnnotationId>,
}

pub struct MapSurface {
    id: egui::Id,
    pub document: MapDocument,
    pub viewport: MapViewport,
    vector_tiles: mvt::VectorTileCache,
    fast_frames_remaining: u8,
    image_loaders_installed: bool,
    /// Interaction state used when this surface renders as a
    /// `ViewNode` leaf (`MaraView::show`): tool, selection, and the
    /// multi-click draft MUST persist across frames or a leaf-hosted
    /// map can never finish drawing a line/polygon. Hosts that drive
    /// the map through [`MaraMap::new`] own their interaction state
    /// separately and never touch this.
    leaf_interaction: MapInteraction,
}

impl MapSurface {
    #[must_use]
    pub fn new(id: impl std::hash::Hash, document: MapDocument, viewport: MapViewport) -> Self {
        Self {
            id: egui::Id::new(id),
            document,
            viewport,
            vector_tiles: mvt::VectorTileCache::default(),
            fast_frames_remaining: 10,
            image_loaders_installed: false,
            leaf_interaction: MapInteraction::default(),
        }
    }

    /// Ask the map to paint a lightweight basemap for the next few
    /// frames. Hosts should call this when switching into a map-heavy
    /// view so persistent app chrome is not blocked by the first full
    /// vector-tile paint.
    pub fn defer_full_detail(&mut self) {
        self.fast_frames_remaining = self.fast_frames_remaining.max(10);
    }

    /// Start/poll vector-tile loading without painting the map.
    ///
    /// Hosts can call this while the map view is inactive so the
    /// current viewport's basemap is already in the shared surface
    /// cache when the user switches to the map.
    pub fn prewarm_tiles(&mut self, ctx: &ViewCtx<'_>, size: impl Into<MaraVec2>) {
        self.__internal_prewarm_tiles(ctx.__internal_egui_ctx(), size);
    }

    #[doc(hidden)]
    pub(crate) fn __internal_prewarm_tiles(
        &mut self,
        ctx: &egui::Context,
        size: impl Into<MaraVec2>,
    ) {
        let size = size.into();
        if size.x < 16.0 || size.y < 16.0 {
            return;
        }
        mvt::prewarm_vector_basemap(ctx, self.viewport, size, &mut self.vector_tiles);
    }
}

pub struct MaraMap<'a> {
    surface: &'a mut MapSurface,
    interaction: &'a mut MapInteraction,
}

impl<'a> MaraMap<'a> {
    #[must_use]
    pub fn new(surface: &'a mut MapSurface, interaction: &'a mut MapInteraction) -> Self {
        Self {
            surface,
            interaction,
        }
    }

    pub fn show(self, ctx: &mut ViewCtx<'_>) -> MaraMapResponse {
        // Render into this node's REGION, not a window-grabbing panel
        // (ADR 0002 / PLAN WS6): a map hosted as a split cell draws and
        // interacts inside its cell rect, so it tiles like any other
        // leaf. Whole-window is just the one-leaf tree.
        let region: egui::Rect = ctx.content_rect().into();
        self.__internal_show_in(ctx.__internal_egui_ctx(), region)
    }

    #[doc(hidden)]
    pub(crate) fn __internal_show(self, ctx: &egui::Context) -> MaraMapResponse {
        let region = ctx.content_rect();
        self.__internal_show_in(ctx, region)
    }

    fn __internal_show_in(self, ctx: &egui::Context, region: egui::Rect) -> MaraMapResponse {
        let mut output = MaraMapResponse::default();
        egui::Area::new(egui::Id::new(("mara_map_view", self.surface.id)))
            .order(egui::Order::Background)
            .fixed_pos(region.min)
            .show(ctx, |ui| {
                ui.set_clip_rect(region);
                ui.set_min_size(region.size());
                ui.set_max_size(region.size());
                output = paint_map(ui, self.surface, self.interaction);
            });
        output
    }
}

impl MaraView for MapSurface {
    fn id(&self) -> ViewId {
        ViewId::from(self.id)
    }

    fn title(&self) -> &str {
        &self.document.title
    }

    fn icon(&self) -> &'static str {
        "location"
    }

    fn content_avoidance(&self) -> RibbonAvoidance {
        RibbonAvoidance::all()
    }

    fn show(&mut self, ctx: &mut ViewCtx<'_>) {
        // Persist the leaf interaction state across frames (tool,
        // selection, multi-click draft) — a fresh Default here would
        // reset the draft every frame and break line/polygon drawing.
        let mut interaction = std::mem::take(&mut self.leaf_interaction);
        let _ = MaraMap::new(self, &mut interaction).show(ctx);
        self.leaf_interaction = interaction;
    }
}

impl MaraModule for MapSurface {
    fn id(&self) -> mara_core::vocab::Id {
        self.id.into()
    }

    fn title(&self) -> &str {
        &self.document.title
    }

    fn icon(&self) -> &'static str {
        "location"
    }

    fn inline(
        &mut self,
        mui: &mut mara_core::MaraUi<'_>,
        ctx: ModuleInlineCtx<'_>,
    ) -> ModuleResponse {
        let annotation_count = self.document.annotations.len().to_string();
        mui.readout("Map", &self.document.title);
        mui.readout("annotations", &annotation_count);
        if ctx.can_enter_workspace() && mui.button("Open map").clicked() {
            ModuleResponse::enter_workspace()
        } else {
            ModuleResponse::none()
        }
    }

    fn workspace(&mut self, _ws: &mut WorkspaceCtx<'_>) {}
}

fn paint_map(
    ui: &mut egui::Ui,
    surface: &mut MapSurface,
    interaction: &mut MapInteraction,
) -> MaraMapResponse {
    if !surface.image_loaders_installed {
        egui_extras::install_image_loaders(ui.ctx());
        surface.image_loaders_installed = true;
    }

    let desired = ui.available_size_before_wrap();
    let (response, painter) = ui.allocate_painter(desired, egui::Sense::click_and_drag());
    let rect = response.rect;
    let mut fast_basemap = surface.fast_frames_remaining > 0;
    if surface.fast_frames_remaining > 0 {
        surface.fast_frames_remaining -= 1;
    }

    if response.dragged_by(egui::PointerButton::Middle) {
        fast_basemap = true;
        let delta = ui.input(|input| input.pointer.delta());
        if delta != egui::Vec2::ZERO {
            pan_viewport(&mut surface.viewport, delta);
            surface.fast_frames_remaining = surface.fast_frames_remaining.max(3);
        }
    }

    if response.hovered() {
        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() > f32::EPSILON {
            zoom_viewport_at(
                &mut surface.viewport,
                rect,
                response.hover_pos().unwrap_or(rect.center()),
                f64::from(scroll) / 320.0,
            );
            fast_basemap = true;
            surface.fast_frames_remaining = surface.fast_frames_remaining.max(3);
        }
    }

    let deleted = if ui.input(|input| input.key_pressed(egui::Key::Delete)) {
        interaction
            .selected
            .and_then(|id| surface.document.remove(id).map(|_| id))
    } else {
        None
    };
    if deleted.is_some() {
        interaction.clear_selection();
    } else if let Some(id) = interaction.selected
        && surface.document.get(id).is_none()
    {
        interaction.clear_selection();
    }

    let mut selected = interaction.selected;

    mvt::paint_vector_basemap(
        ui,
        rect.into(),
        surface.viewport,
        &mut surface.vector_tiles,
        fast_basemap,
    );
    if interaction.basemap_selection_enabled
        && let Some(feature) = interaction.selected_feature.as_ref()
    {
        paint_selected_feature(&painter, rect, surface.viewport, feature);
    }

    for annotation in &surface.document.annotations {
        paint_annotation(
            ui,
            &painter,
            rect,
            surface.viewport,
            annotation,
            interaction.selected,
        );
    }
    paint_draft(&painter, rect, surface.viewport, interaction);
    paint_corner_darkening_overlay(&painter, rect);

    let hovered_position = response
        .hover_pos()
        .map(|pos| screen_to_geo(pos, rect, surface.viewport));
    let clicked_position = response
        .clicked_by(egui::PointerButton::Primary)
        .then(|| response.interact_pointer_pos())
        .flatten()
        .map(|pos| screen_to_geo(pos, rect, surface.viewport));

    if response.clicked_by(egui::PointerButton::Secondary) {
        cancel_tool_step(interaction);
        selected = interaction.selected;
    }

    if let Some(pos) = response
        .interact_pointer_pos()
        .filter(|_| response.clicked_by(egui::PointerButton::Primary))
    {
        if !interaction.basemap_selection_enabled
            && let Some(hit) = hit_test(&surface.document, rect, surface.viewport, pos)
        {
            if let Some(annotation) = surface.document.get(hit) {
                interaction.select(annotation);
            }
            selected = Some(hit);
        } else if interaction.tool == MapTool::Select && interaction.basemap_selection_enabled {
            if let Some(feature) = mvt::hit_test_vector_feature(
                rect.into(),
                surface.viewport,
                &surface.vector_tiles,
                pos.into(),
            ) {
                interaction.select_feature(feature);
                selected = None;
            } else if let Some(geo) = clicked_position {
                apply_tool(
                    &mut surface.document,
                    interaction,
                    geo,
                    response.double_clicked(),
                );
                selected = interaction.selected;
            }
        } else if let Some(geo) = clicked_position {
            apply_tool(
                &mut surface.document,
                interaction,
                geo,
                response.double_clicked(),
            );
            selected = interaction.selected;
        }
    }

    MaraMapResponse {
        hovered_position,
        clicked_position,
        selected,
        selected_kind: interaction.selected_kind,
        selected_uuid: interaction.selected_uuid,
        selected_feature: interaction.selected_feature.clone(),
        deleted,
    }
}

fn paint_corner_darkening_overlay(painter: &egui::Painter, rect: egui::Rect) {
    let accent: egui::Color32 = mara_core::style::raw_accent().into();
    let steps = 28;
    let mut mesh = egui::Mesh::default();

    for y in 0..=steps {
        let ty = y as f32 / steps as f32;
        for x in 0..=steps {
            let tx = x as f32 / steps as f32;
            let pos = egui::pos2(
                egui::lerp(rect.left()..=rect.right(), tx),
                egui::lerp(rect.top()..=rect.bottom(), ty),
            );
            let nx = tx * 2.0 - 1.0;
            let ny = ty * 2.0 - 1.0;
            let radial = (nx * nx + ny * ny).sqrt().min(1.28) / 1.28;
            let vignette = smoothstep(0.62, 1.0, radial);
            let alpha = (vignette * 148.0).round() as u8;
            let color = egui::Color32::from_rgba_unmultiplied(
                (f32::from(accent.r()) * 0.038) as u8,
                (f32::from(accent.g()) * 0.038) as u8,
                (f32::from(accent.b()) * 0.038) as u8,
                alpha,
            );
            mesh.vertices.push(egui::epaint::Vertex {
                pos,
                uv: egui::epaint::WHITE_UV,
                color,
            });
        }
    }

    let row = steps + 1;
    for y in 0..steps {
        for x in 0..steps {
            let i = y * row + x;
            mesh.indices
                .extend_from_slice(&[i, i + 1, i + row + 1, i, i + row + 1, i + row]);
        }
    }

    painter.add(egui::Shape::mesh(mesh));
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn cancel_tool_step(interaction: &mut MapInteraction) {
    match interaction.tool {
        MapTool::Line | MapTool::Polygon => match interaction.draft_len() {
            2.. => interaction.pop_draft(),
            1 => interaction.clear_draft(),
            0 => interaction.set_tool(MapTool::Select),
        },
        MapTool::Point | MapTool::Icon | MapTool::Svg => interaction.set_tool(MapTool::Select),
        MapTool::Select => {}
    }
}

fn apply_tool(
    document: &mut MapDocument,
    interaction: &mut MapInteraction,
    position: GeoPosition,
    finish: bool,
) {
    match interaction.tool {
        MapTool::Select => {
            interaction.clear_selection();
        }
        MapTool::Point => {
            let id = interaction.next_annotation_id();
            let annotation = MapAnnotation::from(MapPoint {
                id,
                position,
                label: None,
                color: default_annotation_color(),
            });
            interaction.select(&annotation);
            document.add(annotation);
        }
        MapTool::Icon => {
            let id = interaction.next_annotation_id();
            let annotation = MapAnnotation::from(MapIcon {
                id,
                position,
                glyph: MapIconGlyph::Fluent("location".to_owned()),
                label: Some("icon".to_owned()),
                color: MaraColor32::WHITE,
                size: 22.0,
            });
            interaction.select(&annotation);
            document.add(annotation);
        }
        MapTool::Svg => {
            let id = interaction.next_annotation_id();
            let annotation = MapAnnotation::from(MapIcon {
                id,
                position,
                glyph: MapIconGlyph::Svg(DEFAULT_SVG_MARKER.to_owned()),
                label: Some("svg".to_owned()),
                color: MaraColor32::WHITE,
                size: 22.0,
            });
            interaction.select(&annotation);
            document.add(annotation);
        }
        MapTool::Line => {
            interaction.draft.push(position);
            if finish && interaction.draft.len() >= 2 {
                let id = interaction.next_annotation_id();
                let mut line = MapLine::new(id, std::mem::take(&mut interaction.draft));
                line.color = default_annotation_color();
                let annotation = MapAnnotation::from(line);
                interaction.select(&annotation);
                document.add(annotation);
            }
        }
        MapTool::Polygon => {
            interaction.draft.push(position);
            if finish && interaction.draft.len() >= 3 {
                let id = interaction.next_annotation_id();
                let mut polygon = MapPolygon::new(id, std::mem::take(&mut interaction.draft));
                polygon.fill = default_annotation_fill();
                polygon.stroke.color = default_annotation_color();
                let annotation = MapAnnotation::from(polygon);
                interaction.select(&annotation);
                document.add(annotation);
            }
        }
    }
}

fn pan_viewport(viewport: &mut MapViewport, delta: egui::Vec2) {
    let center = geo_to_world(viewport.center, viewport.zoom);
    viewport.center = world_to_geo(
        (center.0 - f64::from(delta.x), center.1 - f64::from(delta.y)),
        viewport.zoom,
    );
}

fn zoom_viewport_at(
    viewport: &mut MapViewport,
    rect: egui::Rect,
    anchor: egui::Pos2,
    zoom_delta: f64,
) {
    let old_zoom = viewport.zoom;
    let new_zoom = (old_zoom + zoom_delta).clamp(MIN_ZOOM, MAX_ZOOM);
    if (new_zoom - old_zoom).abs() < f64::EPSILON {
        return;
    }
    let anchor_geo = screen_to_geo(anchor, rect, *viewport);
    let anchor_world = geo_to_world(anchor_geo, new_zoom);
    viewport.center = world_to_geo(
        (
            anchor_world.0 - f64::from(anchor.x - rect.center().x),
            anchor_world.1 - f64::from(anchor.y - rect.center().y),
        ),
        new_zoom,
    );
    viewport.zoom = new_zoom;
}

fn paint_annotation(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
    viewport: MapViewport,
    annotation: &MapAnnotation,
    selected: Option<MapAnnotationId>,
) {
    let is_selected = selected == Some(annotation.id());
    for cmd in map_annotation_paint_cmds(rect, viewport, annotation, is_selected) {
        if matches!(&cmd, PaintCmd::Svg { .. }) {
            mara_core::paint::__internal_render_paint_cmd_egui_ui(ui, cmd);
        } else {
            mara_core::paint::__internal_render_paint_cmd_egui(painter, cmd);
        }
    }
}

fn map_annotation_paint_cmds(
    rect: egui::Rect,
    viewport: MapViewport,
    annotation: &MapAnnotation,
    is_selected: bool,
) -> Vec<PaintCmd> {
    let mut cmds = Vec::new();
    match annotation {
        MapAnnotation::Point(point) => {
            let pos = geo_to_screen(point.position, rect, viewport).into();
            let radius = if is_selected { 8.0 } else { 5.0 };
            cmds.push(PaintCmd::CircleFilled {
                center: pos,
                radius,
                fill: point.color,
            });
            if is_selected {
                cmds.push(PaintCmd::CircleStroke {
                    center: pos,
                    radius: radius + 4.0,
                    stroke: MaraStroke::new(2.0, selection_color(point.color.into()).into()),
                });
            }
        }
        MapAnnotation::Line(line) => {
            let points = screen_points(&line.points, rect, viewport);
            if points.len() >= 2 {
                if is_selected {
                    cmds.push(PaintCmd::Polyline {
                        points: points.iter().copied().map(Into::into).collect(),
                        stroke: MaraStroke::new(7.0, selection_color(line.color.into()).into()),
                    });
                }
                cmds.push(PaintCmd::Polyline {
                    points: points.into_iter().map(Into::into).collect(),
                    stroke: MaraStroke::new(if is_selected { 3.5 } else { 2.5 }, line.color),
                });
            }
        }
        MapAnnotation::Polygon(poly) => {
            let points = normalized_polygon_points(&screen_points(&poly.points, rect, viewport));
            if points.len() >= 3 {
                let points_mara: Vec<_> = points.iter().copied().map(Into::into).collect();
                cmds.push(PaintCmd::Polygon {
                    points: points_mara.clone(),
                    fill: poly.fill,
                    stroke: poly.stroke,
                });
                if is_selected {
                    cmds.push(PaintCmd::Polygon {
                        points: points_mara.clone(),
                        fill: MaraColor32::TRANSPARENT,
                        stroke: MaraStroke::new(
                            poly.stroke.width + 3.0,
                            selection_color(poly.stroke.color.into()).into(),
                        ),
                    });
                    cmds.push(PaintCmd::Polygon {
                        points: points_mara,
                        fill: MaraColor32::TRANSPARENT,
                        stroke: MaraStroke::new(poly.stroke.width + 0.8, poly.stroke.color),
                    });
                }
            }
        }
        MapAnnotation::Icon(icon) => {
            let pos = geo_to_screen(icon.position, rect, viewport);
            if let Some(cmd) = map_icon_paint_cmd(pos, &icon.glyph, icon.size, icon.color) {
                cmds.push(cmd);
            }
            if is_selected {
                cmds.push(PaintCmd::CircleStroke {
                    center: pos.into(),
                    radius: icon.size * 0.7,
                    stroke: MaraStroke::new(2.0, MaraColor32::WHITE),
                });
            }
        }
    }
    cmds
}

fn map_icon_paint_cmd(
    pos: egui::Pos2,
    glyph: &MapIconGlyph,
    size: f32,
    color: MaraColor32,
) -> Option<PaintCmd> {
    match glyph {
        MapIconGlyph::Fluent(name) => {
            let (glyph, family) = mara_core::icons::icon_glyph(name)?;
            Some(PaintCmd::TextWithFamily {
                pos: pos.into(),
                anchor: MaraAlign2::CENTER_CENTER,
                text: glyph.to_string(),
                size,
                color,
                family: TextFamily::Named(family),
            })
        }
        MapIconGlyph::Text(text) => Some(PaintCmd::Text {
            pos: pos.into(),
            anchor: MaraAlign2::CENTER_CENTER,
            text: text.clone(),
            size,
            color,
            mono: false,
        }),
        MapIconGlyph::Svg(svg) => Some(PaintCmd::Svg {
            svg: svg.clone(),
            rect: MaraRect::from_center_size(pos.into(), MaraVec2::new(size, size)),
            tint: color,
        }),
    }
}

fn paint_selected_feature(
    painter: &egui::Painter,
    rect: egui::Rect,
    viewport: MapViewport,
    feature: &MapFeatureInfo,
) {
    for cmd in selected_feature_paint_cmds(rect, viewport, feature) {
        mara_core::paint::__internal_render_paint_cmd_egui(painter, cmd);
    }
}

fn selected_feature_paint_cmds(
    rect: egui::Rect,
    viewport: MapViewport,
    feature: &MapFeatureInfo,
) -> Vec<PaintCmd> {
    let accent = mara_core::style::raw_accent();
    let fill = MaraColor32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 54);
    let halo = MaraColor32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 105);
    let stroke = MaraStroke::new(2.4, accent);
    let halo_stroke = MaraStroke::new(7.0, halo);
    let mut cmds = Vec::new();

    match feature.geometry {
        MapFeatureGeometry::Polygon => {
            for path in &feature.paths {
                let points = normalized_polygon_points(&screen_points(path, rect, viewport));
                if points.len() >= 3 {
                    let points: Vec<_> = points.into_iter().map(Into::into).collect();
                    cmds.push(PaintCmd::Polygon {
                        points: points.clone(),
                        fill,
                        stroke: MaraStroke::NONE,
                    });
                    cmds.push(PaintCmd::Polygon {
                        points: points.clone(),
                        fill: MaraColor32::TRANSPARENT,
                        stroke: halo_stroke,
                    });
                    cmds.push(PaintCmd::Polygon {
                        points,
                        fill: MaraColor32::TRANSPARENT,
                        stroke,
                    });
                }
            }
        }
        MapFeatureGeometry::Line => {
            for path in &feature.paths {
                let points = screen_points(path, rect, viewport);
                if points.len() >= 2 {
                    cmds.push(PaintCmd::Polyline {
                        points: points.iter().copied().map(Into::into).collect(),
                        stroke: halo_stroke,
                    });
                    cmds.push(PaintCmd::Polyline {
                        points: points.into_iter().map(Into::into).collect(),
                        stroke,
                    });
                }
            }
        }
        MapFeatureGeometry::Point => {
            for point in feature.paths.iter().filter_map(|path| path.first()) {
                let pos = geo_to_screen(*point, rect, viewport).into();
                cmds.push(PaintCmd::CircleFilled {
                    center: pos,
                    radius: 8.0,
                    fill,
                });
                cmds.push(PaintCmd::CircleStroke {
                    center: pos,
                    radius: 10.0,
                    stroke,
                });
            }
        }
    }
    cmds
}

fn selection_color(base: egui::Color32) -> egui::Color32 {
    let theme = mara_core::style::theme();
    let target = if theme.is_light {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    };
    blend_color(target, base, 0.22, 190)
}

fn blend_color(a: egui::Color32, b: egui::Color32, b_amount: f32, alpha: u8) -> egui::Color32 {
    let b_amount = b_amount.clamp(0.0, 1.0);
    let a_amount = 1.0 - b_amount;
    let blend = |x: u8, y: u8| (f32::from(x) * a_amount + f32::from(y) * b_amount).round() as u8;
    egui::Color32::from_rgba_unmultiplied(
        blend(a.r(), b.r()),
        blend(a.g(), b.g()),
        blend(a.b(), b.b()),
        alpha,
    )
}

fn paint_draft(
    painter: &egui::Painter,
    rect: egui::Rect,
    viewport: MapViewport,
    interaction: &MapInteraction,
) {
    for cmd in draft_paint_cmds(rect, viewport, interaction) {
        mara_core::paint::__internal_render_paint_cmd_egui(painter, cmd);
    }
}

fn draft_paint_cmds(
    rect: egui::Rect,
    viewport: MapViewport,
    interaction: &MapInteraction,
) -> Vec<PaintCmd> {
    let color = mara_core::style::raw_accent();
    let points = screen_points(&interaction.draft, rect, viewport);
    let mut cmds = Vec::new();
    for point in &points {
        cmds.push(PaintCmd::CircleFilled {
            center: (*point).into(),
            radius: 4.0,
            fill: color,
        });
    }
    if points.len() >= 2 {
        cmds.push(PaintCmd::Polyline {
            points: points.into_iter().map(Into::into).collect(),
            stroke: MaraStroke::new(2.0, color),
        });
    }
    cmds
}

fn normalized_polygon_points(points: &[egui::Pos2]) -> Vec<egui::Pos2> {
    let mut out = Vec::with_capacity(points.len());
    for point in points {
        if out
            .last()
            .is_none_or(|last: &egui::Pos2| last.distance(*point) > f32::EPSILON)
        {
            out.push(*point);
        }
    }
    if out.len() > 1
        && out
            .first()
            .zip(out.last())
            .is_some_and(|(first, last)| first.distance(*last) <= f32::EPSILON)
    {
        out.pop();
    }
    out
}

pub(crate) fn triangulate_polygon(points: &[MaraPos2]) -> Vec<[MaraPos2; 3]> {
    if points.len() < 3 {
        return Vec::new();
    }

    let mut indices: Vec<usize> = (0..points.len()).collect();
    if signed_area(points) < 0.0 {
        indices.reverse();
    }

    let mut triangles = Vec::with_capacity(points.len().saturating_sub(2));
    if is_convex_polygon(points) {
        let root = indices[0];
        for edge in indices[1..].windows(2) {
            triangles.push([points[root], points[edge[0]], points[edge[1]]]);
        }
        return triangles;
    }

    let mut guard = 0;
    while indices.len() > 3 && guard < points.len() * points.len() {
        guard += 1;
        let len = indices.len();
        let mut ear = None;
        for i in 0..len {
            let prev = indices[(i + len - 1) % len];
            let curr = indices[i];
            let next = indices[(i + 1) % len];
            if is_ear(prev, curr, next, &indices, points) {
                ear = Some((i, prev, curr, next));
                break;
            }
        }

        let Some((ear_index, prev, curr, next)) = ear else {
            break;
        };
        triangles.push([points[prev], points[curr], points[next]]);
        indices.remove(ear_index);
    }

    if indices.len() == 3 {
        triangles.push([points[indices[0]], points[indices[1]], points[indices[2]]]);
    }
    triangles
}

fn is_convex_polygon(points: &[MaraPos2]) -> bool {
    if points.len() < 4 {
        return true;
    }

    let mut sign = 0.0_f32;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let c = points[(i + 2) % points.len()];
        let ab = MaraVec2::new(b.x - a.x, b.y - a.y);
        let bc = MaraVec2::new(c.x - b.x, c.y - b.y);
        let cross = ab.x * bc.y - ab.y * bc.x;
        if cross.abs() <= 1.0e-4 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if sign * cross < 0.0 {
            return false;
        }
    }
    true
}

fn is_ear(prev: usize, curr: usize, next: usize, polygon: &[usize], points: &[MaraPos2]) -> bool {
    let a = points[prev];
    let b = points[curr];
    let c = points[next];
    if cross(a, b, c) <= f32::EPSILON {
        return false;
    }
    !polygon.iter().copied().any(|idx| {
        idx != prev && idx != curr && idx != next && point_in_triangle(points[idx], a, b, c)
    })
}

fn signed_area(points: &[MaraPos2]) -> f32 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f32>()
        * 0.5
}

fn cross(a: MaraPos2, b: MaraPos2, c: MaraPos2) -> f32 {
    let ab = MaraVec2::new(b.x - a.x, b.y - a.y);
    let ac = MaraVec2::new(c.x - a.x, c.y - a.y);
    ab.x * ac.y - ab.y * ac.x
}

fn point_in_triangle(p: MaraPos2, a: MaraPos2, b: MaraPos2, c: MaraPos2) -> bool {
    let ab = cross(a, b, p);
    let bc = cross(b, c, p);
    let ca = cross(c, a, p);
    ab >= -f32::EPSILON && bc >= -f32::EPSILON && ca >= -f32::EPSILON
}

fn hit_test(
    document: &MapDocument,
    rect: egui::Rect,
    viewport: MapViewport,
    pos: egui::Pos2,
) -> Option<MapAnnotationId> {
    document.annotations.iter().rev().find_map(|ann| {
        let hit = match ann {
            MapAnnotation::Point(point) => {
                geo_to_screen(point.position, rect, viewport).distance(pos) <= 12.0
            }
            MapAnnotation::Icon(icon) => {
                geo_to_screen(icon.position, rect, viewport).distance(pos) <= icon.size
            }
            MapAnnotation::Line(line) => screen_points(&line.points, rect, viewport)
                .windows(2)
                .any(|w| distance_to_segment(pos, w[0], w[1]) <= 8.0),
            MapAnnotation::Polygon(poly) => {
                polygon_edge_hit(pos, &screen_points(&poly.points, rect, viewport), 8.0)
            }
        };
        hit.then(|| ann.id())
    })
}

fn polygon_edge_hit(pos: egui::Pos2, points: &[egui::Pos2], tolerance: f32) -> bool {
    if points.len() < 2 {
        return false;
    }
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .any(|(a, b)| distance_to_segment(pos, *a, *b) <= tolerance)
}

fn screen_points(
    points: &[GeoPosition],
    rect: egui::Rect,
    viewport: MapViewport,
) -> Vec<egui::Pos2> {
    points
        .iter()
        .map(|point| geo_to_screen(*point, rect, viewport))
        .collect()
}

fn distance_to_segment(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let denom = ab.dot(ab);
    if denom <= f32::EPSILON {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / denom).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

fn geo_to_screen(position: GeoPosition, rect: egui::Rect, viewport: MapViewport) -> egui::Pos2 {
    let center = geo_to_world(viewport.center, viewport.zoom);
    let world = geo_to_world(position, viewport.zoom);
    egui::pos2(
        rect.center().x + (world.0 - center.0) as f32,
        rect.center().y + (world.1 - center.1) as f32,
    )
}

fn screen_to_geo(pos: egui::Pos2, rect: egui::Rect, viewport: MapViewport) -> GeoPosition {
    let center = geo_to_world(viewport.center, viewport.zoom);
    world_to_geo(
        (
            center.0 + f64::from(pos.x - rect.center().x),
            center.1 + f64::from(pos.y - rect.center().y),
        ),
        viewport.zoom,
    )
}

fn geo_to_world(position: GeoPosition, zoom: f64) -> (f64, f64) {
    let p = position.mercator_clamped();
    let size = TILE_SIZE * 2.0_f64.powf(zoom);
    let x = (p.lon + 180.0) / 360.0 * size;
    let lat_rad = p.lat.to_radians();
    let y = (1.0 - ((lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / PI)) * 0.5 * size;
    (x, y)
}

fn world_to_geo(world: (f64, f64), zoom: f64) -> GeoPosition {
    let size = TILE_SIZE * 2.0_f64.powf(zoom);
    let lon = world.0 / size * 360.0 - 180.0;
    let n = PI - 2.0 * PI * world.1 / size;
    lon_lat(wrap_lon(lon), n.sinh().atan().to_degrees()).mercator_clamped()
}

fn wrap_lon(lon: f64) -> f64 {
    (lon + 180.0).rem_euclid(360.0) - 180.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_view<T: MaraView>(_value: &T) {}
    fn assert_module<T: MaraModule>(_value: &T) {}

    fn test_map_rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(256.0, 256.0))
    }

    fn test_map_viewport() -> MapViewport {
        MapViewport::new(lon_lat(0.0, 0.0), 2.0)
    }

    #[test]
    fn map_surface_is_view_and_module() {
        let surface = MapSurface::new(
            "map",
            MapDocument::new("Map"),
            MapViewport::new(lon_lat(4.9041, 52.3676), 12.0),
        );
        assert_view(&surface);
        assert_module(&surface);
    }

    #[test]
    fn f64_projection_round_trips() {
        let pos = lon_lat(4.904_138_900_123, 52.367_573_400_456);
        let world = geo_to_world(pos, 12.5);
        let roundtrip = world_to_geo(world, 12.5);
        assert!((roundtrip.lon - pos.lon).abs() < 1e-9);
        assert!((roundtrip.lat - pos.lat).abs() < 1e-9);
    }

    #[test]
    fn annotations_keep_exact_positions() {
        let pos = lon_lat(1.123_456_789, 2.987_654_321);
        let mut doc = MapDocument::new("Map");
        doc.add(MapPoint::new("point", pos));
        doc.add(MapIcon::svg("svg", pos, DEFAULT_SVG_MARKER));
        assert_eq!(doc.annotations.len(), 2);
        assert!(matches!(&doc.annotations[0], MapAnnotation::Point(p) if p.position == pos));
        assert!(matches!(&doc.annotations[1], MapAnnotation::Icon(i) if i.position == pos));
    }

    #[test]
    fn fluent_icon_annotations_lower_to_mara_text_family_paint_cmd() {
        let cmd = map_icon_paint_cmd(
            egui::pos2(10.0, 20.0),
            &MapIconGlyph::Fluent("search".to_owned()),
            18.0,
            MaraColor32::WHITE,
        )
        .expect("bundled fluent icon should lower to a paint command");

        let PaintCmd::TextWithFamily {
            pos,
            anchor,
            text,
            size,
            color,
            family,
        } = cmd
        else {
            panic!("fluent map icons should lower to named-family text commands");
        };
        assert_eq!(pos, egui::pos2(10.0, 20.0).into());
        assert_eq!(anchor, MaraAlign2::CENTER_CENTER);
        assert!(!text.is_empty());
        assert_eq!(size, 18.0);
        assert_eq!(color, MaraColor32::WHITE);
        assert!(matches!(family, TextFamily::Named(name) if !name.is_empty()));
    }

    #[test]
    fn text_icon_annotations_lower_to_mara_text_paint_cmd() {
        let cmd = map_icon_paint_cmd(
            egui::pos2(3.0, 4.0),
            &MapIconGlyph::Text("A1".to_owned()),
            14.0,
            MaraColor32::BLACK,
        )
        .expect("text icon should lower to a paint command");

        let PaintCmd::Text {
            pos,
            anchor,
            text,
            size,
            color,
            mono,
        } = cmd
        else {
            panic!("text map icons should lower to Mara text commands");
        };
        assert_eq!(pos, egui::pos2(3.0, 4.0).into());
        assert_eq!(anchor, MaraAlign2::CENTER_CENTER);
        assert_eq!(text, "A1");
        assert_eq!(size, 14.0);
        assert_eq!(color, MaraColor32::BLACK);
        assert!(!mono);
    }

    #[test]
    fn svg_icon_annotations_lower_to_mara_svg_paint_cmd() {
        let cmd = map_icon_paint_cmd(
            egui::pos2(20.0, 30.0),
            &MapIconGlyph::Svg(DEFAULT_SVG_MARKER.to_owned()),
            16.0,
            MaraColor32::WHITE,
        )
        .expect("svg icon should lower to a paint command");

        let PaintCmd::Svg { svg, rect, tint } = cmd else {
            panic!("svg map icons should lower to Mara svg commands");
        };
        assert_eq!(svg, DEFAULT_SVG_MARKER);
        assert_eq!(
            rect,
            MaraRect::from_center_size(egui::pos2(20.0, 30.0).into(), MaraVec2::new(16.0, 16.0))
        );
        assert_eq!(tint, MaraColor32::WHITE);
    }

    #[test]
    fn point_annotations_lower_to_mara_circle_paint_cmds() {
        let annotation = MapAnnotation::from(MapPoint {
            id: MapAnnotationId::from_u128(1),
            position: lon_lat(0.0, 0.0),
            label: None,
            color: MaraColor32::from_rgb(10, 20, 30),
        });

        let cmds =
            map_annotation_paint_cmds(test_map_rect(), test_map_viewport(), &annotation, true);
        assert_eq!(cmds.len(), 2);
        assert!(matches!(
            &cmds[0],
            PaintCmd::CircleFilled {
                radius: 8.0,
                fill,
                ..
            } if *fill == MaraColor32::from_rgb(10, 20, 30)
        ));
        assert!(matches!(
            &cmds[1],
            PaintCmd::CircleStroke {
                radius: 12.0,
                stroke,
                ..
            } if stroke.width == 2.0
        ));
    }

    #[test]
    fn line_annotations_lower_to_mara_polyline_paint_cmds() {
        let annotation = MapAnnotation::from(MapLine {
            id: MapAnnotationId::from_u128(2),
            points: vec![lon_lat(-1.0, 0.0), lon_lat(1.0, 0.0)],
            label: None,
            color: MaraColor32::from_rgb(80, 90, 100),
        });

        let cmds =
            map_annotation_paint_cmds(test_map_rect(), test_map_viewport(), &annotation, false);
        assert_eq!(cmds.len(), 1);
        assert!(matches!(
            &cmds[0],
            PaintCmd::Polyline { points, stroke }
                if points.len() == 2
                    && stroke.width == 2.5
                    && stroke.color == MaraColor32::from_rgb(80, 90, 100)
        ));
    }

    #[test]
    fn polygon_annotations_lower_to_mara_polygon_paint_cmds() {
        let annotation = MapAnnotation::from(MapPolygon {
            id: MapAnnotationId::from_u128(3),
            points: vec![
                lon_lat(-1.0, -1.0),
                lon_lat(1.0, -1.0),
                lon_lat(1.0, 1.0),
                lon_lat(-1.0, 1.0),
            ],
            label: None,
            fill: MaraColor32::from_rgba_unmultiplied(10, 20, 30, 40),
            stroke: MaraStroke::new(1.5, MaraColor32::from_rgb(50, 60, 70)),
        });

        let cmds =
            map_annotation_paint_cmds(test_map_rect(), test_map_viewport(), &annotation, true);
        assert_eq!(cmds.len(), 3);
        assert!(matches!(
            &cmds[0],
            PaintCmd::Polygon {
                points,
                fill,
                stroke
            } if points.len() >= 3
                && *fill == MaraColor32::from_rgba_unmultiplied(10, 20, 30, 40)
                && *stroke == MaraStroke::new(1.5, MaraColor32::from_rgb(50, 60, 70))
        ));
        assert!(matches!(
            &cmds[1],
            PaintCmd::Polygon { fill, stroke, .. }
                if *fill == MaraColor32::TRANSPARENT && stroke.width == 4.5
        ));
        assert!(matches!(
            &cmds[2],
            PaintCmd::Polygon { fill, stroke, .. }
                if *fill == MaraColor32::TRANSPARENT && stroke.width == 2.3
        ));
    }

    #[test]
    fn svg_icon_annotations_lower_svg_and_selection_chrome_to_mara_paint_cmds() {
        let annotation = MapAnnotation::from(MapIcon {
            id: MapAnnotationId::from_u128(4),
            position: lon_lat(0.0, 0.0),
            glyph: MapIconGlyph::Svg(DEFAULT_SVG_MARKER.to_owned()),
            label: None,
            size: 16.0,
            color: MaraColor32::WHITE,
        });

        let cmds =
            map_annotation_paint_cmds(test_map_rect(), test_map_viewport(), &annotation, true);
        assert_eq!(cmds.len(), 2);
        assert!(matches!(
            &cmds[0],
            PaintCmd::Svg { svg, tint, .. }
                if svg == DEFAULT_SVG_MARKER && *tint == MaraColor32::WHITE
        ));
        assert!(matches!(
            &cmds[1],
            PaintCmd::CircleStroke { radius, stroke, .. }
                if (*radius - 11.2).abs() < f32::EPSILON
                    && *stroke == MaraStroke::new(2.0, MaraColor32::WHITE)
        ));
    }

    #[test]
    fn selected_polygon_features_lower_to_mara_paint_cmds() {
        let feature = MapFeatureInfo {
            layer: "landuse".to_owned(),
            class: "park".to_owned(),
            geometry: MapFeatureGeometry::Polygon,
            name: None,
            properties: Vec::new(),
            paths: vec![vec![
                lon_lat(-1.0, -1.0),
                lon_lat(1.0, -1.0),
                lon_lat(1.0, 1.0),
                lon_lat(-1.0, 1.0),
            ]],
        };

        let cmds = selected_feature_paint_cmds(test_map_rect(), test_map_viewport(), &feature);
        assert_eq!(cmds.len(), 3);
        assert!(matches!(
            &cmds[0],
            PaintCmd::Polygon {
                points,
                fill,
                stroke
            } if points.len() >= 3
                && fill.a() == 54
                && *stroke == MaraStroke::NONE
        ));
        assert!(matches!(
            &cmds[1],
            PaintCmd::Polygon { fill, stroke, .. }
                if *fill == MaraColor32::TRANSPARENT && stroke.width == 7.0
        ));
        assert!(matches!(
            &cmds[2],
            PaintCmd::Polygon { fill, stroke, .. }
                if *fill == MaraColor32::TRANSPARENT && stroke.width == 2.4
        ));
    }

    #[test]
    fn selected_line_and_point_features_lower_to_mara_paint_cmds() {
        let line_feature = MapFeatureInfo {
            layer: "road".to_owned(),
            class: "primary".to_owned(),
            geometry: MapFeatureGeometry::Line,
            name: None,
            properties: Vec::new(),
            paths: vec![vec![lon_lat(-1.0, 0.0), lon_lat(1.0, 0.0)]],
        };
        let point_feature = MapFeatureInfo {
            layer: "poi".to_owned(),
            class: "label".to_owned(),
            geometry: MapFeatureGeometry::Point,
            name: None,
            properties: Vec::new(),
            paths: vec![vec![lon_lat(0.0, 0.0)]],
        };

        let line_cmds =
            selected_feature_paint_cmds(test_map_rect(), test_map_viewport(), &line_feature);
        assert_eq!(line_cmds.len(), 2);
        assert!(matches!(
            &line_cmds[0],
            PaintCmd::Polyline { points, stroke } if points.len() == 2 && stroke.width == 7.0
        ));
        assert!(matches!(
            &line_cmds[1],
            PaintCmd::Polyline { points, stroke } if points.len() == 2 && stroke.width == 2.4
        ));

        let point_cmds =
            selected_feature_paint_cmds(test_map_rect(), test_map_viewport(), &point_feature);
        assert_eq!(point_cmds.len(), 2);
        assert!(matches!(
            &point_cmds[0],
            PaintCmd::CircleFilled { radius: 8.0, fill, .. } if fill.a() == 54
        ));
        assert!(matches!(
            &point_cmds[1],
            PaintCmd::CircleStroke { radius: 10.0, stroke, .. } if stroke.width == 2.4
        ));
    }

    #[test]
    fn draft_geometry_lowers_to_mara_circle_and_polyline_commands() {
        let interaction = MapInteraction {
            draft: vec![lon_lat(-1.0, 0.0), lon_lat(1.0, 0.0)],
            ..Default::default()
        };

        let cmds = draft_paint_cmds(test_map_rect(), test_map_viewport(), &interaction);
        assert_eq!(cmds.len(), 3);
        assert!(matches!(
            &cmds[0],
            PaintCmd::CircleFilled { radius: 4.0, .. }
        ));
        assert!(matches!(
            &cmds[1],
            PaintCmd::CircleFilled { radius: 4.0, .. }
        ));
        assert!(matches!(
            &cmds[2],
            PaintCmd::Polyline { points, stroke } if points.len() == 2 && stroke.width == 2.0
        ));
    }

    #[test]
    fn polygon_hit_test_uses_edges_only() {
        let points = vec![
            egui::pos2(0.0, 0.0),
            egui::pos2(100.0, 0.0),
            egui::pos2(100.0, 100.0),
            egui::pos2(0.0, 100.0),
        ];
        assert!(polygon_edge_hit(egui::pos2(50.0, 3.0), &points, 8.0));
        assert!(!polygon_edge_hit(egui::pos2(50.0, 50.0), &points, 8.0));
    }

    #[test]
    fn document_removes_annotation_by_uuid_id() {
        let pos = lon_lat(1.0, 2.0);
        let id = MapAnnotationId::new_uuid();
        let mut doc = MapDocument::new("Map");
        doc.add(MapPoint {
            id,
            position: pos,
            label: None,
            color: MaraColor32::WHITE,
        });
        assert_eq!(
            doc.get(id).map(MapAnnotation::kind),
            Some(MapAnnotationKind::Point)
        );
        assert!(doc.remove(id).is_some());
        assert!(doc.get(id).is_none());
    }

    #[test]
    fn triangulates_concave_c_shape_without_filling_notch() {
        let points = vec![
            MaraPos2::new(0.0, 0.0),
            MaraPos2::new(100.0, 0.0),
            MaraPos2::new(100.0, 25.0),
            MaraPos2::new(30.0, 25.0),
            MaraPos2::new(30.0, 75.0),
            MaraPos2::new(100.0, 75.0),
            MaraPos2::new(100.0, 100.0),
            MaraPos2::new(0.0, 100.0),
        ];
        let triangles = triangulate_polygon(&points);
        assert_eq!(triangles.len(), points.len() - 2);
        let filled_area = triangles
            .iter()
            .map(|[a, b, c]| cross(*a, *b, *c).abs() * 0.5)
            .sum::<f32>();
        assert!((filled_area - signed_area(&points).abs()).abs() < 0.1);

        let notch = MaraPos2::new(65.0, 50.0);
        assert!(
            !triangles
                .iter()
                .any(|[a, b, c]| point_in_triangle(notch, *a, *b, *c))
        );
    }

    #[test]
    fn polygon_fill_normalizes_closed_mvt_style_rings() {
        let closed_ring = vec![
            egui::pos2(0.0, 0.0),
            egui::pos2(100.0, 0.0),
            egui::pos2(100.0, 100.0),
            egui::pos2(0.0, 100.0),
            egui::pos2(0.0, 0.0),
        ];
        let points: Vec<MaraPos2> = normalized_polygon_points(&closed_ring)
            .into_iter()
            .map(Into::into)
            .collect();
        assert_eq!(points.len(), 4);

        let triangles = triangulate_polygon(&points);
        assert_eq!(triangles.len(), 2);
        let filled_area = triangles
            .iter()
            .map(|[a, b, c]| cross(*a, *b, *c).abs() * 0.5)
            .sum::<f32>();
        assert!((filled_area - 10_000.0).abs() < 0.1);
    }

    #[test]
    fn right_click_cancel_steps_draft_then_tool() {
        let mut interaction = MapInteraction::default();
        interaction.set_tool(MapTool::Line);
        interaction.draft.push(lon_lat(1.0, 1.0));
        interaction.draft.push(lon_lat(2.0, 2.0));

        cancel_tool_step(&mut interaction);
        assert_eq!(interaction.tool, MapTool::Line);
        assert_eq!(interaction.draft_len(), 1);

        cancel_tool_step(&mut interaction);
        assert_eq!(interaction.tool, MapTool::Line);
        assert_eq!(interaction.draft_len(), 0);

        cancel_tool_step(&mut interaction);
        assert_eq!(interaction.tool, MapTool::Select);
        assert_eq!(interaction.draft_len(), 0);
    }
}
