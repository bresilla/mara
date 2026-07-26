//! # Vocabulary types — Mara-owned public UI data
//!
//! Mara's public API uses its own inert data vocabulary instead of
//! re-exporting egui types. Holding a [`Color32`], [`Rect`], [`Id`],
//! or [`Stroke`] gives app code no access to backend capabilities
//! such as `Ui`, `Context`, `Painter`, or `Response`.
//!
//! The egui backend converts these types at the boundary. Egui stays
//! the reference backend, but it is no longer the public vocabulary.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
    pub const X: Self = Self { x: 1.0, y: 0.0 };
    pub const Y: Self = Self { x: 0.0, y: 1.0 };
    /// Unbounded in both axes — the "no maximum" a size constraint uses
    /// to mean "take whatever you need".
    pub const INFINITY: Self = Self {
        x: f32::INFINITY,
        y: f32::INFINITY,
    };

    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }

    #[must_use]
    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }

    #[must_use]
    pub fn length_sq(self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    /// This offset read as a position from the origin.
    #[must_use]
    pub const fn to_pos2(self) -> Pos2 {
        Pos2::new(self.x, self.y)
    }

    /// Linear interpolation towards `other`, unclamped.
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }

    /// Per-axis maximum. Element-wise, not "the longer vector" — a size
    /// constraint clamps width and height independently.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self::new(self.x.max(other.x), self.y.max(other.y))
    }

    /// Per-axis minimum.
    #[must_use]
    pub fn min(self, other: Self) -> Self {
        Self::new(self.x.min(other.x), self.y.min(other.y))
    }

    /// The larger of the two axes.
    #[must_use]
    pub fn max_elem(self) -> f32 {
        self.x.max(self.y)
    }

    /// The smaller of the two axes.
    #[must_use]
    pub fn min_elem(self) -> f32 {
        self.x.min(self.y)
    }

    /// Unit vector in the same direction, or [`Vec2::ZERO`] when this
    /// vector has no length (rather than producing NaNs).
    #[must_use]
    pub fn normalized(self) -> Self {
        let length = self.length();
        if length > 0.0 {
            Self::new(self.x / length, self.y / length)
        } else {
            Self::ZERO
        }
    }
}

impl std::ops::Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl std::ops::Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

/// Scalar-first multiply, so `2.0 * v` reads as naturally as `v * 2.0`.
impl std::ops::Mul<Vec2> for f32 {
    type Output = Vec2;
    fn mul(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self * rhs.x, self * rhs.y)
    }
}

impl std::ops::Div<f32> for Vec2 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self::new(self.x / rhs, self.y / rhs)
    }
}

impl std::ops::Neg for Vec2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

impl std::ops::AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl std::ops::SubAssign for Vec2 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl From<egui::Vec2> for Vec2 {
    fn from(v: egui::Vec2) -> Self {
        Self { x: v.x, y: v.y }
    }
}

impl From<Vec2> for egui::Vec2 {
    fn from(v: Vec2) -> Self {
        egui::vec2(v.x, v.y)
    }
}

#[must_use]
pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pos2 {
    pub x: f32,
    pub y: f32,
}

impl Pos2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn distance(self, other: Self) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }

    #[must_use]
    pub fn distance_sq(self, other: Self) -> f32 {
        (self.x - other.x).powi(2) + (self.y - other.y).powi(2)
    }

    /// This position read as an offset from the origin.
    #[must_use]
    pub const fn to_vec2(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }

    /// Linear interpolation towards `other`. `t` is not clamped, so
    /// values outside `0..=1` extrapolate — which curve maths relies on.
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }
}

