use std::collections::HashMap;
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use super::{MapViewport, TILE_SIZE, geo_to_world, paint_polygon};

const OPENFREEMAP_TILE_URL: &str = "https://tiles.openfreemap.org/planet/latest";
const MAX_SOURCE_ZOOM: f64 = 14.0;
const MAP_DESATURATION: f32 = 0.46;
const MAP_ACCENT_TINT_SCALE: f32 = 1.08;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TileKey {
    z: u8,
    x: u32,
    y: u32,
}

enum TileEntry {
    Loading,
    Ready(DecodedVectorTile),
    Failed,
}

pub(crate) struct VectorTileCache {
    tiles: HashMap<TileKey, TileEntry>,
    tx: mpsc::Sender<(TileKey, Result<DecodedVectorTile, String>)>,
    rx: Mutex<mpsc::Receiver<(TileKey, Result<DecodedVectorTile, String>)>>,
}

impl Default for VectorTileCache {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tiles: HashMap::new(),
            tx,
            rx: Mutex::new(rx),
        }
    }
}

pub(crate) fn paint_vector_basemap(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    viewport: MapViewport,
    cache: &mut VectorTileCache,
    fast_mode: bool,
) {
    cache.poll_finished();
    let palette = MapPalette::current();
    ui.painter().rect_filled(rect, 0.0, palette.background);

    let z = viewport.zoom.floor().clamp(0.0, MAX_SOURCE_ZOOM) as u8;
    let zf = f64::from(z);
    let scale = 2.0_f64.powf(viewport.zoom - zf);
    let center_world = geo_to_world(viewport.center, zf);
    let top_left_world = (
        center_world.0 - f64::from(rect.width()) / (2.0 * scale),
        center_world.1 - f64::from(rect.height()) / (2.0 * scale),
    );
    let bottom_right_world = (
        center_world.0 + f64::from(rect.width()) / (2.0 * scale),
        center_world.1 + f64::from(rect.height()) / (2.0 * scale),
    );
    let min_x = (top_left_world.0 / TILE_SIZE).floor() as i64;
    let max_x = (bottom_right_world.0 / TILE_SIZE).ceil() as i64;
    let min_y = (top_left_world.1 / TILE_SIZE).floor() as i64;
    let max_y = (bottom_right_world.1 / TILE_SIZE).ceil() as i64;
    let tile_count = 1_i64 << u32::from(z);

    let mut has_loading = false;
    let mut visible_tiles = Vec::new();
    for y in min_y..=max_y {
        if !(0..tile_count).contains(&y) {
            continue;
        }
        for x in min_x..=max_x {
            let wrapped_x = x.rem_euclid(tile_count);
            let key = TileKey {
                z,
                x: wrapped_x as u32,
                y: y as u32,
            };
            cache.request(key);
            visible_tiles.push(key);
            if matches!(cache.tiles.get(&key), Some(TileEntry::Loading) | None) {
                has_loading = true;
            }
        }
    }

    let painter = ui.painter();
    let mut labels = LabelState::default();
    let passes: &[PaintPass] = if fast_mode {
        &PaintPass::FAST
    } else {
        &PaintPass::ALL
    };
    for &pass in passes {
        for key in &visible_tiles {
            let Some(TileEntry::Ready(tile)) = cache.tiles.get(key) else {
                continue;
            };
            paint_tile_pass(
                painter,
                rect,
                viewport,
                *key,
                tile,
                pass,
                &palette,
                &mut labels,
            );
        }
    }

    if has_loading {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
    if fast_mode {
        ui.ctx().request_repaint_after(Duration::from_millis(80));
    }
}

impl VectorTileCache {
    fn poll_finished(&mut self) {
        let Ok(rx) = self.rx.lock() else {
            return;
        };
        while let Ok((key, result)) = rx.try_recv() {
            let entry = match result {
                Ok(tile) => TileEntry::Ready(tile),
                Err(_) => TileEntry::Failed,
            };
            self.tiles.insert(key, entry);
        }
    }

    fn request(&mut self, key: TileKey) {
        if self.tiles.contains_key(&key) {
            return;
        }
        self.tiles.insert(key, TileEntry::Loading);
        request_tile(key, self.tx.clone());
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn request_tile(key: TileKey, tx: mpsc::Sender<(TileKey, Result<DecodedVectorTile, String>)>) {
    std::thread::spawn(move || {
        let result = fetch_tile(key).and_then(|bytes| decode_vector_tile(&bytes));
        let _ = tx.send((key, result));
    });
}

#[cfg(target_arch = "wasm32")]
fn request_tile(key: TileKey, tx: mpsc::Sender<(TileKey, Result<DecodedVectorTile, String>)>) {
    let url = format!("{OPENFREEMAP_TILE_URL}/{}/{}/{}.pbf", key.z, key.x, key.y);
    let mut request = ehttp::Request::get(&url);
    request
        .headers
        .insert("Accept", "application/vnd.mapbox-vector-tile");
    ehttp::fetch(request, move |response| {
        let result = response
            .map_err(|err| format!("failed to fetch {url}: {err}"))
            .and_then(|response| {
                if response.ok {
                    decode_vector_tile(&response.bytes)
                } else {
                    Err(format!(
                        "failed to fetch {url}: HTTP {} {}",
                        response.status, response.status_text
                    ))
                }
            });
        let _ = tx.send((key, result));
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch_tile(key: TileKey) -> Result<Vec<u8>, String> {
    use std::io::Read;

    let url = format!("{OPENFREEMAP_TILE_URL}/{}/{}/{}.pbf", key.z, key.x, key.y);
    let response = ureq::get(&url)
        .set("User-Agent", "mara_map/0.0.2")
        .call()
        .map_err(|err| format!("failed to fetch {url}: {err}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|err| format!("failed to read {url}: {err}"))?;
    Ok(bytes)
}

fn paint_tile_pass(
    painter: &egui::Painter,
    rect: egui::Rect,
    viewport: MapViewport,
    key: TileKey,
    tile: &DecodedVectorTile,
    pass: PaintPass,
    palette: &MapPalette,
    labels: &mut LabelState,
) {
    for layer in &tile.layers {
        for feature in &layer.features {
            match pass {
                PaintPass::LandFill => paint_area_fill(
                    painter,
                    rect,
                    viewport,
                    key,
                    layer,
                    feature,
                    land_fill_color(&layer.name, feature, palette),
                ),
                PaintPass::WaterFill => paint_area_fill(
                    painter,
                    rect,
                    viewport,
                    key,
                    layer,
                    feature,
                    water_fill_color(&layer.name, feature, palette),
                ),
                PaintPass::BuildingFill => {
                    paint_area_fill(
                        painter,
                        rect,
                        viewport,
                        key,
                        layer,
                        feature,
                        building_fill_color(&layer.name, feature, palette),
                    );
                    if layer.name == "building" {
                        paint_feature_lines(
                            painter,
                            rect,
                            viewport,
                            key,
                            layer,
                            feature,
                            egui::Stroke::new(0.6, palette.building_outline),
                        );
                    }
                }
                PaintPass::RoadCasing => {
                    if let Some(stroke) =
                        transportation_casing(&layer.name, feature, viewport.zoom, palette)
                    {
                        paint_feature_lines(painter, rect, viewport, key, layer, feature, stroke);
                    }
                }
                PaintPass::RoadFill => {
                    if let Some(stroke) =
                        transportation_stroke(&layer.name, feature, viewport.zoom, palette)
                    {
                        paint_feature_lines(painter, rect, viewport, key, layer, feature, stroke);
                    }
                }
                PaintPass::LineOverlay => {
                    if let Some(stroke) =
                        overlay_line_stroke(&layer.name, feature, viewport.zoom, palette)
                    {
                        paint_feature_lines(painter, rect, viewport, key, layer, feature, stroke);
                    }
                }
                PaintPass::PointSymbol => {
                    paint_point_symbol(painter, rect, viewport, key, layer, feature, palette);
                }
                PaintPass::Label => {
                    paint_label(
                        painter, rect, viewport, key, layer, feature, palette, labels,
                    );
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaintPass {
    LandFill,
    WaterFill,
    BuildingFill,
    RoadCasing,
    RoadFill,
    LineOverlay,
    PointSymbol,
    Label,
}

impl PaintPass {
    const ALL: [Self; 8] = [
        Self::LandFill,
        Self::WaterFill,
        Self::BuildingFill,
        Self::RoadCasing,
        Self::RoadFill,
        Self::LineOverlay,
        Self::PointSymbol,
        Self::Label,
    ];

    const FAST: [Self; 5] = [
        Self::LandFill,
        Self::WaterFill,
        Self::BuildingFill,
        Self::RoadFill,
        Self::LineOverlay,
    ];
}

#[derive(Default)]
struct LabelState {
    occupied: Vec<egui::Rect>,
}

#[derive(Clone, Copy)]
struct MapPalette {
    background: egui::Color32,
    land_default: egui::Color32,
    forest: egui::Color32,
    grass: egui::Color32,
    scrub: egui::Color32,
    sand: egui::Color32,
    wetland: egui::Color32,
    ice: egui::Color32,
    residential: egui::Color32,
    commercial: egui::Color32,
    industrial: egui::Color32,
    education: egui::Color32,
    hospital: egui::Color32,
    cemetery: egui::Color32,
    farmland: egui::Color32,
    park: egui::Color32,
    aeroway_fill: egui::Color32,
    water: egui::Color32,
    water_line: egui::Color32,
    building: egui::Color32,
    building_outline: egui::Color32,
    boundary: egui::Color32,
    aeroway_line: egui::Color32,
    poi: egui::Color32,
    label: egui::Color32,
    label_secondary: egui::Color32,
    label_halo: egui::Color32,
    water_label: egui::Color32,
    road_default: egui::Color32,
    road_minor: egui::Color32,
    road_medium: egui::Color32,
    road_major: egui::Color32,
    motorway: egui::Color32,
    trunk: egui::Color32,
    rail: egui::Color32,
    road_casing: egui::Color32,
    major_casing: egui::Color32,
    rail_casing: egui::Color32,
}

impl MapPalette {
    fn current() -> Self {
        let theme = mara_core::style::theme();
        let accent = mara_core::style::active_accent();
        if theme.is_light {
            Self {
                background: tint(rgb(0xf2, 0xef, 0xea), accent, 0.045),
                land_default: tint(rgba(0xc9, 0xda, 0xb2, 150), accent, 0.055),
                forest: tint(rgb(0xb7, 0xd2, 0xa3), accent, 0.055),
                grass: tint(rgb(0xc9, 0xe1, 0xb5), accent, 0.055),
                scrub: tint(rgb(0xd2, 0xd9, 0xb0), accent, 0.055),
                sand: tint(rgb(0xe4, 0xdc, 0xc5), accent, 0.035),
                wetland: tint(rgb(0xb9, 0xd3, 0xc1), accent, 0.06),
                ice: tint(rgb(0xe6, 0xf0, 0xf2), accent, 0.055),
                residential: tint(rgb(0xee, 0xea, 0xe2), accent, 0.045),
                commercial: tint(rgb(0xe9, 0xe2, 0xdc), accent, 0.04),
                industrial: tint(rgb(0xe2, 0xdd, 0xd5), accent, 0.055),
                education: tint(rgb(0xe8, 0xe3, 0xd2), accent, 0.04),
                hospital: tint(rgb(0xe9, 0xdc, 0xdc), accent, 0.04),
                cemetery: tint(rgb(0xc9, 0xd8, 0xbd), accent, 0.055),
                farmland: tint(rgb(0xdb, 0xd6, 0xc2), accent, 0.035),
                park: tint(rgb(0xb9, 0xd8, 0xa7), accent, 0.055),
                aeroway_fill: tint(rgb(0xdd, 0xd5, 0xc9), accent, 0.05),
                water: tint(rgb(0xa7, 0xce, 0xde), accent, 0.06),
                water_line: tint(rgb(0x86, 0xbd, 0xcf), accent, 0.07),
                building: tint(rgb(0xd8, 0xd2, 0xc8), accent, 0.06),
                building_outline: tint(rgb(0xbe, 0xb8, 0xae), accent, 0.07),
                boundary: tint(rgba(0x86, 0x7d, 0x74, 155), accent, 0.08),
                aeroway_line: tint(rgb(0xc4, 0xbd, 0xb3), accent, 0.06),
                poi: tint(rgba(0x8b, 0x79, 0x66, 190), accent, 0.12),
                label: tint(rgb(0x4c, 0x45, 0x3d), accent, 0.08),
                label_secondary: tint(rgb(0x6d, 0x62, 0x55), accent, 0.08),
                label_halo: tint(rgba(0xff, 0xfc, 0xf4, 220), accent, 0.025),
                water_label: tint(rgb(0x4b, 0x85, 0x9a), accent, 0.08),
                road_default: tint(rgb(0xf5, 0xf2, 0xec), accent, 0.025),
                road_minor: tint(rgb(0xe8, 0xe5, 0xdf), accent, 0.025),
                road_medium: tint(rgb(0xe8, 0xdc, 0xc4), accent, 0.025),
                road_major: tint(rgb(0xe7, 0xd3, 0xb3), accent, 0.025),
                motorway: tint(rgb(0xd9, 0xb6, 0x92), accent, 0.025),
                trunk: tint(rgb(0xdd, 0xc1, 0x9b), accent, 0.025),
                rail: tint(rgb(0xb4, 0xae, 0xa4), accent, 0.09),
                road_casing: tint(rgb(0xd0, 0xcb, 0xc3), accent, 0.075),
                major_casing: tint(rgb(0xc9, 0xc1, 0xb2), accent, 0.045),
                rail_casing: tint(rgba(0x8d, 0x85, 0x7b, 150), accent, 0.1),
            }
        } else {
            Self {
                background: tint(rgb(0x10, 0x13, 0x18), accent, 0.08),
                land_default: tint(rgba(0x25, 0x31, 0x26, 180), accent, 0.07),
                forest: tint(rgb(0x25, 0x3f, 0x2e), accent, 0.08),
                grass: tint(rgb(0x2f, 0x46, 0x32), accent, 0.08),
                scrub: tint(rgb(0x3b, 0x40, 0x2b), accent, 0.08),
                sand: tint(rgb(0x46, 0x41, 0x34), accent, 0.055),
                wetland: tint(rgb(0x23, 0x42, 0x3d), accent, 0.08),
                ice: tint(rgb(0x35, 0x45, 0x4e), accent, 0.08),
                residential: tint(rgb(0x1b, 0x1d, 0x22), accent, 0.075),
                commercial: tint(rgb(0x25, 0x23, 0x25), accent, 0.06),
                industrial: tint(rgb(0x24, 0x24, 0x28), accent, 0.075),
                education: tint(rgb(0x2b, 0x2a, 0x27), accent, 0.06),
                hospital: tint(rgb(0x2c, 0x25, 0x28), accent, 0.06),
                cemetery: tint(rgb(0x26, 0x36, 0x2b), accent, 0.08),
                farmland: tint(rgb(0x36, 0x32, 0x29), accent, 0.055),
                park: tint(rgb(0x27, 0x44, 0x2e), accent, 0.08),
                aeroway_fill: tint(rgb(0x2d, 0x2c, 0x30), accent, 0.075),
                water: tint(rgb(0x19, 0x35, 0x43), accent, 0.09),
                water_line: tint(rgb(0x2a, 0x67, 0x7a), accent, 0.1),
                building: tint(rgb(0x2b, 0x2d, 0x32), accent, 0.085),
                building_outline: tint(rgb(0x3a, 0x3f, 0x48), accent, 0.09),
                boundary: tint(rgba(0xa3, 0x9b, 0x90, 130), accent, 0.12),
                aeroway_line: tint(rgb(0x57, 0x58, 0x60), accent, 0.11),
                poi: tint(rgba(0xd4, 0xca, 0xb8, 185), accent, 0.16),
                label: tint(rgb(0xd8, 0xd2, 0xc7), accent, 0.09),
                label_secondary: tint(rgb(0xb6, 0xad, 0x9e), accent, 0.1),
                label_halo: tint(rgba(0x09, 0x0b, 0x10, 230), accent, 0.06),
                water_label: tint(rgb(0x8a, 0xc5, 0xd8), accent, 0.1),
                road_default: tint(rgb(0x43, 0x41, 0x3e), accent, 0.075),
                road_minor: tint(rgb(0x34, 0x33, 0x31), accent, 0.075),
                road_medium: tint(rgb(0x52, 0x4b, 0x3c), accent, 0.07),
                road_major: tint(rgb(0x60, 0x53, 0x42), accent, 0.065),
                motorway: tint(rgb(0x6d, 0x55, 0x42), accent, 0.065),
                trunk: tint(rgb(0x68, 0x58, 0x44), accent, 0.065),
                rail: tint(rgb(0x76, 0x73, 0x6e), accent, 0.12),
                road_casing: tint(rgb(0x25, 0x27, 0x2c), accent, 0.12),
                major_casing: tint(rgb(0x34, 0x31, 0x2d), accent, 0.09),
                rail_casing: tint(rgba(0xbc, 0xb6, 0xab, 120), accent, 0.14),
            }
        }
    }
}

fn paint_area_fill(
    painter: &egui::Painter,
    rect: egui::Rect,
    viewport: MapViewport,
    key: TileKey,
    layer: &DecodedLayer,
    feature: &DecodedFeature,
    fill: Option<egui::Color32>,
) {
    if feature.geometry_type != GeometryType::Polygon {
        return;
    }
    let Some(fill) = fill else {
        return;
    };
    for path in &feature.paths {
        let points = screen_points(path, layer.extent, key, rect, viewport);
        if points.len() >= 3 && path_intersects_rect(&points, rect.expand(64.0)) {
            paint_polygon(painter, &points, fill, egui::Stroke::NONE);
        }
    }
}

fn paint_feature_lines(
    painter: &egui::Painter,
    rect: egui::Rect,
    viewport: MapViewport,
    key: TileKey,
    layer: &DecodedLayer,
    feature: &DecodedFeature,
    stroke: egui::Stroke,
) {
    if !matches!(
        feature.geometry_type,
        GeometryType::LineString | GeometryType::Polygon
    ) {
        return;
    }
    for path in &feature.paths {
        let points = screen_points(path, layer.extent, key, rect, viewport);
        if points.len() >= 2 && path_intersects_rect(&points, rect.expand(stroke.width + 16.0)) {
            painter.add(egui::Shape::line(points, stroke));
        }
    }
}

fn land_fill_color(
    layer: &str,
    feature: &DecodedFeature,
    palette: &MapPalette,
) -> Option<egui::Color32> {
    let class = feature.class();
    match layer {
        "landcover" => match class {
            "wood" | "forest" | "tree" => Some(palette.forest),
            "grass" | "grassland" | "meadow" => Some(palette.grass),
            "scrub" | "heath" => Some(palette.scrub),
            "sand" | "beach" | "dune" => Some(palette.sand),
            "wetland" => Some(palette.wetland),
            "glacier" | "ice" => Some(palette.ice),
            _ => Some(palette.land_default),
        },
        "landuse" => match class {
            "residential" | "suburb" | "neighbourhood" => Some(palette.residential),
            "commercial" | "retail" => Some(palette.commercial),
            "industrial" | "railway" => Some(palette.industrial),
            "school" | "university" | "college" => Some(palette.education),
            "hospital" => Some(palette.hospital),
            "cemetery" => Some(palette.cemetery),
            "farmland" | "farm" | "orchard" | "vineyard" => Some(palette.farmland),
            _ => Some(palette.residential),
        },
        "park" => Some(palette.park),
        "aeroway" if feature.geometry_type == GeometryType::Polygon => Some(palette.aeroway_fill),
        _ => None,
    }
}

fn water_fill_color(
    layer: &str,
    _feature: &DecodedFeature,
    palette: &MapPalette,
) -> Option<egui::Color32> {
    match layer {
        "water" => Some(palette.water),
        _ => None,
    }
}

fn building_fill_color(
    layer: &str,
    _feature: &DecodedFeature,
    palette: &MapPalette,
) -> Option<egui::Color32> {
    (layer == "building").then_some(palette.building)
}

fn transportation_casing(
    layer: &str,
    feature: &DecodedFeature,
    zoom: f64,
    palette: &MapPalette,
) -> Option<egui::Stroke> {
    (layer == "transportation").then(|| {
        egui::Stroke::new(
            road_width(feature, zoom) + road_casing_width(feature),
            road_casing_color(feature, palette),
        )
    })
}

fn transportation_stroke(
    layer: &str,
    feature: &DecodedFeature,
    zoom: f64,
    palette: &MapPalette,
) -> Option<egui::Stroke> {
    (layer == "transportation")
        .then(|| egui::Stroke::new(road_width(feature, zoom), road_fill_color(feature, palette)))
}

fn overlay_line_stroke(
    layer: &str,
    feature: &DecodedFeature,
    zoom: f64,
    palette: &MapPalette,
) -> Option<egui::Stroke> {
    let class = feature.class();
    match layer {
        "waterway" => Some(egui::Stroke::new(
            match class {
                "river" => 2.6,
                "canal" => 2.0,
                "stream" | "ditch" | "drain" => 1.2,
                _ => 1.6,
            },
            palette.water_line,
        )),
        "boundary" => Some(egui::Stroke::new(
            if feature.prop_i64("admin_level").unwrap_or(99) <= 4 {
                1.4
            } else {
                0.8
            },
            palette.boundary,
        )),
        "aeroway" if feature.geometry_type == GeometryType::LineString => Some(egui::Stroke::new(
            (zoom as f32 - 9.0).clamp(1.0, 4.5),
            palette.aeroway_line,
        )),
        _ => None,
    }
}

fn road_width(feature: &DecodedFeature, zoom: f64) -> f32 {
    let class = feature.class();
    let base = match class {
        "motorway" => 6.4,
        "trunk" => 5.8,
        "primary" => 5.0,
        "secondary" => 4.3,
        "tertiary" => 3.7,
        "minor" | "street" => 2.6,
        "service" | "track" => 1.7,
        "path" | "pedestrian" | "footway" | "cycleway" | "bridleway" | "steps" => 1.2,
        "rail" | "transit" | "light_rail" | "subway" => 1.4,
        "ferry" => 1.2,
        _ => 2.2,
    };
    let zoom_scale = ((zoom - 9.0) / 6.0).clamp(0.55, 1.55) as f32;
    base * zoom_scale
}

fn road_casing_width(feature: &DecodedFeature) -> f32 {
    match feature.class() {
        "motorway" | "trunk" | "primary" => 2.4,
        "secondary" | "tertiary" => 1.8,
        _ => 1.2,
    }
}

fn road_casing_color(feature: &DecodedFeature, palette: &MapPalette) -> egui::Color32 {
    match feature.class() {
        "motorway" | "trunk" | "primary" | "secondary" | "tertiary" => palette.major_casing,
        "rail" | "transit" | "light_rail" | "subway" => palette.rail_casing,
        _ => palette.road_casing,
    }
}

fn road_fill_color(feature: &DecodedFeature, palette: &MapPalette) -> egui::Color32 {
    match feature.class() {
        "motorway" => palette.motorway,
        "trunk" => palette.trunk,
        "primary" => palette.road_major,
        "secondary" | "tertiary" => palette.road_medium,
        "rail" | "transit" | "light_rail" | "subway" => palette.rail,
        "path" | "pedestrian" | "footway" | "cycleway" | "bridleway" | "steps" => {
            palette.road_minor
        }
        _ => palette.road_default,
    }
}

fn paint_point_symbol(
    painter: &egui::Painter,
    rect: egui::Rect,
    viewport: MapViewport,
    key: TileKey,
    layer: &DecodedLayer,
    feature: &DecodedFeature,
    palette: &MapPalette,
) {
    if feature.geometry_type != GeometryType::Point || !matches!(layer.name.as_str(), "poi") {
        return;
    }
    let Some(point) = feature.paths.first().and_then(|path| path.first()) else {
        return;
    };
    let pos = screen_point(*point, layer.extent, key, rect, viewport);
    painter.circle_filled(pos, 2.3, palette.poi);
}

fn paint_label(
    painter: &egui::Painter,
    rect: egui::Rect,
    viewport: MapViewport,
    key: TileKey,
    layer: &DecodedLayer,
    feature: &DecodedFeature,
    palette: &MapPalette,
    labels: &mut LabelState,
) {
    let Some(text) = feature.label_text() else {
        return;
    };
    let Some(pos) = label_position(feature, layer.extent, key, rect, viewport) else {
        return;
    };
    let Some(style) = label_style(&layer.name, feature, viewport.zoom, palette) else {
        return;
    };
    let galley = painter.layout_no_wrap(text.to_owned(), style.font.clone(), style.color);
    let bounds = egui::Rect::from_center_size(pos, galley.size() + egui::vec2(8.0, 4.0));
    if !rect.intersects(bounds) || labels.occupied.iter().any(|taken| taken.intersects(bounds)) {
        return;
    }
    labels.occupied.push(bounds);
    painter.text(
        pos + egui::vec2(1.0, 1.0),
        egui::Align2::CENTER_CENTER,
        text,
        style.font.clone(),
        style.halo,
    );
    painter.text(
        pos,
        egui::Align2::CENTER_CENTER,
        text,
        style.font,
        style.color,
    );
}

struct LabelStyle {
    font: egui::FontId,
    color: egui::Color32,
    halo: egui::Color32,
}

fn label_style(
    layer: &str,
    feature: &DecodedFeature,
    zoom: f64,
    palette: &MapPalette,
) -> Option<LabelStyle> {
    let class = feature.class();
    let (size, color) = match layer {
        "place" => {
            let size = match class {
                "city" => 17.0,
                "town" => 15.0,
                "village" | "hamlet" => 13.0,
                "suburb" | "neighbourhood" | "quarter" => 11.5,
                _ => 11.0,
            };
            (size + ((zoom - 10.0).max(0.0) * 0.35) as f32, palette.label)
        }
        "transportation_name" if zoom >= 12.0 => {
            let size = match class {
                "motorway" | "trunk" | "primary" => 11.5,
                "secondary" | "tertiary" => 10.5,
                _ => 9.5,
            };
            (size, palette.label_secondary)
        }
        "water_name" | "waterway_name" => (11.0, palette.water_label),
        "poi" if zoom >= 15.0 => (10.0, palette.label_secondary),
        "mountain_peak" if zoom >= 11.0 => (10.5, palette.label_secondary),
        _ => return None,
    };
    Some(LabelStyle {
        font: egui::FontId::proportional(size),
        color,
        halo: palette.label_halo,
    })
}

fn label_position(
    feature: &DecodedFeature,
    extent: u32,
    key: TileKey,
    rect: egui::Rect,
    viewport: MapViewport,
) -> Option<egui::Pos2> {
    match feature.geometry_type {
        GeometryType::Point => feature
            .paths
            .first()
            .and_then(|path| path.first())
            .map(|point| screen_point(*point, extent, key, rect, viewport)),
        GeometryType::LineString => feature
            .paths
            .iter()
            .max_by(|a, b| {
                path_screen_len(a, extent, key, rect, viewport)
                    .total_cmp(&path_screen_len(b, extent, key, rect, viewport))
            })
            .and_then(|path| path_midpoint(path, extent, key, rect, viewport)),
        GeometryType::Polygon => {
            feature
                .paths
                .iter()
                .max_by_key(|path| path.len())
                .and_then(|path| {
                    let points = screen_points(path, extent, key, rect, viewport);
                    polygon_centroid(&points)
                })
        }
        GeometryType::Unknown => None,
    }
}

fn screen_point(
    point: TilePoint,
    extent: u32,
    key: TileKey,
    rect: egui::Rect,
    viewport: MapViewport,
) -> egui::Pos2 {
    let scale = 2.0_f64.powf(viewport.zoom - f64::from(key.z));
    let center_world = geo_to_world(viewport.center, f64::from(key.z));
    let extent = f64::from(extent.max(1));
    let world_x = f64::from(key.x) * TILE_SIZE + f64::from(point.x) / extent * TILE_SIZE;
    let world_y = f64::from(key.y) * TILE_SIZE + f64::from(point.y) / extent * TILE_SIZE;
    egui::pos2(
        rect.center().x + ((world_x - center_world.0) * scale) as f32,
        rect.center().y + ((world_y - center_world.1) * scale) as f32,
    )
}

fn path_screen_len(
    path: &[TilePoint],
    extent: u32,
    key: TileKey,
    rect: egui::Rect,
    viewport: MapViewport,
) -> f32 {
    let points = screen_points(path, extent, key, rect, viewport);
    points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

fn path_midpoint(
    path: &[TilePoint],
    extent: u32,
    key: TileKey,
    rect: egui::Rect,
    viewport: MapViewport,
) -> Option<egui::Pos2> {
    let points = screen_points(path, extent, key, rect, viewport);
    let total = points.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>();
    if total <= f32::EPSILON {
        return points.first().copied();
    }
    let mut walked = 0.0;
    for w in points.windows(2) {
        let segment = w[0].distance(w[1]);
        if walked + segment >= total * 0.5 {
            let t = (total * 0.5 - walked) / segment.max(f32::EPSILON);
            return Some(w[0].lerp(w[1], t));
        }
        walked += segment;
    }
    points.last().copied()
}

fn polygon_centroid(points: &[egui::Pos2]) -> Option<egui::Pos2> {
    if points.is_empty() {
        return None;
    }
    let mut sum = egui::Vec2::ZERO;
    for point in points {
        sum += point.to_vec2();
    }
    Some(egui::pos2(
        sum.x / points.len() as f32,
        sum.y / points.len() as f32,
    ))
}

fn rgb(r: u8, g: u8, b: u8) -> egui::Color32 {
    egui::Color32::from_rgb(r, g, b)
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn tint(base: egui::Color32, accent: egui::Color32, amount: f32) -> egui::Color32 {
    let muted = desaturate_color(base, MAP_DESATURATION);
    lerp_color(muted, accent, amount * MAP_ACCENT_TINT_SCALE)
}

fn desaturate_color(color: egui::Color32, amount: f32) -> egui::Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let luma = (0.2126 * f32::from(color.r())
        + 0.7152 * f32::from(color.g())
        + 0.0722 * f32::from(color.b()))
    .round()
    .clamp(0.0, 255.0) as u8;
    lerp_color(
        color,
        egui::Color32::from_rgba_unmultiplied(luma, luma, luma, color.a()),
        amount,
    )
}

fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let blend = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    egui::Color32::from_rgba_unmultiplied(
        blend(a.r(), b.r()),
        blend(a.g(), b.g()),
        blend(a.b(), b.b()),
        a.a(),
    )
}

fn screen_points(
    path: &[TilePoint],
    extent: u32,
    key: TileKey,
    rect: egui::Rect,
    viewport: MapViewport,
) -> Vec<egui::Pos2> {
    let scale = 2.0_f64.powf(viewport.zoom - f64::from(key.z));
    let center_world = geo_to_world(viewport.center, f64::from(key.z));
    let extent = f64::from(extent.max(1));
    path.iter()
        .map(|point| {
            let world_x = f64::from(key.x) * TILE_SIZE + f64::from(point.x) / extent * TILE_SIZE;
            let world_y = f64::from(key.y) * TILE_SIZE + f64::from(point.y) / extent * TILE_SIZE;
            egui::pos2(
                rect.center().x + ((world_x - center_world.0) * scale) as f32,
                rect.center().y + ((world_y - center_world.1) * scale) as f32,
            )
        })
        .collect()
}

fn path_intersects_rect(points: &[egui::Pos2], rect: egui::Rect) -> bool {
    let Some(mut bounds) = points
        .first()
        .map(|point| egui::Rect::from_min_max(*point, *point))
    else {
        return false;
    };
    for point in &points[1..] {
        bounds.min.x = bounds.min.x.min(point.x);
        bounds.min.y = bounds.min.y.min(point.y);
        bounds.max.x = bounds.max.x.max(point.x);
        bounds.max.y = bounds.max.y.max(point.y);
    }
    bounds.intersects(rect)
}

#[derive(Debug)]
struct DecodedVectorTile {
    layers: Vec<DecodedLayer>,
}

#[derive(Debug)]
struct DecodedLayer {
    name: String,
    extent: u32,
    features: Vec<DecodedFeature>,
}

#[derive(Debug)]
struct DecodedFeature {
    geometry_type: GeometryType,
    paths: Vec<Vec<TilePoint>>,
    properties: HashMap<String, FeatureValue>,
}

impl DecodedFeature {
    fn prop_str(&self, key: &str) -> Option<&str> {
        match self.properties.get(key) {
            Some(FeatureValue::String(value)) => Some(value),
            _ => None,
        }
    }

    fn prop_i64(&self, key: &str) -> Option<i64> {
        match self.properties.get(key) {
            Some(FeatureValue::I64(value)) => Some(*value),
            Some(FeatureValue::U64(value)) => i64::try_from(*value).ok(),
            Some(FeatureValue::F64(value)) => Some(*value as i64),
            _ => None,
        }
    }

    fn class(&self) -> &str {
        self.prop_str("class")
            .or_else(|| self.prop_str("subclass"))
            .unwrap_or("")
    }

    fn label_text(&self) -> Option<&str> {
        self.prop_str("name:latin")
            .or_else(|| self.prop_str("name:en"))
            .or_else(|| self.prop_str("name"))
    }
}

#[derive(Debug, Clone, PartialEq)]
enum FeatureValue {
    String(String),
    F64(f64),
    I64(i64),
    U64(u64),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeometryType {
    Unknown,
    Point,
    LineString,
    Polygon,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TilePoint {
    x: i32,
    y: i32,
}

fn decode_vector_tile(bytes: &[u8]) -> Result<DecodedVectorTile, String> {
    let mut reader = ProtoReader::new(bytes);
    let mut layers = Vec::new();
    while let Some(field) = reader.next_field()? {
        if field.number == 3
            && let FieldValue::Bytes(layer_bytes) = field.value
        {
            layers.push(decode_layer(layer_bytes)?);
        }
    }
    Ok(DecodedVectorTile { layers })
}

fn decode_layer(bytes: &[u8]) -> Result<DecodedLayer, String> {
    let mut reader = ProtoReader::new(bytes);
    let mut name = String::new();
    let mut extent = 4096;
    let mut raw_features = Vec::new();
    let mut keys = Vec::new();
    let mut values = Vec::new();

    while let Some(field) = reader.next_field()? {
        match (field.number, field.value) {
            (1, FieldValue::Bytes(value)) => {
                name = std::str::from_utf8(value)
                    .map_err(|err| format!("invalid MVT layer name: {err}"))?
                    .to_owned();
            }
            (2, FieldValue::Bytes(value)) => raw_features.push(decode_feature(value)?),
            (3, FieldValue::Bytes(value)) => {
                keys.push(
                    std::str::from_utf8(value)
                        .map_err(|err| format!("invalid MVT key: {err}"))?
                        .to_owned(),
                );
            }
            (4, FieldValue::Bytes(value)) => values.push(decode_value(value)?),
            (5, FieldValue::Varint(value)) => extent = value as u32,
            _ => {}
        }
    }

    let features = raw_features
        .into_iter()
        .map(|feature| feature.resolve_properties(&keys, &values))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(DecodedLayer {
        name,
        extent,
        features,
    })
}

#[derive(Debug)]
struct RawFeature {
    geometry_type: GeometryType,
    geometry: Vec<u32>,
    tags: Vec<u32>,
}

impl RawFeature {
    fn resolve_properties(
        self,
        keys: &[String],
        values: &[FeatureValue],
    ) -> Result<DecodedFeature, String> {
        let mut properties = HashMap::new();
        for pair in self.tags.chunks(2) {
            let [key_index, value_index] = pair else {
                continue;
            };
            let Some(key) = keys.get(*key_index as usize) else {
                continue;
            };
            let Some(value) = values.get(*value_index as usize) else {
                continue;
            };
            properties.insert(key.clone(), value.clone());
        }
        Ok(DecodedFeature {
            geometry_type: self.geometry_type,
            paths: decode_geometry(&self.geometry)?,
            properties,
        })
    }
}

fn decode_feature(bytes: &[u8]) -> Result<RawFeature, String> {
    let mut reader = ProtoReader::new(bytes);
    let mut geometry_type = GeometryType::Unknown;
    let mut geometry = Vec::new();
    let mut tags = Vec::new();

    while let Some(field) = reader.next_field()? {
        match (field.number, field.value) {
            (2, FieldValue::Bytes(value)) => tags.extend(read_packed_u32(value)?),
            (2, FieldValue::Varint(value)) => tags.push(value as u32),
            (3, FieldValue::Varint(value)) => {
                geometry_type = match value {
                    1 => GeometryType::Point,
                    2 => GeometryType::LineString,
                    3 => GeometryType::Polygon,
                    _ => GeometryType::Unknown,
                };
            }
            (4, FieldValue::Bytes(value)) => geometry.extend(read_packed_u32(value)?),
            (4, FieldValue::Varint(value)) => geometry.push(value as u32),
            _ => {}
        }
    }

    Ok(RawFeature {
        geometry_type,
        geometry,
        tags,
    })
}

fn decode_value(bytes: &[u8]) -> Result<FeatureValue, String> {
    let mut reader = ProtoReader::new(bytes);
    while let Some(field) = reader.next_field()? {
        match (field.number, field.value) {
            (1, FieldValue::Bytes(value)) => {
                return Ok(FeatureValue::String(
                    std::str::from_utf8(value)
                        .map_err(|err| format!("invalid MVT string value: {err}"))?
                        .to_owned(),
                ));
            }
            (2, FieldValue::Fixed32(value)) => return Ok(FeatureValue::F64(f64::from(value))),
            (3, FieldValue::Fixed64(value)) => return Ok(FeatureValue::F64(value)),
            (4, FieldValue::Varint(value)) => return Ok(FeatureValue::I64(value as i64)),
            (5, FieldValue::Varint(value)) => return Ok(FeatureValue::U64(value)),
            (6, FieldValue::Varint(value)) => {
                return Ok(FeatureValue::I64(i64::from(zig_zag_decode_64(value))));
            }
            (7, FieldValue::Varint(value)) => return Ok(FeatureValue::Bool(value != 0)),
            _ => {}
        }
    }
    Ok(FeatureValue::String(String::new()))
}

fn decode_geometry(geometry: &[u32]) -> Result<Vec<Vec<TilePoint>>, String> {
    let mut cursor = 0;
    let mut x = 0_i32;
    let mut y = 0_i32;
    let mut paths = Vec::new();
    let mut path = Vec::new();

    while cursor < geometry.len() {
        let command = geometry[cursor];
        cursor += 1;
        let command_id = command & 0x7;
        let count = command >> 3;

        match command_id {
            1 | 2 => {
                for _ in 0..count {
                    if cursor + 1 >= geometry.len() {
                        return Err("truncated MVT geometry command".to_owned());
                    }
                    x += zig_zag_decode(geometry[cursor]);
                    y += zig_zag_decode(geometry[cursor + 1]);
                    cursor += 2;
                    if command_id == 1 && !path.is_empty() {
                        paths.push(std::mem::take(&mut path));
                    }
                    path.push(TilePoint { x, y });
                }
            }
            7 => {
                if let Some(first) = path.first().copied()
                    && path.last().copied() != Some(first)
                {
                    path.push(first);
                }
            }
            _ => return Err(format!("unsupported MVT geometry command {command_id}")),
        }
    }

    if !path.is_empty() {
        paths.push(path);
    }
    Ok(paths)
}

fn zig_zag_decode(value: u32) -> i32 {
    ((value >> 1) as i32) ^ (-((value & 1) as i32))
}

fn zig_zag_decode_64(value: u64) -> i64 {
    ((value >> 1) as i64) ^ (-((value & 1) as i64))
}

fn read_packed_u32(bytes: &[u8]) -> Result<Vec<u32>, String> {
    let mut reader = RawReader::new(bytes);
    let mut values = Vec::new();
    while !reader.is_empty() {
        values.push(reader.read_varint()? as u32);
    }
    Ok(values)
}

struct ProtoReader<'a> {
    raw: RawReader<'a>,
}

impl<'a> ProtoReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            raw: RawReader::new(bytes),
        }
    }

    fn next_field(&mut self) -> Result<Option<Field<'a>>, String> {
        if self.raw.is_empty() {
            return Ok(None);
        }

        let tag = self.raw.read_varint()?;
        let number = (tag >> 3) as u32;
        let wire_type = (tag & 0x7) as u8;
        let value = match wire_type {
            0 => FieldValue::Varint(self.raw.read_varint()?),
            1 => FieldValue::Fixed64(self.raw.read_fixed64()?),
            2 => {
                let len = self.raw.read_varint()? as usize;
                FieldValue::Bytes(self.raw.read_bytes(len)?)
            }
            5 => FieldValue::Fixed32(self.raw.read_fixed32()?),
            _ => return Err(format!("unsupported protobuf wire type {wire_type}")),
        };
        Ok(Some(Field { number, value }))
    }
}

struct Field<'a> {
    number: u32,
    value: FieldValue<'a>,
}

enum FieldValue<'a> {
    Varint(u64),
    Fixed32(f32),
    Fixed64(f64),
    Bytes(&'a [u8]),
}

struct RawReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> RawReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn is_empty(&self) -> bool {
        self.cursor >= self.bytes.len()
    }

    fn read_varint(&mut self) -> Result<u64, String> {
        let mut result = 0_u64;
        let mut shift = 0;
        loop {
            if self.cursor >= self.bytes.len() {
                return Err("truncated protobuf varint".to_owned());
            }
            let byte = self.bytes[self.cursor];
            self.cursor += 1;
            result |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
            shift += 7;
            if shift >= 64 {
                return Err("protobuf varint is too large".to_owned());
            }
        }
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .cursor
            .checked_add(len)
            .ok_or_else(|| "protobuf length overflow".to_owned())?;
        if end > self.bytes.len() {
            return Err("truncated protobuf bytes field".to_owned());
        }
        let bytes = &self.bytes[self.cursor..end];
        self.cursor = end;
        Ok(bytes)
    }

    fn read_fixed32(&mut self) -> Result<f32, String> {
        let bytes = self.read_bytes(4)?;
        Ok(f32::from_le_bytes(
            bytes.try_into().expect("read_bytes returned 4 bytes"),
        ))
    }

    fn read_fixed64(&mut self) -> Result<f64, String> {
        let bytes = self.read_bytes(8)?;
        Ok(f64::from_le_bytes(
            bytes.try_into().expect("read_bytes returned 8 bytes"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_minimal_polygon_tile() {
        let tile = decode_vector_tile(&minimal_polygon_tile()).expect("tile decodes");
        assert_eq!(tile.layers.len(), 1);
        assert_eq!(tile.layers[0].name, "water");
        assert_eq!(tile.layers[0].extent, 4096);
        assert_eq!(tile.layers[0].features.len(), 1);

        let feature = &tile.layers[0].features[0];
        assert_eq!(feature.geometry_type, GeometryType::Polygon);
        assert_eq!(feature.paths.len(), 1);
        assert_eq!(
            feature.paths[0],
            vec![
                TilePoint { x: 0, y: 0 },
                TilePoint { x: 4096, y: 0 },
                TilePoint { x: 4096, y: 4096 },
                TilePoint { x: 0, y: 4096 },
                TilePoint { x: 0, y: 0 },
            ]
        );
    }

    fn minimal_polygon_tile() -> Vec<u8> {
        let geometry = packed(&[
            9,
            zig_zag_encode(0),
            zig_zag_encode(0),
            26,
            zig_zag_encode(4096),
            zig_zag_encode(0),
            zig_zag_encode(0),
            zig_zag_encode(4096),
            zig_zag_encode(-4096),
            zig_zag_encode(0),
            15,
        ]);

        let mut feature = Vec::new();
        field_varint(&mut feature, 3, 3);
        field_bytes(&mut feature, 4, &geometry);

        let mut layer = Vec::new();
        field_bytes(&mut layer, 1, b"water");
        field_bytes(&mut layer, 2, &feature);
        field_varint(&mut layer, 5, 4096);
        field_varint(&mut layer, 15, 2);

        let mut tile = Vec::new();
        field_bytes(&mut tile, 3, &layer);
        tile
    }

    fn zig_zag_encode(value: i32) -> u32 {
        ((value << 1) ^ (value >> 31)) as u32
    }

    fn packed(values: &[u32]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for value in values {
            varint(&mut bytes, u64::from(*value));
        }
        bytes
    }

    fn field_varint(bytes: &mut Vec<u8>, number: u32, value: u64) {
        varint(bytes, u64::from(number << 3));
        varint(bytes, value);
    }

    fn field_bytes(bytes: &mut Vec<u8>, number: u32, value: &[u8]) {
        varint(bytes, u64::from((number << 3) | 2));
        varint(bytes, value.len() as u64);
        bytes.extend_from_slice(value);
    }

    fn varint(bytes: &mut Vec<u8>, mut value: u64) {
        while value >= 0x80 {
            bytes.push((value as u8) | 0x80);
            value >>= 7;
        }
        bytes.push(value as u8);
    }
}
