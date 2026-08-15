//! Stroke rendering, end to end.
//!
//! The stroker's own geometry tests live in `src/stroke.rs`; these check what
//! comes out the far end of the pipeline — including the parts only visible
//! once a stroke has been scan-converted, like whether a closed stroke leaves a
//! hole and whether a width override lands where it should.

mod common;

use common::*;
use svg_raster::{RasterOptions, render};

/// A horizontal stroke of width 4 in a 32-unit view box, rendered 1:1, must
/// cover exactly a 4-pixel band.
#[test]
fn a_stroke_covers_the_band_its_width_describes() {
    let image = render_source(
        r##"<svg viewBox="0 0 32 32">
          <path d="M4 16 H28" stroke="#000000" stroke-width="4" fill="none"/>
        </svg>"##,
        &RasterOptions::square(32),
    );
    assert_eq!(painted_bounds(&image), Some((4, 14, 27, 17)));
    assert!(alpha_at(&image, 16, 15) > 250);
    assert_eq!(alpha_at(&image, 16, 13), 0);
    assert_eq!(alpha_at(&image, 16, 18), 0);
}

#[test]
fn stroke_width_scales_with_the_target_size() {
    let source = r##"<svg viewBox="0 0 32 32">
      <path d="M4 16 H28" stroke="#000000" stroke-width="4" fill="none"/>
    </svg>"##;
    let small = render_source(source, &RasterOptions::square(32));
    let large = render_source(source, &RasterOptions::square(64));

    let band = |image: &svg_raster::RasterImage| {
        let (_, min_y, _, max_y) = painted_bounds(image).unwrap();
        max_y - min_y + 1
    };
    assert_eq!(band(&small), 4);
    assert_eq!(band(&large), 8, "twice the size, twice the stroke");
}

// ---------------------------------------------------------------------------
// Caps
// ---------------------------------------------------------------------------

const CAPPED_LINE: &str = r##"<svg viewBox="0 0 32 32">
  <path d="M8 16 H24" stroke="#000000" stroke-width="8" fill="none" stroke-linecap="{CAP}"/>
</svg>"##;

fn capped(cap: &str) -> svg_raster::RasterImage {
    render_source(
        &CAPPED_LINE.replace("{CAP}", cap),
        &RasterOptions::square(32),
    )
}

#[test]
fn a_butt_cap_stops_at_the_endpoint() {
    let image = capped("butt");
    assert_eq!(painted_bounds(&image), Some((8, 12, 23, 19)));
}

#[test]
fn a_square_cap_extends_by_half_the_width() {
    let image = capped("square");
    assert_eq!(painted_bounds(&image), Some((4, 12, 27, 19)));
    // Fully square: the corner of the extension is solid.
    assert!(alpha_at(&image, 5, 13) > 250, "{}", ascii_alpha(&image));
}

#[test]
fn a_round_cap_is_a_half_disc() {
    let image = capped("round");
    let (min_x, min_y, max_x, max_y) = painted_bounds(&image).unwrap();
    assert_eq!((min_x, min_y, max_y), (4, 12, 19));
    assert_eq!(max_x, 27);
    // The corner of the bounding box is *outside* the disc, unlike a square cap.
    assert_eq!(alpha_at(&image, 4, 12), 0, "{}", ascii_alpha(&image));
    assert!(alpha_at(&image, 5, 16) > 250, "but the middle is solid");
}

/// The three caps must differ only at the ends, by exactly the area their
/// definitions describe.
#[test]
fn the_three_caps_add_the_area_they_should() {
    let butt = alpha_mass(&capped("butt"));
    let square = alpha_mass(&capped("square"));
    let round = alpha_mass(&capped("round"));

    // Two square caps add 2 * 8 * 4 = 64 pixels.
    assert!((square - butt - 64.0).abs() < 1.0, "{square} vs {butt}");
    // Two round caps make one full disc of radius 4: pi * 16 ~ 50.3.
    assert!((round - butt - 50.3).abs() < 1.5, "{round} vs {butt}");
}

// ---------------------------------------------------------------------------
// Joins
// ---------------------------------------------------------------------------

const CORNER: &str = r##"<svg viewBox="0 0 32 32">
  <path d="M6 6 H26 V26" stroke="#000000" stroke-width="8" fill="none"
        stroke-linejoin="{JOIN}" stroke-miterlimit="{LIMIT}"/>
</svg>"##;

fn cornered(join: &str, limit: &str) -> svg_raster::RasterImage {
    render_source(
        &CORNER.replace("{JOIN}", join).replace("{LIMIT}", limit),
        &RasterOptions::square(32),
    )
}

