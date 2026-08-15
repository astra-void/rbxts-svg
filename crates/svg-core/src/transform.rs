//! 2D affine transforms.
//!
//! Transforms are a *compile-time* concept in this pipeline. The compiler
//! resolves every inherited transform and bakes it into the geometry, so a
//! finished [`crate::document::SvgDocument`] contains no transforms at all.
//! That is what lets the runtime decoder stay trivial.

/// A row-major 2x3 affine matrix, laid out exactly like SVG's `matrix(...)`:
///
/// ```text
/// | sx  kx  tx |
/// | ky  sy  ty |
/// |  0   0   1 |
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub sx: f32,
    pub ky: f32,
    pub kx: f32,
    pub sy: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        sx: 1.0,
        ky: 0.0,
        kx: 0.0,
        sy: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    /// Matches the argument order of SVG's `matrix(a b c d e f)`.
    #[inline]
    pub const fn from_row(sx: f32, ky: f32, kx: f32, sy: f32, tx: f32, ty: f32) -> Self {
        Self {
            sx,
            ky,
            kx,
            sy,
            tx,
            ty,
        }
    }

    #[inline]
    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    /// Applies the transform to a point.
    #[inline]
    pub fn map_point(&self, p: crate::geometry::Point) -> crate::geometry::Point {
        crate::geometry::Point::new(
            self.sx * p.x + self.kx * p.y + self.tx,
            self.ky * p.x + self.sy * p.y + self.ty,
        )
    }

    /// `self` followed by `other` (i.e. `other * self`).
    pub fn post_concat(&self, other: &Self) -> Self {
        Self {
            sx: other.sx * self.sx + other.kx * self.ky,
            ky: other.ky * self.sx + other.sy * self.ky,
            kx: other.sx * self.kx + other.kx * self.sy,
            sy: other.ky * self.kx + other.sy * self.sy,
            tx: other.sx * self.tx + other.kx * self.ty + other.tx,
            ty: other.ky * self.tx + other.sy * self.ty + other.ty,
        }
    }

    /// The determinant of the linear part.
    #[inline]
    pub fn determinant(&self) -> f32 {
        self.sx * self.sy - self.kx * self.ky
    }

    /// The factor by which this transform scales *lengths*, used to carry
    /// stroke widths through a baked transform.
    ///
    /// SVG strokes a shape in its own user space, so a transformed shape has a
    /// transformed stroke outline. We bake geometry into view box space, which
    /// means the stroke width has to be scaled to match. For a uniform scale
    /// this is exact: `sqrt(|det|)` is precisely the scale factor. For a
    /// non-uniform or skewed transform no single width is correct — the true
    /// outline is an ellipse-swept envelope rather than a circle-swept one — so
    /// callers are expected to emit a diagnostic when [`Self::is_uniform_scale`]
    /// is false.
    #[inline]
    pub fn length_scale(&self) -> f32 {
        self.determinant().abs().sqrt()
    }

    /// True when the linear part is a rotation and/or uniform scale (possibly
    /// with a flip), meaning [`Self::length_scale`] is exact.
    pub fn is_uniform_scale(&self) -> bool {
        // Columns must be orthogonal and of equal length.
        let col_dot = self.sx * self.kx + self.ky * self.sy;
        let len_a = self.sx * self.sx + self.ky * self.ky;
        let len_b = self.kx * self.kx + self.sy * self.sy;
        // Relative tolerance: the magnitudes here scale with the transform.
        let scale = (len_a + len_b).max(1.0);
        col_dot.abs() <= 1e-5 * scale && (len_a - len_b).abs() <= 1e-5 * scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    #[test]
    fn identity_maps_points_unchanged() {
        let p = Point::new(3.0, -4.0);
        assert_eq!(Transform::IDENTITY.map_point(p), p);
    }

    #[test]
    fn post_concat_applies_self_then_other() {
        let scale = Transform::from_row(2.0, 0.0, 0.0, 2.0, 0.0, 0.0);
        let translate = Transform::from_row(1.0, 0.0, 0.0, 1.0, 10.0, 0.0);

        // scale first, then translate => (1,1) -> (2,2) -> (12,2)
        let combined = scale.post_concat(&translate);
        assert_eq!(
            combined.map_point(Point::new(1.0, 1.0)),
            Point::new(12.0, 2.0)
        );

        // translate first, then scale => (1,1) -> (11,1) -> (22,2)
        let other = translate.post_concat(&scale);
        assert_eq!(other.map_point(Point::new(1.0, 1.0)), Point::new(22.0, 2.0));
    }

    #[test]
    fn length_scale_of_uniform_scale_is_exact() {
        let t = Transform::from_row(3.0, 0.0, 0.0, 3.0, 5.0, 5.0);
        assert!(t.is_uniform_scale());
        assert!((t.length_scale() - 3.0).abs() < 1e-6);
    }

    #[test]
    fn rotation_counts_as_uniform() {
        let (s, c) = (0.5f32).sin_cos();
        let t = Transform::from_row(c, s, -s, c, 0.0, 0.0);
        assert!(t.is_uniform_scale());
        assert!((t.length_scale() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn non_uniform_scale_is_detected() {
        let t = Transform::from_row(2.0, 0.0, 0.0, 5.0, 0.0, 0.0);
        assert!(!t.is_uniform_scale());
    }

    #[test]
    fn skew_is_not_uniform() {
        let t = Transform::from_row(1.0, 0.0, 0.7, 1.0, 0.0, 0.0);
        assert!(!t.is_uniform_scale());
    }
}
