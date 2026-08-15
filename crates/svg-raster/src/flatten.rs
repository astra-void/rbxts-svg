//! Adaptive cubic flattening, and the contour extraction that uses it.
//!
//! # Why flattening happens here and not in the compiler
//!
//! How finely a curve must be subdivided depends on how large it will be drawn.
//! The compiler does not know that — an asset is compiled once and rendered at
//! every size — so it keeps curves as curves and this is where they become line
//! segments, against a tolerance expressed in *output pixels*.
//!
//! That is also why the transform is applied before flattening rather than
//! after: a curve flattened in view box space and then scaled up by 8 would
//! show its facets.
//!
//! # The algorithm
//!
//! Iterative binary subdivision against a flatness test, with an explicit stack
//! rather than recursion.
//!
//! A cubic is *flat enough* when both control points lie within
//! [`FLATNESS_TOLERANCE`] of the chord joining its endpoints; then the chord
//! replaces it. Otherwise it is split at its midpoint (de Casteljau, which is
//! exact) and both halves are reconsidered. Subdivision halves the parameter
//! interval, which reduces the deviation roughly fourfold, so the test
//! converges quickly and unevenly-curved sections get points where the
//! curvature actually is — a near-straight run costs one segment however long
//! it is.
//!
//! Properties this buys:
//!
//! - **Resolution dependent.** Tolerance is in device pixels, and the geometry
//!   arrives already scaled, so a 64×64 render subdivides more than a 16×16 one.
//! - **Curvature adaptive.** Flat sections stop immediately; tight corners keep
//!   splitting.
//! - **Deterministic.** No floating-point comparison depends on evaluation
//!   order, and the output sequence is a pure function of the input.
//! - **Bounded.** [`MAX_SUBDIVISION_DEPTH`] caps the work per curve, so no
//!   input — however extreme its control points — can subdivide forever.

use svg_core::{Path, PathCommand, Transform};

use crate::geom::Vec2;

/// Flatness tolerance, in **output pixels**.
///
/// A tenth of a pixel is comfortably below what coverage anti-aliasing can
/// express — the supersampler quantises vertical coverage in quarters of a
/// pixel — so tightening it further would cost subdivisions that cannot change
/// a single output byte.
///
/// It is a constant rather than an option on purpose. The Luau rasterizer has
/// to reproduce this renderer's output, and a knob that must be set identically
/// in two places is a knob that eventually is not.
pub const FLATNESS_TOLERANCE: f32 = 0.1;

/// Hard ceiling on how many times one cubic may be halved.
///
/// The flatness test converges quadratically, so real geometry stops long
/// before this: a curve would need control points about a million pixels off
/// its chord to reach the limit. It exists so that adversarial input — control
/// points at `1e30`, a degenerate cusp — terminates rather than subdividing
/// until it exhausts memory.
pub const MAX_SUBDIVISION_DEPTH: u8 = 12;

/// One flattened subpath, in device space.
#[derive(Debug, Clone, Default)]
pub struct Contour {
    /// Points along the subpath. Consecutive duplicates are already removed, so
    /// every adjacent pair has a well-defined direction.
    pub points: Vec<Vec2>,
    /// Whether the subpath was explicitly closed with `Z`.
    ///
    /// This matters to the stroker, which must decide between joining the ends
    /// and capping them. It does *not* matter to the filler: SVG closes fill
    /// contours implicitly, so filling always treats a contour as closed
    /// regardless of this flag. Recording the authored fact and letting each
    /// consumer apply its own rule is what keeps the canonical geometry
    /// unmodified.
    pub closed: bool,
}

impl Contour {
    /// True when the contour has no segment at all — a lone `MoveTo`, or a run
    /// of coincident points.
    pub fn is_degenerate(&self) -> bool {
        self.points.len() < 2
    }
}

/// Points closer together than this are treated as the same point.
///
/// Device space is pixels, so a thousandth of a pixel is far below anything
/// visible while still being large enough to absorb the rounding that a
/// transform leaves behind.
const COINCIDENT_EPSILON: f32 = 1e-3;

