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

    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
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

    #[must_use]
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color32(egui::Color32);

impl Color32 {
    pub const TRANSPARENT: Self = Self(egui::Color32::TRANSPARENT);
    pub const BLACK: Self = Self(egui::Color32::BLACK);
    pub const WHITE: Self = Self(egui::Color32::WHITE);
    pub const GRAY: Self = Self(egui::Color32::GRAY);

    #[must_use]
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self(egui::Color32::from_rgb(r, g, b))
    }

    #[must_use]
    pub fn from_rgba_unmultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self(egui::Color32::from_rgba_unmultiplied(r, g, b, a))
    }

    #[must_use]
    pub fn from_rgba_premultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self(egui::Color32::from_rgba_premultiplied(r, g, b, a))
    }

    #[must_use]
    pub const fn from_gray(gray: u8) -> Self {
        Self(egui::Color32::from_gray(gray))
    }

    #[must_use]
    pub const fn from_black_alpha(alpha: u8) -> Self {
        Self(egui::Color32::from_black_alpha(alpha))
    }

    #[must_use]
    pub const fn r(self) -> u8 {
        self.0.r()
    }

    #[must_use]
    pub const fn g(self) -> u8 {
        self.0.g()
    }

    #[must_use]
    pub const fn b(self) -> u8 {
        self.0.b()
    }

    #[must_use]
    pub const fn a(self) -> u8 {
        self.0.a()
    }

    #[must_use]
    pub fn to_srgba_unmultiplied(self) -> [u8; 4] {
        self.0.to_srgba_unmultiplied()
    }
}

impl From<egui::Color32> for Color32 {
    fn from(c: egui::Color32) -> Self {
        Self(c)
    }
}

impl From<Color32> for egui::Color32 {
    fn from(c: Color32) -> Self {
        c.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub color: Color32,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Align2(egui::Align2);

impl Align2 {
    pub const LEFT_TOP: Self = Self(egui::Align2::LEFT_TOP);
    pub const LEFT_CENTER: Self = Self(egui::Align2::LEFT_CENTER);
    pub const LEFT_BOTTOM: Self = Self(egui::Align2::LEFT_BOTTOM);
    pub const CENTER_TOP: Self = Self(egui::Align2::CENTER_TOP);
    pub const CENTER_CENTER: Self = Self(egui::Align2::CENTER_CENTER);
    pub const CENTER_BOTTOM: Self = Self(egui::Align2::CENTER_BOTTOM);
    pub const RIGHT_TOP: Self = Self(egui::Align2::RIGHT_TOP);
    pub const RIGHT_CENTER: Self = Self(egui::Align2::RIGHT_CENTER);
    pub const RIGHT_BOTTOM: Self = Self(egui::Align2::RIGHT_BOTTOM);

    #[must_use]
    pub fn anchor_rect(self, rect: Rect) -> Rect {
        self.0.anchor_rect(rect.into()).into()
    }
}

impl From<egui::Align2> for Align2 {
    fn from(a: egui::Align2) -> Self {
        Self(a)
    }
}

impl From<Align2> for egui::Align2 {
    fn from(a: Align2) -> Self {
        a.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CornerRadius(egui::CornerRadius);

impl CornerRadius {
    pub const ZERO: Self = Self(egui::CornerRadius::ZERO);

    #[must_use]
    pub const fn same(radius: u8) -> Self {
        Self(egui::CornerRadius::same(radius))
    }

    #[must_use]
    pub const fn from_corners(nw: u8, ne: u8, sw: u8, se: u8) -> Self {
        Self(egui::CornerRadius { nw, ne, sw, se })
    }
}

impl From<egui::CornerRadius> for CornerRadius {
    fn from(radius: egui::CornerRadius) -> Self {
        Self(radius)
    }
}

impl From<CornerRadius> for egui::CornerRadius {
    fn from(radius: CornerRadius) -> Self {
        radius.0
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
