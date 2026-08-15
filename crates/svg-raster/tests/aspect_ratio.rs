//! `preserveAspectRatio`, rendered.
//!
//! This is the architecture fix made visible. Version 1 of the IR discarded the
//! authored fitting policy, so a renderer handed a target of a different shape
//! had no choice but to guess — and whichever it guessed was wrong for half of
//! all documents. These tests pin that it now reaches the pixels.
//!
//! Every case uses the same 24x12 artwork, a rectangle filling its view box, so
//! the *only* thing that can change the output is the policy.

mod common;

use common::*;
use svg_raster::RasterOptions;

/// A rectangle filling a 24x12 view box, under a given fitting policy.
fn wide_rectangle(policy: &str) -> String {
    format!(
        r##"<svg viewBox="0 0 24 12" preserveAspectRatio="{policy}">
          <rect width="24" height="12" fill="#000000"/>
        </svg>"##
    )
}

fn fitted(policy: &str, width: u32, height: u32) -> svg_raster::RasterImage {
    render_source(&wide_rectangle(policy), &RasterOptions::new(width, height))
}

// ---------------------------------------------------------------------------
// The worked examples from the specification: 24x12 into 100x100
// ---------------------------------------------------------------------------

/// `meet` fits the whole view box inside the target: 100x50, centred.
#[test]
fn meet_letterboxes_a_wide_asset_in_a_square_target() {
    let image = fitted("xMidYMid meet", 100, 100);
    assert_eq!(painted_bounds(&image), Some((0, 25, 99, 74)));
    assert_eq!(alpha_at(&image, 50, 10), 0, "the letterbox bar is empty");
    assert!(alpha_at(&image, 50, 50) > 250);
}

/// `slice` covers the target and lets the artwork overflow: 200x100, cropped.
#[test]
fn slice_covers_a_square_target_and_crops_the_overflow() {
    let image = fitted("xMidYMid slice", 100, 100);
    assert_eq!(painted_bounds(&image), Some((0, 0, 99, 99)));
    assert_eq!(alpha_mass(&image), 10_000.0, "every pixel is covered");
}

/// `none` stretches to fill exactly, with no uniform scale at all.
#[test]
fn none_stretches_a_wide_asset_to_fill_a_square_target() {
    let image = fitted("none", 100, 100);
    assert_eq!(painted_bounds(&image), Some((0, 0, 99, 99)));
    assert_eq!(alpha_mass(&image), 10_000.0);
}

/// `slice` and `none` both fill a square target, so a test that only counted
/// pixels could not tell them apart. The interior geometry can.
#[test]
fn slice_and_none_differ_in_where_the_artwork_lands() {
    let source = r##"<svg viewBox="0 0 24 12" preserveAspectRatio="{POLICY}">
      <rect x="0" y="0" width="6" height="12" fill="#000000"/>
    </svg>"##;

    // `none`: the 6-unit-wide bar is a quarter of the width, so 25 pixels.
    let stretched = render_source(
        &source.replace("{POLICY}", "none"),
        &RasterOptions::square(100),
    );
    assert_eq!(painted_bounds(&stretched), Some((0, 0, 24, 99)));

    // `xMinYMin slice`: uniform scale 100/12, so the same bar is 50 pixels
    // wide — twice as much, from identical geometry.
    let sliced = render_source(
        &source.replace("{POLICY}", "xMinYMin slice"),
        &RasterOptions::square(100),
    );
    assert_eq!(painted_bounds(&sliced), Some((0, 0, 49, 99)));
}

// ---------------------------------------------------------------------------
// Alignment
// ---------------------------------------------------------------------------

#[test]
fn meet_alignment_moves_the_artwork_along_the_free_axis() {
    // 24x12 into 100x100 leaves 50 pixels of slack vertically.
    for (policy, top) in [
        ("xMidYMin meet", 0),
        ("xMidYMid meet", 25),
        ("xMidYMax meet", 50),
    ] {
        let image = fitted(policy, 100, 100);
        let (_, min_y, _, max_y) = painted_bounds(&image).unwrap();
        assert_eq!((min_y, max_y), (top, top + 49), "{policy}");
    }
}

#[test]
fn meet_alignment_moves_the_artwork_horizontally_when_that_is_the_free_axis() {
    // A 12x24 view box in a 100x100 target leaves the slack horizontally.
    let source = r##"<svg viewBox="0 0 12 24" preserveAspectRatio="{POLICY}">
      <rect width="12" height="24" fill="#000000"/>
    </svg>"##;
    for (policy, left) in [
        ("xMinYMid meet", 0),
        ("xMidYMid meet", 25),
        ("xMaxYMid meet", 50),
    ] {
        let image = render_source(
            &source.replace("{POLICY}", policy),
            &RasterOptions::square(100),
        );
        let (min_x, _, max_x, _) = painted_bounds(&image).unwrap();
        assert_eq!((min_x, max_x), (left, left + 49), "{policy}");
    }
}