/// The corner turns at (26, 6), so the miter apex lands at (30, 2): half the
/// stroke width out along each of the two offset directions.
#[test]
fn a_miter_join_fills_the_corner_square() {
    let image = cornered("miter", "4");
    assert_eq!(painted_bounds(&image), Some((6, 2, 29, 25)));
    assert!(alpha_at(&image, 29, 2) > 250, "{}", ascii_alpha(&image));
}

#[test]
fn a_bevel_join_cuts_the_corner_off() {
    let miter = cornered("miter", "4");
    let bevel = cornered("bevel", "4");
    assert!(
        alpha_mass(&bevel) < alpha_mass(&miter),
        "a bevel must remove area"
    );
    assert_eq!(alpha_at(&bevel, 29, 2), 0, "the apex is gone");
    // The two agree well inside the stroke.
    assert_eq!(alpha_at(&miter, 10, 6), alpha_at(&bevel, 10, 6));
}

#[test]
fn a_round_join_sits_between_a_miter_and_a_bevel() {
    let miter = alpha_mass(&cornered("miter", "4"));
    let round = alpha_mass(&cornered("round", "4"));
    let bevel = alpha_mass(&cornered("bevel", "4"));
    assert!(bevel < round, "{bevel} < {round}");
    assert!(round < miter, "{round} < {miter}");
}

/// A right angle's miter ratio is sqrt(2) ~ 1.414, so a limit either side of it
/// decides between a miter and a bevel.
#[test]
fn the_miter_limit_switches_to_bevel_at_the_documented_ratio() {
    let above = cornered("miter", "1.5");
    let below = cornered("miter", "1.3");
    let bevel = cornered("bevel", "1.3");

    assert!(alpha_at(&above, 29, 2) > 250, "1.5 > sqrt(2): mitred");
    assert_eq!(alpha_at(&below, 29, 2), 0, "1.3 < sqrt(2): bevelled");
    assert_eq!(below.pixels, bevel.pixels);
}

// ---------------------------------------------------------------------------
// Closed contours
// ---------------------------------------------------------------------------

#[test]
fn a_closed_stroke_is_a_hollow_band() {
    let image = render_source(
        r##"<svg viewBox="0 0 32 32">
          <rect x="8" y="8" width="16" height="16" stroke="#000000" stroke-width="4" fill="none"/>
        </svg>"##,
        &RasterOptions::square(32),
    );
    assert!(alpha_at(&image, 16, 8) > 250, "on the top edge");
    assert!(alpha_at(&image, 8, 16) > 250, "on the left edge");
    assert_eq!(
        alpha_at(&image, 16, 16),
        0,
        "hollow: {}",
        ascii_alpha(&image)
    );
    assert_eq!(painted_bounds(&image), Some((6, 6, 25, 25)));
}

#[test]
fn a_stroked_circle_is_an_annulus() {
    let image = render_source(
        r##"<svg viewBox="0 0 64 64">
          <circle cx="32" cy="32" r="20" stroke="#000000" stroke-width="6" fill="none"/>
        </svg>"##,
        &RasterOptions::square(64),
    );
    assert_eq!(alpha_at(&image, 32, 32), 0, "hollow centre");
    assert!(alpha_at(&image, 32, 12) > 250, "on the ring");

    // The annulus area is pi * (23^2 - 17^2) ~ 754.
    let mass = alpha_mass(&image);
    assert!((mass - 754.0).abs() < 754.0 * 0.02, "mass was {mass}");
}

// ---------------------------------------------------------------------------
// Fill and stroke together
// ---------------------------------------------------------------------------

#[test]
fn a_shape_with_both_paints_draws_the_fill_beneath_the_stroke() {
    let image = render_source(
        r##"<svg viewBox="0 0 32 32">
          <rect x="8" y="8" width="16" height="16" fill="#ff0000"
                stroke="#0000ff" stroke-width="8"/>
        </svg>"##,
        &RasterOptions::square(32),
    );
    assert_eq!(rgb_at(&image, 16, 16), [255, 0, 0], "fill in the middle");
    // The stroke straddles the boundary, so the fill is hidden under it.
    assert_eq!(rgb_at(&image, 16, 8), [0, 0, 255], "stroke on the edge");
}

#[test]
fn paint_order_stroke_puts_the_fill_on_top() {
    let image = render_source(
        r##"<svg viewBox="0 0 32 32">
          <rect x="8" y="8" width="16" height="16" fill="#ff0000"
                stroke="#0000ff" stroke-width="8" paint-order="stroke"/>
        </svg>"##,
        &RasterOptions::square(32),
    );
    // The stroke's inner half is now covered by the fill.
    assert_eq!(
        rgb_at(&image, 16, 9),
        [255, 0, 0],
        "{}",
        ascii_alpha(&image)
    );
    // Its outer half still shows.
    assert_eq!(rgb_at(&image, 16, 6), [0, 0, 255]);
}