/// Flattens a path into device-space contours.
///
/// `transform` maps view box space onto the target rectangle; see
/// [`svg_core::view_box_transform`]. Existing contents of `out` are replaced.
///
/// Returns `false` if any coordinate is non-finite after transformation, which
/// the caller turns into a [`crate::RasterError::NonFiniteGeometry`]. Partial
/// output is left in `out` and must not be used.
pub fn flatten_path(path: &Path, transform: &Transform, out: &mut Vec<Contour>) -> bool {
    out.clear();

    let mut current = Vec2::ZERO;
    let mut subpath_start = Vec2::ZERO;
    let mut open: Option<Contour> = None;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo(p) => {
                push_contour(&mut open, out);
                let p = map(transform, p);
                if !p.is_finite() {
                    return false;
                }
                current = p;
                subpath_start = p;
                open = Some(Contour {
                    points: vec![p],
                    closed: false,
                });
            }
            PathCommand::LineTo(p) => {
                let p = map(transform, p);
                if !p.is_finite() {
                    return false;
                }
                let contour = reopen(&mut open, current);
                push_unique(&mut contour.points, p);
                current = p;
            }
            PathCommand::CubicTo(c1, c2, end) => {
                let (c1, c2, end) = (map(transform, c1), map(transform, c2), map(transform, end));
                if !(c1.is_finite() && c2.is_finite() && end.is_finite()) {
                    return false;
                }
                let contour = reopen(&mut open, current);
                flatten_cubic(current, c1, c2, end, &mut contour.points);
                current = end;
            }
            PathCommand::Close => {
                if let Some(contour) = open.as_mut() {
                    contour.closed = true;
                }
                push_contour(&mut open, out);
                // SVG puts the current point back at the subpath's start, so a
                // drawing command after `Z` continues from there.
                current = subpath_start;
            }
        }
    }

    push_contour(&mut open, out);
    // A closed contour's final point repeats its first; the segment builders
    // wrap around instead, so carrying the duplicate would only produce
    // zero-length edges.
    for contour in out.iter_mut() {
        if contour.closed
            && contour.points.len() > 1
            && coincident(contour.points[0], contour.points[contour.points.len() - 1])
        {
            contour.points.pop();
        }
    }
    out.retain(|c| !c.points.is_empty());
    true
}

/// Appends a cubic's flattened segments to `out`, excluding its start point,
/// which the caller has already emitted.
///
/// Split out from [`flatten_path`] so the subdivision can be tested directly,
/// against curves whose exact shape is known.
pub fn flatten_cubic(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, out: &mut Vec<Vec2>) {
    // An explicit stack rather than recursion: the depth limit is then a plain
    // loop bound instead of a promise about the call stack, and the worst case
    // cannot overflow it.
    let mut stack: Vec<(Cubic, u8)> = Vec::new();
    stack.push((Cubic { p0, p1, p2, p3 }, 0));

    while let Some((curve, depth)) = stack.pop() {
        if depth >= MAX_SUBDIVISION_DEPTH || curve.is_flat(FLATNESS_TOLERANCE) {
            push_unique(out, curve.p3);
            continue;
        }
        let (left, right) = curve.split();
        // The far half goes on first so the near half pops first: the output
        // has to come out in curve order.
        stack.push((right, depth + 1));
        stack.push((left, depth + 1));
    }
}

#[derive(Debug, Clone, Copy)]
struct Cubic {
    p0: Vec2,
    p1: Vec2,
    p2: Vec2,
    p3: Vec2,
}

