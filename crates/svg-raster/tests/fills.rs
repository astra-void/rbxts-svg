//! Fill rendering, end to end.
//!
//! These go through the compiler and the IR, so they test the pipeline rather
//! than the scan converter — the scan converter's own fill-rule tests live in
//! `src/edges.rs`, against geometry with no SVG in the way.

mod common;

use common::*;
use svg_core::Color;
use svg_raster::{RasterMode, RasterOptions, render};

/// A rectangle covering exactly the view box must fill the raster exactly, with
/// no soft border from a half-pixel drift in the transform.
#[test]
fn a_full_bleed_rectangle_fills_every_pixel() {
    let image = render_source(
        r##"<svg viewBox="0 0 10 10"><rect width="10" height="10" fill="#336699"/></svg>"##,
        &RasterOptions::square(20),
    );
    for y in 0..20 {
        for x in 0..20 {
            assert_eq!(
                image.pixel(x, y),
                Some([0x33, 0x66, 0x99, 255]),
                "({x}, {y})"
            );
        }
    }
}

#[test]
fn a_quarter_rectangle_lands_in_the_right_quadrant() {
    let image = render_source(
        r##"<svg viewBox="0 0 8 8"><rect width="4" height="4" fill="#ff0000"/></svg>"##,
        &RasterOptions::square(8),
    );
    assert_eq!(painted_bounds(&image), Some((0, 0, 3, 3)));
    assert_eq!(rgb_at(&image, 1, 1), [255, 0, 0]);
    assert_eq!(alpha_at(&image, 5, 5), 0);
}

/// The view box origin is subtracted, so a shape at the origin of a shifted
/// coordinate system lands in the middle of the raster, not at its corner.
#[test]
fn a_non_zero_view_box_origin_is_honoured() {
    let image = render_fixture("basic/offset-viewbox.svg", 24, 24);
    // A radius-10 circle at (0, 0) in a "-12 -12 24 24" view box.
    assert!(alpha_at(&image, 12, 12) > 250, "centre should be solid");
    assert_eq!(alpha_at(&image, 0, 0), 0, "corner is outside the circle");

    let (min_x, min_y, max_x, max_y) = painted_bounds(&image).expect("something should be drawn");
    assert!((min_x as i32 - 2).abs() <= 1, "{min_x}");
    assert!((min_y as i32 - 2).abs() <= 1, "{min_y}");
    assert!((max_x as i32 - 21).abs() <= 1, "{max_x}");
    assert!((max_y as i32 - 21).abs() <= 1, "{max_y}");
}

/// A circle's area is `pi r^2`. Coverage anti-aliasing should reproduce it to
/// well within a percent — that is really a test that partial coverage is being
/// accumulated rather than rounded to a hard edge.
#[test]
fn a_filled_circle_has_the_area_it_should() {
    let image = render_source(
        r##"<svg viewBox="0 0 64 64"><circle cx="32" cy="32" r="24" fill="#000000"/></svg>"##,
        &RasterOptions::square(64),
    );
    let expected = std::f64::consts::PI * 24.0 * 24.0;
    let actual = alpha_mass(&image);
    assert!(
        (actual - expected).abs() < expected * 0.01,
        "expected about {expected:.1}, got {actual:.1}"
    );
}

/// Edges must be anti-aliased, not stair-stepped: a diagonal has to produce
/// partial coverage somewhere.
#[test]
fn a_diagonal_edge_is_anti_aliased() {
    let image = render_source(
        r##"<svg viewBox="0 0 32 32"><path d="M0 0 L32 32 L0 32 Z" fill="#000000"/></svg>"##,
        &RasterOptions::square(32),
    );
    let partial = image.alpha().iter().filter(|&&a| a > 8 && a < 247).count();
    assert!(partial > 20, "only {partial} partially covered pixels");
}

// ---------------------------------------------------------------------------
// Fill rules
// ---------------------------------------------------------------------------

