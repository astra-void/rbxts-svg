//! Stroke expansion: an outline turned into the area it covers.
//!
//! # Why expansion rather than line drawing
//!
//! A stroke *is* a filled region — SVG defines it as the area swept by a pen
//! along the path — and treating it as one is what makes everything else fall
//! out for free. Caps and joins become geometry rather than special cases in a
//! line routine; a stroke that crosses itself composites once instead of
//! blending with itself; a stroke and a fill anti-alias identically because
//! they go through the same scan conversion. A `DrawLine`-style renderer has to
//! solve each of those separately, and typically does not.
//!
//! So this module's entire job is to produce polygons. [`crate::edges`] takes
//! it from there.
//!
//! # Construction: a union of pieces
//!
//! The stroke is emitted as a *set* of simple polygons — one quadrilateral per
//! segment, one wedge per join, one per cap — all wound the same way and filled
//! with the non-zero rule. Non-zero over same-wound polygons is exactly their
//! union, so the pieces need only cover the right area between them; they do
//! not have to be stitched into a single outline.
//!
//! That is worth the redundancy. The obvious alternative — walking one
//! continuous offset outline down each side of the path — has to answer, at
//! every corner, what the *inside* of the bend looks like: the two offset
//! segments cross there, and joining their endpoints leaves a notch while
//! computing their intersection needs its own clamping for near-cusps. Emitting
//! overlapping pieces sidesteps the question entirely, because overlap is what
//! the fill rule is for. Self-intersecting paths, hairpins and zero-length
//! segments then need no special handling either.
//!
//! It also ports cleanly: a Luau implementation emits the same short list of
//! polygons without any outline bookkeeping.
//!
//! # Where dashing would go
//!
//! `stroke-dasharray` is not supported (the compiler rejects it). When it
//! arrives it belongs *in front* of this module: split each contour into the
//! dash segments, then expand those. Nothing here would change — a dash is just
//! a shorter open contour with its own caps.

use svg_core::{LineCap, LineJoin};

use crate::flatten::Contour;
use crate::geom::Vec2;

/// How a stroke is drawn. Width is in device pixels, unlike
/// [`svg_core::Stroke`], whose width is in view box units.
#[derive(Debug, Clone, Copy)]
pub struct StrokeStyle {
    /// Total width in device pixels. The pen's radius is half this.
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    /// SVG's `stroke-miterlimit`: the largest miter length, as a multiple of
    /// the stroke width, before a miter join gives up and bevels.
    pub miter_limit: f32,
}

/// Below this, two unit directions count as parallel and their turn as no turn.
///
/// They are unit vectors, so their cross product is the sine of the turn angle:
/// this is about a thousandth of a degree.
const PARALLEL_EPSILON: f32 = 1e-5;

/// Arc chord tolerance for round joins and caps, in device pixels.
///
/// Deliberately the same scale as [`crate::flatten::FLATNESS_TOLERANCE`]: a
/// round join is a curve like any other, and there is no reason for a cap to be
/// visibly coarser than the path it terminates. Arcs are *inscribed*, so a
/// round cap falls short of the true circle by at most this much.
pub const ARC_TOLERANCE: f32 = 0.1;

/// Ceiling on the segments in one arc, so a degenerate radius or tolerance
/// cannot produce an unbounded loop.
const MAX_ARC_SEGMENTS: u32 = 256;

/// Expands `contours` into the polygons their stroke covers.
///
/// The polygons are implicitly closed, wound consistently, and must be filled
/// with the **non-zero** rule: they overlap by design, and even-odd would punch
/// holes through every overlap. Existing contents of `out` are replaced.
pub fn expand(contours: &[Contour], style: StrokeStyle, out: &mut Vec<Vec<Vec2>>) {
    out.clear();

    let radius = style.width * 0.5;
    // `is_finite` first, so a NaN width takes this branch rather than falling
    // through a comparison that would silently answer `false`.
    if !radius.is_finite() || radius <= 0.0 {
        return;
    }

    let mut segments: Vec<Segment> = Vec::new();

    for contour in contours {
        build_segments(&contour.points, contour.closed, &mut segments);

        if segments.is_empty() {
            // No length at all. A cap with area still paints here — SVG says so
            // explicitly, and it is how a single-point subpath draws a dot.
            if let Some(&point) = contour.points.first()
                && !contour.closed
            {
                emit_degenerate_cap(out, point, radius, style.cap);
            }
            continue;
        }

        // One quadrilateral per segment: the pen swept along it.
        for segment in &segments {
            push_polygon(
                out,
                vec![
                    segment.start.mul_add(segment.normal, radius),
                    segment.end.mul_add(segment.normal, radius),
                    segment.end.mul_add(segment.normal, -radius),
                    segment.start.mul_add(segment.normal, -radius),
                ],
            );
        }

        // One wedge per interior vertex, filling the gap the bend opens on the
        // outside of the corner. A closed contour bends at its start point too.
        let count = segments.len();
        let first_join = if contour.closed { 0 } else { 1 };
        for index in first_join..count {
            let current = segments[index];
            let previous = segments[(index + count - 1) % count];
            emit_join(out, current.start, previous, current, radius, style);
        }

        if !contour.closed {
            let last = segments[count - 1];
            let first = segments[0];
            emit_cap(out, last.end, last.direction, radius, style.cap);
            emit_cap(out, first.start, first.direction * -1.0, radius, style.cap);
        }
    }
}

