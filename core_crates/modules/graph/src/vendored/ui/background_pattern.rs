use egui::{Painter, Rect, Style, Vec2, emath::Rot2, vec2};

use super::GraphStyle;

///Grid background pattern.
///Use `GraphStyle::background_pattern_stroke` for change stroke options
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(all(feature = "egui-probe", any()), derive(egui_probe::EguiProbe))]
pub struct Grid {
    /// Spacing between grid lines.
    pub spacing: Vec2,

    /// Angle of the grid.
    #[cfg_attr(all(feature = "egui-probe", any()), egui_probe(as egui_probe::angle))]
    pub angle: f32,
}

const DEFAULT_GRID_SPACING: Vec2 = vec2(50.0, 50.0);
macro_rules! default_grid_spacing {
    () => {
        stringify!(vec2(50.0, 50.0))
    };
}

const DEFAULT_GRID_ANGLE: f32 = 1.0;
macro_rules! default_grid_angle {
    () => {
        stringify!(1.0)
    };
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            spacing: DEFAULT_GRID_SPACING,
            angle: DEFAULT_GRID_ANGLE,
        }
    }
}

impl Grid {
    /// Create new grid with given spacing and angle.
    #[must_use]
    pub const fn new(spacing: Vec2, angle: f32) -> Self {
        Self { spacing, angle }
    }

    fn draw(&self, viewport: &Rect, graph_style: &GraphStyle, style: &Style, painter: &Painter) {
        let bg_stroke = graph_style.get_bg_pattern_stroke(style);

        let spacing = vec2(self.spacing.x.max(1.0), self.spacing.y.max(1.0));

        let rot = Rot2::from_angle(self.angle);
        let rot_inv = rot.inverse();

        let pattern_bounds = viewport.rotate_bb(rot_inv);

        let min_x = (pattern_bounds.min.x / spacing.x).ceil();
        let max_x = (pattern_bounds.max.x / spacing.x).floor();

        #[allow(clippy::cast_possible_truncation)]
        for x in 0..=f32::ceil(max_x - min_x) as i64 {
            #[allow(clippy::cast_precision_loss)]
            let x = (x as f32 + min_x) * spacing.x;

            let top = (rot * vec2(x, pattern_bounds.min.y)).to_pos2();
            let bottom = (rot * vec2(x, pattern_bounds.max.y)).to_pos2();

            painter.line_segment([top, bottom], bg_stroke);
        }

        let min_y = (pattern_bounds.min.y / spacing.y).ceil();
        let max_y = (pattern_bounds.max.y / spacing.y).floor();

        #[allow(clippy::cast_possible_truncation)]
        for y in 0..=f32::ceil(max_y - min_y) as i64 {
            #[allow(clippy::cast_precision_loss)]
            let y = (y as f32 + min_y) * spacing.y;

            let top = (rot * vec2(pattern_bounds.min.x, y)).to_pos2();
            let bottom = (rot * vec2(pattern_bounds.max.x, y)).to_pos2();

            painter.line_segment([top, bottom], bg_stroke);
        }
    }
}

/// Dot grid background pattern — small filled circles at each
/// grid intersection. Matches Blender's node-editor dot grid.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(all(feature = "egui-probe", any()), derive(egui_probe::EguiProbe))]
pub struct Dots {
    /// Spacing between dots.
    pub spacing: Vec2,
    /// Dot radius in points.
    pub radius: f32,
}

impl Default for Dots {
    fn default() -> Self {
        Self {
            spacing: DEFAULT_GRID_SPACING,
            radius: 1.0,
        }
    }
}

impl Dots {
    /// Create new dot pattern with given spacing and radius.
    #[must_use]
    pub const fn new(spacing: Vec2, radius: f32) -> Self {
        Self { spacing, radius }
    }

    fn draw(&self, viewport: &Rect, graph_style: &GraphStyle, style: &Style, painter: &Painter) {
        let stroke = graph_style.get_bg_pattern_stroke(style);
        let fill = stroke.color;

        let spacing = vec2(self.spacing.x.max(1.0), self.spacing.y.max(1.0));

        let min_x = (viewport.min.x / spacing.x).ceil();
        let max_x = (viewport.max.x / spacing.x).floor();
        let min_y = (viewport.min.y / spacing.y).ceil();
        let max_y = (viewport.max.y / spacing.y).floor();

        #[allow(clippy::cast_possible_truncation)]
        let nx = f32::ceil(max_x - min_x) as i64;
        #[allow(clippy::cast_possible_truncation)]
        let ny = f32::ceil(max_y - min_y) as i64;

        for ix in 0..=nx {
            #[allow(clippy::cast_precision_loss)]
            let x = (ix as f32 + min_x) * spacing.x;
            for iy in 0..=ny {
                #[allow(clippy::cast_precision_loss)]
                let y = (iy as f32 + min_y) * spacing.y;
                painter.circle_filled(egui::pos2(x, y), self.radius, fill);
            }
        }
    }
}