/// Two nested squares wound the same way. Under non-zero the middle stays
/// solid; under even-odd it becomes a hole. Same geometry, different rule.
const NESTED_SAME_DIRECTION: &str = r##"<svg viewBox="0 0 16 16">
  <path d="M0 0 H16 V16 H0 Z M4 4 H12 V12 H4 Z" fill="#000000" fill-rule="{RULE}"/>
</svg>"##;

#[test]
fn nonzero_keeps_a_same_direction_inner_contour_solid() {
    let image = render_source(
        &NESTED_SAME_DIRECTION.replace("{RULE}", "nonzero"),
        &RasterOptions::square(16),
    );
    assert!(alpha_at(&image, 8, 8) > 250, "{}", ascii_alpha(&image));
}

#[test]
fn evenodd_cuts_a_hole_through_a_same_direction_inner_contour() {
    let image = render_source(
        &NESTED_SAME_DIRECTION.replace("{RULE}", "evenodd"),
        &RasterOptions::square(16),
    );
    assert_eq!(alpha_at(&image, 8, 8), 0, "{}", ascii_alpha(&image));
    assert!(alpha_at(&image, 1, 8) > 250, "the ring must stay solid");
}

/// Nested contours wound *oppositely* are the case both rules agree on — which
/// is exactly why an implementation faking even-odd with orientation passes it.
#[test]
fn both_rules_agree_on_oppositely_wound_nested_contours() {
    let source = r##"<svg viewBox="0 0 16 16">
      <path d="M0 0 H16 V16 H0 Z M4 4 V12 H12 V4 Z" fill="#000000" fill-rule="{RULE}"/>
    </svg>"##;
    let nonzero = render_source(
        &source.replace("{RULE}", "nonzero"),
        &RasterOptions::square(16),
    );
    let evenodd = render_source(
        &source.replace("{RULE}", "evenodd"),
        &RasterOptions::square(16),
    );

    assert_eq!(alpha_at(&nonzero, 8, 8), 0, "{}", ascii_alpha(&nonzero));
    assert_eq!(evenodd.pixels, nonzero.pixels);
}

#[test]
fn the_rules_differ_on_overlapping_contours() {
    let source = r##"<svg viewBox="0 0 24 16">
      <path d="M0 0 H14 V16 H0 Z M10 0 H24 V16 H10 Z" fill="#000000" fill-rule="{RULE}"/>
    </svg>"##;
    let nonzero = render_source(
        &source.replace("{RULE}", "nonzero"),
        &RasterOptions::new(24, 16),
    );
    let evenodd = render_source(
        &source.replace("{RULE}", "evenodd"),
        &RasterOptions::new(24, 16),
    );

    // The 10..14 overlap: doubly wound, so solid under non-zero and empty under
    // even-odd.
    assert!(nonzero.pixel(12, 8).unwrap()[3] > 250);
    assert_eq!(alpha_at(&evenodd, 12, 8), 0, "{}", ascii_alpha(&evenodd));
    // Outside the overlap they agree.
    assert!(alpha_at(&nonzero, 4, 8) > 250);
    assert!(alpha_at(&evenodd, 4, 8) > 250);
}

#[test]
fn the_even_odd_fixture_renders_with_a_hole() {
    let image = render_fixture("basic/evenodd.svg", 32, 32);
    assert!(alpha_mass(&image) > 0.0);
    assert_eq!(alpha_at(&image, 16, 16), 0, "{}", ascii_alpha(&image));
}

#[test]
fn multiple_subpaths_all_fill() {
    let image = render_source(
        r##"<svg viewBox="0 0 16 8">
          <path d="M0 0 H6 V8 H0 Z M10 0 H16 V8 H10 Z" fill="#000000"/>
        </svg>"##,
        &RasterOptions::new(16, 8),
    );
    assert!(alpha_at(&image, 2, 4) > 250);
    assert_eq!(alpha_at(&image, 8, 4), 0);
    assert!(alpha_at(&image, 13, 4) > 250);
}