/// One segment of a contour, with its unit direction and offset normal.
#[derive(Debug, Clone, Copy)]
struct Segment {
    start: Vec2,
    end: Vec2,
    direction: Vec2,
    normal: Vec2,
}

fn build_segments(points: &[Vec2], closed: bool, out: &mut Vec<Segment>) {
    out.clear();
    if points.len() < 2 {
        return;
    }
    let count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    for index in 0..count {
        let start = points[index];
        let end = points[(index + 1) % points.len()];
        // Flattening already collapsed coincident points, but a directly
        // constructed contour might not have, and a zero-length segment has no
        // direction to offset along.
        let Some(direction) = (end - start).normalize() else {
            continue;
        };
        out.push(Segment {
            start,
            end,
            direction,
            normal: direction.normal(),
        });
    }
}

/// Adds a polygon, normalising its winding.
///
/// The non-zero union only works if every piece is wound the same way — a
/// reversed wedge would *subtract* itself from the segment it sits on. Rather
/// than reason about which construction produces which orientation, each
/// polygon's signed area is measured and it is flipped if need be. That reduces
/// a class of sign bugs to one line, and costs a pass over a handful of points.
fn push_polygon(out: &mut Vec<Vec<Vec2>>, mut points: Vec<Vec2>) {
    if points.len() < 3 {
        return;
    }
    if !points.iter().all(|p| p.is_finite()) {
        return;
    }
    if signed_area(&points) < 0.0 {
        points.reverse();
    }
    out.push(points);
}

/// Twice the signed area — the sign is all that is needed, so the halving is
/// left out.
fn signed_area(points: &[Vec2]) -> f32 {
    let mut total = 0.0;
    for index in 0..points.len() {
        total += points[index].cross(points[(index + 1) % points.len()]);
    }
    total
}

/// Fills the wedge a bend opens on the outside of a corner.
///
/// Only the outside needs anything: on the inside the two segment
/// quadrilaterals already overlap, and the non-zero rule merges them.
fn emit_join(
    out: &mut Vec<Vec<Vec2>>,
    vertex: Vec2,
    previous: Segment,
    current: Segment,
    radius: f32,
    style: StrokeStyle,
) {
    let turn = previous.direction.cross(current.direction);
    let straight = previous.direction.dot(current.direction);

    if turn.abs() <= PARALLEL_EPSILON {
        if straight >= 0.0 {
            // Collinear and continuing: the quadrilaterals already meet flush.
            return;
        }
        // A cusp — the path doubles back. Both sides are "outside", and the
        // miter is infinite by definition, so only a round join has anything
        // to add: the disc the pen sweeps as it pivots through half a turn on
        // each side, which together is a full one.
        if style.join == LineJoin::Round {
            emit_degenerate_cap(out, vertex, radius, LineCap::Round);
        }
        return;
    }

    // The bend turns towards the normal side when the cross product is
    // positive, which puts the *outside* on the other one.
    let side = if turn > 0.0 { -1.0 } else { 1.0 };
    let from = previous.normal * side;
    let to = current.normal * side;

    let mut wedge = vec![vertex, vertex.mul_add(from, radius)];

    match style.join {
        LineJoin::Bevel => {}
        LineJoin::Miter => {
            if let Some(apex) = miter_apex(vertex, from, to, radius, style.miter_limit) {
                wedge.push(apex);
            }
            // Beyond the limit SVG falls back to a bevel, which is this wedge
            // without its apex: a triangle straight across the corner.
        }
        LineJoin::Round => {
            emit_arc(&mut wedge, vertex, from, to, radius);
        }
    }

    wedge.push(vertex.mul_add(to, radius));
    push_polygon(out, wedge);
}