impl std::ops::Sub for Pos2 {
    type Output = Vec2;
    fn sub(self, rhs: Self) -> Vec2 {
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl std::ops::Add<Vec2> for Pos2 {
    type Output = Self;
    fn add(self, rhs: Vec2) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl std::ops::Sub<Vec2> for Pos2 {
    type Output = Self;
    fn sub(self, rhs: Vec2) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl std::ops::AddAssign<Vec2> for Pos2 {
    fn add_assign(&mut self, rhs: Vec2) {
        *self = *self + rhs;
    }
}

impl std::ops::SubAssign<Vec2> for Pos2 {
    fn sub_assign(&mut self, rhs: Vec2) {
        *self = *self - rhs;
    }
}

impl From<egui::Pos2> for Pos2 {
    fn from(p: egui::Pos2) -> Self {
        Self { x: p.x, y: p.y }
    }
}

impl From<Pos2> for egui::Pos2 {
    fn from(p: Pos2) -> Self {
        egui::pos2(p.x, p.y)
    }
}

#[must_use]
pub const fn pos2(x: f32, y: f32) -> Pos2 {
    Pos2::new(x, y)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: Pos2,
    pub max: Pos2,
}

impl Rect {
    /// Unbounded in every direction — the "no constraint" a surface
    /// passes when the parent imposes no limit.
    pub const EVERYTHING: Self = Self {
        min: Pos2 {
            x: f32::NEG_INFINITY,
            y: f32::NEG_INFINITY,
        },
        max: Pos2 {
            x: f32::INFINITY,
            y: f32::INFINITY,
        },
    };

    /// All corners NaN — a sentinel for "not measured yet", distinct
    /// from [`Rect::NOTHING`]'s "empty but valid". Anything derived
    /// from it stays NaN, so a rect that was never written cannot
    /// quietly pass for one at the origin.
    pub const NAN: Self = Self {
        min: Pos2 {
            x: f32::NAN,
            y: f32::NAN,
        },
        max: Pos2 {
            x: f32::NAN,
            y: f32::NAN,
        },
    };

    pub const NOTHING: Self = Self {
        min: Pos2 {
            x: f32::INFINITY,
            y: f32::INFINITY,
        },
        max: Pos2 {
            x: f32::NEG_INFINITY,
            y: f32::NEG_INFINITY,
        },
    };

    #[must_use]
    pub const fn from_min_max(min: Pos2, max: Pos2) -> Self {
        Self { min, max }
    }

    #[must_use]
    pub fn from_min_size(min: Pos2, size: Vec2) -> Self {
        Self {
            min,
            max: Pos2::new(min.x + size.x, min.y + size.y),
        }
    }

    #[must_use]
    pub fn from_center_size(center: Pos2, size: Vec2) -> Self {
        let half = Vec2::new(size.x * 0.5, size.y * 0.5);
        Self {
            min: Pos2::new(center.x - half.x, center.y - half.y),
            max: Pos2::new(center.x + half.x, center.y + half.y),
        }
    }

    /// Smallest rect containing both positions, in either order.
    #[must_use]
    pub fn from_two_pos(a: Pos2, b: Pos2) -> Self {
        Self {
            min: Pos2::new(a.x.min(b.x), a.y.min(b.y)),
            max: Pos2::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    /// Smallest rect containing every point. Empty input gives
    /// [`Rect::NOTHING`], which intersects nothing.
    #[must_use]
    pub fn from_points(points: &[Pos2]) -> Self {
        points.iter().fold(Self::NOTHING, |rect, p| Self {
            min: Pos2::new(rect.min.x.min(p.x), rect.min.y.min(p.y)),
            max: Pos2::new(rect.max.x.max(p.x), rect.max.y.max(p.y)),
        })
    }

    #[must_use]
    pub fn left(&self) -> f32 {
        self.min.x
    }

    #[must_use]
    pub fn right(&self) -> f32 {
        self.max.x
    }

    #[must_use]
    pub fn top(&self) -> f32 {
        self.min.y
    }

    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.max.y
    }

    #[must_use]
    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    #[must_use]
    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    #[must_use]
    pub fn size(&self) -> Vec2 {
        Vec2::new(self.width(), self.height())
    }

    #[must_use]
    pub fn center(&self) -> Pos2 {
        Pos2::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
        )
    }

    #[must_use]
    pub fn left_center(&self) -> Pos2 {
        Pos2::new(self.left(), self.center().y)
    }

    #[must_use]
    pub fn right_center(&self) -> Pos2 {
        Pos2::new(self.right(), self.center().y)
    }

    /// Everything at or below `y`, unbounded horizontally — the room a
    /// surface has left once earlier content has been placed.
    #[must_use]
    pub const fn everything_below(y: f32) -> Self {
        Self {
            min: Pos2 {
                x: f32::NEG_INFINITY,
                y,
            },
            max: Pos2 {
                x: f32::INFINITY,
                y: f32::INFINITY,
            },
        }
    }

    /// `true` when every corner is a real number.
    ///
    /// A rect accumulated from a bounding box starts at
    /// [`Rect::NOTHING`] (infinite), so callers test this before using
    /// one as geometry — "did anything actually go into it?".
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.min.x.is_finite()
            && self.min.y.is_finite()
            && self.max.x.is_finite()
            && self.max.y.is_finite()
    }

    pub fn contains(&self, pos: Pos2) -> bool {
        pos.x >= self.left()
            && pos.x <= self.right()
            && pos.y >= self.top()
            && pos.y <= self.bottom()
    }

    #[must_use]
    pub fn intersects(&self, other: Self) -> bool {
        self.min.x <= other.max.x
            && other.min.x <= self.max.x
            && self.min.y <= other.max.y
            && other.min.y <= self.max.y
    }

    #[must_use]
    pub fn intersect(self, other: Self) -> Self {
        Self {
            min: Pos2::new(self.min.x.max(other.min.x), self.min.y.max(other.min.y)),
            max: Pos2::new(self.max.x.min(other.max.x), self.max.y.min(other.max.y)),
        }
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            min: Pos2::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Pos2::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }

    #[must_use]
    pub fn translate(self, offset: Vec2) -> Self {
        Self {
            min: Pos2::new(self.min.x + offset.x, self.min.y + offset.y),
            max: Pos2::new(self.max.x + offset.x, self.max.y + offset.y),
        }
    }

    #[must_use]
    pub fn expand(self, amount: f32) -> Self {
        Self {
            min: Pos2::new(self.min.x - amount, self.min.y - amount),
            max: Pos2::new(self.max.x + amount, self.max.y + amount),
        }
    }

    #[must_use]
    pub fn shrink(self, amount: f32) -> Self {
        self.expand(-amount)
    }

    /// Grow by a margin — the rect a framed body occupies once the
    /// frame's spacing is added around it.
    ///
    /// Per-edge rather than uniform, because a margin need not be
    /// symmetric and collapsing it to one number silently misplaces
    /// anything anchored to an edge.
    #[must_use]
    pub fn expand_by(self, margin: crate::style::MarginSpec) -> Self {
        Self {
            min: Pos2::new(self.min.x - margin.leftf(), self.min.y - margin.topf()),
            max: Pos2::new(self.max.x + margin.rightf(), self.max.y + margin.bottomf()),
        }
    }

    /// Shrink by a margin — the content area inside a frame.
    #[must_use]
    pub fn shrink_by(self, margin: crate::style::MarginSpec) -> Self {
        Self {
            min: Pos2::new(self.min.x + margin.leftf(), self.min.y + margin.topf()),
            max: Pos2::new(self.max.x - margin.rightf(), self.max.y - margin.bottomf()),
        }
    }
}

impl Default for Rect {
    fn default() -> Self {
        Self::NOTHING
    }
}

impl From<egui::Rect> for Rect {
    fn from(r: egui::Rect) -> Self {
        Self {
            min: r.min.into(),
            max: r.max.into(),
        }
    }
}

impl From<Rect> for egui::Rect {
    fn from(r: Rect) -> Self {
        egui::Rect::from_min_max(r.min.into(), r.max.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Id(egui::Id);

impl Id {
    #[must_use]
    pub fn new(source: impl std::hash::Hash) -> Self {
        Self(egui::Id::new(source))
    }

    #[must_use]
    pub fn with(self, child: impl std::hash::Hash) -> Self {
        Self(self.0.with(child))
    }
}

impl std::borrow::Borrow<egui::Id> for Id {
    fn borrow(&self) -> &egui::Id {
        &self.0
    }
}

impl PartialEq<egui::Id> for Id {
    fn eq(&self, other: &egui::Id) -> bool {
        self.0 == *other
    }
}

impl PartialEq<Id> for egui::Id {
    fn eq(&self, other: &Id) -> bool {
        *self == other.0
    }
}

impl From<&str> for Id {
    fn from(source: &str) -> Self {
        Self::new(source)
    }
}

impl From<String> for Id {
    fn from(source: String) -> Self {
        Self::new(source)
    }
}

impl From<egui::Id> for Id {
    fn from(id: egui::Id) -> Self {
        Self(id)
    }
}

impl From<Id> for egui::Id {
    fn from(id: Id) -> Self {
        id.0
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Straight-alpha-in, premultiplied-stored sRGB colour — WS-E4 native
/// (see [`CornerRadius`]).
///
/// Stores premultiplied bytes, the same representation the renderer
/// consumes, so no conversion happens on the way to the GPU. The
/// premultiply and un-premultiply arithmetic mirrors the backend's
/// exactly — a test cross-checks every alpha value against it, because
/// a rounding difference here would tint every translucent surface.
pub struct Color32([u8; 4]);

/// Hex, the way a colour is actually read. Kept identical to the
/// backend's formatting so paint-stream goldens stay legible and did
/// not need regenerating when this type went native.
impl std::fmt::Debug for Color32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let [r, g, b, a] = self.0;
        f.debug_tuple("Color32")
            .field(&format_args!("#{r:02X}_{g:02X}_{b:02X}_{a:02X}"))
            .finish()
    }
}

/// `(x + 0.5) as u8`, saturating — the backend's rounding rule.
const fn round_u8(x: f32) -> u8 {
    (x + 0.5) as u8
}

impl Color32 {
    pub const TRANSPARENT: Self = Self([0, 0, 0, 0]);
    pub const BLACK: Self = Self([0, 0, 0, 255]);
    pub const WHITE: Self = Self([255, 255, 255, 255]);
    pub const GRAY: Self = Self([160, 160, 160, 255]);

    #[must_use]
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self([r, g, b, 255])
    }

    /// Straight (un-premultiplied) alpha in; stored premultiplied.
    #[must_use]
    pub const fn from_rgba_unmultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        match a {
            0 => Self::TRANSPARENT,
            255 => Self::from_rgb(r, g, b),
            a => {
                let factor = a as f32 / 255.0;
                Self([
                    round_u8(r as f32 * factor),
                    round_u8(g as f32 * factor),
                    round_u8(b as f32 * factor),
                    a,
                ])
            }
        }
    }

    #[must_use]
    pub const fn from_rgba_premultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self([r, g, b, a])
    }

    #[must_use]
    pub const fn from_gray(gray: u8) -> Self {
        Self([gray, gray, gray, 255])
    }

    #[must_use]
    pub const fn from_black_alpha(alpha: u8) -> Self {
        Self([0, 0, 0, alpha])
    }

    #[must_use]
    pub const fn r(self) -> u8 {
        self.0[0]
    }

    #[must_use]
    pub const fn g(self) -> u8 {
        self.0[1]
    }

    #[must_use]
    pub const fn b(self) -> u8 {
        self.0[2]
    }

    #[must_use]
    pub const fn a(self) -> u8 {
        self.0[3]
    }

    /// Back to straight alpha.
    #[must_use]
    pub fn to_srgba_unmultiplied(self) -> [u8; 4] {
        let [r, g, b, a] = self.0;
        match a {
            0 | 255 => self.0,
            a => {
                let factor = 255.0 / a as f32;
                [
                    round_u8(factor * r as f32),
                    round_u8(factor * g as f32),
                    round_u8(factor * b as f32),
                    a,
                ]
            }
        }
    }
}

#[cfg(feature = "backend-egui-conv")]
impl From<egui::Color32> for Color32 {
    fn from(c: egui::Color32) -> Self {
        Self(c.to_array())
    }
}

#[cfg(feature = "backend-egui-conv")]
impl From<Color32> for egui::Color32 {
    fn from(c: Color32) -> Self {
        let [r, g, b, a] = c.0;
        egui::Color32::from_rgba_premultiplied(r, g, b, a)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Stroke {
    pub width: f32,
    pub color: Color32,
}

/// No stroke — width zero, fully transparent.
///
/// Matches the backend's own default, so a struct deriving `Default`
/// with a `Stroke` field keeps meaning "draws no outline".
impl Default for Stroke {
    fn default() -> Self {
        Self::NONE
    }
}

impl Stroke {
    pub const NONE: Self = Self {
        width: 0.0,
        color: Color32::TRANSPARENT,
    };

    #[must_use]
    pub const fn new(width: f32, color: Color32) -> Self {
        Self { width, color }
    }
}

impl From<egui::Stroke> for Stroke {
    fn from(s: egui::Stroke) -> Self {
        Self {
            width: s.width,
            color: s.color.into(),
        }
    }
}

impl From<Stroke> for egui::Stroke {
    fn from(s: Stroke) -> Self {
        egui::Stroke::new(s.width, s.color)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureId(egui::TextureId);

impl From<egui::TextureId> for TextureId {
    fn from(id: egui::TextureId) -> Self {
        Self(id)
    }
}

impl From<TextureId> for egui::TextureId {
    fn from(id: TextureId) -> Self {
        id.0
    }
}

#[derive(Clone)]
pub struct TextureHandle(egui::TextureHandle);

impl TextureHandle {
    /// Id for painting this texture with
    /// [`crate::MaraPainter::image`].
    #[must_use]
    pub fn id(&self) -> TextureId {
        self.0.id().into()
    }

    /// Size in pixels.
    #[must_use]
    pub fn size(&self) -> [usize; 2] {
        self.0.size()
    }
}

impl From<egui::TextureHandle> for TextureHandle {
    fn from(handle: egui::TextureHandle) -> Self {
        Self(handle)
    }
}

impl From<TextureHandle> for egui::TextureHandle {
    fn from(handle: TextureHandle) -> Self {
        handle.0
    }
}

#[derive(Clone, Debug)]
pub struct ColorImage(pub(crate) egui::ColorImage);

impl ColorImage {
    /// Build an image from sRGB pixels in row-major order.
    ///
    /// Surfaces that generate imagery — noise previews, plots
    /// rasterised to a buffer — need this without naming a backend
    /// image type. `pixels.len()` must be `size[0] * size[1]`; a
    /// mismatch yields a 1×1 transparent image rather than panicking,
    /// because a malformed preview should not take the app down.
    #[must_use]
    pub fn from_rgba_pixels(size: [usize; 2], pixels: &[Color32]) -> Self {
        if pixels.len() != size[0] * size[1] || size[0] == 0 || size[1] == 0 {
            return Self(egui::ColorImage::filled([1, 1], egui::Color32::TRANSPARENT));
        }
        let pixels: Vec<egui::Color32> = pixels.iter().map(|c| (*c).into()).collect();
        Self(egui::ColorImage {
            size,
            pixels,
            source_size: egui::vec2(size[0] as f32, size[1] as f32),
        })
    }
}

impl From<egui::ColorImage> for ColorImage {
    fn from(image: egui::ColorImage) -> Self {
        Self(image)
    }
}

impl From<ColorImage> for egui::ColorImage {
    fn from(image: ColorImage) -> Self {
        image.0
    }
}

/// Which physical pointer button an interaction used.
///
/// Mara's own enum rather than the backend's, so drawing surfaces can
/// branch on middle-drag / right-click without naming egui. Extra
/// buttons some backends report (side/extra) collapse into nothing —
/// Mara exposes only the three every platform agrees on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

impl PointerButton {
    /// All three buttons, in the order [`crate::mui::MaraResponse`]
    /// stores its per-button flags.
    pub const ALL: [Self; 3] = [Self::Primary, Self::Secondary, Self::Middle];

    #[must_use]
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Primary => 0,
            Self::Secondary => 1,
            Self::Middle => 2,
        }
    }
}

/// Where a point sits along one axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Align {
    Min,
    Center,
    Max,
}

impl Align {
    /// Offset from `min` for a span of `size` — 0, half, or all of the
    /// slack.
    #[must_use]
    fn offset(self, size: f32) -> f32 {
        match self {
            Self::Min => 0.0,
            Self::Center => -size * 0.5,
            Self::Max => -size,
        }
    }
}

/// Two-axis alignment — WS-E4 native (see [`CornerRadius`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Align2 {
    pub x: Align,
    pub y: Align,
}

impl Align2 {
    pub const LEFT_TOP: Self = Self::new(Align::Min, Align::Min);
    pub const LEFT_CENTER: Self = Self::new(Align::Min, Align::Center);
    pub const LEFT_BOTTOM: Self = Self::new(Align::Min, Align::Max);
    pub const CENTER_TOP: Self = Self::new(Align::Center, Align::Min);
    pub const CENTER_CENTER: Self = Self::new(Align::Center, Align::Center);
    pub const CENTER_BOTTOM: Self = Self::new(Align::Center, Align::Max);
    pub const RIGHT_TOP: Self = Self::new(Align::Max, Align::Min);
    pub const RIGHT_CENTER: Self = Self::new(Align::Max, Align::Center);
    pub const RIGHT_BOTTOM: Self = Self::new(Align::Max, Align::Max);

    #[must_use]
    pub const fn new(x: Align, y: Align) -> Self {
        Self { x, y }
    }

    /// Place `rect` so that this alignment's anchor point sits at
    /// `rect.min`, keeping its size.
    #[must_use]
    pub fn anchor_rect(self, rect: Rect) -> Rect {
        let size = rect.size();
        let min = Pos2::new(
            rect.min.x + self.x.offset(size.x),
            rect.min.y + self.y.offset(size.y),
        );
        Rect::from_min_size(min, size)
    }
}

#[cfg(feature = "backend-egui-conv")]
impl From<egui::Align2> for Align2 {
    fn from(a: egui::Align2) -> Self {
        fn axis(a: egui::Align) -> Align {
            match a {
                egui::Align::Min => Align::Min,
                egui::Align::Center => Align::Center,
                egui::Align::Max => Align::Max,
            }
        }
        Self::new(axis(a.x()), axis(a.y()))
    }
}

#[cfg(feature = "backend-egui-conv")]
impl From<Align2> for egui::Align2 {
    fn from(a: Align2) -> Self {
        fn axis(a: Align) -> egui::Align {
            match a {
                Align::Min => egui::Align::Min,
                Align::Center => egui::Align::Center,
                Align::Max => egui::Align::Max,
            }
        }
        egui::Align2([axis(a.x), axis(a.y)])
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
/// Per-corner radii — WS-E4's first native type.
///
/// Owns its data rather than wrapping the backend's type. Every other
/// vocab newtype follows this shape for the WS-G1 split: the struct is
/// backend-free, and the backend conversions sit behind the
/// `backend-egui-conv` feature so a backend-free build of this crate
/// never names egui at all.
pub struct CornerRadius {
    pub nw: u8,
    pub ne: u8,
    pub sw: u8,
    pub se: u8,
}

impl CornerRadius {
    pub const ZERO: Self = Self::same(0);

    #[must_use]
    pub const fn same(radius: u8) -> Self {
        Self {
            nw: radius,
            ne: radius,
            sw: radius,
            se: radius,
        }
    }

    #[must_use]
    pub const fn from_corners(nw: u8, ne: u8, sw: u8, se: u8) -> Self {
        Self { nw, ne, sw, se }
    }

    /// The four radii, clockwise from north-west: `[nw, ne, se, sw]`.
    ///
    /// A sealed type that can be built but not read forces callers back
    /// to the backend type to inspect one — which is the coupling this
    /// vocabulary exists to remove.
    #[must_use]
    pub const fn corners(self) -> [u8; 4] {
        [self.nw, self.ne, self.se, self.sw]
    }
}

#[cfg(feature = "backend-egui-conv")]
impl From<egui::CornerRadius> for CornerRadius {
    fn from(radius: egui::CornerRadius) -> Self {
        Self::from_corners(radius.nw, radius.ne, radius.sw, radius.se)
    }
}

#[cfg(feature = "backend-egui-conv")]
impl From<CornerRadius> for egui::CornerRadius {
    fn from(radius: CornerRadius) -> Self {
        egui::CornerRadius {
            nw: radius.nw,
            ne: radius.ne,
            sw: radius.sw,
            se: radius.se,
        }
    }
}

impl From<u8> for CornerRadius {
    fn from(radius: u8) -> Self {
        Self::same(radius)
    }
}

impl From<i32> for CornerRadius {
    fn from(radius: i32) -> Self {
        Self::same(radius.clamp(0, u8::MAX as i32) as u8)
    }
}

impl From<f32> for CornerRadius {
    fn from(radius: f32) -> Self {
        Self::same(radius.round().clamp(0.0, u8::MAX as f32) as u8)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TextureOptions(egui::TextureOptions);

impl TextureOptions {
    pub const NEAREST: Self = Self(egui::TextureOptions::NEAREST);
    pub const LINEAR: Self = Self(egui::TextureOptions::LINEAR);
}

impl From<egui::TextureOptions> for TextureOptions {
    fn from(options: egui::TextureOptions) -> Self {
        Self(options)
    }
}

impl From<TextureOptions> for egui::TextureOptions {
    fn from(options: TextureOptions) -> Self {
        options.0
    }
}

#[must_use]
pub fn remap(
    x: f64,
    from: std::ops::RangeInclusive<f64>,
    to: std::ops::RangeInclusive<f64>,
) -> f64 {
    egui::remap(x, from, to)
}

#[must_use]
pub fn remap_clamp(
    x: f64,
    from: std::ops::RangeInclusive<f64>,
    to: std::ops::RangeInclusive<f64>,
) -> f64 {
    egui::remap_clamp(x, from, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_options_are_mara_owned_constants() {
        assert_eq!(
            egui::TextureOptions::from(TextureOptions::NEAREST),
            egui::TextureOptions::NEAREST
        );
        assert_eq!(
            egui::TextureOptions::from(TextureOptions::LINEAR),
            egui::TextureOptions::LINEAR
        );
    }

    #[test]
    fn align2_can_anchor_mara_rects_without_public_egui_geometry() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(8.0, 6.0));
        let anchored = Align2::CENTER_CENTER.anchor_rect(rect);

        assert_eq!(
            anchored,
            Rect::from_min_max(Pos2::new(6.0, 17.0), Pos2::new(14.0, 23.0))
        );
    }

    #[test]
    fn rect_union_is_mara_geometry() {
        let a = Rect::from_min_max(Pos2::new(2.0, 5.0), Pos2::new(8.0, 9.0));
        let b = Rect::from_min_max(Pos2::new(1.0, 7.0), Pos2::new(12.0, 10.0));

        assert_eq!(
            a.union(b),
            Rect::from_min_max(Pos2::new(1.0, 5.0), Pos2::new(12.0, 10.0))
        );
    }
}
