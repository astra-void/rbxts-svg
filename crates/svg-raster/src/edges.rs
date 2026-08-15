//! Directed edges: the one geometry representation everything is rasterized
//! from.
//!
//! Fills and strokes converge here. A fill contributes its contours directly; a
//! stroke is first expanded into the polygons that outline it
//! ([`crate::stroke`]) and contributes *those*. Nothing in this crate draws a
//! line — a stroke is an area like any other, which is what makes caps, joins
//! and self-overlap fall out of the fill rule instead of needing their own
//! blending rules.
//!
//! # Representation
//!
//! Each non-horizontal segment becomes an [`Edge`] normalised to point
//! downwards, remembering with `winding` whether it originally did. Horizontal
//! segments are dropped: a scanline never crosses one, so it can only
//! contribute to the count spuriously.
//!
//! Edges are then sorted by their top y. Combined with an active-edge list that
//! is advanced as the scan moves down, that gives the classic active edge table
//! — each edge is looked at only on the rows it actually spans, rather than
//! every edge being tested against every row.

use svg_core::FillRule;

use crate::geom::Vec2;

/// A directed, non-horizontal segment, normalised to run downwards.
#[derive(Debug, Clone, Copy)]
pub struct Edge {
    /// Smaller y. The edge is live for scanlines in `[y_top, y_bottom)`.
    pub y_top: f32,
    pub y_bottom: f32,
    /// x where the edge meets `y_top`.
    pub x_top: f32,
    /// dx/dy, finite because `y_bottom > y_top` by construction.
    pub dx_dy: f32,
    /// `+1` if the segment originally ran downwards, `-1` if upwards. This is
    /// the sign the non-zero rule accumulates.
    pub winding: i32,
}

/// A set of edges ready to be scan-converted.
///
/// Reused across shapes: [`Self::clear`] keeps the allocation, which matters
/// because a document is a few dozen shapes and each one would otherwise
/// allocate twice.
#[derive(Debug)]
pub struct EdgeSet {
    edges: Vec<Edge>,
    y_min: f32,
    y_max: f32,
}

/// Written out rather than derived: the derived default would set the y bounds
/// to zero, which is not "no bounds yet" but "the bounds are exactly the
/// origin". `clear` already establishes the right sentinels, so the two ways of
/// getting an empty set had better agree.
impl Default for EdgeSet {
    fn default() -> Self {
        Self {
            edges: Vec::new(),
            y_min: f32::MAX,
            y_max: f32::MIN,
        }
    }
}

impl EdgeSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.edges.clear();
        self.y_min = f32::MAX;
        self.y_max = f32::MIN;
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// Adds a polygon, closing it implicitly.
    ///
    /// Implicit closure is not a convenience: SVG fills a subpath as if it were
    /// closed whether or not the author wrote `Z`, so the wrap-around edge is
    /// part of the specification. Stroke outlines are closed by construction,
    /// so the same rule serves both.
    pub fn add_polygon(&mut self, points: &[Vec2]) {
        if points.len() < 2 {
            return;
        }
        for index in 0..points.len() {
            let a = points[index];
            let b = points[(index + 1) % points.len()];
            self.add_segment(a, b);
        }
    }

    fn add_segment(&mut self, a: Vec2, b: Vec2) {
        // Non-finite input cannot reach here through `flatten`, which rejects
        // it, but the stroker also produces points and a guard here is cheaper
        // than an invariant spread across two modules.
        if !(a.is_finite() && b.is_finite()) || a.y == b.y {
            return;
        }

        let (top, bottom, winding) = if a.y < b.y { (a, b, 1) } else { (b, a, -1) };
        let dy = bottom.y - top.y;
        let dx_dy = (bottom.x - top.x) / dy;
        if !dx_dy.is_finite() {
            return;
        }

        self.y_min = self.y_min.min(top.y);
        self.y_max = self.y_max.max(bottom.y);
        self.edges.push(Edge {
            y_top: top.y,
            y_bottom: bottom.y,
            x_top: top.x,
            dx_dy,
            winding,
        });
    }

    /// Sorts edges by their top y, which the active edge table relies on.
    ///
    /// A total ordering on the bits — rather than a partial one on the values —
    /// means the sort cannot depend on how ties happen to be laid out, so the
    /// output stays a pure function of the input.
    pub fn finish(&mut self) {
        self.edges.sort_by(|a, b| a.y_top.total_cmp(&b.y_top));
    }

    /// The rows this edge set can possibly touch, clipped to `height`.
    ///
    /// Everything outside contributes nothing, and skipping it is what keeps a
    /// small icon in a large raster cheap.
    pub fn row_range(&self, height: u32) -> (u32, u32) {
        if self.edges.is_empty() {
            return (0, 0);
        }
        let first = self.y_min.max(0.0).floor();
        let last = self.y_max.min(height as f32).ceil();
        if !(first.is_finite() && last.is_finite()) || last <= first {
            return (0, 0);
        }
        (first as u32, (last as u32).min(height))
    }

    fn edges(&self) -> &[Edge] {
        &self.edges
    }
}

