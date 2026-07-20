use std::collections::{HashMap, HashSet, VecDeque};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, OnceLock};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use super::{
    GeoPosition, MapFeatureGeometry, MapFeatureInfo, MapViewport, TILE_SIZE, geo_to_world,
    triangulate_polygon, world_to_geo,
};
use mara_core::{
    layout::TextMeasureSpec,
    paint::{PaintCmd, PaintVertex},
    vocab::{
        Align2 as MaraAlign2, Color32 as MaraColor32, CornerRadius, Pos2 as MaraPos2,
        Rect as MaraRect, Stroke as MaraStroke, Vec2 as MaraVec2,
    },
};

const OPENFREEMAP_TILE_URL: &str = "https://tiles.openfreemap.org/planet/latest";
const MAX_SOURCE_ZOOM: f64 = 14.0;
const MAP_DESATURATION: f32 = 0.46;
const MAP_ACCENT_TINT_SCALE: f32 = 0.54;
const MAX_TILE_REQUESTS_PER_PAINT: usize = 6;
#[cfg(target_arch = "wasm32")]
const MAX_WASM_DECODE_PER_POLL: usize = 1;

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

enum TilePayload {
    #[cfg(not(target_arch = "wasm32"))]
    Decoded(DecodedVectorTile),
    #[cfg(target_arch = "wasm32")]
    Bytes(Vec<u8>),
}

type TileMessage = (TileKey, Result<TilePayload, String>);

pub(crate) struct VectorTileCache {
    tiles: HashMap<TileKey, TileEntry>,
    pending_decodes: VecDeque<(TileKey, Vec<u8>)>,
    tx: mpsc::Sender<TileMessage>,
    rx: Mutex<mpsc::Receiver<TileMessage>>,
}

impl Default for VectorTileCache {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tiles: HashMap::new(),
            pending_decodes: VecDeque::new(),
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
    let cache_changed = cache.poll_finished();
    let palette = MapPalette::current();
    render_paint_cmd(
        ui.painter(),
        PaintCmd::RectFilled {
            rect: rect.into(),
            corner: CornerRadius::ZERO,
            fill: palette.background.into(),
        },
    );

    let visible_tiles = visible_tile_keys(viewport, rect.size());

    let has_loading = request_visible_tiles(cache, &visible_tiles);

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

    if cache_changed {
        ui.ctx().request_repaint();
    }
    if has_loading {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
    if fast_mode {
        ui.ctx().request_repaint_after(Duration::from_millis(80));
    }
}

pub(crate) fn prewarm_vector_basemap(
    ctx: &egui::Context,
    viewport: MapViewport,
    size: egui::Vec2,
    cache: &mut VectorTileCache,
) {
    let cache_changed = cache.poll_finished();
    let visible_tiles = visible_tile_keys(viewport, size);
    let has_loading = request_visible_tiles(cache, &visible_tiles);
    if cache_changed {
        ctx.request_repaint();
    }
    if has_loading || cache.has_pending_decode() {
        ctx.request_repaint_after(Duration::from_millis(40));
    }
}

pub(crate) fn hit_test_vector_feature(
    rect: egui::Rect,
    viewport: MapViewport,
    cache: &VectorTileCache,
    pos: egui::Pos2,
) -> Option<MapFeatureInfo> {
    let visible_tiles = visible_tile_keys(viewport, rect.size());
    let mut best: Option<FeatureHit> = None;
    for &key in &visible_tiles {
        let Some(TileEntry::Ready(tile)) = cache.tiles.get(&key) else {
            continue;
        };
        for layer in &tile.layers {
            if !is_interactive_layer(&layer.name) {
                continue;
            }
            for feature in &layer.features {
                let Some(score) = hit_score(rect, viewport, key, layer, feature, pos) else {
                    continue;
                };
                let hit = FeatureHit {
                    priority: interactive_priority(&layer.name, feature),
                    score: score.score,
                    info: feature_info(
                        rect,
                        viewport,
                        cache,
                        &visible_tiles,
                        key,
                        layer,
                        feature,
                        score.path_index,
                    ),
                };
                if best.as_ref().is_none_or(|best| hit.is_better_than(best)) {
                    best = Some(hit);
                }
            }
        }
    }
    best.map(|hit| hit.info)
}

fn visible_tile_keys(viewport: MapViewport, size: egui::Vec2) -> Vec<TileKey> {
    let z = viewport.zoom.floor().clamp(0.0, MAX_SOURCE_ZOOM) as u8;
    let zf = f64::from(z);
    let scale = 2.0_f64.powf(viewport.zoom - zf);
    let center_world = geo_to_world(viewport.center, zf);
    let top_left_world = (
        center_world.0 - f64::from(size.x) / (2.0 * scale),
        center_world.1 - f64::from(size.y) / (2.0 * scale),
    );
    let bottom_right_world = (
        center_world.0 + f64::from(size.x) / (2.0 * scale),
        center_world.1 + f64::from(size.y) / (2.0 * scale),
    );
    let min_x = (top_left_world.0 / TILE_SIZE).floor() as i64;
    let max_x = (bottom_right_world.0 / TILE_SIZE).ceil() as i64;
    let min_y = (top_left_world.1 / TILE_SIZE).floor() as i64;
    let max_y = (bottom_right_world.1 / TILE_SIZE).ceil() as i64;
    let tile_count = 1_i64 << u32::from(z);

    let center_tile_x = center_world.0 / TILE_SIZE;
    let center_tile_y = center_world.1 / TILE_SIZE;
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
            visible_tiles.push(key);
        }
    }
    visible_tiles.sort_by(|a, b| {
        let ax = f64::from(a.x) + 0.5 - center_tile_x;
        let ay = f64::from(a.y) + 0.5 - center_tile_y;
        let bx = f64::from(b.x) + 0.5 - center_tile_x;
        let by = f64::from(b.y) + 0.5 - center_tile_y;
        (ax * ax + ay * ay).total_cmp(&(bx * bx + by * by))
    });
    visible_tiles
}

struct FeatureHit {
    priority: i32,
    score: f32,
    info: MapFeatureInfo,
}

impl FeatureHit {
    fn is_better_than(&self, other: &Self) -> bool {
        self.priority > other.priority
            || (self.priority == other.priority && self.score < other.score)
    }
}

fn is_interactive_layer(layer: &str) -> bool {
    matches!(
        layer,
        "building"
            | "transportation"
            | "water"
            | "waterway"
            | "landuse"
            | "landcover"
            | "park"
            | "aeroway"
            | "boundary"
    )
}