/// Pointy-top hex grid — flat hex tiles tessellated across the
/// canvas. Each hex is drawn as a 6-segment polygonal outline
/// using the active background-pattern stroke. `size` is the
/// circumradius (centre→vertex distance). The sci-fi HUD motif
/// — Halo CE waypoint, Stellaris galaxy map, NMS scanner.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(all(feature = "egui-probe", any()), derive(egui_probe::EguiProbe))]
pub struct Hex {
    /// Hex circumradius in points (centre → vertex).
    pub size: f32,
}

impl Default for Hex {
    fn default() -> Self {
        Self { size: 24.0 }
    }
}

impl Hex {
    /// Create a hex pattern with the given circumradius.
    #[must_use]
    pub const fn new(size: f32) -> Self {
        Self { size }
    }

    fn draw(&self, viewport: &Rect, graph_style: &GraphStyle, style: &Style, painter: &Painter) {
        let stroke = graph_style.get_bg_pattern_stroke(style);
        let s = self.size.max(2.0);
        // Pointy-top hex: row pitch = 1.5 * s, col pitch =
        // sqrt(3) * s, odd rows offset by half-col.
        let row_pitch = 1.5 * s;
        // sqrt(3) — not available as a stable f32 constant.
        const SQRT_3: f32 = 1.732_050_8;
        let col_pitch = SQRT_3 * s;
        // Pad by one cell so partial hexes at the viewport edge
        // still draw without popping.
        let min_row = ((viewport.min.y / row_pitch).floor() as i64) - 1;
        let max_row = ((viewport.max.y / row_pitch).ceil() as i64) + 1;
        let min_col = ((viewport.min.x / col_pitch).floor() as i64) - 1;
        let max_col = ((viewport.max.x / col_pitch).ceil() as i64) + 1;
        // Pre-compute 6 unit vertex offsets for a pointy-top hex
        // (vertex 0 at the top, going clockwise).
        let verts: [Vec2; 6] = [
            vec2(0.0, -s),
            vec2(col_pitch * 0.5, -s * 0.5),
            vec2(col_pitch * 0.5, s * 0.5),
            vec2(0.0, s),
            vec2(-col_pitch * 0.5, s * 0.5),
            vec2(-col_pitch * 0.5, -s * 0.5),
        ];
        for row in min_row..=max_row {
            let cy = row as f32 * row_pitch;
            let row_offset = if row.rem_euclid(2) == 0 {
                0.0
            } else {
                col_pitch * 0.5
            };
            for col in min_col..=max_col {
                let cx = col as f32 * col_pitch + row_offset;
                let centre = egui::pos2(cx, cy);
                // Draw 6 segments forming the hex outline.
                for i in 0..6 {
                    let a = centre + verts[i];
                    let b = centre + verts[(i + 1) % 6];
                    painter.line_segment([a, b], stroke);
                }
            }
        }
    }
}

/// Background pattern show beneath nodes and wires.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(all(feature = "egui-probe", any()), derive(egui_probe::EguiProbe))]
pub enum BackgroundPattern {
    /// No pattern.
    NoPattern,

    /// Linear grid.
    #[cfg_attr(all(feature = "egui-probe", any()), egui_probe(transparent))]
    Grid(Grid),

    /// Dot grid (Blender-style) — a filled circle at each
    /// intersection of an invisible grid.
    #[cfg_attr(all(feature = "egui-probe", any()), egui_probe(transparent))]
    Dots(Dots),

    /// Pointy-top hex tiles — sci-fi HUD motif.
    #[cfg_attr(all(feature = "egui-probe", any()), egui_probe(transparent))]
    Hex(Hex),
}

impl Default for BackgroundPattern {
    fn default() -> Self {
        BackgroundPattern::new()
    }
}

impl BackgroundPattern {
    /// Create new background pattern with default values.
    ///
    /// Default patter is `Grid` with spacing - `
    #[doc = default_grid_spacing!()]
    /// ` and angle - `
    #[doc = default_grid_angle!()]
    /// ` radian.
    #[must_use]
    pub const fn new() -> Self {
        Self::Grid(Grid::new(DEFAULT_GRID_SPACING, DEFAULT_GRID_ANGLE))
    }

    /// Create new grid background pattern with given spacing and angle.
    #[must_use]
    pub const fn grid(spacing: Vec2, angle: f32) -> Self {
        Self::Grid(Grid::new(spacing, angle))
    }

    /// Draws background pattern.
    pub fn draw(
        &self,
        viewport: &Rect,
        graph_style: &GraphStyle,
        style: &Style,
        painter: &Painter,
    ) {
        match self {
            BackgroundPattern::Grid(g) => g.draw(viewport, graph_style, style, painter),
            BackgroundPattern::Dots(d) => d.draw(viewport, graph_style, style, painter),
            BackgroundPattern::Hex(h) => h.draw(viewport, graph_style, style, painter),
            BackgroundPattern::NoPattern => {}
        }
    }
}