/// SVG fills a subpath as if it were closed, whether or not `Z` was written.
#[test]
fn an_unclosed_subpath_is_filled_as_if_closed() {
    let open = render_source(
        r##"<svg viewBox="0 0 16 16"><path d="M2 2 H14 V14 H2" fill="#000000"/></svg>"##,
        &RasterOptions::square(16),
    );
    let closed = render_source(
        r##"<svg viewBox="0 0 16 16"><path d="M2 2 H14 V14 H2 Z" fill="#000000"/></svg>"##,
        &RasterOptions::square(16),
    );
    assert_eq!(open.pixels, closed.pixels, "{}", ascii_alpha(&open));
    assert!(alpha_at(&open, 8, 8) > 250);
}

// ---------------------------------------------------------------------------
// Paint
// ---------------------------------------------------------------------------

#[test]
fn current_color_is_supplied_by_the_caller_not_forced_to_black() {
    let source =
        r##"<svg viewBox="0 0 8 8"><rect width="8" height="8" fill="currentColor"/></svg>"##;
    for colour in [Color::rgb(255, 0, 0), Color::rgb(0, 128, 255), Color::WHITE] {
        let image = render_source(source, &RasterOptions::square(8).with_current_color(colour));
        assert_eq!(rgb_at(&image, 4, 4), [colour.r, colour.g, colour.b]);
    }
}

#[test]
fn a_baked_colour_ignores_the_current_color_option() {
    let image = render_source(
        r##"<svg viewBox="0 0 8 8"><rect width="8" height="8" fill="#00ff00"/></svg>"##,
        &RasterOptions::square(8).with_current_color(Color::rgb(255, 0, 0)),
    );
    assert_eq!(rgb_at(&image, 4, 4), [0, 255, 0]);
}

#[test]
fn fill_opacity_is_folded_into_the_alpha_channel() {
    let image = render_source(
        r##"<svg viewBox="0 0 8 8">
          <rect width="8" height="8" fill="#000000" fill-opacity="0.5"/>
        </svg>"##,
        &RasterOptions::square(8),
    );
    let alpha = alpha_at(&image, 4, 4);
    assert!(alpha.abs_diff(128) <= 1, "{alpha}");
}

#[test]
fn shapes_are_composited_in_painters_order() {
    let image = render_source(
        r##"<svg viewBox="0 0 8 8">
          <rect width="8" height="8" fill="#ff0000"/>
          <rect width="8" height="8" fill="#0000ff"/>
        </svg>"##,
        &RasterOptions::square(8),
    );
    assert_eq!(rgb_at(&image, 4, 4), [0, 0, 255], "the later shape wins");
}

#[test]
fn a_translucent_shape_blends_with_what_is_beneath_it() {
    let image = render_source(
        r##"<svg viewBox="0 0 8 8">
          <rect width="8" height="8" fill="#ff0000"/>
          <rect width="8" height="8" fill="#0000ff" fill-opacity="0.5"/>
        </svg>"##,
        &RasterOptions::square(8),
    );
    let [r, g, b] = rgb_at(&image, 4, 4);
    assert_eq!(alpha_at(&image, 4, 4), 255);
    assert!(r.abs_diff(128) <= 2, "{r}");
    assert_eq!(g, 0);
    assert!(b.abs_diff(128) <= 2, "{b}");
}

#[test]
fn a_fully_transparent_fill_paints_nothing() {
    let image = render_source(
        r##"<svg viewBox="0 0 8 8">
          <rect width="8" height="8" fill="#000000" fill-opacity="0"/>
        </svg>"##,
        &RasterOptions::square(8),
    );
    assert_eq!(alpha_mass(&image), 0.0);
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn rendering_the_same_document_twice_is_byte_identical() {
    let document = compile_fixture("lucide/settings.svg");
    let options = RasterOptions::square(48);
    let first = render(&document, &options).unwrap();
    let second = render(&document, &options).unwrap();
    assert_eq!(first, second);
}

#[test]
fn the_mask_and_colour_paths_agree_on_coverage() {
    let document = compile_fixture("lucide/search.svg");
    let colour = render(&document, &RasterOptions::square(32)).unwrap();
    let mask = render(
        &document,
        &RasterOptions::square(32).with_mode(RasterMode::AlphaMask),
    )
    .unwrap();
    assert_eq!(colour.alpha(), mask.alpha());
}
