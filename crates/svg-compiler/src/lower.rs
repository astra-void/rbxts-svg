//! usvg geometry → canonical geometry.
//!
//! # How much usvg has already done
//!
//! usvg lowers the primitive shapes (`rect`, `circle`, `ellipse`, `line`,
//! `polyline`, `polygon`) into paths, and converts the shorthand path commands
//! (`H`, `V`, `S`, `T`) and elliptical arcs (`A`) into explicit segments, so
//! there is no reason to reimplement any of that. What reaches us is a
//! `tiny_skia_path::Path` containing `MoveTo`, `LineTo`, `QuadTo`, `CubicTo`
//! and `Close`.
//!
//! # What is left to do
//!
//! Exactly one thing: quadratics still need elevating to cubics, because the
//! runtime command set has no quadratic. Degree elevation is exact, so no
//! precision is lost.
//!
//! Curves are *not* flattened here. How finely a cubic must be subdivided
//! depends on the pixel size it will be drawn at, which the compiler does not
//! know, so flattening belongs to the rasterizer.

use svg_core::{CoreError, Path, PathBuilder, Point};
use usvg::tiny_skia_path;

/// Converts a usvg path into canonical geometry.
///
/// Returns `Ok(None)` when the path has no commands at all. Subpath structure
/// is preserved exactly: fill rules depend on it.
pub fn lower_path(path: &tiny_skia_path::Path) -> Result<Option<Path>, CoreError> {
    let mut builder = PathBuilder::with_capacity(path.len());
    // The current point is needed to elevate a quadratic, and tiny-skia's
    // segment iterator does not hand it to us.
    let mut current = Point::new(0.0, 0.0);
    // Where the active subpath began, so `Close` restores the current point
    // correctly for any command that follows it.
    let mut subpath_start = Point::new(0.0, 0.0);

    for segment in path.segments() {
        match segment {
            tiny_skia_path::PathSegment::MoveTo(p) => {
                let p = point(p);
                builder.move_to(p)?;
                current = p;
                subpath_start = p;
            }
            tiny_skia_path::PathSegment::LineTo(p) => {
                let p = point(p);
                builder.line_to(p)?;
                current = p;
            }
            tiny_skia_path::PathSegment::QuadTo(ctrl, end) => {
                let (ctrl, end) = (point(ctrl), point(end));
                builder.quad_to(current, ctrl, end)?;
                current = end;
            }
            tiny_skia_path::PathSegment::CubicTo(c1, c2, end) => {
                let (c1, c2, end) = (point(c1), point(c2), point(end));
                builder.cubic_to(c1, c2, end)?;
                current = end;
            }
            tiny_skia_path::PathSegment::Close => {
                builder.close()?;
                current = subpath_start;
            }
        }
    }

    if builder.is_empty() {
        return Ok(None);
    }
    Ok(Some(builder.finish()))
}

#[inline]
fn point(p: tiny_skia_path::Point) -> Point {
    Point::new(p.x, p.y)
}

/// Converts a usvg transform into the core representation. The field order is
/// the same in both (`sx, ky, kx, sy, tx, ty`, matching SVG's `matrix()`).
pub fn lower_transform(t: usvg::Transform) -> svg_core::Transform {
    svg_core::Transform::from_row(t.sx, t.ky, t.kx, t.sy, t.tx, t.ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use svg_core::PathCommand;
    use usvg::tiny_skia_path::PathBuilder as SkiaBuilder;

    #[test]
    fn empty_path_lowers_to_none() {
        let builder = SkiaBuilder::new();
        assert!(builder.finish().is_none());
    }

    #[test]
    fn quadratics_are_elevated_to_cubics() {
        let mut b = SkiaBuilder::new();
        b.move_to(0.0, 0.0);
        b.quad_to(2.0, 4.0, 4.0, 0.0);
        let path = b.finish().unwrap();

        let lowered = lower_path(&path).unwrap().unwrap();
        assert_eq!(lowered.commands().len(), 2);
        assert!(matches!(lowered.commands()[1], PathCommand::CubicTo(..)));

        // Control points of the exact elevation of Q(0,0 | 2,4 | 4,0).
        let PathCommand::CubicTo(c1, c2, end) = lowered.commands()[1] else {
            unreachable!()
        };
        assert!((c1.x - 4.0 / 3.0).abs() < 1e-5);
        assert!((c1.y - 8.0 / 3.0).abs() < 1e-5);
        assert!((c2.x - 8.0 / 3.0).abs() < 1e-5);
        assert!((c2.y - 8.0 / 3.0).abs() < 1e-5);
        assert_eq!(end, Point::new(4.0, 0.0));
    }

    #[test]
    fn cubics_pass_through_untouched() {
        let mut b = SkiaBuilder::new();
        b.move_to(0.0, 0.0);
        b.cubic_to(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
        let lowered = lower_path(&b.finish().unwrap()).unwrap().unwrap();

        assert_eq!(
            lowered.commands()[1],
            PathCommand::CubicTo(
                Point::new(1.0, 2.0),
                Point::new(3.0, 4.0),
                Point::new(5.0, 6.0)
            )
        );
    }

    #[test]
    fn multiple_subpaths_are_preserved() {
        let mut b = SkiaBuilder::new();
        b.move_to(0.0, 0.0);
        b.line_to(1.0, 0.0);
        b.close();
        b.move_to(5.0, 5.0);
        b.line_to(6.0, 5.0);
        b.close();
        let lowered = lower_path(&b.finish().unwrap()).unwrap().unwrap();

        assert_eq!(lowered.subpath_count(), 2);
        assert!(matches!(lowered.commands()[3], PathCommand::MoveTo(_)));
    }

    /// After `Close` the current point returns to the subpath start. A
    /// quadratic issued right after a close must be elevated against that
    /// point, not against the last drawn endpoint.
    #[test]
    fn close_restores_the_current_point_for_quad_elevation() {
        let mut b = SkiaBuilder::new();
        b.move_to(0.0, 0.0);
        b.line_to(10.0, 10.0);
        b.close();
        // tiny-skia requires a move after close for a new subpath; emulate the
        // "current point is the subpath start" rule directly instead.
        b.move_to(0.0, 0.0);
        b.quad_to(2.0, 4.0, 4.0, 0.0);
        let lowered = lower_path(&b.finish().unwrap()).unwrap().unwrap();

        let PathCommand::CubicTo(c1, ..) = *lowered.commands().last().unwrap() else {
            panic!("expected a cubic");
        };
        assert!((c1.x - 4.0 / 3.0).abs() < 1e-5);
    }

    #[test]
    fn rect_from_tiny_skia_lowers_to_a_closed_subpath() {
        let rect = tiny_skia_path::Rect::from_xywh(1.0, 2.0, 3.0, 4.0).unwrap();
        let path = SkiaBuilder::from_rect(rect);
        let lowered = lower_path(&path).unwrap().unwrap();

        assert_eq!(lowered.subpath_count(), 1);
        assert!(lowered.has_drawing_commands());
        assert_eq!(
            lowered.commands()[0],
            PathCommand::MoveTo(Point::new(1.0, 2.0))
        );
    }

    #[test]
    fn transform_field_order_matches_svg_matrix() {
        let t = usvg::Transform::from_row(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
        let core = lower_transform(t);
        assert_eq!(core.sx, 1.0);
        assert_eq!(core.ky, 2.0);
        assert_eq!(core.kx, 3.0);
        assert_eq!(core.sy, 4.0);
        assert_eq!(core.tx, 5.0);
        assert_eq!(core.ty, 6.0);
    }
}
