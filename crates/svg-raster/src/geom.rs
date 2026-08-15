//! Device-space vector arithmetic.
//!
//! Everything in this crate below [`crate::render`] works in *device space*:
//! pixels, with the origin at the top-left of the raster and y increasing
//! downwards. The view box → device mapping happens exactly once, at the top of
//! the pipeline, which is what makes flattening tolerances and stroke widths
//! meaningful in pixels rather than in an arbitrary user unit.
//!
//! A separate `Vec2` rather than [`svg_core::Point`] is deliberate: `Point` is
//! a *semantic* coordinate in view box space, and letting the two share a type
//! would make "have I transformed this yet?" a question the compiler stops
//! answering.

use svg_core::Point;

/// A point or direction in device space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[inline]
    pub fn from_point(p: Point) -> Self {
        Self { x: p.x, y: p.y }
    }

    /// `self + other * k`, the shape almost every offset calculation takes.
    #[inline]
    pub fn mul_add(self, other: Self, k: f32) -> Self {
        Self::new(self.x + other.x * k, self.y + other.y * k)
    }

    #[inline]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }

    /// The z component of the 3D cross product. Its sign is which way one
    /// direction turns to reach another, which is how stroke joins decide
    /// which side of a corner is the outside.
    #[inline]
    pub fn cross(self, other: Self) -> f32 {
        self.x * other.y - self.y * other.x
    }

    #[inline]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// The unit vector in the same direction, or `None` for a vector too short
    /// to have a well-defined direction.
    ///
    /// Returning `None` rather than dividing by a near-zero length is the whole
    /// reason degenerate segments cannot produce NaN coordinates downstream.
    #[inline]
    pub fn normalize(self) -> Option<Self> {
        let length_squared = self.length_squared();
        // `is_finite` first, so a NaN takes this branch rather than falling
        // through a comparison it would silently answer `false` to.
        if !length_squared.is_finite() || length_squared <= f32::MIN_POSITIVE {
            return None;
        }
        let length = length_squared.sqrt();
        Some(Self::new(self.x / length, self.y / length))
    }

    /// The direction rotated a quarter turn. Used to offset a segment sideways.
    ///
    /// Which visual side this lands on depends on the handedness of the
    /// coordinate system; the stroker only relies on it being *consistent*, and
    /// works out which side is the outside of a corner from
    /// [`Self::cross`].
    #[inline]
    pub fn normal(self) -> Self {
        Self::new(-self.y, self.x)
    }

    #[inline]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl core::ops::Add for Vec2 {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl core::ops::Sub for Vec2 {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

/// Scaling by a scalar. `Vec2 * f32` only, deliberately: `f32 * Vec2` would
/// read the same and add nothing.
impl core::ops::Mul<f32> for Vec2 {
    type Output = Self;

    #[inline]
    fn mul(self, k: f32) -> Self {
        Self::new(self.x * k, self.y * k)
    }
}

impl core::ops::Neg for Vec2 {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rejects_a_degenerate_vector() {
        assert!(Vec2::ZERO.normalize().is_none());
        assert!(Vec2::new(f32::NAN, 0.0).normalize().is_none());
        assert!(Vec2::new(1e-30, 0.0).normalize().is_none());
        assert_eq!(Vec2::new(3.0, 4.0).normalize(), Some(Vec2::new(0.6, 0.8)));
    }

    #[test]
    fn normal_is_perpendicular_and_length_preserving() {
        let v = Vec2::new(3.0, 4.0);
        let n = v.normal();
        assert_eq!(v.dot(n), 0.0);
        assert_eq!(n.length(), v.length());
    }

    #[test]
    fn cross_sign_reports_turn_direction() {
        let right = Vec2::new(1.0, 0.0);
        assert!(right.cross(Vec2::new(0.0, 1.0)) > 0.0);
        assert!(right.cross(Vec2::new(0.0, -1.0)) < 0.0);
        assert_eq!(right.cross(Vec2::new(2.0, 0.0)), 0.0);
    }

    #[test]
    fn mul_add_matches_the_long_form() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, -4.0);
        assert_eq!(a.mul_add(b, 2.0), a + b * 2.0);
    }

    #[test]
    fn the_operators_agree_with_arithmetic() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, -4.0);
        assert_eq!(a + b, Vec2::new(4.0, -2.0));
        assert_eq!(a - b, Vec2::new(-2.0, 6.0));
        assert_eq!(a * 3.0, Vec2::new(3.0, 6.0));
        assert_eq!(-a, Vec2::new(-1.0, -2.0));
    }
}
