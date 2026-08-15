//! Viewport fitting: `preserveAspectRatio` and the view box → target mapping.
//!
//! # Why this lives in `svg-core`
//!
//! A compiled asset is resolution independent: its geometry is in view box
//! space and the target rectangle is supplied at render time. Turning those two
//! into a transform is therefore something *every* renderer has to do, and
//! doing it twice — once in the Rust reference rasterizer, once from memory in
//! Luau — is how the two quietly stop agreeing.
//!
//! So the policy ([`PreserveAspectRatio`]) and the mathematics
//! ([`view_box_transform`]) are defined once, here, in the crate both sides
//! already speak. The compiler uses the same function to *undo* the mapping
//! usvg applies, which means the forward and inverse directions cannot drift
//! apart either.
//!
//! # The mapping
//!
//! ```text
//! sx = target_width  / view_box.width
//! sy = target_height / view_box.height
//!
//! none   →  (sx, sy)                 non-uniform, fills the target exactly
//! meet   →  (min(sx,sy), same)       whole view box visible, letterboxed
//! slice  →  (max(sx,sy), same)       target fully covered, view box cropped
//! ```
//!
//! The leftover space (negative under `slice`) is then distributed according to
//! [`AspectAlign`], and the view box origin is subtracted so that
//! `(view_box.x, view_box.y)` lands on the aligned corner.

use crate::geometry::ViewBox;
use crate::transform::Transform;

/// Where the scaled view box sits inside the target rectangle.
///
/// Mirrors the alignment half of SVG's `preserveAspectRatio`. [`Self::None`] is
/// SVG's `none`: no alignment, because the scaled view box already fills the
/// target exactly in both axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AspectAlign {
    /// `none` — scale X and Y independently and stretch to fit.
    None,

    XMinYMin,
    XMidYMin,
    XMaxYMin,

    XMinYMid,
    /// The SVG default.
    #[default]
    XMidYMid,
    XMaxYMid,

    XMinYMax,
    XMidYMax,
    XMaxYMax,
}

impl AspectAlign {
    /// The fraction of the leftover horizontal space placed *before* the view
    /// box: 0.0 for `xMin`, 0.5 for `xMid`, 1.0 for `xMax`.
    #[inline]
    pub fn x_fraction(self) -> f32 {
        match self {
            Self::None | Self::XMinYMin | Self::XMinYMid | Self::XMinYMax => 0.0,
            Self::XMidYMin | Self::XMidYMid | Self::XMidYMax => 0.5,
            Self::XMaxYMin | Self::XMaxYMid | Self::XMaxYMax => 1.0,
        }
    }

    /// The fraction of the leftover vertical space placed *above* the view box.
    #[inline]
    pub fn y_fraction(self) -> f32 {
        match self {
            Self::None | Self::XMinYMin | Self::XMidYMin | Self::XMaxYMin => 0.0,
            Self::XMinYMid | Self::XMidYMid | Self::XMaxYMid => 0.5,
            Self::XMinYMax | Self::XMidYMax | Self::XMaxYMax => 1.0,
        }
    }

    /// True for `none`, i.e. "stretch, do not preserve the aspect ratio".
    #[inline]
    pub fn is_none(self) -> bool {
        matches!(self, Self::None)
    }
}

/// Whether the view box is fitted inside the target or made to cover it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AspectScale {
    /// `meet` — the whole view box is visible; unused target space remains.
    #[default]
    Meet,
    /// `slice` — the target is fully covered; the view box overflows and is
    /// cropped.
    Slice,
}

/// An asset's viewport-fitting policy, i.e. SVG's `preserveAspectRatio`.
///
/// Carried through the entire pipeline because it is *not* recoverable from the
/// view box: a 24×12 asset drawn into a 100×100 square looks completely
/// different under `xMidYMid meet` and under `none`, and only the source
/// document knows which the author meant.
///
/// The default is SVG's default, `xMidYMid meet`.
///
/// `preserveAspectRatio`'s `defer` keyword is deliberately not modelled: it only
/// has meaning on `<image>`, where it defers to the referenced content's own
/// policy, and is ignored on the root `<svg>` element that this describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PreserveAspectRatio {
    pub align: AspectAlign,
    pub scale: AspectScale,
}