// ---------------------------------------------------------------------------
// Stroke width overrides
// ---------------------------------------------------------------------------

const LINE: &str = r##"<svg viewBox="0 0 32 32">
  <path d="M4 16 H28" stroke="#000000" stroke-width="4" fill="none"/>
</svg>"##;

fn band_height(image: &svg_raster::RasterImage) -> u32 {
    let (_, min_y, _, max_y) = painted_bounds(image).unwrap();
    max_y - min_y + 1
}

#[test]
fn a_relative_override_replaces_the_width_in_view_box_units() {
    let image = render_source(LINE, &RasterOptions::square(32).with_stroke_width(8.0));
    assert_eq!(band_height(&image), 8);

    // At twice the resolution the same view box width doubles in pixels, just
    // as the asset's own width would.
    let doubled = render_source(LINE, &RasterOptions::square(64).with_stroke_width(8.0));
    assert_eq!(band_height(&doubled), 16);
}

/// The point of `absoluteStrokeWidth`: the same pixel weight at every size.
#[test]
fn an_absolute_override_is_in_pixels_and_does_not_scale() {
    for size in [32, 64, 128] {
        let image = render_source(
            LINE,
            &RasterOptions::square(size).with_absolute_stroke_width(6.0),
        );
        assert_eq!(band_height(&image), 6, "at {size}px");
    }
}

#[test]
fn no_override_keeps_each_shapes_own_width() {
    let image = render_source(
        r##"<svg viewBox="0 0 32 32">
          <path d="M4 8 H28" stroke="#000000" stroke-width="2" fill="none"/>
          <path d="M4 24 H28" stroke="#000000" stroke-width="6" fill="none"/>
        </svg>"##,
        &RasterOptions::square(32),
    );
    let column: Vec<u8> = (0..32).map(|y| alpha_at(&image, 16, y)).collect();
    let thin = column[0..16].iter().filter(|&&a| a > 250).count();
    let thick = column[16..32].iter().filter(|&&a| a > 250).count();
    assert_eq!(thin, 2);
    assert_eq!(thick, 6);
}

/// An override replaces *every* shape's width, which is what a Lucide
/// `strokeWidth` prop means.
#[test]
fn an_override_applies_to_every_shape() {
    let image = render_source(
        r##"<svg viewBox="0 0 32 32">
          <path d="M4 8 H28" stroke="#000000" stroke-width="2" fill="none"/>
          <path d="M4 24 H28" stroke="#000000" stroke-width="6" fill="none"/>
        </svg>"##,
        &RasterOptions::square(32).with_stroke_width(4.0),
    );
    let column: Vec<u8> = (0..32).map(|y| alpha_at(&image, 16, y)).collect();
    assert_eq!(column[0..16].iter().filter(|&&a| a > 250).count(), 4);
    assert_eq!(column[16..32].iter().filter(|&&a| a > 250).count(), 4);
}

/// Under a non-uniform fit there is no single scale, so an override in view box
/// units uses the geometric mean — documented on `RasterOptions`. An *absolute*
/// override sidesteps the question entirely by already being in pixels.
#[test]
fn an_absolute_override_is_exact_even_under_a_non_uniform_fit() {
    let image = render_source(
        r##"<svg viewBox="0 0 32 16" preserveAspectRatio="none">
          <path d="M4 8 H28" stroke="#000000" stroke-width="2" fill="none"/>
        </svg>"##,
        &RasterOptions::new(64, 64).with_absolute_stroke_width(8.0),
    );
    assert_eq!(band_height(&image), 8);
}

#[test]
fn a_relative_override_under_a_non_uniform_fit_uses_the_geometric_mean() {
    // x scales by 2 and y by 4, so the mean is sqrt(8) ~ 2.83. A width of 2
    // view box units becomes about 5.66 pixels.
    let image = render_source(
        r##"<svg viewBox="0 0 32 16" preserveAspectRatio="none">
          <path d="M4 8 H28" stroke="#000000" stroke-width="1" fill="none"/>
        </svg>"##,
        &RasterOptions::new(64, 64).with_stroke_width(2.0),
    );
    let (_, min_y, _, max_y) = painted_bounds(&image).unwrap();
    let height = (max_y - min_y + 1) as f32;
    assert!((height - 6.0).abs() <= 1.0, "band was {height} pixels");
}

// ---------------------------------------------------------------------------
// Degenerate geometry
// ---------------------------------------------------------------------------

