//! Compositing, paint order, and the tintable alpha-mask path.
//!
//! The mask path is the one the whole caching design rests on: if a tintable
//! asset's coverage depends on the colour it was asked for, then one raster
//! cannot serve every colour and the cache has to key on colour after all.

mod common;

use common::*;
use svg_core::Color;
use svg_raster::{RasterMode, RasterOptions, render};

// ---------------------------------------------------------------------------
// Source-over
// ---------------------------------------------------------------------------

#[test]
fn opaque_shapes_are_painted_in_order() {
    let image = render_source(
        r##"<svg viewBox="0 0 4 4">
          <rect width="4" height="4" fill="#ff0000"/>
          <rect width="4" height="2" fill="#00ff00"/>
        </svg>"##,
        &RasterOptions::square(4),
    );
    assert_eq!(rgb_at(&image, 2, 0), [0, 255, 0], "the later shape on top");
    assert_eq!(rgb_at(&image, 2, 3), [255, 0, 0], "the earlier one below");
}

#[test]
fn stacked_transparency_accumulates_rather_than_replacing() {
    let image = render_source(
        r##"<svg viewBox="0 0 4 4">
          <rect width="4" height="4" fill="#000000" fill-opacity="0.5"/>
          <rect width="4" height="4" fill="#000000" fill-opacity="0.5"/>
        </svg>"##,
        &RasterOptions::square(4),
    );
    // 1 - 0.5^2 = 0.75.
    assert_eq!(alpha_at(&image, 2, 2), 191);
}

#[test]
fn stroke_opacity_and_fill_opacity_are_independent() {
    let image = render_source(
        r##"<svg viewBox="0 0 32 32">
          <rect x="8" y="8" width="16" height="16" fill="#000000" fill-opacity="0.25"
                stroke="#000000" stroke-width="4" stroke-opacity="0.75"/>
        </svg>"##,
        &RasterOptions::square(32),
    );
    assert_eq!(alpha_at(&image, 16, 16), 64, "the fill alone");
    // On the boundary the stroke covers the fill: 0.25 then 0.75 over it.
    let edge = alpha_at(&image, 16, 8);
    let expected = ((0.75f32 + 0.25 * 0.25) * 255.0).round() as u8;
    assert!(edge.abs_diff(expected) <= 1, "{edge} vs {expected}");
}

/// A group's opacity is folded into its children at compile time. It still has
/// to reach the pixels.
#[test]
fn group_opacity_reaches_the_output() {
    let image = render_fixture("basic/group-opacity.svg", 32, 32);
    let alphas: Vec<u8> = image.alpha().into_iter().filter(|&a| a > 0).collect();
    assert!(!alphas.is_empty());
    assert!(
        alphas.iter().all(|&a| a < 250),
        "a half-opaque group must not render opaque"
    );
}

#[test]
fn paint_order_changes_which_paint_is_visible_on_the_boundary() {
    let source = r##"<svg viewBox="0 0 32 32">
      <rect x="8" y="8" width="16" height="16" fill="#ff0000"
            stroke="#0000ff" stroke-width="8" paint-order="{ORDER}"/>
    </svg>"##;
    let normal = render_source(
        &source.replace("{ORDER}", "normal"),
        &RasterOptions::square(32),
    );
    let stroke_first = render_source(
        &source.replace("{ORDER}", "stroke"),
        &RasterOptions::square(32),
    );

    // Just inside the boundary: the stroke wins normally, the fill wins when
    // the stroke goes down first.
    assert_eq!(rgb_at(&normal, 16, 9), [0, 0, 255]);
    assert_eq!(rgb_at(&stroke_first, 16, 9), [255, 0, 0]);
    // Outside the fill, only the stroke is there either way.
    assert_eq!(rgb_at(&normal, 16, 6), [0, 0, 255]);
    assert_eq!(rgb_at(&stroke_first, 16, 6), [0, 0, 255]);
    // The silhouette is identical: only the order changed.
    assert_eq!(normal.alpha(), stroke_first.alpha());
}

// ---------------------------------------------------------------------------
// currentColor
// ---------------------------------------------------------------------------