/// How coverage is estimated from an edge set.
///
/// # Why this is a trait
///
/// The coverage strategy is the part of a rasterizer most likely to be replaced
/// — today's vertical supersampling is a deliberately simple starting point,
/// and analytical edge coverage is the obvious upgrade. Putting it behind an
/// interface means that upgrade touches this file and nothing else: flattening,
/// stroke expansion, fill rules and compositing all sit on either side of it
/// and none of them know how a coverage number was arrived at.
///
/// It is `pub(crate)` on purpose. Callers choose *what* to draw, not how it is
/// anti-aliased, and an AA strategy exposed as public API is an AA strategy
/// that can no longer change.
pub(crate) trait CoverageRasterizer {
    /// Computes coverage row by row, calling `emit` with each row's index and a
    /// slice of per-pixel coverage in `0.0..=1.0`, plus the half-open range of
    /// pixels that are actually non-zero.
    ///
    /// Rows are visited in ascending order and each is visited once — the
    /// active edge table is a forward-only walk.
    fn rasterize<F>(&mut self, edges: &EdgeSet, rule: FillRule, width: u32, height: u32, emit: F)
    where
        F: FnMut(u32, &[f32], usize, usize);
}

/// Sub-scanlines sampled per pixel row.
///
/// # Why this number
///
/// Coverage is exact in x — every span contributes its true fractional width to
/// the pixels it partly covers — and sampled in y. So this count quantises
/// vertical coverage only, in steps of `1/16` of a pixel, and the worst case is
/// a feature that is both nearly horizontal and thinner than a pixel: a
/// half-pixel-tall band can be off by up to `1/32`, about 8 of 255 alpha
/// levels.
///
/// Four samples is the conventional choice and would put that error near 32
/// levels, which is visible on a hairline. Sixteen brings it under a tenth of
/// that for four times the sub-scanline work — and since the work per
/// sub-scanline is proportional to the handful of edges crossing that row, not
/// to the raster width, it stays cheap enough to reproduce in Luau.
///
/// A power of two also makes the weight `1/16` exact in binary, so the
/// accumulated coverage of a fully covered pixel is exactly 1 rather than
/// 0.99999994 — one less source of drift between two implementations.
///
/// Raising this further has diminishing returns; the real fix is analytical
/// coverage in y, which is what [`CoverageRasterizer`] exists to allow without
/// disturbing anything else.
const SUB_SCANLINES: u32 = 16;

/// Vertical supersampling with exact horizontal coverage.
///
/// For each of [`SUB_SCANLINES`] sample lines through a pixel row, the
/// intersections with every active edge are found, sorted, and walked to
/// produce the intervals the fill rule says are inside. Each interval adds its
/// exact width to the pixels it covers — fully for the interior, fractionally
/// for the two ends — weighted by `1 / SUB_SCANLINES`.
///
/// The result is exact along x and quantised along y. That asymmetry is
/// deliberate: exact-in-x costs nothing (the span endpoints are already
/// floating point) and it is where most of the visible quality is, while
/// exact-in-y would require the analytical accumulation this scheme exists to
/// defer.
///
/// It is also about as simple a scheme as can be ported to Luau and still
/// produce output worth comparing against.
#[derive(Debug, Default)]
pub(crate) struct ScanlineSupersampler {
    /// Indices into the edge set, for edges the scan has reached but not passed.
    active: Vec<usize>,
    /// Scratch for one sub-scanline's intersections: `(x, winding)`.
    crossings: Vec<(f32, i32)>,
    /// One row of accumulated coverage. Reused, and cleared only over the range
    /// that was written.
    row: Vec<f32>,
}

