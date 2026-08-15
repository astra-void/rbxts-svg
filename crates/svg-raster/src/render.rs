//! The pipeline: a semantic document in, an image out.
//!
//! ```text
//! SvgDocument
//!     │
//!     ├─ view box + preserveAspectRatio + target size -> transform
//!     │
//!     └─ for each shape, in painter's order:
//!            geometry -> adaptive flattening -> device-space contours
//!                                                  │
//!                        ┌─────────────────────────┴────────────┐
//!                        ▼                                      ▼
//!                   fill edges                          stroke expansion
//!                (implicitly closed)                            │
//!                        │                                 stroke edges
//!                        └──────────────┬───────────────────────┘
//!                                       ▼
//!                             scan conversion + coverage
//!                                       ▼
//!                             source-over compositing
//! ```
//!
//! Two things are worth noticing in that diagram.
//!
//! The contours are flattened **once per shape** and used by both the fill and
//! the stroke. Flattening is the expensive part, and a shape that is both
//! filled and stroked would otherwise pay for it twice.
//!
//! Fill and stroke converge on the *same* scan conversion. A stroke is not
//! drawn; it is expanded into the region it covers and then filled. That is
//! what makes caps, joins, self-overlap and anti-aliasing behave identically
//! for both, rather than being solved twice and agreeing only approximately.

use svg_core::{Color, FillRule, Paint, PaintOrder, Shape, SvgDocument, Transform};

use crate::edges::{CoverageRasterizer, EdgeSet, ScanlineSupersampler};
use crate::error::{MAX_DIMENSION, RasterError};
use crate::flatten::{Contour, flatten_path};
use crate::geom::Vec2;
use crate::image::{Canvas, RasterImage};
use crate::options::{RasterMode, RasterOptions};
use crate::stroke::{StrokeStyle, expand};

/// Rasterizes a compiled document.
///
/// Shapes are drawn in painter's order — index 0 first, furthest back — and
/// composited source-over. Within a shape, [`PaintOrder`] decides whether the
/// fill or the stroke goes down first.
///
/// # Errors
///
/// [`RasterError::InvalidDimensions`] for a zero or oversized raster, and
/// [`RasterError::NonFiniteGeometry`] if the requested scale pushes a
/// coordinate out of `f32`'s range.
pub fn render(document: &SvgDocument, options: &RasterOptions) -> Result<RasterImage, RasterError> {
    if options.width == 0
        || options.height == 0
        || options.width > MAX_DIMENSION
        || options.height > MAX_DIMENSION
    {
        return Err(RasterError::InvalidDimensions {
            width: options.width,
            height: options.height,
        });
    }

    // The single definition of viewport fitting, shared with the compiler and
    // with the runtime. Deriving a scale here instead would be how this
    // renderer and the Luau one start disagreeing.
    let transform = document.target_transform(options.width as f32, options.height as f32);
    if !transform_is_finite(&transform) {
        return Err(RasterError::NonFiniteGeometry);
    }

    // How the fit scales lengths, which is what converts a stroke width in view
    // box units into one in pixels. Exact for `meet` and `slice`; see
    // `RasterOptions::absolute_stroke_width` for what it means under `none`.
    let length_scale = transform.length_scale();

    let mut canvas = Canvas::new(options.width, options.height);
    let mut scratch = Scratch::default();

    for shape in &document.shapes {
        if !flatten_path(&shape.geometry, &transform, &mut scratch.contours) {
            return Err(RasterError::NonFiniteGeometry);
        }
        if scratch.contours.is_empty() {
            continue;
        }

        match shape.paint_order {
            PaintOrder::FillThenStroke => {
                draw_fill(&mut canvas, &mut scratch, shape, options);
                draw_stroke(&mut canvas, &mut scratch, shape, options, length_scale);
            }
            PaintOrder::StrokeThenFill => {
                draw_stroke(&mut canvas, &mut scratch, shape, options, length_scale);
                draw_fill(&mut canvas, &mut scratch, shape, options);
            }
        }
    }

    Ok(canvas.finish(options.mode == RasterMode::AlphaMask))
}