fn interactive_priority(layer: &str, feature: &DecodedFeature) -> i32 {
    match layer {
        "building" => 120,
        "transportation" => 105,
        "waterway" => 98,
        "water" => 82,
        "park" => 74,
        "landuse" => match feature.class() {
            "farmland" | "farm" | "orchard" | "vineyard" => 72,
            _ => 66,
        },
        "landcover" => 64,
        "aeroway" => 56,
        "boundary" => 22,
        _ => 0,
    }
}

fn hit_score(
    rect: egui::Rect,
    viewport: MapViewport,
    key: TileKey,
    layer: &DecodedLayer,
    feature: &DecodedFeature,
    pos: egui::Pos2,
) -> Option<PathHit> {
    match feature.geometry_type {
        GeometryType::Point | GeometryType::Unknown => None,
        GeometryType::LineString => {
            let tolerance = match layer.name.as_str() {
                "transportation" => 9.0,
                "waterway" => 10.0,
                "boundary" => 7.0,
                _ => 8.0,
            };
            feature
                .paths
                .iter()
                .enumerate()
                .filter_map(|(path_index, path)| {
                    let points = screen_points(path, layer.extent, key, rect, viewport);
                    let score = points
                        .windows(2)
                        .map(|w| distance_to_segment(pos, w[0], w[1]))
                        .min_by(f32::total_cmp)?;
                    (score <= tolerance).then_some(PathHit { path_index, score })
                })
                .min_by(|a, b| a.score.total_cmp(&b.score))
        }
        GeometryType::Polygon => feature
            .paths
            .iter()
            .enumerate()
            .filter_map(|(path_index, path)| {
                let points = screen_points(path, layer.extent, key, rect, viewport);
                if points.len() < 3 || !path_intersects_rect(&points, rect) {
                    return None;
                }
                if point_in_polygon(pos, &points) {
                    Some(PathHit {
                        path_index,
                        score: polygon_abs_area(&normalized_screen_ring(&points)),
                    })
                } else {
                    None
                }
            })
            .min_by(|a, b| a.score.total_cmp(&b.score)),
    }
}

#[derive(Clone, Copy)]
struct PathHit {
    path_index: usize,
    score: f32,
}

#[allow(clippy::too_many_arguments)]
fn feature_info(
    rect: egui::Rect,
    viewport: MapViewport,
    cache: &VectorTileCache,
    visible_tiles: &[TileKey],
    key: TileKey,
    layer: &DecodedLayer,
    feature: &DecodedFeature,
    path_index: usize,
) -> MapFeatureInfo {
    let mut properties = feature
        .properties
        .iter()
        .map(|(key, value)| (key.clone(), value.display_value()))
        .collect::<Vec<_>>();
    properties.sort_by(|a, b| a.0.cmp(&b.0));
    MapFeatureInfo {
        layer: layer.name.clone(),
        class: feature.class().to_owned(),
        geometry: match feature.geometry_type {
            GeometryType::Point => MapFeatureGeometry::Point,
            GeometryType::LineString => MapFeatureGeometry::Line,
            GeometryType::Polygon => MapFeatureGeometry::Polygon,
            GeometryType::Unknown => MapFeatureGeometry::Point,
        },
        name: feature.label_text().map(ToOwned::to_owned),
        properties,
        paths: connected_feature_geo_paths(
            rect,
            viewport,
            cache,
            visible_tiles,
            key,
            layer,
            feature,
            path_index,
        ),
    }
}

#[derive(Clone)]
struct CandidatePath {
    screen: Vec<egui::Pos2>,
    geo: Vec<GeoPosition>,
}

#[allow(clippy::too_many_arguments)]
fn connected_feature_geo_paths(
    rect: egui::Rect,
    viewport: MapViewport,
    cache: &VectorTileCache,
    visible_tiles: &[TileKey],
    seed_key: TileKey,
    seed_layer: &DecodedLayer,
    seed_feature: &DecodedFeature,
    seed_path_index: usize,
) -> Vec<Vec<GeoPosition>> {
    connected_feature_paths(
        rect,
        viewport,
        cache,
        visible_tiles,
        seed_key,
        seed_layer,
        seed_feature,
        seed_path_index,
    )
    .into_iter()
    .map(|candidate| candidate.geo)
    .filter(|path| !path.is_empty())
    .collect()
}

#[allow(clippy::too_many_arguments)]
fn connected_feature_paths(
    rect: egui::Rect,
    viewport: MapViewport,
    cache: &VectorTileCache,
    visible_tiles: &[TileKey],
    seed_key: TileKey,
    seed_layer: &DecodedLayer,
    seed_feature: &DecodedFeature,
    seed_path_index: usize,
) -> Vec<CandidatePath> {
    let Some(seed_path) = seed_feature.paths.get(seed_path_index) else {
        return Vec::new();
    };
    let seed = CandidatePath {
        screen: screen_points(seed_path, seed_layer.extent, seed_key, rect, viewport),
        geo: path_geo_points(seed_path, seed_layer.extent, seed_key),
    };
    if seed.geo.is_empty() {
        return Vec::new();
    }

    let mut selected = vec![seed.clone()];
    let mut candidates = matching_candidate_paths(
        rect,
        viewport,
        cache,
        visible_tiles,
        &seed_layer.name,
        seed_feature,
    );

    candidates.retain(|candidate| !same_screen_path(&candidate.screen, &seed.screen));

    let tolerance = if seed_feature.id.is_some() { 6.0 } else { 2.5 };
    let mut changed = true;
    while changed {
        changed = false;
        let mut index = 0;
        while index < candidates.len() {
            let connects = selected.iter().any(|existing| {
                paths_touch(&existing.screen, &candidates[index].screen, tolerance)
            });
            if connects {
                selected.push(candidates.remove(index));
                changed = true;
            } else {
                index += 1;
            }
        }
    }

    selected
}

fn same_screen_path(a: &[egui::Pos2], b: &[egui::Pos2]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.distance(*b) <= f32::EPSILON)
}

fn matching_candidate_paths(
    rect: egui::Rect,
    viewport: MapViewport,
    cache: &VectorTileCache,
    visible_tiles: &[TileKey],
    seed_layer_name: &str,
    seed_feature: &DecodedFeature,
) -> Vec<CandidatePath> {
    let mut out = Vec::new();
    for key in visible_tiles {
        let Some(TileEntry::Ready(tile)) = cache.tiles.get(key) else {
            continue;
        };
        for layer in &tile.layers {
            if layer.name != seed_layer_name {
                continue;
            }
            for feature in &layer.features {
                if !same_selectable_feature(seed_feature, feature) {
                    continue;
                }
                for path in &feature.paths {
                    let screen = screen_points(path, layer.extent, *key, rect, viewport);
                    if screen.is_empty() {
                        continue;
                    }
                    out.push(CandidatePath {
                        geo: path_geo_points(path, layer.extent, *key),
                        screen,
                    });
                }
            }
        }
    }
    out
}