impl Cubic {
    /// True when both control points lie within `tolerance` of the chord.
    ///
    /// The distance of a control point from the chord bounds how far the curve
    /// itself can stray from it: a cubic lies inside the convex hull of its
    /// control points, so if neither control point is more than `tolerance`
    /// from the chord, no point of the curve is either.
    fn is_flat(&self, tolerance: f32) -> bool {
        let chord = self.p3 - self.p0;
        let chord_length_squared = chord.length_squared();

        if chord_length_squared <= COINCIDENT_EPSILON * COINCIDENT_EPSILON {
            // A closed loop: the chord is a point, so "distance from the chord"
            // degenerates. Measure from that point instead — the curve is flat
            // only if the whole hull has collapsed onto it.
            let d1 = (self.p1 - self.p0).length();
            let d2 = (self.p2 - self.p0).length();
            return d1.max(d2) <= tolerance;
        }

        // |cross| / |chord| is the perpendicular distance; comparing squares
        // keeps the square root out of the inner loop.
        let d1 = chord.cross(self.p1 - self.p0);
        let d2 = chord.cross(self.p2 - self.p0);
        let worst = d1.abs().max(d2.abs());
        worst * worst <= tolerance * tolerance * chord_length_squared
    }

    /// Splits at `t = 0.5` by de Casteljau's algorithm, which is exact: the two
    /// halves together are the same curve, not an approximation of it.
    fn split(&self) -> (Self, Self) {
        let mid = |a: Vec2, b: Vec2| Vec2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);

        let p01 = mid(self.p0, self.p1);
        let p12 = mid(self.p1, self.p2);
        let p23 = mid(self.p2, self.p3);
        let p012 = mid(p01, p12);
        let p123 = mid(p12, p23);
        let centre = mid(p012, p123);

        (
            Self {
                p0: self.p0,
                p1: p01,
                p2: p012,
                p3: centre,
            },
            Self {
                p0: centre,
                p1: p123,
                p2: p23,
                p3: self.p3,
            },
        )
    }
}

#[inline]
fn map(transform: &Transform, p: svg_core::Point) -> Vec2 {
    Vec2::from_point(transform.map_point(p))
}

#[inline]
fn coincident(a: Vec2, b: Vec2) -> bool {
    (a.x - b.x).abs() <= COINCIDENT_EPSILON && (a.y - b.y).abs() <= COINCIDENT_EPSILON
}

/// Appends a point unless it repeats the previous one.
///
/// Coincident points have no direction, and a stroker that took their
/// difference as a tangent would produce NaN. Dropping them here means every
/// later stage can assume adjacent points differ.
fn push_unique(points: &mut Vec<Vec2>, p: Vec2) {
    if let Some(&last) = points.last()
        && coincident(last, p)
    {
        return;
    }
    points.push(p);
}

fn push_contour(open: &mut Option<Contour>, out: &mut Vec<Contour>) {
    if let Some(contour) = open.take() {
        out.push(contour);
    }
}

/// Returns the open contour, starting a fresh one at `current` if the previous
/// was ended by a `Z`.
///
/// `M 0 0 L 5 0 Z L 9 9` is legal: after the close the current point returns to
/// the subpath start and a *new*, unclosed subpath continues from there.
fn reopen(open: &mut Option<Contour>, current: Vec2) -> &mut Contour {
    if open.is_none() {
        *open = Some(Contour {
            points: vec![current],
            closed: false,
        });
    }
    open.as_mut().expect("just populated")
}

#[cfg(test)]
mod tests {
    use super::*;
    use svg_core::{PathBuilder, Point};

    fn flatten(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2) -> Vec<Vec2> {
        let mut out = vec![p0];
        flatten_cubic(p0, p1, p2, p3, &mut out);
        out
    }

    /// A cubic whose control points sit on the chord is already a line. One
    /// segment, however long it is.
    #[test]
    fn a_straight_cubic_needs_no_subdivision() {
        let points = flatten(
            Vec2::new(0.0, 0.0),
            Vec2::new(100.0, 0.0),
            Vec2::new(200.0, 0.0),
            Vec2::new(300.0, 0.0),
        );
        assert_eq!(points.len(), 2, "{points:?}");
    }

