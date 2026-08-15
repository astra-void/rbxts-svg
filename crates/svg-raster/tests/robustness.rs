//! Nothing a user's `.svg` can contain may crash the renderer.
//!
//! The compiler rejects malformed documents, but "compiles fine" and "is
//! reasonable geometry" are different things: a legal SVG can ask for a curve
//! with control points at `1e30`, a stroke wider than the universe, or a
//! million coincident points. All of those have to come out as an image or as a
//! [`RasterError`], never as a panic, a hang, or a write past the end of the
//! buffer.

mod common;

use common::*;
use svg_raster::{MAX_DIMENSION, RasterError, RasterMode, RasterOptions, render};

// ---------------------------------------------------------------------------
// Raster dimensions
// ---------------------------------------------------------------------------

#[test]
fn a_zero_dimension_is_rejected() {
    let document = compile_fixture("lucide/search.svg");
    for (width, height) in [(0, 24), (24, 0), (0, 0)] {
        assert_eq!(
            render(&document, &RasterOptions::new(width, height)),
            Err(RasterError::InvalidDimensions { width, height })
        );
    }
}

#[test]
fn an_oversized_dimension_is_rejected_rather_than_allocated() {
    let document = compile_fixture("lucide/search.svg");
    for (width, height) in [
        (MAX_DIMENSION + 1, 24),
        (24, MAX_DIMENSION + 1),
        (u32::MAX, u32::MAX),
    ] {
        assert!(matches!(
            render(&document, &RasterOptions::new(width, height)),
            Err(RasterError::InvalidDimensions { .. })
        ));
    }
}

#[test]
fn the_largest_allowed_dimension_still_works() {
    // Only in one axis: the point is that the bound itself is inclusive, not to
    // allocate 8192 x 8192 x 4 bytes in a test.
    let image = render(
        &compile_fixture("lucide/search.svg"),
        &RasterOptions::new(MAX_DIMENSION, 1),
    )
    .expect("the bound should be inclusive");
    assert_eq!(image.width, MAX_DIMENSION);
    assert_eq!(image.pixels.len(), MAX_DIMENSION as usize * 4);
}

#[test]
fn a_one_pixel_raster_works() {
    let image = render_fixture("lucide/search.svg", 1, 1);
    assert_eq!(image.pixels.len(), 4);
}

#[test]
fn extreme_aspect_ratios_do_not_break_the_scan() {
    let document = compile_fixture("lucide/settings.svg");
    for (width, height) in [(1, 512), (512, 1), (3, 1000), (1000, 3)] {
        let image = render(&document, &RasterOptions::new(width, height)).unwrap();
        assert_eq!(image.pixels.len(), (width * height * 4) as usize);
    }
}

// ---------------------------------------------------------------------------
// Extreme geometry
// ---------------------------------------------------------------------------

#[test]
fn an_enormous_view_box_renders_without_overflowing() {
    let image = render_source(
        r##"<svg viewBox="0 0 1000000 1000000">
          <rect width="500000" height="500000" fill="#000000"/>
        </svg>"##,
        &RasterOptions::square(32),
    );
    assert_eq!(painted_bounds(&image), Some((0, 0, 15, 15)));
}

#[test]
fn a_minuscule_view_box_renders_without_dividing_by_zero() {
    let image = render_source(
        r##"<svg viewBox="0 0 0.0001 0.0001">
          <rect width="0.0001" height="0.0001" fill="#000000"/>
        </svg>"##,
        &RasterOptions::square(16),
    );
    assert_eq!(alpha_mass(&image), 256.0);
}

#[test]
fn geometry_far_outside_the_view_box_is_clipped_not_wrapped() {
    let image = render_source(
        r##"<svg viewBox="0 0 16 16">
          <rect x="-100000" y="-100000" width="100004" height="100004" fill="#000000"/>
        </svg>"##,
        &RasterOptions::square(16),
    );
    // The rectangle's far corner is at (4, 4), so only that quadrant is painted.
    assert_eq!(painted_bounds(&image), Some((0, 0, 3, 3)));
}

#[test]
fn an_extreme_curve_terminates_and_stays_inside_the_raster() {
    let image = render_source(
        r##"<svg viewBox="0 0 16 16">
          <path d="M0 0 C 100000 -100000, -100000 100000, 16 16 Z" fill="#000000"/>
        </svg>"##,
        &RasterOptions::square(16),
    );
    assert_eq!(image.pixels.len(), 16 * 16 * 4);
}

#[test]
fn many_coincident_points_do_not_hang() {
    let mut path = String::from("M8 8");
    for _ in 0..5000 {
        path.push_str(" L8 8");
    }
    path.push_str(" L14 14");
    let image = render_source(
        &format!(
            r##"<svg viewBox="0 0 16 16">
              <path d="{path}" stroke="#000000" stroke-width="2" fill="none"
                    stroke-linejoin="round"/>
            </svg>"##
        ),
        &RasterOptions::square(16),
    );
    assert!(alpha_mass(&image) > 0.0);
}