impl CoverageRasterizer for ScanlineSupersampler {
    fn rasterize<F>(
        &mut self,
        edges: &EdgeSet,
        rule: FillRule,
        width: u32,
        height: u32,
        mut emit: F,
    ) where
        F: FnMut(u32, &[f32], usize, usize),
    {
        let (first_row, last_row) = edges.row_range(height);
        if width == 0 || first_row >= last_row {
            return;
        }

        let all = edges.edges();
        self.active.clear();
        self.row.clear();
        self.row.resize(width as usize, 0.0);

        // Edges are sorted by `y_top`, so a single cursor advancing with the
        // scan admits each edge exactly once.
        let mut next_edge = 0usize;
        while next_edge < all.len() && all[next_edge].y_top < first_row as f32 {
            if all[next_edge].y_bottom > first_row as f32 {
                self.active.push(next_edge);
            }
            next_edge += 1;
        }

        let weight = 1.0 / SUB_SCANLINES as f32;

        for y in first_row..last_row {
            let row_top = y as f32;
            let row_bottom = row_top + 1.0;

            while next_edge < all.len() && all[next_edge].y_top < row_bottom {
                self.active.push(next_edge);
                next_edge += 1;
            }
            self.active.retain(|&index| all[index].y_bottom > row_top);
            if self.active.is_empty() {
                continue;
            }

            let mut dirty_start = width as usize;
            let mut dirty_end = 0usize;

            for sample in 0..SUB_SCANLINES {
                let sample_y = row_top + (sample as f32 + 0.5) * weight;

                self.crossings.clear();
                for &index in &self.active {
                    let edge = &all[index];
                    // Half-open in y, so a vertex shared by two edges is
                    // counted once: the edge ending there does not fire, the
                    // one starting there does.
                    if sample_y < edge.y_top || sample_y >= edge.y_bottom {
                        continue;
                    }
                    let x = edge.x_top + (sample_y - edge.y_top) * edge.dx_dy;
                    if x.is_finite() {
                        self.crossings.push((x, edge.winding));
                    }
                }
                if self.crossings.len() < 2 {
                    continue;
                }
                self.crossings.sort_by(|a, b| a.0.total_cmp(&b.0));

                let mut winding = 0i32;
                let mut span_start = 0.0f32;
                let mut inside = false;

                for &(x, edge_winding) in &self.crossings {
                    let was_inside = inside;
                    winding += edge_winding;
                    inside = match rule {
                        // Non-zero: inside wherever the accumulated winding is
                        // not zero. Two nested contours wound the same way stay
                        // inside; wound oppositely, the inner one cancels out
                        // and becomes a hole.
                        FillRule::NonZero => winding != 0,
                        // Even-odd: inside on every odd crossing, whatever the
                        // directions were. Counted from the crossings
                        // themselves rather than inferred from orientation,
                        // which is the only way it is actually correct.
                        FillRule::EvenOdd => (winding & 1) != 0,
                    };

                    if !was_inside && inside {
                        span_start = x;
                    } else if was_inside && !inside {
                        accumulate_span(
                            &mut self.row,
                            span_start,
                            x,
                            weight,
                            width,
                            &mut dirty_start,
                            &mut dirty_end,
                        );
                    }
                }
            }

            if dirty_start < dirty_end {
                emit(y, &self.row, dirty_start, dirty_end);
                for value in &mut self.row[dirty_start..dirty_end] {
                    *value = 0.0;
                }
            }
        }
    }
}