fn same_selectable_feature(a: &DecodedFeature, b: &DecodedFeature) -> bool {
    if a.geometry_type != b.geometry_type {
        return false;
    }
    match (a.id, b.id) {
        (Some(a), Some(b)) => a == b,
        _ => {
            a.class() == b.class()
                && a.label_text() == b.label_text()
                && a.properties == b.properties
        }
    }
}

fn paths_touch(a: &[egui::Pos2], b: &[egui::Pos2], tolerance: f32) -> bool {
    if a.len() < 2 || b.len() < 2 {
        return false;
    }
    if !path_bounds(a).expand(tolerance).intersects(path_bounds(b)) {
        return false;
    }
    a.iter()
        .any(|point| distance_to_path(*point, b) <= tolerance)
        || b.iter()
            .any(|point| distance_to_path(*point, a) <= tolerance)
}

fn distance_to_path(point: egui::Pos2, path: &[egui::Pos2]) -> f32 {
    path.windows(2)
        .map(|w| distance_to_segment(point, w[0], w[1]))
        .min_by(f32::total_cmp)
        .unwrap_or(f32::INFINITY)
}

fn path_bounds(points: &[egui::Pos2]) -> egui::Rect {
    let Some(first) = points.first().copied() else {
        return egui::Rect::NOTHING;
    };
    let mut bounds = egui::Rect::from_min_max(first, first);
    for point in &points[1..] {
        bounds.min.x = bounds.min.x.min(point.x);
        bounds.min.y = bounds.min.y.min(point.y);
        bounds.max.x = bounds.max.x.max(point.x);
        bounds.max.y = bounds.max.y.max(point.y);
    }
    bounds
}

fn path_geo_points(path: &[TilePoint], extent: u32, key: TileKey) -> Vec<GeoPosition> {
    path.iter()
        .map(|point| tile_point_to_geo(*point, extent, key))
        .collect()
}

fn tile_point_to_geo(point: TilePoint, extent: u32, key: TileKey) -> GeoPosition {
    let extent = f64::from(extent.max(1));
    let world_x = f64::from(key.x) * TILE_SIZE + f64::from(point.x) / extent * TILE_SIZE;
    let world_y = f64::from(key.y) * TILE_SIZE + f64::from(point.y) / extent * TILE_SIZE;
    world_to_geo((world_x, world_y), f64::from(key.z))
}

fn request_visible_tiles(cache: &mut VectorTileCache, visible_tiles: &[TileKey]) -> bool {
    let mut has_loading = cache.has_pending_decode();
    let mut started_requests = 0;
    for key in visible_tiles {
        if cache.is_missing(*key) {
            if started_requests < MAX_TILE_REQUESTS_PER_PAINT {
                started_requests += usize::from(cache.request(*key));
            } else {
                has_loading = true;
                continue;
            }
        }
        if matches!(cache.tiles.get(key), Some(TileEntry::Loading) | None) {
            has_loading = true;
        }
    }
    has_loading
}

impl VectorTileCache {
    fn poll_finished(&mut self) -> bool {
        let mut changed = self.decode_pending();
        let Ok(rx) = self.rx.lock() else {
            return changed;
        };
        while let Ok((key, result)) = rx.try_recv() {
            let entry = match result {
                #[cfg(not(target_arch = "wasm32"))]
                Ok(TilePayload::Decoded(tile)) => TileEntry::Ready(tile),
                #[cfg(target_arch = "wasm32")]
                Ok(TilePayload::Bytes(bytes)) => {
                    self.pending_decodes.push_back((key, bytes));
                    continue;
                }
                Err(_) => TileEntry::Failed,
            };
            self.tiles.insert(key, entry);
            changed = true;
        }
        drop(rx);
        changed || self.decode_pending()
    }

    fn decode_pending(&mut self) -> bool {
        let mut changed = false;
        let mut remaining_budget = decode_budget_per_poll();
        while remaining_budget > 0 {
            let Some((key, bytes)) = self.pending_decodes.pop_front() else {
                break;
            };
            let entry = match decode_vector_tile(&bytes) {
                Ok(tile) => TileEntry::Ready(tile),
                Err(_) => TileEntry::Failed,
            };
            self.tiles.insert(key, entry);
            remaining_budget -= 1;
            changed = true;
        }
        changed
    }

    fn has_pending_decode(&self) -> bool {
        !self.pending_decodes.is_empty()
    }

    fn is_missing(&self, key: TileKey) -> bool {
        !self.tiles.contains_key(&key)
    }