    #[test]
    fn a_curved_cubic_is_subdivided() {
        let points = flatten(
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 100.0),
            Vec2::new(100.0, 100.0),
            Vec2::new(100.0, 0.0),
        );
        assert!(points.len() > 8, "got {} points", points.len());
    }

    /// The whole reason flattening is not a compile-time step: the same curve
    /// drawn larger must be subdivided more.
    #[test]
    fn subdivision_scales_with_output_resolution() {
        let quarter_circle = |r: f32| {
            const K: f32 = 0.552_285;
            flatten(
                Vec2::new(r, 0.0),
                Vec2::new(r, r * K),
                Vec2::new(r * K, r),
                Vec2::new(0.0, r),
            )
            .len()
        };
        let small = quarter_circle(8.0);
        let large = quarter_circle(512.0);
        assert!(large > small * 4, "{small} -> {large}");
    }

    /// Every flattened point must be within tolerance of the true curve. This
    /// is the property the tolerance constant actually promises.
    #[test]
    fn the_polyline_stays_within_tolerance_of_the_curve() {
        let (p0, p1, p2, p3) = (
            Vec2::new(10.0, 200.0),
            Vec2::new(60.0, -80.0),
            Vec2::new(180.0, 300.0),
            Vec2::new(240.0, 20.0),
        );
        let polyline = flatten(p0, p1, p2, p3);

        // Sample the exact curve densely and measure each sample's distance to
        // the nearest polyline segment.
        let mut worst: f32 = 0.0;
        for i in 0..=2000 {
            let t = i as f32 / 2000.0;
            let u = 1.0 - t;
            let exact = Vec2::new(
                u * u * u * p0.x
                    + 3.0 * u * u * t * p1.x
                    + 3.0 * u * t * t * p2.x
                    + t * t * t * p3.x,
                u * u * u * p0.y
                    + 3.0 * u * u * t * p1.y
                    + 3.0 * u * t * t * p2.y
                    + t * t * t * p3.y,
            );
            let mut nearest = f32::MAX;
            for pair in polyline.windows(2) {
                nearest = nearest.min(distance_to_segment(exact, pair[0], pair[1]));
            }
            worst = worst.max(nearest);
        }
        assert!(worst <= FLATNESS_TOLERANCE, "worst deviation was {worst}");
    }

    fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
        let ab = b - a;
        let length_squared = ab.length_squared();
        if length_squared <= 0.0 {
            return (p - a).length();
        }
        let t = ((p - a).dot(ab) / length_squared).clamp(0.0, 1.0);
        (p - a.mul_add(ab, t)).length()
    }

    #[test]
    fn output_is_in_curve_order() {
        let points = flatten(
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 100.0),
            Vec2::new(100.0, 100.0),
            Vec2::new(100.0, 0.0),
        );
        assert_eq!(points[0], Vec2::new(0.0, 0.0));
        assert_eq!(*points.last().unwrap(), Vec2::new(100.0, 0.0));
        // x is monotonically non-decreasing along this particular curve, so any
        // out-of-order emission would show up immediately.
        for pair in points.windows(2) {
            assert!(pair[1].x >= pair[0].x - 1e-4, "{pair:?}");
        }
    }

    #[test]
    fn flattening_is_deterministic() {
        let run = || {
            flatten(
                Vec2::new(3.0, 7.0),
                Vec2::new(-40.0, 90.0),
                Vec2::new(120.0, -30.0),
                Vec2::new(77.0, 55.0),
            )
        };
        assert_eq!(run(), run());
    }

    /// Extreme control points must terminate, not subdivide forever.
    #[test]
    fn an_extreme_curve_is_bounded_by_the_depth_limit() {
        let points = flatten(
            Vec2::new(0.0, 0.0),
            Vec2::new(1e9, 1e9),
            Vec2::new(-1e9, 1e9),
            Vec2::new(1.0, 0.0),
        );
        assert!(points.len() <= (1 << MAX_SUBDIVISION_DEPTH) + 1);
        assert!(points.iter().all(|p| p.is_finite()));
    }

    /// A degenerate "loop" cubic — start and end coincident — has no chord to
    /// measure against and must not divide by zero.
    #[test]
    fn a_loop_cubic_does_not_produce_non_finite_points() {
        let points = flatten(
            Vec2::new(50.0, 50.0),
            Vec2::new(150.0, 0.0),
            Vec2::new(-50.0, 0.0),
            Vec2::new(50.0, 50.0),
        );
        assert!(points.iter().all(|p| p.is_finite()));
        assert!(points.len() > 4);
    }

    // ---- contour extraction ---------------------------------------------

    fn path_of(build: impl FnOnce(&mut PathBuilder)) -> Path {
        let mut b = PathBuilder::new();
        build(&mut b);
        b.finish()
    }

    fn contours(path: &Path) -> Vec<Contour> {
        let mut out = Vec::new();
        assert!(flatten_path(path, &Transform::IDENTITY, &mut out));
        out
    }

    #[test]
    fn each_move_to_starts_a_new_contour() {
        let path = path_of(|b| {
            b.move_to(Point::new(0.0, 0.0)).unwrap();
            b.line_to(Point::new(10.0, 0.0)).unwrap();
            b.move_to(Point::new(20.0, 20.0)).unwrap();
            b.line_to(Point::new(30.0, 20.0)).unwrap();
        });
        let contours = contours(&path);
        assert_eq!(contours.len(), 2);
        assert!(contours.iter().all(|c| !c.closed));
    }

    #[test]
    fn close_marks_the_contour_and_drops_the_repeated_point() {
        let path = path_of(|b| {
            b.move_to(Point::new(0.0, 0.0)).unwrap();
            b.line_to(Point::new(10.0, 0.0)).unwrap();
            b.line_to(Point::new(10.0, 10.0)).unwrap();
            b.line_to(Point::new(0.0, 0.0)).unwrap();
            b.close().unwrap();
        });
        let contours = contours(&path);
        assert_eq!(contours.len(), 1);
        assert!(contours[0].closed);
        // The explicit return to the start is dropped: wrapping around supplies
        // that edge, and keeping it would only add a zero-length one.
        assert_eq!(contours[0].points.len(), 3);
    }

    /// `Z` followed by more drawing commands starts a fresh, open subpath at
    /// the closed one's start point.
    #[test]
    fn drawing_after_close_reopens_at_the_subpath_start() {
        let path = path_of(|b| {
            b.move_to(Point::new(0.0, 0.0)).unwrap();
            b.line_to(Point::new(10.0, 0.0)).unwrap();
            b.close().unwrap();
            b.line_to(Point::new(0.0, 20.0)).unwrap();
        });
        let contours = contours(&path);
        assert_eq!(contours.len(), 2);
        assert!(contours[0].closed);
        assert!(!contours[1].closed);
        assert_eq!(contours[1].points[0], Vec2::new(0.0, 0.0));
        assert_eq!(contours[1].points[1], Vec2::new(0.0, 20.0));
    }

    #[test]
    fn coincident_points_are_collapsed() {
        let path = path_of(|b| {
            b.move_to(Point::new(5.0, 5.0)).unwrap();
            b.line_to(Point::new(5.0, 5.0)).unwrap();
            b.line_to(Point::new(5.0, 5.0)).unwrap();
        });
        let contours = contours(&path);
        assert_eq!(contours.len(), 1);
        assert!(contours[0].is_degenerate());
    }

    #[test]
    fn the_transform_is_applied_before_flattening() {
        let path = path_of(|b| {
            b.move_to(Point::new(0.0, 0.0)).unwrap();
            b.line_to(Point::new(1.0, 2.0)).unwrap();
        });
        let scale = Transform::from_row(10.0, 0.0, 0.0, 10.0, 5.0, 0.0);
        let mut out = Vec::new();
        assert!(flatten_path(&path, &scale, &mut out));
        assert_eq!(out[0].points[0], Vec2::new(5.0, 0.0));
        assert_eq!(out[0].points[1], Vec2::new(15.0, 20.0));
    }

    #[test]
    fn a_transform_that_overflows_is_reported_rather_than_emitted() {
        let path = path_of(|b| {
            b.move_to(Point::new(0.0, 0.0)).unwrap();
            b.line_to(Point::new(1e30, 1e30)).unwrap();
        });
        let huge = Transform::from_row(1e30, 0.0, 0.0, 1e30, 0.0, 0.0);
        let mut out = Vec::new();
        assert!(!flatten_path(&path, &huge, &mut out));
    }
}