/// Under `slice` the slack is negative, so the same alignment crops instead of
/// letterboxing — and which edge survives is the alignment's decision.
#[test]
fn slice_alignment_decides_which_edge_is_cropped() {
    let source = r##"<svg viewBox="0 0 24 12" preserveAspectRatio="{POLICY}">
      <rect x="0" y="0" width="2" height="12" fill="#000000"/>
    </svg>"##;
    // Uniform scale 100/12; the 2-unit bar becomes about 17 pixels wide and the
    // artwork overhangs by 50 pixels on one side or the other.
    let at_min = render_source(
        &source.replace("{POLICY}", "xMinYMin slice"),
        &RasterOptions::square(100),
    );
    // 2 units at a scale of 100/12 is 16.67 pixels, so pixel 16 is the last
    // one the bar reaches into.
    assert_eq!(painted_bounds(&at_min), Some((0, 0, 16, 99)));

    let at_max = render_source(
        &source.replace("{POLICY}", "xMaxYMax slice"),
        &RasterOptions::square(100),
    );
    // Shifted fully left off the canvas: the bar's right edge is at
    // 2 * 100/12 - 100 = -83, so nothing shows.
    assert_eq!(painted_bounds(&at_max), None);
}

// ---------------------------------------------------------------------------
// Target shapes
// ---------------------------------------------------------------------------

/// The identity case: matching aspect ratios must render the same whatever the
/// policy says, because there is no slack to distribute.
#[test]
fn a_matching_target_renders_identically_under_every_policy() {
    let reference = fitted("xMidYMid meet", 96, 48);
    for policy in ["none", "xMinYMin meet", "xMaxYMax slice", "xMidYMid slice"] {
        assert_eq!(fitted(policy, 96, 48).pixels, reference.pixels, "{policy}");
    }
}

#[test]
fn a_wide_target_pillarboxes_a_square_asset_under_meet() {
    let image = render_source(
        r##"<svg viewBox="0 0 10 10"><rect width="10" height="10" fill="#000000"/></svg>"##,
        &RasterOptions::new(80, 40),
    );
    assert_eq!(painted_bounds(&image), Some((20, 0, 59, 39)));
}

#[test]
fn a_tall_target_letterboxes_a_square_asset_under_meet() {
    let image = render_source(
        r##"<svg viewBox="0 0 10 10"><rect width="10" height="10" fill="#000000"/></svg>"##,
        &RasterOptions::new(40, 80),
    );
    assert_eq!(painted_bounds(&image), Some((0, 20, 39, 59)));
}

// ---------------------------------------------------------------------------
// Non-zero view box origin
// ---------------------------------------------------------------------------

#[test]
fn a_shifted_view_box_still_aligns_its_own_corner() {
    // The artwork fills "-10 -5 24 12"; under `xMinYMin meet` in a 100x100
    // target its top-left corner must land on the raster's.
    let image = render_source(
        r##"<svg viewBox="-10 -5 24 12" preserveAspectRatio="xMinYMin meet">
          <rect x="-10" y="-5" width="24" height="12" fill="#000000"/>
        </svg>"##,
        &RasterOptions::square(100),
    );
    assert_eq!(painted_bounds(&image), Some((0, 0, 99, 49)));
}

#[test]
fn a_shifted_view_box_centres_correctly_under_x_mid_y_mid() {
    let image = render_source(
        r##"<svg viewBox="-12 -12 24 24"><circle cx="0" cy="0" r="12" fill="#000000"/></svg>"##,
        &RasterOptions::new(96, 48),
    );
    // Uniform scale 2, centred in a target 48 pixels wider than the artwork.
    assert_eq!(painted_bounds(&image), Some((24, 0, 71, 47)));
}

// ---------------------------------------------------------------------------
// The fixtures, which differ only in policy
// ---------------------------------------------------------------------------

#[test]
fn the_three_aspect_fixtures_render_differently_from_the_same_geometry() {
    let meet = render_fixture("basic/aspect-meet.svg", 64, 64);
    let none = render_fixture("basic/aspect-none.svg", 64, 64);
    let slice = render_fixture("basic/aspect-slice.svg", 64, 64);

    assert_eq!(painted_bounds(&meet), Some((0, 16, 63, 47)));
    assert_eq!(painted_bounds(&none), Some((0, 0, 63, 63)));
    assert_eq!(painted_bounds(&slice), Some((0, 0, 63, 63)));

    assert_ne!(meet.pixels, none.pixels);
    // `none` and `slice` both fill this particular target, so they agree here;
    // `slice_and_none_differ_in_where_the_artwork_lands` is what separates them.
    assert_eq!(alpha_mass(&none), alpha_mass(&slice));
}

/// A square target is where the policy matters; a matching one is where it must
/// not. Both, from real compiled fixtures.
#[test]
fn the_fixtures_agree_when_the_target_matches_their_view_box() {
    let meet = render_fixture("basic/aspect-meet.svg", 48, 24);
    let none = render_fixture("basic/aspect-none.svg", 48, 24);
    let slice = render_fixture("basic/aspect-slice.svg", 48, 24);
    assert_eq!(meet.pixels, none.pixels);
    assert_eq!(meet.pixels, slice.pixels);
}

/// An icon under `meet` keeps its proportions however odd the target is, which
/// is the property that stops a stretched-looking Lucide glyph.
#[test]
fn a_lucide_icon_keeps_its_proportions_in_a_rectangular_target() {
    let square = render_fixture("lucide/circle-alert.svg", 64, 64);
    let wide = render_fixture("lucide/circle-alert.svg", 128, 64);

    let span = |image: &svg_raster::RasterImage| {
        let (min_x, min_y, max_x, max_y) = painted_bounds(image).unwrap();
        (max_x - min_x, max_y - min_y)
    };
    // The uniform scale is set by the shorter axis, so the artwork is the same
    // size in both — just centred differently.
    assert_eq!(span(&square), span(&wide));

    let (min_x, _, max_x, _) = painted_bounds(&wide).unwrap();
    let centre = (min_x + max_x) / 2;
    assert!((centre as i32 - 64).abs() <= 1, "centred at {centre}");
}