/// SVG paints a zero-length subpath as a dot under a round or square cap.
///
/// Built as a document rather than as SVG source on purpose: `usvg` discards a
/// path with no drawing command before we ever see it, so an end-to-end fixture
/// would be testing that limitation instead of this behaviour. The rasterizer
/// is what has to be right, because the geometry can still arrive this way —
/// through the IR, or once the compiler stops relying on usvg for it.
#[test]
fn a_zero_length_subpath_paints_a_dot_under_a_round_cap() {
    use svg_core::{
        FeatureFlags, LineCap, LineJoin, Opacity, Paint, PathBuilder, Point, Shape, Stroke,
        SvgDocument, ViewBox,
    };

    let mut builder = PathBuilder::new();
    builder.move_to(Point::new(8.0, 8.0)).unwrap();
    let stroke = Stroke::new(
        Paint::Solid(svg_core::Color::BLACK),
        Opacity::OPAQUE,
        8.0,
        LineCap::Round,
        LineJoin::Miter,
        4.0,
    )
    .unwrap();
    let document = SvgDocument::new(
        ViewBox::new(0.0, 0.0, 16.0, 16.0).unwrap(),
        vec![Shape::new(builder.finish(), None, Some(stroke))],
        FeatureFlags::HAS_STROKE,
    );

    let image = render(&document, &RasterOptions::square(16)).unwrap();
    assert!(alpha_at(&image, 8, 8) > 250, "{}", ascii_alpha(&image));
    let mass = alpha_mass(&image);
    assert!((mass - std::f64::consts::PI * 16.0).abs() < 2.0, "{mass}");
}

#[test]
fn a_stroke_thinner_than_a_pixel_still_shows() {
    let image = render_source(
        r##"<svg viewBox="0 0 32 32">
          <path d="M4 16 H28" stroke="#000000" stroke-width="0.25" fill="none"/>
        </svg>"##,
        &RasterOptions::square(32),
    );
    let mass = alpha_mass(&image);
    // 24 units long at a quarter of a pixel wide: 6 pixels' worth of coverage,
    // spread out rather than dropped. A rasterizer that snapped thin strokes to
    // whole pixels would report 24 here, and one that dropped them, zero.
    assert!((mass - 6.0).abs() < 0.5, "{mass}");
    assert!(image.alpha().iter().all(|&a| a < 250), "should be faint");
}

#[test]
fn a_stroke_larger_than_the_raster_does_not_escape_it() {
    let image = render_source(
        r##"<svg viewBox="0 0 32 32">
          <path d="M16 16 H17" stroke="#000000" stroke-width="500" fill="none"
                stroke-linecap="round"/>
        </svg>"##,
        &RasterOptions::square(16),
    );
    assert!(image.alpha().iter().all(|&a| a == 255));
}

// ---------------------------------------------------------------------------
// Real icons
// ---------------------------------------------------------------------------

#[test]
fn every_lucide_fixture_renders_something_sensible() {
    for fixture in [
        "lucide/search.svg",
        "lucide/chevron-down.svg",
        "lucide/git-branch.svg",
        "lucide/settings.svg",
        "lucide/bell.svg",
        "lucide/circle-alert.svg",
    ] {
        for size in [16u32, 24, 32, 64] {
            let image = render_fixture(fixture, size, size);
            let mass = alpha_mass(&image);
            assert!(mass > 0.0, "{fixture} at {size} drew nothing");

            // An icon covers a real but modest fraction of its box. Anything
            // near zero means the geometry collapsed; anything near total means
            // it flooded.
            let fraction = mass / (size * size) as f64;
            assert!(
                (0.02..0.6).contains(&fraction),
                "{fixture} at {size}: covered {:.1}% of the raster",
                fraction * 100.0
            );

            // The ink must be spread across the box rather than huddled in a
            // corner, which is what a dropped or mis-transformed subpath looks
            // like. Some icons are deliberately squat — a chevron is twice as
            // wide as it is tall — so only the longer axis has to fill half.
            let (min_x, min_y, max_x, max_y) = painted_bounds(&image).unwrap();
            let (span_x, span_y) = (max_x - min_x, max_y - min_y);
            assert!(
                span_x.max(span_y) > size / 2 && span_x.min(span_y) > size / 4,
                "{fixture} at {size}: ink spans {span_x}x{span_y}"
            );
        }
    }
}

/// Two shapes, both stroked with `currentColor`, drawn from one document.
#[test]
fn lucide_search_draws_both_its_shapes() {
    let document = compile_fixture("lucide/search.svg");
    assert_eq!(document.shapes.len(), 2);

    let image = render(&document, &RasterOptions::square(48)).unwrap();
    // The circle's ring, near its left edge at (11, 11) r 8 scaled by 2.
    assert!(alpha_at(&image, 6, 22) > 200, "{}", ascii_alpha(&image));
    // The handle, running to the bottom-right corner.
    assert!(alpha_at(&image, 40, 40) > 200);
    // Inside the circle is hollow: Lucide sets `fill="none"`.
    assert_eq!(alpha_at(&image, 22, 22), 0);
}