impl PreserveAspectRatio {
    /// `xMidYMid meet`, the SVG default.
    pub const DEFAULT: Self = Self {
        align: AspectAlign::XMidYMid,
        scale: AspectScale::Meet,
    };

    /// `none`, i.e. stretch independently in X and Y.
    pub const STRETCH: Self = Self {
        align: AspectAlign::None,
        scale: AspectScale::Meet,
    };

    #[inline]
    pub const fn new(align: AspectAlign, scale: AspectScale) -> Self {
        Self { align, scale }
    }

    /// The transform that maps view box space onto a target rectangle whose
    /// top-left corner is the origin. See [`view_box_transform`].
    #[inline]
    pub fn view_box_transform(
        self,
        view_box: ViewBox,
        target_width: f32,
        target_height: f32,
    ) -> Transform {
        view_box_transform(view_box, self, target_width, target_height)
    }
}

/// Maps view box space onto a `target_width` × `target_height` rectangle whose
/// top-left corner is at the origin.
///
/// This is *the* definition of viewport fitting for this project. The reference
/// rasterizer applies it, the compiler inverts it to recover resolution-
/// independent geometry from usvg, and the future Luau rasterizer is expected to
/// be a direct port of it.
///
/// `target_width` and `target_height` are expected to be finite and positive;
/// callers that accept them from users validate first (the rasterizer rejects
/// degenerate raster dimensions outright). A non-positive target simply yields a
/// degenerate — but still finite — transform rather than a panic.
///
/// # Examples
///
/// ```
/// use svg_core::{AspectAlign, AspectScale, PreserveAspectRatio, Point, ViewBox, view_box_transform};
///
/// // A 24x12 view box, letterboxed into a 100x100 square.
/// let view_box = ViewBox::new(0.0, 0.0, 24.0, 12.0).unwrap();
/// let t = view_box_transform(view_box, PreserveAspectRatio::DEFAULT, 100.0, 100.0);
///
/// // Uniform scale of 100/24, centred vertically in the leftover 50 pixels.
/// assert_eq!(t.map_point(Point::new(0.0, 0.0)), Point::new(0.0, 25.0));
/// assert_eq!(t.map_point(Point::new(24.0, 12.0)), Point::new(100.0, 75.0));
/// ```
pub fn view_box_transform(
    view_box: ViewBox,
    aspect: PreserveAspectRatio,
    target_width: f32,
    target_height: f32,
) -> Transform {
    let sx = target_width / view_box.width;
    let sy = target_height / view_box.height;

    let (sx, sy) = if aspect.align.is_none() {
        (sx, sy)
    } else {
        let uniform = match aspect.scale {
            AspectScale::Meet => sx.min(sy),
            AspectScale::Slice => sx.max(sy),
        };
        (uniform, uniform)
    };

    // Leftover space along each axis. Negative under `slice`, which is exactly
    // what makes the same alignment arithmetic crop instead of letterbox.
    let leftover_x = target_width - view_box.width * sx;
    let leftover_y = target_height - view_box.height * sy;

    let tx = -view_box.x * sx + leftover_x * aspect.align.x_fraction();
    let ty = -view_box.y * sy + leftover_y * aspect.align.y_fraction();

    Transform::from_row(sx, 0.0, 0.0, sy, tx, ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    fn vb(x: f32, y: f32, w: f32, h: f32) -> ViewBox {
        ViewBox::new(x, y, w, h).unwrap()
    }

    fn fit(view_box: ViewBox, align: AspectAlign, scale: AspectScale, w: f32, h: f32) -> Transform {
        view_box_transform(view_box, PreserveAspectRatio::new(align, scale), w, h)
    }

    fn assert_point(actual: Point, x: f32, y: f32) {
        assert!(
            (actual.x - x).abs() < 1e-4 && (actual.y - y).abs() < 1e-4,
            "expected ({x}, {y}), got ({}, {})",
            actual.x,
            actual.y
        );
    }

    #[test]
    fn default_is_x_mid_y_mid_meet() {
        let d = PreserveAspectRatio::default();
        assert_eq!(d.align, AspectAlign::XMidYMid);
        assert_eq!(d.scale, AspectScale::Meet);
        assert_eq!(d, PreserveAspectRatio::DEFAULT);
    }

    #[test]
    fn matching_aspect_and_size_is_the_identity() {
        let t = fit(
            vb(0.0, 0.0, 24.0, 24.0),
            AspectAlign::XMidYMid,
            AspectScale::Meet,
            24.0,
            24.0,
        );
        assert!(t.is_identity(), "{t:?}");
    }

    // ---- none ------------------------------------------------------------

    #[test]
    fn none_stretches_independently() {
        let t = fit(
            vb(0.0, 0.0, 24.0, 12.0),
            AspectAlign::None,
            AspectScale::Meet,
            100.0,
            100.0,
        );
        assert_point(t.map_point(Point::new(0.0, 0.0)), 0.0, 0.0);
        assert_point(t.map_point(Point::new(24.0, 12.0)), 100.0, 100.0);
        assert!(!t.is_uniform_scale());
    }

    /// `slice` is meaningless with `none` — the scaled view box already fills
    /// the target — so it must not change anything.
    #[test]
    fn none_ignores_the_meet_slice_keyword() {
        let view_box = vb(0.0, 0.0, 24.0, 12.0);
        let meet = fit(view_box, AspectAlign::None, AspectScale::Meet, 100.0, 40.0);
        let slice = fit(view_box, AspectAlign::None, AspectScale::Slice, 100.0, 40.0);
        assert_eq!(meet, slice);
    }

    // ---- meet ------------------------------------------------------------

    /// The worked example from the specification: 24x12 into 100x100 under
    /// `xMidYMid meet` occupies 100x50, centred vertically.
    #[test]
    fn meet_letterboxes_a_wide_view_box_in_a_square_target() {
        let t = fit(
            vb(0.0, 0.0, 24.0, 12.0),
            AspectAlign::XMidYMid,
            AspectScale::Meet,
            100.0,
            100.0,
        );
        assert_point(t.map_point(Point::new(0.0, 0.0)), 0.0, 25.0);
        assert_point(t.map_point(Point::new(24.0, 12.0)), 100.0, 75.0);
        assert!(t.is_uniform_scale());
    }

    #[test]
    fn meet_pillarboxes_a_square_view_box_in_a_wide_target() {
        let t = fit(
            vb(0.0, 0.0, 24.0, 24.0),
            AspectAlign::XMidYMid,
            AspectScale::Meet,
            48.0,
            24.0,
        );
        assert_point(t.map_point(Point::new(0.0, 0.0)), 12.0, 0.0);
        assert_point(t.map_point(Point::new(24.0, 24.0)), 36.0, 24.0);
    }

    #[test]
    fn meet_pillarboxes_a_square_view_box_in_a_tall_target() {
        let t = fit(
            vb(0.0, 0.0, 24.0, 24.0),
            AspectAlign::XMidYMid,
            AspectScale::Meet,
            24.0,
            48.0,
        );
        assert_point(t.map_point(Point::new(0.0, 0.0)), 0.0, 12.0);
        assert_point(t.map_point(Point::new(24.0, 24.0)), 24.0, 36.0);
    }

    #[test]
    fn meet_min_alignment_hugs_the_top_left() {
        let t = fit(
            vb(0.0, 0.0, 24.0, 12.0),
            AspectAlign::XMinYMin,
            AspectScale::Meet,
            100.0,
            100.0,
        );
        assert_point(t.map_point(Point::new(0.0, 0.0)), 0.0, 0.0);
        assert_point(t.map_point(Point::new(24.0, 12.0)), 100.0, 50.0);
    }

    #[test]
    fn meet_max_alignment_hugs_the_bottom_right() {
        let t = fit(
            vb(0.0, 0.0, 24.0, 12.0),
            AspectAlign::XMaxYMax,
            AspectScale::Meet,
            100.0,
            100.0,
        );
        assert_point(t.map_point(Point::new(0.0, 0.0)), 0.0, 50.0);
        assert_point(t.map_point(Point::new(24.0, 12.0)), 100.0, 100.0);
    }

    /// With a wide view box in a square target the leftover space is vertical,
    /// so the X half of the alignment cannot move anything.
    #[test]
    fn alignment_only_moves_along_the_axis_with_leftover_space() {
        let view_box = vb(0.0, 0.0, 24.0, 12.0);
        for align in [
            AspectAlign::XMinYMid,
            AspectAlign::XMidYMid,
            AspectAlign::XMaxYMid,
        ] {
            let t = fit(view_box, align, AspectScale::Meet, 100.0, 100.0);
            assert_point(t.map_point(Point::new(0.0, 0.0)), 0.0, 25.0);
        }
    }

    // ---- slice -----------------------------------------------------------

    /// 24x12 into 100x100 under `slice` covers the target at 200x100, cropped
    /// horizontally: with `xMidYMid` half the overflow hangs off each edge.
    #[test]
    fn slice_covers_the_target_and_crops() {
        let t = fit(
            vb(0.0, 0.0, 24.0, 12.0),
            AspectAlign::XMidYMid,
            AspectScale::Slice,
            100.0,
            100.0,
        );
        assert_point(t.map_point(Point::new(0.0, 0.0)), -50.0, 0.0);
        assert_point(t.map_point(Point::new(24.0, 12.0)), 150.0, 100.0);
    }

    #[test]
    fn slice_min_alignment_crops_off_the_far_edge() {
        let t = fit(
            vb(0.0, 0.0, 24.0, 12.0),
            AspectAlign::XMinYMin,
            AspectScale::Slice,
            100.0,
            100.0,
        );
        assert_point(t.map_point(Point::new(0.0, 0.0)), 0.0, 0.0);
        assert_point(t.map_point(Point::new(24.0, 12.0)), 200.0, 100.0);
    }

    #[test]
    fn slice_max_alignment_crops_off_the_near_edge() {
        let t = fit(
            vb(0.0, 0.0, 24.0, 12.0),
            AspectAlign::XMaxYMax,
            AspectScale::Slice,
            100.0,
            100.0,
        );
        assert_point(t.map_point(Point::new(0.0, 0.0)), -100.0, 0.0);
        assert_point(t.map_point(Point::new(24.0, 12.0)), 100.0, 100.0);
    }

    // ---- non-zero view box origin ---------------------------------------

    #[test]
    fn non_zero_origin_maps_to_the_aligned_corner_under_meet() {
        let t = fit(
            vb(-4.0, -8.0, 24.0, 12.0),
            AspectAlign::XMinYMin,
            AspectScale::Meet,
            100.0,
            100.0,
        );
        assert_point(t.map_point(Point::new(-4.0, -8.0)), 0.0, 0.0);
        assert_point(t.map_point(Point::new(20.0, 4.0)), 100.0, 50.0);
    }

    #[test]
    fn non_zero_origin_is_still_centred_under_x_mid_y_mid() {
        let t = fit(
            vb(-12.0, -12.0, 24.0, 24.0),
            AspectAlign::XMidYMid,
            AspectScale::Meet,
            48.0,
            24.0,
        );
        // Uniform scale 1, centred in a target 24 units wider than the content.
        assert_point(t.map_point(Point::new(0.0, 0.0)), 24.0, 12.0);
        assert_point(t.map_point(Point::new(-12.0, -12.0)), 12.0, 0.0);
    }

    #[test]
    fn non_zero_origin_under_none_still_stretches_exactly() {
        let t = fit(
            vb(10.0, 20.0, 24.0, 12.0),
            AspectAlign::None,
            AspectScale::Meet,
            96.0,
            96.0,
        );
        assert_point(t.map_point(Point::new(10.0, 20.0)), 0.0, 0.0);
        assert_point(t.map_point(Point::new(34.0, 32.0)), 96.0, 96.0);
    }

    #[test]
    fn non_zero_origin_under_slice_crops_around_the_shifted_box() {
        let t = fit(
            vb(2.0, 3.0, 24.0, 12.0),
            AspectAlign::XMidYMid,
            AspectScale::Slice,
            100.0,
            100.0,
        );
        // Scale 100/12; the 24-unit width becomes 200 and overhangs by 50 a side.
        assert_point(t.map_point(Point::new(2.0, 3.0)), -50.0, 0.0);
        assert_point(t.map_point(Point::new(26.0, 15.0)), 150.0, 100.0);
    }

    // ---- exhaustive sanity ----------------------------------------------

    /// Whatever the alignment, the scaled view box must keep its size: only the
    /// translation may differ. This is the invariant that a mis-signed leftover
    /// term would break.
    #[test]
    fn every_alignment_preserves_the_scaled_extent() {
        let view_box = vb(-3.0, 7.0, 24.0, 12.0);
        for scale in [AspectScale::Meet, AspectScale::Slice] {
            for align in [
                AspectAlign::XMinYMin,
                AspectAlign::XMidYMin,
                AspectAlign::XMaxYMin,
                AspectAlign::XMinYMid,
                AspectAlign::XMidYMid,
                AspectAlign::XMaxYMid,
                AspectAlign::XMinYMax,
                AspectAlign::XMidYMax,
                AspectAlign::XMaxYMax,
            ] {
                for (w, h) in [(100.0, 100.0), (200.0, 50.0), (32.0, 96.0)] {
                    let t = fit(view_box, align, scale, w, h);
                    let a = t.map_point(Point::new(view_box.x, view_box.y));
                    let b = t.map_point(Point::new(
                        view_box.x + view_box.width,
                        view_box.y + view_box.height,
                    ));
                    let expected = match scale {
                        AspectScale::Meet => (w / view_box.width).min(h / view_box.height),
                        AspectScale::Slice => (w / view_box.width).max(h / view_box.height),
                    };
                    assert!(
                        ((b.x - a.x) - view_box.width * expected).abs() < 1e-3,
                        "{align:?} {scale:?} {w}x{h}"
                    );
                    assert!(
                        ((b.y - a.y) - view_box.height * expected).abs() < 1e-3,
                        "{align:?} {scale:?} {w}x{h}"
                    );
                    assert!(t.is_uniform_scale());
                }
            }
        }
    }

    #[test]
    fn alignment_fractions_are_zero_half_one() {
        assert_eq!(AspectAlign::XMinYMax.x_fraction(), 0.0);
        assert_eq!(AspectAlign::XMidYMax.x_fraction(), 0.5);
        assert_eq!(AspectAlign::XMaxYMax.x_fraction(), 1.0);
        assert_eq!(AspectAlign::XMaxYMin.y_fraction(), 0.0);
        assert_eq!(AspectAlign::XMaxYMid.y_fraction(), 0.5);
        assert_eq!(AspectAlign::XMaxYMax.y_fraction(), 1.0);
        // `none` aligns at the origin; there is never leftover space anyway.
        assert_eq!(AspectAlign::None.x_fraction(), 0.0);
        assert_eq!(AspectAlign::None.y_fraction(), 0.0);
    }
}