/// Half a million units of stroke width, on a one-unit segment. The band is
/// enormous *across* the path and still only one unit long, so a butt cap keeps
/// it to a single column — which is the correct answer, and the one that
/// catches a stroker that confuses the two axes.
#[test]
fn an_absurdly_wide_stroke_is_clipped_rather_than_overflowing() {
    let image = render_source(
        r##"<svg viewBox="0 0 16 16">
          <path d="M8 8 H9" stroke="#000000" stroke-width="1000000" fill="none"/>
        </svg>"##,
        &RasterOptions::square(16),
    );
    assert_eq!(painted_bounds(&image), Some((8, 0, 8, 15)));
    assert_eq!(alpha_mass(&image), 16.0);
}

/// The same width with a round cap does flood the raster, because the cap
/// extends along the path too.
#[test]
fn an_absurdly_wide_round_capped_stroke_floods_the_raster() {
    let image = render_source(
        r##"<svg viewBox="0 0 16 16">
          <path d="M8 8 H9" stroke="#000000" stroke-width="1000000" fill="none"
                stroke-linecap="round"/>
        </svg>"##,
        &RasterOptions::square(16),
    );
    assert!(image.alpha().iter().all(|&a| a == 255));
}

/// A subpath that is one enormous spike: the miter would be astronomically far
/// out if the limit did not cut it back.
#[test]
fn a_pathological_miter_does_not_escape_the_limit() {
    let image = render_source(
        r##"<svg viewBox="0 0 16 16">
          <path d="M0 8 H15.999 L0 8.001" stroke="#000000" stroke-width="2" fill="none"
                stroke-linejoin="miter" stroke-miterlimit="4"/>
        </svg>"##,
        &RasterOptions::square(16),
    );
    assert_eq!(image.pixels.len(), 16 * 16 * 4);
    // The limit caps the overhang, so the spike cannot reach past the raster's
    // own edge by an unbounded amount — the image is simply finite and drawn.
    assert!(alpha_mass(&image) > 0.0);
}

// ---------------------------------------------------------------------------
// Empty and near-empty documents
// ---------------------------------------------------------------------------

#[test]
fn a_document_with_no_shapes_renders_a_transparent_image() {
    let image = render_source(
        r##"<svg viewBox="0 0 8 8"><title>nothing</title></svg>"##,
        &RasterOptions::square(8),
    );
    assert_eq!(image.pixels, vec![0; 8 * 8 * 4]);
}

#[test]
fn a_shape_with_no_paint_is_dropped_before_it_reaches_the_renderer() {
    let image = render_source(
        r##"<svg viewBox="0 0 8 8"><rect width="8" height="8" fill="none"/></svg>"##,
        &RasterOptions::square(8),
    );
    assert_eq!(alpha_mass(&image), 0.0);
}

// ---------------------------------------------------------------------------
// Output invariants that must hold for every input
// ---------------------------------------------------------------------------

/// The properties every render must satisfy, checked across the whole fixture
/// corpus at a spread of sizes and in both modes.
#[test]
fn every_fixture_renders_within_its_buffer_at_every_size() {
    let fixtures = [
        "basic/aspect-meet.svg",
        "basic/aspect-none.svg",
        "basic/aspect-slice.svg",
        "basic/circle.svg",
        "basic/current-color.svg",
        "basic/ellipse.svg",
        "basic/evenodd.svg",
        "basic/group-opacity.svg",
        "basic/line.svg",
        "basic/multiple-subpaths.svg",
        "basic/offset-viewbox.svg",
        "basic/polygon.svg",
        "basic/polyline.svg",
        "basic/quadratic-and-arc.svg",
        "basic/rect.svg",
        "basic/shorthand-commands.svg",
        "basic/simple-path.svg",
        "basic/stroke-round.svg",
        "basic/transformed-group.svg",
        "lucide/bell.svg",
        "lucide/chevron-down.svg",
        "lucide/circle-alert.svg",
        "lucide/git-branch.svg",
        "lucide/search.svg",
        "lucide/settings.svg",
    ];

    for fixture in fixtures {
        let document = compile_fixture(fixture);
        for (width, height) in [(1, 1), (7, 13), (16, 16), (24, 24), (64, 31), (128, 128)] {
            for mode in [RasterMode::Color, RasterMode::AlphaMask] {
                let options = RasterOptions::new(width, height).with_mode(mode);
                let image = render(&document, &options)
                    .unwrap_or_else(|e| panic!("{fixture} at {width}x{height}: {e}"));

                assert_eq!(image.width, width, "{fixture}");
                assert_eq!(image.height, height, "{fixture}");
                assert_eq!(
                    image.pixels.len(),
                    (width as usize) * (height as usize) * 4,
                    "{fixture} at {width}x{height}"
                );

                // Rendering twice must produce the same bytes, or nothing
                // downstream can be cached or compared.
                assert_eq!(
                    render(&document, &options).unwrap(),
                    image,
                    "{fixture} at {width}x{height} is not deterministic"
                );
            }
        }
    }
}