/// Where the two outer offset lines meet, or `None` when that point is further
/// out than `miter_limit` allows.
///
/// SVG defines the limit as `miterLength / strokeWidth`, and for a corner whose
/// interior angle is `θ` that ratio is `1 / sin(θ/2)`. The bisector of the two
/// outer normals is a unit vector whose dot product with either of them is
/// exactly `sin(θ/2)`, so the ratio falls straight out — no angles, no
/// trigonometry, and no branch on which way the corner turns.
fn miter_apex(vertex: Vec2, from: Vec2, to: Vec2, radius: f32, miter_limit: f32) -> Option<Vec2> {
    // A cusp makes the two normals opposite and the bisector vanish. That is
    // the infinite-miter case, so bevelling is the correct answer anyway.
    let bisector = (from + to).normalize()?;

    let half_angle_sine = bisector.dot(from);
    if half_angle_sine <= f32::MIN_POSITIVE {
        return None;
    }
    let ratio = 1.0 / half_angle_sine;
    if !miter_limit.is_finite() || ratio > miter_limit {
        return None;
    }

    let apex = vertex.mul_add(bisector, radius * ratio);
    apex.is_finite().then_some(apex)
}

/// Appends the interior points of an arc of `radius` about `centre`, sweeping
/// from `from` to `to` (both unit vectors) the short way round.
fn emit_arc(out: &mut Vec<Vec2>, centre: Vec2, from: Vec2, to: Vec2, radius: f32) {
    let sweep = from.cross(to).atan2(from.dot(to));
    emit_arc_sweep(out, centre, from, sweep, radius);
}

/// Appends the interior points of an arc starting at `from` and turning through
/// `sweep` radians.
///
/// Separate from [`emit_arc`] because a half turn's direction cannot be
/// recovered from its endpoints — they are exactly opposite, and `atan2` would
/// have to guess.
fn emit_arc_sweep(out: &mut Vec<Vec2>, centre: Vec2, from: Vec2, sweep: f32, radius: f32) {
    if !sweep.is_finite() || sweep.abs() <= PARALLEL_EPSILON {
        return;
    }

    // A chord subtending angle `d` on a circle of radius `r` falls short of the
    // arc by `r * (1 - cos(d / 2))`. Solving that for the tolerance gives the
    // largest step that stays within it.
    let step = if radius > ARC_TOLERANCE {
        2.0 * (1.0 - ARC_TOLERANCE / radius).clamp(-1.0, 1.0).acos()
    } else {
        // The whole arc is already smaller than the tolerance, so one chord
        // does the job.
        sweep.abs()
    };

    let segments = if step > 0.0 {
        ((sweep.abs() / step).ceil() as u32).clamp(1, MAX_ARC_SEGMENTS)
    } else {
        1
    };

    let (sin_step, cos_step) = (sweep / segments as f32).sin_cos();
    let mut direction = from;
    for _ in 1..segments {
        direction = Vec2::new(
            direction.x * cos_step - direction.y * sin_step,
            direction.x * sin_step + direction.y * cos_step,
        );
        out.push(centre.mul_add(direction, radius));
    }
}

/// Closes off an open end.
///
/// `direction` points *out* of the path at this end.
fn emit_cap(out: &mut Vec<Vec<Vec2>>, end: Vec2, direction: Vec2, radius: f32, cap: LineCap) {
    let normal = direction.normal();
    match cap {
        // Nothing to add: the segment's own quadrilateral already stops
        // exactly at the endpoint, which is what `butt` means.
        LineCap::Butt => {}
        LineCap::Square => {
            let extended = end.mul_add(direction, radius);
            push_polygon(
                out,
                vec![
                    end.mul_add(normal, radius),
                    extended.mul_add(normal, radius),
                    extended.mul_add(normal, -radius),
                    end.mul_add(normal, -radius),
                ],
            );
        }
        LineCap::Round => {
            // A half turn from `normal` round to `-normal`. `normal` is
            // `direction` rotated a quarter turn one way, so sweeping the other
            // way is what carries the arc out over `direction` rather than back
            // across the path.
            let mut half_disc = vec![end.mul_add(normal, radius)];
            emit_arc_sweep(&mut half_disc, end, normal, -core::f32::consts::PI, radius);
            half_disc.push(end.mul_add(normal, -radius));
            push_polygon(out, half_disc);
        }
    }
}