#[test]
fn current_color_paints_the_requested_colour() {
    let document = compile_fixture("lucide/search.svg");
    for colour in [
        Color::rgb(255, 0, 0),
        Color::rgb(0, 255, 0),
        Color::rgb(17, 34, 51),
        Color::WHITE,
    ] {
        let image = render(
            &document,
            &RasterOptions::square(48).with_current_color(colour),
        )
        .unwrap();
        // Somewhere on the circle's ring, where coverage is full.
        let solid = (0..48)
            .flat_map(|y| (0..48).map(move |x| (x, y)))
            .find(|&(x, y)| alpha_at(&image, x, y) == 255)
            .expect("something should be fully covered");
        assert_eq!(
            rgb_at(&image, solid.0, solid.1),
            [colour.r, colour.g, colour.b],
            "{colour:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// The tintable alpha-mask path
// ---------------------------------------------------------------------------

/// The property the render cache depends on: for a tintable asset, coverage is
/// the same whatever colour was asked for. If this fails, the cache cannot
/// leave colour out of its key.
#[test]
fn a_mask_is_identical_whatever_colour_is_requested() {
    let document = compile_fixture("lucide/search.svg");
    assert!(document.features.is_tintable());

    let mask = |colour| {
        render(
            &document,
            &RasterOptions::square(32)
                .with_mode(RasterMode::AlphaMask)
                .with_current_color(colour),
        )
        .unwrap()
    };

    let reference = mask(Color::BLACK);
    for colour in [
        Color::WHITE,
        Color::rgb(255, 0, 0),
        Color::rgb(1, 2, 3),
        Color::rgb(200, 200, 200),
    ] {
        assert_eq!(mask(colour), reference, "{colour:?}");
    }
}

/// A mask's RGB must be white, so that `ImageColor3` — which multiplies —
/// reproduces the requested tint exactly rather than darkening it.
#[test]
fn a_mask_is_white_so_that_image_color3_reproduces_a_tint_exactly() {
    let image = render(
        &compile_fixture("lucide/search.svg"),
        &RasterOptions::square(32).with_mode(RasterMode::AlphaMask),
    )
    .unwrap();

    for y in 0..image.height {
        for x in 0..image.width {
            assert_eq!(rgb_at(&image, x, y), [255, 255, 255], "({x}, {y})");
        }
    }
}

/// The mask must carry exactly the coverage the colour path produces, or a
/// tinted icon would not be the same shape as an untinted one.
#[test]
fn a_mask_carries_the_same_coverage_as_a_colour_render() {
    for fixture in [
        "lucide/search.svg",
        "lucide/settings.svg",
        "lucide/git-branch.svg",
        "basic/evenodd.svg",
    ] {
        let document = compile_fixture(fixture);
        for size in [16u32, 24, 64] {
            let colour = render(&document, &RasterOptions::square(size)).unwrap();
            let mask = render(
                &document,
                &RasterOptions::square(size).with_mode(RasterMode::AlphaMask),
            )
            .unwrap();
            assert_eq!(colour.alpha(), mask.alpha(), "{fixture} at {size}");
        }
    }
}

/// Opacity is part of how much of the shape is *there*, not of what colour it
/// is, so it belongs in the mask. Dropping it would make a half-transparent
/// icon render solid once tinted.
#[test]
fn a_mask_keeps_paint_opacity() {
    let image = render_source(
        r##"<svg viewBox="0 0 4 4">
          <rect width="4" height="4" fill="currentColor" fill-opacity="0.5"/>
        </svg>"##,
        &RasterOptions::square(4).with_mode(RasterMode::AlphaMask),
    );
    assert_eq!(alpha_at(&image, 2, 2), 128);
}

/// Masking a *non*-tintable asset is allowed — it is just a silhouette — but it
/// must not silently claim the colours were preserved.
#[test]
fn masking_a_multicoloured_asset_produces_its_silhouette() {
    let source = r##"<svg viewBox="0 0 8 4">
      <rect width="4" height="4" fill="#ff0000"/>
      <rect x="4" width="4" height="4" fill="#0000ff"/>
    </svg>"##;
    let image = render_source(
        source,
        &RasterOptions::new(8, 4).with_mode(RasterMode::AlphaMask),
    );
    assert!(image.alpha().iter().all(|&a| a == 255));
    assert_eq!(rgb_at(&image, 1, 1), rgb_at(&image, 6, 1));
}