/// Buffers reused across every shape.
///
/// A document is dozens of shapes and each would otherwise allocate a contour
/// list, an edge list and a coverage row. None of this changes what is drawn;
/// it just stops the renderer spending most of its time in the allocator.
#[derive(Default)]
struct Scratch {
    contours: Vec<Contour>,
    polygons: Vec<Vec<Vec2>>,
    edges: EdgeSet,
    sampler: ScanlineSupersampler,
}

fn draw_fill(canvas: &mut Canvas, scratch: &mut Scratch, shape: &Shape, options: &RasterOptions) {
    let Some(fill) = shape.fill else { return };
    if fill.opacity.is_fully_transparent() {
        return;
    }

    scratch.edges.clear();
    for contour in &scratch.contours {
        // Every contour is closed here, whether or not the author wrote `Z`.
        // SVG closes fill contours implicitly, and doing it at the edge builder
        // rather than by editing the geometry leaves the canonical path — which
        // the stroker still needs to see as open — untouched.
        scratch.edges.add_polygon(&contour.points);
    }
    scratch.edges.finish();

    composite(
        canvas,
        &mut scratch.sampler,
        &scratch.edges,
        fill.rule,
        options,
        resolve(fill.paint, options.current_color),
        fill.opacity.get(),
    );
}

fn draw_stroke(
    canvas: &mut Canvas,
    scratch: &mut Scratch,
    shape: &Shape,
    options: &RasterOptions,
    length_scale: f32,
) {
    let Some(stroke) = shape.stroke else { return };
    if stroke.opacity.is_fully_transparent() {
        return;
    }

    let width = device_stroke_width(stroke.width, options, length_scale);
    if !width.is_finite() || width <= 0.0 {
        return;
    }

    expand(
        &scratch.contours,
        StrokeStyle {
            width,
            cap: stroke.line_cap,
            join: stroke.line_join,
            miter_limit: stroke.miter_limit,
        },
        &mut scratch.polygons,
    );
    if scratch.polygons.is_empty() {
        return;
    }

    scratch.edges.clear();
    for polygon in &scratch.polygons {
        scratch.edges.add_polygon(polygon);
    }
    scratch.edges.finish();

    composite(
        canvas,
        &mut scratch.sampler,
        &scratch.edges,
        // Always non-zero. A stroke outline overlaps itself wherever the path
        // does, and even-odd would punch holes through exactly those places.
        // The shape's own fill rule governs its interior, not its outline.
        FillRule::NonZero,
        options,
        resolve(stroke.paint, options.current_color),
        stroke.opacity.get(),
    );
}

/// The stroke width to use, in device pixels.
///
/// Three cases, and the distinction between the last two is the whole point of
/// `absoluteStrokeWidth`:
///
/// - no override: the asset's own width, in view box units, scaled by the fit;
/// - a relative override: likewise, but with the caller's width;
/// - an absolute override: already in pixels, so used unchanged — which is what
///   makes a stroke keep its apparent weight at every size.
fn device_stroke_width(shape_width: f32, options: &RasterOptions, length_scale: f32) -> f32 {
    match options.stroke_width_override {
        Some(width) if options.absolute_stroke_width => width,
        Some(width) => width * length_scale,
        None => shape_width * length_scale,
    }
}

fn resolve(paint: Paint, current_color: Color) -> Color {
    match paint {
        Paint::CurrentColor => current_color,
        Paint::Solid(color) => color,
    }
}

fn composite(
    canvas: &mut Canvas,
    sampler: &mut ScanlineSupersampler,
    edges: &EdgeSet,
    rule: FillRule,
    options: &RasterOptions,
    colour: Color,
    alpha: f32,
) {
    if edges.is_empty() {
        return;
    }
    let mask = options.mode == RasterMode::AlphaMask;
    sampler.rasterize(
        edges,
        rule,
        options.width,
        options.height,
        |y, coverage, start, end| {
            if mask {
                canvas.blend_row_alpha(y, coverage, start, end, alpha);
            } else {
                canvas.blend_row(y, coverage, start, end, colour, alpha);
            }
        },
    );
}

fn transform_is_finite(transform: &Transform) -> bool {
    transform.sx.is_finite()
        && transform.sy.is_finite()
        && transform.kx.is_finite()
        && transform.ky.is_finite()
        && transform.tx.is_finite()
        && transform.ty.is_finite()
}
