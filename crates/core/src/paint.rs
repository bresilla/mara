//! Backend-neutral paint command vocabulary.
//!
//! This is the first slice of the future Mara paint IR. The current
//! egui backend renders these commands immediately, but app-facing
//! drawing code can now be described in Mara-owned data instead of
//! treating `egui::Painter` calls as the semantic model.

use crate::vocab::{Align2, Color32, CornerRadius, Pos2, Rect, Stroke, TextureId, Vec2};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextFamily {
    Proportional,
    Monospace,
    Named(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaintVertex {
    pub pos: Pos2,
    pub color: Color32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub size: f32,
    pub color: Color32,
    pub family: TextFamily,
    pub extra_letter_spacing: f32,
    pub leading_space: f32,
}

#[derive(Clone, Debug)]
pub enum PaintCmd {
    /// Paints nothing. Used as the inert placeholder a reserved paint
    /// slot ([`crate::layout::UiBackend::reserve_paint_slot`]) holds
    /// until it is filled — or keeps, when a slot is filled with no
    /// command.
    Noop,
    Line {
        a: Pos2,
        b: Pos2,
        stroke: Stroke,
    },
    Polyline {
        points: Vec<Pos2>,
        stroke: Stroke,
    },
    Polygon {
        points: Vec<Pos2>,
        fill: Color32,
        stroke: Stroke,
    },
    RectFilled {
        rect: Rect,
        corner: CornerRadius,
        fill: Color32,
    },
    RectStroke {
        rect: Rect,
        corner: CornerRadius,
        stroke: Stroke,
    },
    RectStrokeOutside {
        rect: Rect,
        corner: CornerRadius,
        stroke: Stroke,
    },
    CircleFilled {
        center: Pos2,
        radius: f32,
        fill: Color32,
    },
    CircleStroke {
        center: Pos2,
        radius: f32,
        stroke: Stroke,
    },
    /// Axis-aligned ellipse bounded by `rect` (filled and/or stroked).
    Ellipse {
        rect: Rect,
        fill: Color32,
        stroke: Stroke,
    },
    /// Open elliptical arc — a curved line, no fill. Angles are in
    /// radians, `0` at the +x axis, increasing clockwise (screen y-down).
    Arc {
        center: Pos2,
        radius: Vec2,
        start_angle: f32,
        end_angle: f32,
        stroke: Stroke,
    },
    /// Filled circular/elliptical sector — a pie wedge from `center`
    /// across the arc. Angles as in [`PaintCmd::Arc`].
    Sector {
        center: Pos2,
        radius: Vec2,
        start_angle: f32,
        end_angle: f32,
        fill: Color32,
        stroke: Stroke,
    },
    Arrow {
        origin: Pos2,
        vec: Vec2,
        stroke: Stroke,
    },
    Text {
        pos: Pos2,
        anchor: Align2,
        text: String,
        size: f32,
        color: Color32,
        mono: bool,
    },
    TextWithFamily {
        pos: Pos2,
        anchor: Align2,
        text: String,
        size: f32,
        color: Color32,
        family: TextFamily,
    },
    TextRuns {
        pos: Pos2,
        anchor: Align2,
        angle: f32,
        runs: Vec<TextRun>,
    },
    Image {
        texture: TextureId,
        rect: Rect,
        uv: Rect,
        tint: Color32,
    },
    Svg {
        svg: String,
        rect: Rect,
        tint: Color32,
    },
    Mesh {
        vertices: Vec<PaintVertex>,
        indices: Vec<u32>,
    },
    Shadow {
        rect: Rect,
        corner: CornerRadius,
        offset: [i8; 2],
        blur: u8,
        spread: u8,
        color: Color32,
    },
    Clip {
        rect: Rect,
        children: Vec<PaintCmd>,
    },
    /// Several commands treated as one.
    ///
    /// Unlike [`PaintCmd::Clip`] this adds no clipping — it exists so a
    /// batch can occupy a single reserved paint slot
    /// ([`crate::layout::UiBackend::fill_paint_slot`]), which takes one
    /// command. A node renderer uses it to drop a frame's worth of
    /// wires in behind the nodes.
    Group(Vec<PaintCmd>),
}

/// Retained paint-command buffer for tests and future non-egui
/// renderers.
#[derive(Clone, Debug, Default)]
pub struct PaintList {
    commands: Vec<PaintCmd>,
}

impl PaintList {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, cmd: PaintCmd) {
        self.commands.push(cmd);
    }

    pub fn extend(&mut self, commands: impl IntoIterator<Item = PaintCmd>) {
        self.commands.extend(commands);
    }

    #[must_use]
    pub fn commands(&self) -> &[PaintCmd] {
        &self.commands
    }

    #[must_use]
    pub fn into_commands(self) -> Vec<PaintCmd> {
        self.commands
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }
}

/// Internal egui adapter for first-party crates that have already
/// lowered drawing semantics into Mara [`PaintCmd`] values but still
/// need to render through the current egui backend.
///
/// This is not app-facing API; future backends should consume
/// `PaintCmd` directly through their own renderer.
#[cfg(feature = "backend-egui-conv")]
#[doc(hidden)]
pub fn __internal_render_paint_cmd_egui(painter: &egui::Painter, cmd: PaintCmd) {
    crate::backend::egui::render_paint_cmd(painter, cmd);
}

#[cfg(feature = "backend-egui-conv")]
#[doc(hidden)]
pub fn __internal_render_paint_cmd_egui_ui(ui: &mut egui::Ui, cmd: PaintCmd) {
    crate::backend::egui::render_paint_cmd_ui(ui, cmd);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_list_retains_commands_for_backend_independent_inspection() {
        let mut list = PaintList::new();
        list.push(PaintCmd::Line {
            a: Pos2::new(0.0, 1.0),
            b: Pos2::new(2.0, 3.0),
            stroke: Stroke::new(2.0, Color32::WHITE),
        });

        let [PaintCmd::Line { a, b, stroke }] = list.commands() else {
            panic!("expected one retained line command");
        };

        assert_eq!(*a, Pos2::new(0.0, 1.0));
        assert_eq!(*b, Pos2::new(2.0, 3.0));
        assert_eq!(*stroke, Stroke::new(2.0, Color32::WHITE));
    }

    #[test]
    fn paint_list_retains_clip_and_text_commands() {
        let clip_rect = Rect::from_min_size(Pos2::new(1.0, 2.0), Vec2::new(30.0, 10.0));
        let text_pos = Pos2::new(4.0, 7.0);
        let mut list = PaintList::new();

        list.push(PaintCmd::Clip {
            rect: clip_rect,
            children: vec![PaintCmd::Text {
                pos: text_pos,
                anchor: Align2::LEFT_CENTER,
                text: "clipped".to_owned(),
                size: 12.0,
                color: Color32::WHITE,
                mono: false,
            }],
        });

        let [PaintCmd::Clip { rect, children }] = list.commands() else {
            panic!("expected one retained clip command");
        };
        assert_eq!(*rect, clip_rect);
        let [
            PaintCmd::Text {
                pos,
                anchor,
                text,
                size,
                color,
                mono,
            },
        ] = children.as_slice()
        else {
            panic!("clip should retain its child text command");
        };
        assert_eq!(*pos, text_pos);
        assert_eq!(*anchor, Align2::LEFT_CENTER);
        assert_eq!(text, "clipped");
        assert_eq!(*size, 12.0);
        assert_eq!(*color, Color32::WHITE);
        assert!(!mono);
    }

    #[test]
    fn paint_list_retains_rich_text_runs() {
        let mut list = PaintList::new();

        list.push(PaintCmd::TextRuns {
            pos: Pos2::new(10.0, 12.0),
            anchor: Align2::LEFT_CENTER,
            angle: std::f32::consts::FRAC_PI_2,
            runs: vec![
                TextRun {
                    text: "[ ".to_owned(),
                    size: 13.0,
                    color: Color32::TRANSPARENT,
                    family: TextFamily::Proportional,
                    extra_letter_spacing: 1.5,
                    leading_space: 0.0,
                },
                TextRun {
                    text: "TITLE".to_owned(),
                    size: 13.0,
                    color: Color32::WHITE,
                    family: TextFamily::Named("MaraTitle".to_owned()),
                    extra_letter_spacing: 1.5,
                    leading_space: 3.0,
                },
            ],
        });

        let [
            PaintCmd::TextRuns {
                pos,
                anchor,
                angle,
                runs,
            },
        ] = list.commands()
        else {
            panic!("expected retained rich text command");
        };

        assert_eq!(*pos, Pos2::new(10.0, 12.0));
        assert_eq!(*anchor, Align2::LEFT_CENTER);
        assert_eq!(*angle, std::f32::consts::FRAC_PI_2);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "[ ");
        assert_eq!(runs[0].color, Color32::TRANSPARENT);
        assert_eq!(runs[1].text, "TITLE");
        assert_eq!(runs[1].leading_space, 3.0);
    }

    #[test]
    fn paint_list_retains_image_commands_and_clear_semantics() {
        let mut list = PaintList::new();
        let texture = TextureId::from(egui::TextureId::User(7));
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(64.0, 32.0));
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));

        list.extend([PaintCmd::Image {
            texture,
            rect,
            uv,
            tint: Color32::WHITE,
        }]);

        let [
            PaintCmd::Image {
                texture: retained_texture,
                rect: retained_rect,
                uv: retained_uv,
                tint,
            },
        ] = list.commands()
        else {
            panic!("expected one retained image command");
        };
        assert_eq!(
            egui::TextureId::from(*retained_texture),
            egui::TextureId::User(7)
        );
        assert_eq!(*retained_rect, rect);
        assert_eq!(*retained_uv, uv);
        assert_eq!(*tint, Color32::WHITE);

        list.clear();
        assert!(list.commands().is_empty());
    }

    #[test]
    fn paint_list_retains_svg_commands() {
        let mut list = PaintList::new();
        let rect = Rect::from_min_size(Pos2::new(2.0, 3.0), Vec2::new(24.0, 24.0));

        list.push(PaintCmd::Svg {
            svg: "<svg/>".to_owned(),
            rect,
            tint: Color32::WHITE,
        });

        let [PaintCmd::Svg { svg, rect: r, tint }] = list.commands() else {
            panic!("expected one retained svg command");
        };

        assert_eq!(svg, "<svg/>");
        assert_eq!(*r, rect);
        assert_eq!(*tint, Color32::WHITE);
    }

    #[test]
    fn paint_list_retains_mesh_commands() {
        let mut list = PaintList::new();
        list.push(PaintCmd::Mesh {
            vertices: vec![
                PaintVertex {
                    pos: Pos2::new(0.0, 0.0),
                    color: Color32::WHITE,
                },
                PaintVertex {
                    pos: Pos2::new(1.0, 0.0),
                    color: Color32::BLACK,
                },
                PaintVertex {
                    pos: Pos2::new(0.0, 1.0),
                    color: Color32::GRAY,
                },
            ],
            indices: vec![0, 1, 2],
        });

        let [PaintCmd::Mesh { vertices, indices }] = list.commands() else {
            panic!("expected one retained mesh command");
        };

        assert_eq!(vertices.len(), 3);
        assert_eq!(vertices[0].pos, Pos2::new(0.0, 0.0));
        assert_eq!(vertices[1].color, Color32::BLACK);
        assert_eq!(indices, &[0, 1, 2]);
    }
}