/// Adds `weight` × the horizontal overlap of `[x0, x1)` to each pixel it
/// touches.
///
/// This is where "exact in x" happens: a span ending a third of the way into a
/// pixel adds a third of the weight to it, rather than a whole one or none.
#[allow(clippy::too_many_arguments)]
fn accumulate_span(
    row: &mut [f32],
    x0: f32,
    x1: f32,
    weight: f32,
    width: u32,
    dirty_start: &mut usize,
    dirty_end: &mut usize,
) {
    let width_f = width as f32;
    let start = x0.max(0.0).min(width_f);
    let end = x1.max(0.0).min(width_f);
    if end <= start {
        return;
    }

    let first = start.floor();
    let last = end.floor();
    let first_index = first as usize;
    // `end` can land exactly on the right edge, whose floor is one past the
    // last pixel; the interior loop stops before it and the trailing partial is
    // skipped, so no clamp is needed beyond this bound.
    let last_index = (last as usize).min(row.len());

    *dirty_start = (*dirty_start).min(first_index);
    *dirty_end = (*dirty_end).max((last_index + 1).min(row.len()));

    if first_index == last_index {
        if first_index < row.len() {
            row[first_index] += (end - start) * weight;
        }
        return;
    }

    let length = row.len();
    if first_index < length {
        row[first_index] += (first + 1.0 - start) * weight;
    }
    for value in row
        .iter_mut()
        .take(last_index.min(length))
        .skip(first_index + 1)
    {
        *value += weight;
    }
    if last_index < length {
        row[last_index] += (end - last) * weight;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rasterizes into a plain coverage buffer, for tests that care about
    /// geometry rather than colour.
    fn coverage(polygons: &[&[Vec2]], rule: FillRule, width: u32, height: u32) -> Vec<f32> {
        let mut edges = EdgeSet::new();
        for polygon in polygons {
            edges.add_polygon(polygon);
        }
        edges.finish();

        let mut out = vec![0.0; (width * height) as usize];
        let mut sampler = ScanlineSupersampler::default();
        sampler.rasterize(&edges, rule, width, height, |y, row, start, end| {
            for x in start..end {
                out[y as usize * width as usize + x] = row[x];
            }
        });
        out
    }

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Vec2> {
        vec![
            Vec2::new(x0, y0),
            Vec2::new(x1, y0),
            Vec2::new(x1, y1),
            Vec2::new(x0, y1),
        ]
    }

    fn reversed(points: &[Vec2]) -> Vec<Vec2> {
        points.iter().rev().copied().collect()
    }

    #[test]
    fn a_pixel_aligned_rectangle_is_fully_covered_with_no_fringe() {
        let square = rect(2.0, 2.0, 6.0, 6.0);
        let out = coverage(&[&square], FillRule::NonZero, 8, 8);

        for y in 0..8 {
            for x in 0..8 {
                let expected = if (2..6).contains(&x) && (2..6).contains(&y) {
                    1.0
                } else {
                    0.0
                };
                assert!(
                    (out[y * 8 + x] - expected).abs() < 1e-4,
                    "({x}, {y}) was {}",
                    out[y * 8 + x]
                );
            }
        }
    }

    #[test]
    fn a_half_pixel_offset_edge_is_half_covered() {
        let out = coverage(&[&rect(0.5, 0.0, 3.0, 1.0)], FillRule::NonZero, 4, 1);
        assert!((out[0] - 0.5).abs() < 1e-4, "{}", out[0]);
        assert!((out[1] - 1.0).abs() < 1e-4);
        assert!((out[2] - 1.0).abs() < 1e-4);
        assert!((out[3] - 0.0).abs() < 1e-4);
    }

    /// Winding direction must not change what a single contour fills.
    #[test]
    fn orientation_does_not_change_a_simple_fill() {
        let square = rect(1.0, 1.0, 5.0, 5.0);
        let forward = coverage(&[&square], FillRule::NonZero, 6, 6);
        let backward = coverage(&[&reversed(&square)], FillRule::NonZero, 6, 6);
        assert_eq!(forward, backward);
    }

    // ---- fill rules ------------------------------------------------------

    #[test]
    fn nonzero_keeps_a_same_direction_inner_contour_solid() {
        let outer = rect(0.0, 0.0, 8.0, 8.0);
        let inner = rect(2.0, 2.0, 6.0, 6.0);
        let out = coverage(&[&outer, &inner], FillRule::NonZero, 8, 8);
        // Winding 2 in the middle is still non-zero, so there is no hole.
        assert!((out[4 * 8 + 4] - 1.0).abs() < 1e-4, "{}", out[4 * 8 + 4]);
    }

    #[test]
    fn nonzero_cuts_a_hole_for_an_opposite_direction_inner_contour() {
        let outer = rect(0.0, 0.0, 8.0, 8.0);
        let inner = reversed(&rect(2.0, 2.0, 6.0, 6.0));
        let out = coverage(&[&outer, &inner], FillRule::NonZero, 8, 8);
        assert!(out[4 * 8 + 4].abs() < 1e-4, "centre should be a hole");
        assert!((out[8 + 1] - 1.0).abs() < 1e-4, "ring should be solid");
    }

    /// The case that separates a real even-odd implementation from one faked
    /// with orientation: nested contours wound the *same* way.
    #[test]
    fn evenodd_cuts_a_hole_regardless_of_direction() {
        let outer = rect(0.0, 0.0, 8.0, 8.0);
        for inner in [
            rect(2.0, 2.0, 6.0, 6.0),
            reversed(&rect(2.0, 2.0, 6.0, 6.0)),
        ] {
            let out = coverage(&[&outer, &inner], FillRule::EvenOdd, 8, 8);
            assert!(out[4 * 8 + 4].abs() < 1e-4, "centre should be a hole");
            assert!((out[8 + 1] - 1.0).abs() < 1e-4, "ring should be solid");
        }
    }

    #[test]
    fn evenodd_and_nonzero_differ_on_overlapping_contours() {
        let a = rect(0.0, 0.0, 6.0, 6.0);
        let b = rect(3.0, 0.0, 9.0, 6.0);
        let nonzero = coverage(&[&a, &b], FillRule::NonZero, 10, 6);
        let evenodd = coverage(&[&a, &b], FillRule::EvenOdd, 10, 6);

        // The overlap is doubly wound: solid under non-zero, empty under
        // even-odd.
        assert!((nonzero[2 * 10 + 4] - 1.0).abs() < 1e-4);
        assert!(evenodd[2 * 10 + 4].abs() < 1e-4);
        // Outside the overlap both agree.
        assert!((nonzero[2 * 10 + 1] - 1.0).abs() < 1e-4);
        assert!((evenodd[2 * 10 + 1] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn three_nested_contours_alternate_under_evenodd() {
        let a = rect(0.0, 0.0, 12.0, 12.0);
        let b = rect(2.0, 2.0, 10.0, 10.0);
        let c = rect(4.0, 4.0, 8.0, 8.0);
        let out = coverage(&[&a, &b, &c], FillRule::EvenOdd, 12, 12);
        assert!((out[6 * 12 + 1] - 1.0).abs() < 1e-4, "outermost ring");
        assert!(out[6 * 12 + 3].abs() < 1e-4, "second ring is a hole");
        assert!((out[6 * 12 + 6] - 1.0).abs() < 1e-4, "core is filled again");
    }

    #[test]
    fn multiple_disjoint_subpaths_all_fill() {
        let a = rect(0.0, 0.0, 3.0, 3.0);
        let b = rect(5.0, 0.0, 8.0, 3.0);
        let out = coverage(&[&a, &b], FillRule::NonZero, 8, 3);
        assert!((out[1] - 1.0).abs() < 1e-4);
        assert!(out[4].abs() < 1e-4);
        assert!((out[6] - 1.0).abs() < 1e-4);
    }

    // ---- degenerate and out-of-range input ------------------------------

    #[test]
    fn geometry_outside_the_raster_is_clipped_not_wrapped() {
        let out = coverage(&[&rect(-50.0, -50.0, 2.0, 2.0)], FillRule::NonZero, 4, 4);
        assert!((out[0] - 1.0).abs() < 1e-4);
        assert!(out[3].abs() < 1e-4);
        assert!(out[4 * 3 + 3].abs() < 1e-4);
    }

    #[test]
    fn geometry_entirely_outside_the_raster_draws_nothing() {
        let out = coverage(
            &[&rect(100.0, 100.0, 200.0, 200.0)],
            FillRule::NonZero,
            4,
            4,
        );
        assert!(out.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn horizontal_and_degenerate_segments_contribute_nothing() {
        let mut edges = EdgeSet::new();
        edges.add_polygon(&[Vec2::new(0.0, 1.0), Vec2::new(5.0, 1.0)]);
        edges.add_polygon(&[Vec2::new(2.0, 2.0)]);
        assert!(edges.is_empty());
    }

    #[test]
    fn non_finite_points_are_dropped_rather_than_poisoning_the_scan() {
        let mut edges = EdgeSet::new();
        edges.add_polygon(&[
            Vec2::new(0.0, 0.0),
            Vec2::new(f32::NAN, 4.0),
            Vec2::new(4.0, 4.0),
        ]);
        edges.finish();
        let mut sampler = ScanlineSupersampler::default();
        // The point is that this terminates and writes nothing insane.
        sampler.rasterize(&edges, FillRule::NonZero, 4, 4, |_, row, start, end| {
            for value in &row[start..end] {
                assert!(value.is_finite());
            }
        });
    }

    #[test]
    fn a_zero_width_raster_is_a_no_op() {
        let out = coverage(&[&rect(0.0, 0.0, 4.0, 4.0)], FillRule::NonZero, 0, 4);
        assert!(out.is_empty());
    }

    #[test]
    fn coverage_never_exceeds_one() {
        // Ten stacked copies of the same square: winding 10, coverage still 1.
        let square = rect(1.0, 1.0, 5.0, 5.0);
        let polygons: Vec<&[Vec2]> = (0..10).map(|_| square.as_slice()).collect();
        let out = coverage(&polygons, FillRule::NonZero, 6, 6);
        assert!(out.iter().all(|&v| v <= 1.0 + 1e-4), "{out:?}");
    }

    #[test]
    fn a_triangle_covers_about_half_its_bounding_box() {
        let triangle = [
            Vec2::new(0.0, 0.0),
            Vec2::new(32.0, 0.0),
            Vec2::new(0.0, 32.0),
        ];
        let out = coverage(&[&triangle], FillRule::NonZero, 32, 32);
        let total: f32 = out.iter().sum();
        // Exact area is 512. Vertical sampling costs a little along the
        // hypotenuse; a percent is a generous bound on that.
        assert!((total - 512.0).abs() < 512.0 * 0.01, "total was {total}");
    }
}