/// The area a cap paints where the path has no length at all.
///
/// SVG: a zero-length subpath paints nothing under a butt cap, a full circle
/// under a round cap, and a square under a square cap. The square's orientation
/// is formally undefined — there is no tangent to align it to — so it is
/// axis-aligned, matching what browsers do.
fn emit_degenerate_cap(out: &mut Vec<Vec<Vec2>>, point: Vec2, radius: f32, cap: LineCap) {
    match cap {
        LineCap::Butt => {}
        LineCap::Square => push_polygon(
            out,
            vec![
                Vec2::new(point.x - radius, point.y - radius),
                Vec2::new(point.x + radius, point.y - radius),
                Vec2::new(point.x + radius, point.y + radius),
                Vec2::new(point.x - radius, point.y + radius),
            ],
        ),
        LineCap::Round => {
            let mut circle = vec![Vec2::new(point.x + radius, point.y)];
            emit_arc_sweep(
                &mut circle,
                point,
                Vec2::new(1.0, 0.0),
                core::f32::consts::TAU,
                radius,
            );
            push_polygon(out, circle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edges::{CoverageRasterizer, EdgeSet, ScanlineSupersampler};
    use svg_core::FillRule;

    fn style(width: f32, cap: LineCap, join: LineJoin, miter_limit: f32) -> StrokeStyle {
        StrokeStyle {
            width,
            cap,
            join,
            miter_limit,
        }
    }

    fn contour(points: &[(f32, f32)], closed: bool) -> Contour {
        Contour {
            points: points.iter().map(|&(x, y)| Vec2::new(x, y)).collect(),
            closed,
        }
    }

    fn stroke(contours: &[Contour], style: StrokeStyle) -> Vec<Vec<Vec2>> {
        let mut out = Vec::new();
        expand(contours, style, &mut out);
        out
    }

    fn bounds(polygons: &[Vec<Vec2>]) -> (f32, f32, f32, f32) {
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for polygon in polygons {
            for point in polygon {
                min_x = min_x.min(point.x);
                min_y = min_y.min(point.y);
                max_x = max_x.max(point.x);
                max_y = max_y.max(point.y);
            }
        }
        (min_x, min_y, max_x, max_y)
    }

    fn all_finite(polygons: &[Vec<Vec2>]) -> bool {
        polygons
            .iter()
            .all(|p| p.iter().all(|point| point.is_finite()))
    }

    /// Scan-converts the expansion, which is the only way to check the property
    /// that actually matters: what area the stroke ends up covering.
    fn coverage(polygons: &[Vec<Vec2>], width: u32, height: u32) -> Vec<f32> {
        let mut edges = EdgeSet::new();
        for polygon in polygons {
            edges.add_polygon(polygon);
        }
        edges.finish();

        let mut out = vec![0.0; (width * height) as usize];
        let mut sampler = ScanlineSupersampler::default();
        sampler.rasterize(
            &edges,
            FillRule::NonZero,
            width,
            height,
            |y, row, start, end| {
                for x in start..end {
                    out[y as usize * width as usize + x] = row[x];
                }
            },
        );
        out
    }

    /// Every piece must wind the same way, or the non-zero union would
    /// subtract one from another instead of merging them.
    fn windings_agree(polygons: &[Vec<Vec2>]) -> bool {
        polygons
            .iter()
            .filter(|p| signed_area(p).abs() > 1e-6)
            .all(|p| signed_area(p) > 0.0)
    }

    // ---- caps ------------------------------------------------------------

    /// A butt cap stops exactly at the endpoint: the stroke of a 10-unit
    /// horizontal line at width 4 is precisely 10 x 4.
    #[test]
    fn butt_caps_do_not_extend_the_path() {
        let polygons = stroke(
            &[contour(&[(0.0, 0.0), (10.0, 0.0)], false)],
            style(4.0, LineCap::Butt, LineJoin::Miter, 4.0),
        );
        let (min_x, min_y, max_x, max_y) = bounds(&polygons);
        assert!((min_x - 0.0).abs() < 1e-4, "{min_x}");
        assert!((max_x - 10.0).abs() < 1e-4, "{max_x}");
        assert!((min_y + 2.0).abs() < 1e-4);
        assert!((max_y - 2.0).abs() < 1e-4);
    }

    #[test]
    fn square_caps_extend_by_half_the_width() {
        let polygons = stroke(
            &[contour(&[(0.0, 0.0), (10.0, 0.0)], false)],
            style(4.0, LineCap::Square, LineJoin::Miter, 4.0),
        );
        let (min_x, min_y, max_x, max_y) = bounds(&polygons);
        assert!((min_x + 2.0).abs() < 1e-4, "{min_x}");
        assert!((max_x - 12.0).abs() < 1e-4, "{max_x}");
        assert!((min_y + 2.0).abs() < 1e-4);
        assert!((max_y - 2.0).abs() < 1e-4);
    }

    /// A round cap is a half disc about the endpoint. Arcs are inscribed, so it
    /// falls at most [`ARC_TOLERANCE`] short of the true circle — and never
    /// beyond it.
    #[test]
    fn round_caps_are_a_half_disc_about_the_endpoint() {
        let polygons = stroke(
            &[contour(&[(0.0, 0.0), (10.0, 0.0)], false)],
            style(4.0, LineCap::Round, LineJoin::Miter, 4.0),
        );
        let (min_x, min_y, max_x, max_y) = bounds(&polygons);
        assert!(
            (-2.0 - 1e-4..=-2.0 + ARC_TOLERANCE).contains(&min_x),
            "{min_x}"
        );
        assert!(
            (12.0 - ARC_TOLERANCE..=12.0 + 1e-4).contains(&max_x),
            "{max_x}"
        );
        assert!((min_y + 2.0).abs() < 1e-4);
        assert!((max_y - 2.0).abs() < 1e-4);

        // Every cap point sits on its endpoint's circle, to within the chord
        // tolerance. Points from the segment quad have x in 0..10.
        for polygon in &polygons {
            for point in polygon {
                if (0.0..=10.0).contains(&point.x) {
                    continue;
                }
                let centre = if point.x < 0.0 {
                    Vec2::new(0.0, 0.0)
                } else {
                    Vec2::new(10.0, 0.0)
                };
                let distance = (*point - centre).length();
                assert!((distance - 2.0).abs() < 1e-3, "{point:?} at {distance}");
            }
        }
    }

    #[test]
    fn a_zero_length_subpath_paints_only_under_a_cap_with_area() {
        let point = contour(&[(5.0, 5.0)], false);

        assert!(
            stroke(
                std::slice::from_ref(&point),
                style(4.0, LineCap::Butt, LineJoin::Miter, 4.0)
            )
            .is_empty()
        );

        let square = stroke(
            std::slice::from_ref(&point),
            style(4.0, LineCap::Square, LineJoin::Miter, 4.0),
        );
        assert_eq!(bounds(&square), (3.0, 3.0, 7.0, 7.0));

        let round = stroke(&[point], style(4.0, LineCap::Round, LineJoin::Miter, 4.0));
        let (min_x, min_y, max_x, max_y) = bounds(&round);
        for (value, expected) in [(min_x, 3.0), (min_y, 3.0), (max_x, 7.0), (max_y, 7.0)] {
            assert!(
                (value - expected).abs() <= ARC_TOLERANCE,
                "{value} vs {expected}"
            );
        }
    }

    /// A dot must actually cover pixels, not merely produce points.
    #[test]
    fn a_round_dot_fills_a_disc() {
        let polygons = stroke(
            &[contour(&[(8.0, 8.0)], false)],
            style(8.0, LineCap::Round, LineJoin::Miter, 4.0),
        );
        let out = coverage(&polygons, 16, 16);
        assert!((out[8 * 16 + 8] - 1.0).abs() < 1e-3, "centre");
        assert!(out[0].abs() < 1e-3, "corner is outside the disc");
        let total: f32 = out.iter().sum();
        // Area of a radius-4 disc is about 50.3.
        assert!((total - 50.3).abs() < 1.5, "total was {total}");
    }

    // ---- joins -----------------------------------------------------------

    /// A right-angle miter reaches `radius * sqrt(2)` past the corner, which
    /// for an axis-aligned corner puts the apex exactly on both offsets.
    #[test]
    fn a_right_angle_miter_reaches_the_expected_apex() {
        let polygons = stroke(
            &[contour(&[(0.0, 10.0), (10.0, 10.0), (10.0, 0.0)], false)],
            style(4.0, LineCap::Butt, LineJoin::Miter, 4.0),
        );
        let apex = Vec2::new(12.0, 12.0);
        assert!(
            polygons
                .iter()
                .flatten()
                .any(|p| (*p - apex).length() < 1e-4),
            "expected a miter apex at (12, 12): {polygons:?}"
        );
    }

    /// Past the limit, SVG requires the miter to become a bevel. This corner is
    /// a narrow spike whose ratio is about 13.5.
    #[test]
    fn a_miter_beyond_the_limit_falls_back_to_bevel() {
        let sharp = contour(&[(0.0, 0.0), (20.0, 0.0), (0.0, 3.0)], false);

        let generous = stroke(
            std::slice::from_ref(&sharp),
            style(2.0, LineCap::Butt, LineJoin::Miter, 20.0),
        );
        let strict = stroke(
            std::slice::from_ref(&sharp),
            style(2.0, LineCap::Butt, LineJoin::Miter, 2.0),
        );
        let bevelled = stroke(&[sharp], style(2.0, LineCap::Butt, LineJoin::Bevel, 2.0));

        let (_, _, generous_max_x, _) = bounds(&generous);
        let (_, _, strict_max_x, _) = bounds(&strict);
        let (_, _, bevel_max_x, _) = bounds(&bevelled);

        assert!(
            generous_max_x > 30.0,
            "a long miter should overhang: {generous_max_x}"
        );
        assert!(
            (strict_max_x - bevel_max_x).abs() < 1e-4,
            "{strict_max_x} should have bevelled to {bevel_max_x}"
        );
    }

    /// The limit is a ratio, so the same corner behaves differently either side
    /// of its exact value. A right angle's ratio is sqrt(2) ~ 1.4142.
    #[test]
    fn the_miter_limit_is_applied_as_svg_defines_it() {
        let corner = contour(&[(0.0, 10.0), (10.0, 10.0), (10.0, 0.0)], false);
        let apex = Vec2::new(12.0, 12.0);
        let has_apex = |polygons: &[Vec<Vec2>]| {
            polygons
                .iter()
                .flatten()
                .any(|p| (*p - apex).length() < 1e-4)
        };

        let above = stroke(
            std::slice::from_ref(&corner),
            style(4.0, LineCap::Butt, LineJoin::Miter, 1.5),
        );
        let below = stroke(&[corner], style(4.0, LineCap::Butt, LineJoin::Miter, 1.3));

        assert!(has_apex(&above), "1.5 > sqrt(2): should be mitred");
        assert!(!has_apex(&below), "1.3 < sqrt(2): should be bevelled");
    }

    #[test]
    fn a_round_join_stays_on_the_circle_about_the_vertex() {
        let polygons = stroke(
            &[contour(&[(0.0, 10.0), (10.0, 10.0), (10.0, 0.0)], false)],
            style(4.0, LineCap::Butt, LineJoin::Round, 4.0),
        );
        let vertex = Vec2::new(10.0, 10.0);

        // The join wedge is the only polygon containing the vertex itself.
        let wedge = polygons
            .iter()
            .find(|polygon| polygon.iter().any(|p| (*p - vertex).length() < 1e-6))
            .expect("expected a join wedge");

        for point in wedge {
            let distance = (*point - vertex).length();
            assert!(
                distance < 1e-6 || (distance - 2.0).abs() < 1e-3,
                "{point:?} at {distance}"
            );
        }
        // A round join is many chords; a bevel would be a bare triangle.
        assert!(wedge.len() > 4, "{} points", wedge.len());
    }

    #[test]
    fn a_bevel_join_is_a_bare_triangle_and_does_not_overhang() {
        let polygons = stroke(
            &[contour(&[(0.0, 10.0), (10.0, 10.0), (10.0, 0.0)], false)],
            style(4.0, LineCap::Butt, LineJoin::Bevel, 4.0),
        );
        let vertex = Vec2::new(10.0, 10.0);
        let wedge = polygons
            .iter()
            .find(|polygon| polygon.iter().any(|p| (*p - vertex).length() < 1e-6))
            .expect("expected a join wedge");
        assert_eq!(wedge.len(), 3);

        let (_, _, max_x, max_y) = bounds(&polygons);
        assert!((max_x - 12.0).abs() < 1e-4);
        assert!((max_y - 12.0).abs() < 1e-4);
    }

    /// The three join styles must differ where it counts — right at the corner
    /// — and agree everywhere else.
    #[test]
    fn the_three_join_styles_cover_different_corners() {
        let corner = contour(&[(4.0, 20.0), (20.0, 20.0), (20.0, 4.0)], false);
        let render = |join| {
            coverage(
                &stroke(
                    std::slice::from_ref(&corner),
                    style(8.0, LineCap::Butt, join, 8.0),
                ),
                32,
                32,
            )
        };
        let miter = render(LineJoin::Miter);
        let round = render(LineJoin::Round);
        let bevel = render(LineJoin::Bevel);

        let at = |image: &Vec<f32>, x: usize, y: usize| image[y * 32 + x];
        // A pixel just outside the round join's arc but inside the miter's apex.
        assert!(at(&miter, 23, 23) > 0.9, "miter fills the corner");
        assert!(at(&round, 23, 23) < 0.1, "round is cut back");
        assert!(at(&bevel, 23, 23) < 0.1, "bevel is cut back");
        // A pixel inside the round arc but outside the bevel's chord.
        assert!(at(&round, 22, 21) > at(&bevel, 22, 21));
        // Well inside the stroke, all three agree.
        assert!((at(&miter, 10, 20) - at(&round, 10, 20)).abs() < 1e-3);
    }

    /// Nearly-collinear segments have an almost-zero cross product. The join
    /// must degrade gracefully rather than divide by it.
    #[test]
    fn nearly_collinear_segments_are_handled_without_blowing_up() {
        for epsilon in [1e-7f32, 1e-5, 1e-3] {
            for join in [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel] {
                let polygons = stroke(
                    &[contour(&[(0.0, 0.0), (10.0, 0.0), (20.0, epsilon)], false)],
                    style(2.0, LineCap::Butt, join, 4.0),
                );
                assert!(all_finite(&polygons), "{join:?} at {epsilon}");
                assert!(windings_agree(&polygons), "{join:?} at {epsilon}");
                let (_, _, max_x, _) = bounds(&polygons);
                assert!(max_x < 21.0, "{join:?} at {epsilon}: {max_x}");
            }
        }
    }

    /// A path doubling back on itself is the infinite-miter case.
    #[test]
    fn a_cusp_does_not_produce_an_infinite_miter() {
        for join in [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel] {
            let polygons = stroke(
                &[contour(&[(0.0, 0.0), (10.0, 0.0), (0.0, 0.0001)], false)],
                style(2.0, LineCap::Butt, join, 100.0),
            );
            assert!(all_finite(&polygons), "{join:?}");
            let (_, _, max_x, _) = bounds(&polygons);
            assert!(max_x < 100.0, "{join:?}: {max_x}");
        }
    }

    /// At a cusp a round join must still pivot the pen through a full turn, or
    /// the hairpin's tip would be squared off.
    #[test]
    fn a_round_join_at_a_cusp_sweeps_a_full_disc() {
        let polygons = stroke(
            &[contour(&[(0.0, 8.0), (12.0, 8.0), (0.0, 8.0000001)], false)],
            style(8.0, LineCap::Butt, LineJoin::Round, 4.0),
        );
        let (_, _, max_x, _) = bounds(&polygons);
        assert!((max_x - 16.0).abs() < ARC_TOLERANCE + 1e-3, "{max_x}");
    }

    // ---- closed contours -------------------------------------------------

    /// A closed stroke is a band: solid where the pen went, empty inside.
    #[test]
    fn a_closed_contour_strokes_a_hollow_band() {
        let polygons = stroke(
            &[contour(
                &[(4.0, 4.0), (28.0, 4.0), (28.0, 28.0), (4.0, 28.0)],
                true,
            )],
            style(4.0, LineCap::Butt, LineJoin::Miter, 4.0),
        );
        assert!(windings_agree(&polygons));

        let out = coverage(&polygons, 32, 32);
        let at = |x: usize, y: usize| out[y * 32 + x];
        assert!((at(16, 4) - 1.0).abs() < 1e-3, "on the top edge");
        assert!((at(4, 16) - 1.0).abs() < 1e-3, "on the left edge");
        assert!(at(16, 16).abs() < 1e-3, "the middle must stay hollow");
        assert!(at(16, 0).abs() < 1e-3, "outside the band");
        // The corners are mitred, so the outer boundary reaches (2, 2).
        assert!((at(2, 2) - 1.0).abs() < 1e-3, "mitred outer corner");
    }

    #[test]
    fn a_closed_contour_has_no_caps() {
        let square = contour(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)], true);
        let butt = stroke(
            std::slice::from_ref(&square),
            style(2.0, LineCap::Butt, LineJoin::Miter, 4.0),
        );
        let round = stroke(&[square], style(2.0, LineCap::Round, LineJoin::Miter, 4.0));
        assert_eq!(bounds(&butt), bounds(&round));
    }

    /// Every corner is joined, including the one at the contour's start point,
    /// which is the one an off-by-one loses.
    #[test]
    fn every_corner_of_a_closed_contour_is_mitred() {
        let polygons = stroke(
            &[contour(
                &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)],
                true,
            )],
            style(2.0, LineCap::Butt, LineJoin::Miter, 4.0),
        );
        for apex in [
            Vec2::new(-1.0, -1.0),
            Vec2::new(11.0, -1.0),
            Vec2::new(11.0, 11.0),
            Vec2::new(-1.0, 11.0),
        ] {
            assert!(
                polygons
                    .iter()
                    .flatten()
                    .any(|p| (*p - apex).length() < 1e-4),
                "no miter apex at {apex:?}"
            );
        }
    }

    /// The direction a contour was authored in must not change its stroke.
    #[test]
    fn contour_orientation_does_not_change_the_stroke() {
        let forward = contour(&[(4.0, 4.0), (28.0, 4.0), (28.0, 28.0), (4.0, 28.0)], true);
        let backward = contour(&[(4.0, 28.0), (28.0, 28.0), (28.0, 4.0), (4.0, 4.0)], true);
        let render = |c: Contour| {
            coverage(
                &stroke(&[c], style(4.0, LineCap::Butt, LineJoin::Miter, 4.0)),
                32,
                32,
            )
        };
        let a = render(forward);
        let b = render(backward);
        for (index, (left, right)) in a.iter().zip(b.iter()).enumerate() {
            assert!(
                (left - right).abs() < 1e-3,
                "pixel {index}: {left} vs {right}"
            );
        }
    }

    // ---- overlap and winding --------------------------------------------

    /// A stroke crossing itself must be solid, not doubly blended and not
    /// holed. That is exactly what the non-zero union buys.
    #[test]
    fn a_self_crossing_stroke_stays_solid() {
        let polygons = stroke(
            &[contour(
                &[(2.0, 2.0), (30.0, 30.0), (2.0, 30.0), (30.0, 2.0)],
                false,
            )],
            style(6.0, LineCap::Butt, LineJoin::Miter, 4.0),
        );
        let out = coverage(&polygons, 32, 32);
        assert!(out.iter().all(|&v| v <= 1.0 + 1e-4));
        // The crossing point in the middle must be covered once, fully.
        assert!(
            (out[16 * 32 + 16] - 1.0).abs() < 1e-3,
            "{}",
            out[16 * 32 + 16]
        );
    }

    #[test]
    fn all_pieces_wind_the_same_way() {
        let shapes = [
            contour(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], false),
            contour(&[(10.0, 10.0), (10.0, 0.0), (0.0, 0.0)], false),
            contour(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)], true),
            contour(&[(0.0, 10.0), (10.0, 10.0), (10.0, 0.0), (0.0, 0.0)], true),
        ];
        for cap in [LineCap::Butt, LineCap::Round, LineCap::Square] {
            for join in [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel] {
                for shape in &shapes {
                    let polygons = stroke(std::slice::from_ref(shape), style(3.0, cap, join, 4.0));
                    assert!(windings_agree(&polygons), "{cap:?} {join:?} {shape:?}");
                }
            }
        }
    }

    // ---- degenerate input ------------------------------------------------

    #[test]
    fn a_non_positive_width_paints_nothing() {
        let line = contour(&[(0.0, 0.0), (10.0, 0.0)], false);
        for width in [0.0f32, -1.0, f32::NAN] {
            assert!(
                stroke(
                    std::slice::from_ref(&line),
                    style(width, LineCap::Round, LineJoin::Round, 4.0)
                )
                .is_empty(),
                "{width}"
            );
        }
    }

    #[test]
    fn an_empty_contour_list_produces_nothing() {
        assert!(stroke(&[], style(2.0, LineCap::Round, LineJoin::Round, 4.0)).is_empty());
    }

    #[test]
    fn a_very_thin_stroke_still_produces_geometry() {
        let polygons = stroke(
            &[contour(&[(0.0, 0.0), (10.0, 0.0)], false)],
            style(0.01, LineCap::Round, LineJoin::Round, 4.0),
        );
        assert!(!polygons.is_empty());
        assert!(all_finite(&polygons));
    }

    #[test]
    fn a_very_wide_stroke_stays_finite() {
        let polygons = stroke(
            &[contour(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)], false)],
            style(10_000.0, LineCap::Round, LineJoin::Round, 100.0),
        );
        assert!(all_finite(&polygons));
        assert!(windings_agree(&polygons));
    }

    #[test]
    fn coincident_points_do_not_produce_non_finite_geometry() {
        let polygons = stroke(
            &[contour(
                &[(5.0, 5.0), (5.0, 5.0), (9.0, 5.0), (9.0, 5.0)],
                false,
            )],
            style(2.0, LineCap::Round, LineJoin::Round, 4.0),
        );
        assert!(all_finite(&polygons));
        let (min_x, _, max_x, _) = bounds(&polygons);
        assert!(
            min_x >= 4.0 - 1e-3 && max_x <= 10.0 + 1e-3,
            "{min_x}..{max_x}"
        );
    }

    #[test]
    fn expansion_is_deterministic() {
        let path = contour(&[(0.0, 0.0), (10.0, 3.0), (4.0, 9.0), (12.0, 12.0)], false);
        let run = || {
            stroke(
                std::slice::from_ref(&path),
                style(3.0, LineCap::Round, LineJoin::Round, 4.0),
            )
        };
        assert_eq!(run(), run());
    }
}