    fn request(&mut self, key: TileKey) -> bool {
        if self.tiles.contains_key(&key) {
            return false;
        }
        self.tiles.insert(key, TileEntry::Loading);
        request_tile(key, self.tx.clone());
        true
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_budget_per_poll() -> usize {
    usize::MAX
}

#[cfg(target_arch = "wasm32")]
fn decode_budget_per_poll() -> usize {
    MAX_WASM_DECODE_PER_POLL
}

#[cfg(not(target_arch = "wasm32"))]
struct TileJob {
    key: TileKey,
    tx: mpsc::Sender<TileMessage>,
}

#[cfg(not(target_arch = "wasm32"))]
fn request_tile(key: TileKey, tx: mpsc::Sender<TileMessage>) {
    if let Err(err) = tile_worker_queue().send(TileJob { key, tx }) {
        let TileJob { key, tx } = err.0;
        // Extremely unlikely, but keep the map usable if the shared
        // worker queue went away for any reason.
        std::thread::spawn(move || {
            let result = fetch_tile(key)
                .and_then(|bytes| decode_vector_tile(&bytes))
                .map(TilePayload::Decoded);
            let _ = tx.send((key, result));
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn tile_worker_queue() -> &'static mpsc::Sender<TileJob> {
    static TILE_WORKER_QUEUE: OnceLock<mpsc::Sender<TileJob>> = OnceLock::new();
    TILE_WORKER_QUEUE.get_or_init(|| {
        let (job_tx, job_rx) = mpsc::channel::<TileJob>();
        let shared_rx = Arc::new(Mutex::new(job_rx));
        let worker_count = std::thread::available_parallelism()
            .map_or(2, |parallelism| (parallelism.get() / 2).clamp(2, 4));
        for index in 0..worker_count {
            let shared_rx = Arc::clone(&shared_rx);
            let name = format!("mara-map-tile-worker-{index}");
            let _ = std::thread::Builder::new().name(name).spawn(move || {
                loop {
                    let job = {
                        let Ok(rx) = shared_rx.lock() else {
                            return;
                        };
                        rx.recv()
                    };
                    let Ok(TileJob { key, tx }) = job else {
                        return;
                    };
                    let result = fetch_tile(key)
                        .and_then(|bytes| decode_vector_tile(&bytes))
                        .map(TilePayload::Decoded);
                    let _ = tx.send((key, result));
                }
            });
        }
        job_tx
    })
}

#[cfg(target_arch = "wasm32")]
fn request_tile(key: TileKey, tx: mpsc::Sender<TileMessage>) {
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
                    Ok(TilePayload::Bytes(response.bytes))
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

#[allow(clippy::too_many_arguments)]
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
        if !pass_accepts_layer(pass, &layer.name) {
            continue;
        }
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
                    Some(palette.background),
                ),
                PaintPass::WaterFill => paint_area_fill(
                    painter,
                    rect,
                    viewport,
                    key,
                    layer,
                    feature,
                    water_fill_color(&layer.name, feature, palette),
                    Some(palette.land_default),
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
                        Some(palette.land_default),
                    );
                    if layer.name == "building" {
                        paint_building_extrusion(
                            painter, rect, viewport, key, layer, feature, palette,
                        );
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
                PaintPass::Label => {
                    paint_label(
                        painter, rect, viewport, key, layer, feature, palette, labels,
                    );
                }
            }
        }
    }
}

fn pass_accepts_layer(pass: PaintPass, layer: &str) -> bool {
    match pass {
        PaintPass::LandFill => matches!(layer, "landcover" | "landuse" | "park" | "aeroway"),
        PaintPass::WaterFill => layer == "water",
        PaintPass::BuildingFill => layer == "building",
        PaintPass::RoadCasing | PaintPass::RoadFill => layer == "transportation",
        PaintPass::LineOverlay => matches!(layer, "waterway" | "boundary" | "aeroway"),
        PaintPass::Label => matches!(
            layer,
            "place"
                | "transportation_name"
                | "water_name"
                | "waterway_name"
                | "poi"
                | "mountain_peak"
        ),
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
    Label,
}

impl PaintPass {
    const ALL: [Self; 7] = [
        Self::LandFill,
        Self::WaterFill,
        Self::BuildingFill,
        Self::RoadCasing,
        Self::RoadFill,
        Self::LineOverlay,
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
    occupied: Vec<MaraRect>,
    road_names: HashSet<String>,
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
    building_side: egui::Color32,
    boundary: egui::Color32,
    aeroway_line: egui::Color32,
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
        let accent: egui::Color32 = mara_core::style::active_accent().into();
        if theme.is_light {
            Self {
                background: theme.palette.bg_window,
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
                building_side: tint(rgb(0xb8, 0xb0, 0xa4), accent, 0.07),
                boundary: tint(rgba(0x86, 0x7d, 0x74, 155), accent, 0.08),
                aeroway_line: tint(rgb(0xc4, 0xbd, 0xb3), accent, 0.06),
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
                background: theme.palette.bg_window,
                land_default: dark_style(rgba(0x18, 0x1f, 0x23, 238), accent, 0.05),
                forest: dark_style(rgb(0x20, 0x30, 0x28), accent, 0.24),
                grass: dark_style(rgb(0x27, 0x35, 0x2b), accent, 0.28),
                scrub: dark_style(rgb(0x34, 0x37, 0x2c), accent, 0.18),
                sand: dark_style(rgb(0x3d, 0x35, 0x29), accent, 0.06),
                wetland: dark_style(rgb(0x1c, 0x35, 0x34), accent, 0.11),
                ice: dark_style(rgb(0x2b, 0x3b, 0x42), accent, 0.08),
                residential: dark_style(rgb(0x17, 0x1c, 0x21), accent, 0.045),
                commercial: dark_style(rgb(0x1d, 0x21, 0x26), accent, 0.04),
                industrial: dark_style(rgb(0x1f, 0x22, 0x27), accent, 0.04),
                education: dark_style(rgb(0x23, 0x25, 0x24), accent, 0.035),
                hospital: dark_style(rgb(0x26, 0x22, 0x24), accent, 0.035),
                cemetery: dark_style(rgb(0x25, 0x32, 0x29), accent, 0.20),
                farmland: dark_style(rgb(0x3d, 0x34, 0x28), accent, 0.22),
                park: dark_style(rgb(0x22, 0x34, 0x29), accent, 0.28),
                aeroway_fill: dark_style(rgb(0x20, 0x23, 0x27), accent, 0.035),
                water: dark_style(rgb(0x0b, 0x2b, 0x3a), accent, 0.07),
                water_line: dark_style(rgb(0x14, 0x56, 0x70), accent, 0.075),
                building: dark_style(rgb(0x26, 0x2b, 0x31), accent, 0.035),
                building_outline: dark_style(rgb(0x3a, 0x42, 0x49), accent, 0.045),
                building_side: dark_style(rgb(0x15, 0x19, 0x1d), accent, 0.035),
                boundary: dark_style(rgba(0x88, 0x90, 0x94, 115), accent, 0.04),
                aeroway_line: dark_style(rgb(0x5b, 0x60, 0x64), accent, 0.035),
                label: dark_style(rgb(0xdc, 0xdf, 0xe3), accent, 0.035),
                label_secondary: dark_style(rgb(0xa4, 0xaa, 0xb0), accent, 0.04),
                label_halo: dark_style(rgba(0x0a, 0x0e, 0x12, 240), accent, 0.02),
                water_label: dark_style(rgb(0x8f, 0xbe, 0xcf), accent, 0.05),
                road_default: dark_style(rgb(0x36, 0x3b, 0x40), accent, 0.035),
                road_minor: dark_style(rgb(0x28, 0x2d, 0x32), accent, 0.03),
                road_medium: dark_style(rgb(0x48, 0x45, 0x3d), accent, 0.025),
                road_major: dark_style(rgb(0x5c, 0x53, 0x42), accent, 0.02),
                motorway: dark_style(rgb(0x6e, 0x5d, 0x43), accent, 0.018),
                trunk: dark_style(rgb(0x66, 0x58, 0x40), accent, 0.018),
                rail: dark_style(rgb(0x71, 0x75, 0x76), accent, 0.035),
                road_casing: dark_style(rgb(0x11, 0x15, 0x19), accent, 0.025),
                major_casing: dark_style(rgb(0x20, 0x22, 0x25), accent, 0.025),
                rail_casing: dark_style(rgba(0xbb, 0xc3, 0xc7, 110), accent, 0.035),
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_area_fill(
    painter: &egui::Painter,
    rect: egui::Rect,
    viewport: MapViewport,
    key: TileKey,
    layer: &DecodedLayer,
    feature: &DecodedFeature,
    fill: Option<egui::Color32>,
    hole_fill: Option<egui::Color32>,
) {
    for cmd in area_fill_paint_cmds(rect, viewport, key, layer, feature, fill, hole_fill) {
        render_paint_cmd(painter, cmd);
    }
}

fn area_fill_paint_cmds(
    rect: egui::Rect,
    viewport: MapViewport,
    key: TileKey,
    layer: &DecodedLayer,
    feature: &DecodedFeature,
    fill: Option<egui::Color32>,
    hole_fill: Option<egui::Color32>,
) -> Vec<PaintCmd> {
    if feature.geometry_type != GeometryType::Polygon {
        return Vec::new();
    }
    let Some(fill) = fill else {
        return Vec::new();
    };
    let fill = MaraColor32::from(fill);
    let hole_fill = hole_fill.map(MaraColor32::from);
    let mut cmds = Vec::new();

    let rings = screen_rings_for_feature(feature, layer.extent, key, rect, viewport);
    let has_exterior = rings.iter().any(|ring| !ring.is_hole);
    let (exteriors, holes): (Vec<_>, Vec<_>) = rings.into_iter().partition(|ring| !ring.is_hole);
    for ring in exteriors.into_iter().chain(holes) {
        if !path_intersects_rect(&ring.points, rect.expand(64.0)) {
            continue;
        }

        let ring_fill = if ring.is_hole && has_exterior {
            let Some(hole_fill) = hole_fill else {
                continue;
            };
            hole_fill
        } else {
            fill
        };

        if let Some(cmd) = mesh_polygon_paint_cmd(&ring.points, ring_fill.into()) {
            cmds.push(cmd);
        } else {
            cmds.push(PaintCmd::Polygon {
                points: ring.points.into_iter().map(Into::into).collect(),
                fill: ring_fill,
                stroke: MaraStroke::NONE,
            });
        }
    }
    cmds
}

struct ScreenRing {
    points: Vec<egui::Pos2>,
    is_hole: bool,
}

fn screen_rings_for_feature(
    feature: &DecodedFeature,
    extent: u32,
    key: TileKey,
    rect: egui::Rect,
    viewport: MapViewport,
) -> Vec<ScreenRing> {
    let mut rings = feature
        .paths
        .iter()
        .filter_map(|path| {
            let points = normalized_screen_ring(&screen_points(path, extent, key, rect, viewport));
            if points.len() < 3 {
                return None;
            }
            Some(ScreenRing {
                is_hole: polygon_signed_area(&points) < 0.0,
                points,
            })
        })
        .collect::<Vec<_>>();

    // Real MVT polygons use opposite winding for exterior and interior
    // rings. If a provider gives us non-conforming winding for every ring,
    // keep rendering all rings as exteriors instead of dropping the feature.
    if rings.iter().all(|ring| ring.is_hole) {
        for ring in &mut rings {
            ring.is_hole = false;
        }
    }
    rings
}

fn paint_building_extrusion(
    painter: &egui::Painter,
    rect: egui::Rect,
    viewport: MapViewport,
    key: TileKey,
    layer: &DecodedLayer,
    feature: &DecodedFeature,
    palette: &MapPalette,
) {
    for cmd in building_extrusion_paint_cmds(rect, viewport, key, layer, feature, palette) {
        render_paint_cmd(painter, cmd);
    }
}

fn building_extrusion_paint_cmds(
    rect: egui::Rect,
    viewport: MapViewport,
    key: TileKey,
    layer: &DecodedLayer,
    feature: &DecodedFeature,
    palette: &MapPalette,
) -> Vec<PaintCmd> {
    if viewport.zoom < 17.0 || feature.geometry_type != GeometryType::Polygon {
        return Vec::new();
    }
    let render_height = feature
        .prop_f64("render_height")
        .or_else(|| feature.prop_f64("height"))
        .unwrap_or(14.0)
        .max(0.0);
    if render_height <= f64::EPSILON {
        return Vec::new();
    }

    let zoom_gain = smoothstep(17.0, 18.0, viewport.zoom as f32);
    let meters_to_px = (0.36 + (viewport.zoom as f32 - 15.0).max(0.0) * 0.15).clamp(0.36, 1.12);
    let height_px = (render_height as f32 * meters_to_px * zoom_gain).clamp(8.0, 300.0);
    let top_offset = fixed_building_extrusion_offset(height_px);
    let mut cmds = Vec::new();

    for path in &feature.paths {
        let screen_path = screen_points_raw(path, layer.extent, key, rect, viewport);
        let points = normalized_screen_ring(&screen_path);
        if points.len() < 3 || !path_intersects_rect(&points, rect.expand(height_px + 64.0)) {
            continue;
        }
        if polygon_abs_area(&points) < 18.0 {
            continue;
        }

        let mut side_vertices = Vec::new();
        let mut side_indices = Vec::new();
        for (screen_edge, tile_edge) in screen_path.windows(2).zip(path.windows(2)) {
            let a = screen_edge[0];
            let b = screen_edge[1];
            if a.distance(b) <= 0.5 {
                continue;
            }
            if is_tile_boundary_edge(tile_edge[0], tile_edge[1], layer.extent) {
                continue;
            }
            push_mesh_quad(
                &mut side_vertices,
                &mut side_indices,
                a,
                b,
                b + top_offset,
                a + top_offset,
                palette.building_side,
            );
        }
        if !side_indices.is_empty() {
            cmds.push(PaintCmd::Mesh {
                vertices: side_vertices,
                indices: side_indices,
            });
        }
    }

    let rings = screen_rings_for_feature(feature, layer.extent, key, rect, viewport);
    let has_exterior = rings.iter().any(|ring| !ring.is_hole);
    let (exteriors, holes): (Vec<_>, Vec<_>) = rings.into_iter().partition(|ring| !ring.is_hole);
    for ring in exteriors.into_iter().chain(holes) {
        if !path_intersects_rect(&ring.points, rect.expand(height_px + 64.0)) {
            continue;
        }
        let color = if ring.is_hole && has_exterior {
            palette.land_default
        } else {
            palette.building
        };
        let roof = ring
            .points
            .iter()
            .map(|point| *point + top_offset)
            .collect::<Vec<_>>();
        if let Some(cmd) = mesh_polygon_paint_cmd(&roof, color) {
            cmds.push(cmd);
        }
    }

    cmds.extend(building_roof_outline_paint_cmds(
        feature,
        layer.extent,
        key,
        rect,
        viewport,
        top_offset,
        egui::Stroke::new(0.65, palette.building_outline),
    ));
    cmds
}

fn fixed_building_extrusion_offset(height_px: f32) -> egui::Vec2 {
    // Use one camera vector for every building fragment. The previous
    // centroid-radial projection made tile-clipped halves lean in different
    // directions, which looked like buildings being split apart.
    egui::vec2(0.48, -0.78).normalized() * height_px * 0.82
}

fn building_roof_outline_paint_cmds(
    feature: &DecodedFeature,
    extent: u32,
    key: TileKey,
    rect: egui::Rect,
    viewport: MapViewport,
    top_offset: egui::Vec2,
    stroke: egui::Stroke,
) -> Vec<PaintCmd> {
    let mut cmds = Vec::new();
    for path in &feature.paths {
        let screen_path = screen_points_raw(path, extent, key, rect, viewport);
        for (screen_edge, tile_edge) in screen_path.windows(2).zip(path.windows(2)) {
            let a = screen_edge[0];
            let b = screen_edge[1];
            if a.distance(b) <= 0.5 || is_tile_boundary_edge(tile_edge[0], tile_edge[1], extent) {
                continue;
            }
            cmds.push(PaintCmd::Line {
                a: (a + top_offset).into(),
                b: (b + top_offset).into(),
                stroke: stroke.into(),
            });
        }
    }
    cmds
}

fn is_tile_boundary_edge(a: TilePoint, b: TilePoint, extent: u32) -> bool {
    let extent = extent as i32;
    const TOL: i32 = 1;
    let near = |value: i32, target: i32| (value - target).abs() <= TOL;
    (near(a.x, 0) && near(b.x, 0))
        || (near(a.x, extent) && near(b.x, extent))
        || (near(a.y, 0) && near(b.y, 0))
        || (near(a.y, extent) && near(b.y, extent))
}

fn normalized_screen_ring(points: &[egui::Pos2]) -> Vec<egui::Pos2> {
    let mut out = Vec::with_capacity(points.len());
    for point in points {
        if out
            .last()
            .is_none_or(|last: &egui::Pos2| last.distance(*point) > 0.5)
        {
            out.push(*point);
        }
    }
    if out.len() > 1
        && out
            .first()
            .zip(out.last())
            .is_some_and(|(first, last)| first.distance(*last) <= 0.5)
    {
        out.pop();
    }
    out
}

fn polygon_abs_area(points: &[egui::Pos2]) -> f32 {
    polygon_signed_area(points).abs()
}

fn polygon_signed_area(points: &[egui::Pos2]) -> f32 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f32>()
        * 0.5
}

fn mesh_polygon_paint_cmd(points: &[egui::Pos2], color: egui::Color32) -> Option<PaintCmd> {
    let triangles = triangulate_polygon(points);
    if triangles.is_empty() {
        return None;
    }
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for [a, b, c] in triangles {
        push_mesh_triangle(&mut vertices, &mut indices, a, b, c, color);
    }
    Some(PaintCmd::Mesh { vertices, indices })
}

fn push_mesh_triangle(
    vertices: &mut Vec<PaintVertex>,
    indices: &mut Vec<u32>,
    a: egui::Pos2,
    b: egui::Pos2,
    c: egui::Pos2,
    color: egui::Color32,
) {
    let start = vertices.len() as u32;
    for pos in [a, b, c] {
        vertices.push(PaintVertex {
            pos: pos.into(),
            color: color.into(),
        });
    }
    indices.extend_from_slice(&[start, start + 1, start + 2]);
}

fn push_mesh_quad(
    vertices: &mut Vec<PaintVertex>,
    indices: &mut Vec<u32>,
    a: egui::Pos2,
    b: egui::Pos2,
    c: egui::Pos2,
    d: egui::Pos2,
    color: egui::Color32,
) {
    let start = vertices.len() as u32;
    for pos in [a, b, c, d] {
        vertices.push(PaintVertex {
            pos: pos.into(),
            color: color.into(),
        });
    }
    indices.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
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
    for cmd in feature_line_paint_cmds(rect, viewport, key, layer, feature, stroke) {
        render_paint_cmd(painter, cmd);
    }
}

fn feature_line_paint_cmds(
    rect: egui::Rect,
    viewport: MapViewport,
    key: TileKey,
    layer: &DecodedLayer,
    feature: &DecodedFeature,
    stroke: egui::Stroke,
) -> Vec<PaintCmd> {
    if !matches!(
        feature.geometry_type,
        GeometryType::LineString | GeometryType::Polygon
    ) {
        return Vec::new();
    }
    let mut cmds = Vec::new();
    for path in &feature.paths {
        let points = screen_points(path, layer.extent, key, rect, viewport);
        if points.len() >= 2 && path_intersects_rect(&points, rect.expand(stroke.width + 16.0)) {
            cmds.push(PaintCmd::Polyline {
                points: points.into_iter().map(Into::into).collect(),
                stroke: stroke.into(),
            });
        }
    }
    cmds
}

fn render_paint_cmd(painter: &egui::Painter, cmd: PaintCmd) {
    mara_core::paint::__internal_render_paint_cmd_egui(painter, cmd);
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
    let zoom_scale = if zoom <= 14.0 {
        ((zoom - 9.0) / 6.0).clamp(0.55, 1.45) as f32
    } else {
        (1.25 + (zoom as f32 - 14.0) * 0.34).clamp(1.25, 4.2)
    };
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

#[allow(clippy::too_many_arguments)]
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
    let text = text.to_uppercase();
    if layer.name == "transportation_name" && !labels.road_names.insert(text.clone()) {
        return;
    }
    let measured_size = mara_core::layout::__internal_measure_text_egui(
        painter,
        &TextMeasureSpec::new(text.clone(), style.size, false),
    );
    for cmd in label_paint_cmds(
        &layer.name,
        pos.into(),
        text,
        &style,
        measured_size,
        rect.into(),
        labels,
    ) {
        render_paint_cmd(painter, cmd);
    }
}

fn label_paint_cmds(
    layer_name: &str,
    pos: MaraPos2,
    text: String,
    style: &LabelStyle,
    measured_size: MaraVec2,
    rect: MaraRect,
    labels: &mut LabelState,
) -> Vec<PaintCmd> {
    let padding = if layer_name == "transportation_name" {
        MaraVec2::new(72.0, 30.0)
    } else {
        MaraVec2::new(12.0, 8.0)
    };
    let bounds = MaraRect::from_center_size(
        pos,
        MaraVec2::new(measured_size.x + padding.x, measured_size.y + padding.y),
    );
    if !rect.intersects(bounds) || labels.occupied.iter().any(|taken| taken.intersects(bounds)) {
        return Vec::new();
    }
    labels.occupied.push(bounds);
    vec![
        PaintCmd::Text {
            pos: MaraPos2::new(pos.x + 1.0, pos.y + 1.0),
            anchor: MaraAlign2::CENTER_CENTER,
            text: text.clone(),
            size: style.size,
            color: style.halo.into(),
            mono: false,
        },
        PaintCmd::Text {
            pos,
            anchor: MaraAlign2::CENTER_CENTER,
            text,
            size: style.size,
            color: style.color.into(),
            mono: false,
        },
    ]
}

struct LabelStyle {
    size: f32,
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
        "transportation_name" if road_label_visible(class, zoom) => {
            let size = match class {
                "motorway" | "trunk" | "primary" => 11.0,
                "secondary" | "tertiary" => 10.0,
                _ => 9.0,
            };
            (size, palette.label_secondary)
        }
        "water_name" | "waterway_name" => (11.0, palette.water_label),
        "poi" if zoom >= 15.0 => (10.0, palette.label_secondary),
        "mountain_peak" if zoom >= 11.0 => (10.5, palette.label_secondary),
        _ => return None,
    };
    Some(LabelStyle {
        size,
        color,
        halo: palette.label_halo,
    })
}

fn road_label_visible(class: &str, zoom: f64) -> bool {
    match class {
        "motorway" | "trunk" | "primary" => zoom >= 12.6,
        "secondary" | "tertiary" => zoom >= 14.2,
        "minor" | "street" => zoom >= 15.8,
        "service" | "track" => zoom >= 17.0,
        "path" | "pedestrian" | "footway" | "cycleway" | "bridleway" | "steps" => zoom >= 17.5,
        "rail" | "transit" | "light_rail" | "subway" => zoom >= 14.8,
        _ => zoom >= 16.2,
    }
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

fn dark_style(base: egui::Color32, accent: egui::Color32, amount: f32) -> egui::Color32 {
    let softened = desaturate_color(base, 0.18);
    lerp_color(softened, desaturate_color(accent, 0.58), amount)
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

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn screen_points(
    path: &[TilePoint],
    extent: u32,
    key: TileKey,
    rect: egui::Rect,
    viewport: MapViewport,
) -> Vec<egui::Pos2> {
    // Drop consecutive points that project to within < ~0.5 pixels of
    // the previous kept point. MVT geometry is encoded at extent 4096
    // and contains many runs of densely packed points (curves, road
    // segments). Forwarding all of them to egui's tessellator can blow
    // past wgpu's per-buffer limit at typical viewport zooms.
    const MIN_STEP_SQ: f32 = 0.25;
    let scale = 2.0_f64.powf(viewport.zoom - f64::from(key.z));
    let center_world = geo_to_world(viewport.center, f64::from(key.z));
    let extent = f64::from(extent.max(1));
    let mut out: Vec<egui::Pos2> = Vec::with_capacity(path.len());
    for point in path {
        let world_x = f64::from(key.x) * TILE_SIZE + f64::from(point.x) / extent * TILE_SIZE;
        let world_y = f64::from(key.y) * TILE_SIZE + f64::from(point.y) / extent * TILE_SIZE;
        let p = egui::pos2(
            rect.center().x + ((world_x - center_world.0) * scale) as f32,
            rect.center().y + ((world_y - center_world.1) * scale) as f32,
        );
        if let Some(last) = out.last() {
            let dx = p.x - last.x;
            let dy = p.y - last.y;
            if dx * dx + dy * dy < MIN_STEP_SQ {
                continue;
            }
        }
        out.push(p);
    }
    out
}

fn screen_points_raw(
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

fn point_in_polygon(pos: egui::Pos2, points: &[egui::Pos2]) -> bool {
    if points.len() < 3 {
        return false;
    }
    let mut inside = false;
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        let crosses = (a.y > pos.y) != (b.y > pos.y);
        if crosses {
            let denom = b.y - a.y;
            if denom.abs() <= f32::EPSILON {
                continue;
            }
            let x = (b.x - a.x) * (pos.y - a.y) / denom + a.x;
            if pos.x < x {
                inside = !inside;
            }
        }
    }
    inside
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
    id: Option<u64>,
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

    fn prop_f64(&self, key: &str) -> Option<f64> {
        match self.properties.get(key) {
            Some(FeatureValue::F64(value)) => Some(*value),
            Some(FeatureValue::I64(value)) => Some(*value as f64),
            Some(FeatureValue::U64(value)) => Some(*value as f64),
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

impl FeatureValue {
    fn display_value(&self) -> String {
        match self {
            Self::String(value) => value.clone(),
            Self::F64(value) => {
                let mut formatted = format!("{value:.3}");
                while formatted.contains('.') && formatted.ends_with('0') {
                    formatted.pop();
                }
                if formatted.ends_with('.') {
                    formatted.pop();
                }
                formatted
            }
            Self::I64(value) => value.to_string(),
            Self::U64(value) => value.to_string(),
            Self::Bool(value) => value.to_string(),
        }
    }
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
    id: Option<u64>,
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
            id: self.id,
            geometry_type: self.geometry_type,
            paths: decode_geometry(&self.geometry)?,
            properties,
        })
    }
}

fn decode_feature(bytes: &[u8]) -> Result<RawFeature, String> {
    let mut reader = ProtoReader::new(bytes);
    let mut id = None;
    let mut geometry_type = GeometryType::Unknown;
    let mut geometry = Vec::new();
    let mut tags = Vec::new();

    while let Some(field) = reader.next_field()? {
        match (field.number, field.value) {
            (1, FieldValue::Varint(value)) => id = Some(value),
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
        id,
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
                return Ok(FeatureValue::I64(zig_zag_decode_64(value)));
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

    fn test_rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(256.0, 256.0))
    }

    fn test_viewport() -> MapViewport {
        MapViewport::new(GeoPosition::lon_lat(0.0, 0.0), 0.0)
    }

    fn test_tile_key() -> TileKey {
        TileKey { z: 0, x: 0, y: 0 }
    }

    fn test_layer(feature: DecodedFeature) -> DecodedLayer {
        DecodedLayer {
            name: "water".to_owned(),
            extent: 4096,
            features: vec![feature],
        }
    }

    fn polygon_feature() -> DecodedFeature {
        DecodedFeature {
            id: Some(1),
            geometry_type: GeometryType::Polygon,
            paths: vec![vec![
                TilePoint { x: 0, y: 0 },
                TilePoint { x: 4096, y: 0 },
                TilePoint { x: 4096, y: 4096 },
                TilePoint { x: 0, y: 4096 },
                TilePoint { x: 0, y: 0 },
            ]],
            properties: HashMap::new(),
        }
    }

    fn line_feature() -> DecodedFeature {
        DecodedFeature {
            id: Some(2),
            geometry_type: GeometryType::LineString,
            paths: vec![vec![
                TilePoint { x: 0, y: 2048 },
                TilePoint { x: 4096, y: 2048 },
            ]],
            properties: HashMap::new(),
        }
    }

    fn polygon_with_hole_feature() -> DecodedFeature {
        DecodedFeature {
            id: Some(4),
            geometry_type: GeometryType::Polygon,
            paths: vec![
                vec![
                    TilePoint { x: 0, y: 0 },
                    TilePoint { x: 4096, y: 0 },
                    TilePoint { x: 4096, y: 4096 },
                    TilePoint { x: 0, y: 4096 },
                    TilePoint { x: 0, y: 0 },
                ],
                vec![
                    TilePoint { x: 3072, y: 1024 },
                    TilePoint { x: 1024, y: 1024 },
                    TilePoint { x: 1024, y: 3072 },
                    TilePoint { x: 3072, y: 3072 },
                    TilePoint { x: 3072, y: 1024 },
                ],
            ],
            properties: HashMap::new(),
        }
    }

    fn building_feature() -> DecodedFeature {
        DecodedFeature {
            id: Some(3),
            geometry_type: GeometryType::Polygon,
            paths: vec![vec![
                TilePoint { x: 100, y: 100 },
                TilePoint { x: 700, y: 100 },
                TilePoint { x: 700, y: 700 },
                TilePoint { x: 100, y: 700 },
                TilePoint { x: 100, y: 100 },
            ]],
            properties: HashMap::from([("height".to_owned(), FeatureValue::F64(20.0))]),
        }
    }

    fn building_layer(feature: DecodedFeature) -> DecodedLayer {
        DecodedLayer {
            name: "building".to_owned(),
            extent: 4096,
            features: vec![feature],
        }
    }

    fn center_tile_key(z: u8) -> TileKey {
        let world = geo_to_world(GeoPosition::lon_lat(0.0, 0.0), f64::from(z));
        TileKey {
            z,
            x: (world.0 / TILE_SIZE).floor() as u32,
            y: (world.1 / TILE_SIZE).floor() as u32,
        }
    }

    #[test]
    fn mvt_area_fills_lower_to_mara_mesh_commands() {
        let feature = polygon_feature();
        let layer = test_layer(polygon_feature());

        let cmds = area_fill_paint_cmds(
            test_rect(),
            test_viewport(),
            test_tile_key(),
            &layer,
            &feature,
            Some(egui::Color32::from_rgb(10, 20, 30)),
            Some(egui::Color32::from_rgb(1, 2, 3)),
        );

        assert_eq!(cmds.len(), 1);
        assert!(matches!(
            &cmds[0],
            PaintCmd::Mesh { vertices, indices }
                if vertices.len() >= 3
                    && indices.len() >= 3
                    && vertices.iter().all(|vertex| vertex.color == MaraColor32::from_rgb(10, 20, 30))
        ));
    }

    #[test]
    fn mvt_area_fills_use_opposite_winding_rings_as_holes() {
        let feature = polygon_with_hole_feature();
        let layer = test_layer(polygon_with_hole_feature());
        let fill = egui::Color32::from_rgb(10, 20, 30);
        let hole_fill = egui::Color32::from_rgb(1, 2, 3);

        let cmds = area_fill_paint_cmds(
            test_rect(),
            test_viewport(),
            test_tile_key(),
            &layer,
            &feature,
            Some(fill),
            Some(hole_fill),
        );

        assert_eq!(cmds.len(), 2);
        assert!(matches!(
            &cmds[0],
            PaintCmd::Mesh { vertices, .. }
                if vertices.iter().all(|vertex| vertex.color == MaraColor32::from(fill))
        ));
        assert!(matches!(
            &cmds[1],
            PaintCmd::Mesh { vertices, .. }
                if vertices.iter().all(|vertex| vertex.color == MaraColor32::from(hole_fill))
        ));
    }

    #[test]
    fn mvt_building_extrusions_lower_to_side_roof_and_outline_commands() {
        let feature = building_feature();
        let layer = building_layer(building_feature());
        let viewport = MapViewport::new(GeoPosition::lon_lat(0.0, 0.0), 18.0);
        let palette = MapPalette::current();

        let cmds = building_extrusion_paint_cmds(
            test_rect(),
            viewport,
            center_tile_key(17),
            &layer,
            &feature,
            &palette,
        );

        assert!(cmds.iter().any(|cmd| matches!(
            cmd,
            PaintCmd::Mesh { vertices, indices }
                if vertices.len() >= 4
                    && indices.len() >= 6
                    && vertices.iter().all(|vertex| vertex.color == MaraColor32::from(palette.building_side))
        )));
        assert!(cmds.iter().any(|cmd| matches!(
            cmd,
            PaintCmd::Mesh { vertices, indices }
                if vertices.len() >= 3
                    && indices.len() >= 3
                    && vertices.iter().all(|vertex| vertex.color == MaraColor32::from(palette.building))
        )));
        assert!(cmds.iter().any(|cmd| matches!(
            cmd,
            PaintCmd::Line { stroke, .. } if stroke.width == 0.65
        )));
    }

    #[test]
    fn mvt_feature_lines_lower_to_mara_polyline_commands() {
        let feature = line_feature();
        let layer = test_layer(line_feature());

        let cmds = feature_line_paint_cmds(
            test_rect(),
            test_viewport(),
            test_tile_key(),
            &layer,
            &feature,
            egui::Stroke::new(3.0, egui::Color32::from_rgb(40, 50, 60)),
        );

        assert_eq!(cmds.len(), 1);
        assert!(matches!(
            &cmds[0],
            PaintCmd::Polyline { points, stroke }
                if points.len() == 2
                    && *stroke == MaraStroke::new(3.0, MaraColor32::from_rgb(40, 50, 60))
        ));
    }

    #[test]
    fn mvt_labels_lower_to_mara_text_commands() {
        let style = LabelStyle {
            size: 12.0,
            color: egui::Color32::from_rgb(70, 80, 90),
            halo: egui::Color32::from_rgb(1, 2, 3),
        };
        let mut labels = LabelState::default();

        let cmds = label_paint_cmds(
            "place",
            MaraPos2::new(128.0, 128.0),
            "AMSTERDAM".to_owned(),
            &style,
            MaraVec2::new(80.0, 16.0),
            test_rect().into(),
            &mut labels,
        );

        assert_eq!(cmds.len(), 2);
        assert_eq!(labels.occupied.len(), 1);
        assert!(matches!(
            &cmds[0],
            PaintCmd::Text {
                pos,
                anchor,
                text,
                size,
                color,
                mono,
            } if *pos == egui::pos2(129.0, 129.0).into()
                && *anchor == MaraAlign2::CENTER_CENTER
                && text == "AMSTERDAM"
                && *size == 12.0
                && *color == MaraColor32::from_rgb(1, 2, 3)
                && !mono
        ));
        assert!(matches!(
            &cmds[1],
            PaintCmd::Text {
                pos,
                anchor,
                text,
                size,
                color,
                mono,
            } if *pos == egui::pos2(128.0, 128.0).into()
                && *anchor == MaraAlign2::CENTER_CENTER
                && text == "AMSTERDAM"
                && *size == 12.0
                && *color == MaraColor32::from_rgb(70, 80, 90)
                && !mono
        ));
    }

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
