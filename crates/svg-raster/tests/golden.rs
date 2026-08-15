//! Golden image tests.
//!
//! # What these are for, and what they are not for
//!
//! A golden image answers one question: *did the output change?* It does not
//! answer whether the output is right — that is what the geometry tests in
//! `src/` and the property tests in the other suites are for, and they are the
//! ones to read when a golden fails. A golden that is updated without
//! understanding why it moved is worse than no golden at all.
//!
//! They earn their place for the Luau port. The eventual `EditableImage`
//! renderer needs a fixed target to reproduce, and "these exact bytes, for
//! these exact inputs" is the only unambiguous form of one.
//!
//! # Storage
//!
//! Goldens are stored as PNG, which is only a container: the rasterizer itself
//! hands back raw RGBA and has no idea image files exist. Both `png` and the
//! encoder below are dev-only, so the crate stays dependency-free for anyone
//! consuming it.
//!
//! PNG rather than raw bytes because a golden that has to be *looked at* when
//! it fails is worth being able to open. A `.raw` blob would make regressions
//! invisible until someone wrote a viewer.
//!
//! # Regenerating
//!
//! ```text
//! UPDATE_GOLDEN=1 cargo test -p svg-raster
//! ```
//!
//! Review the diff. A change in anti-aliasing touches every edge pixel by a
//! level or two; a change in geometry moves whole regions. They look nothing
//! alike, and only one of them is ever intended.

mod common;

use std::path::PathBuf;

use common::*;
use svg_raster::{RasterImage, RasterMode, RasterOptions, render};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn updating() -> bool {
    std::env::var("UPDATE_GOLDEN").is_ok_and(|value| value == "1")
}

/// Encodes an image as an 8-bit RGBA PNG.
fn encode_png(image: &RasterImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        // Fixed compression, so the same pixels always produce the same file and
        // a golden's bytes do not shift with a `png` point release.
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().expect("PNG header");
        writer.write_image_data(&image.pixels).expect("PNG data");
    }
    bytes
}

fn decode_png(bytes: &[u8]) -> RasterImage {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("PNG info");
    let mut pixels = vec![0; reader.output_buffer_size().expect("PNG buffer size")];
    let info = reader.next_frame(&mut pixels).expect("PNG frame");
    pixels.truncate(info.buffer_size());
    RasterImage {
        width: info.width,
        height: info.height,
        pixels,
    }
}

/// Compares an image against its golden, or rewrites it under `UPDATE_GOLDEN=1`.
fn assert_golden(name: &str, image: &RasterImage) {
    let path = golden_dir().join(format!("{name}.png"));

    if updating() {
        std::fs::create_dir_all(path.parent().expect("golden path has a parent"))
            .expect("create golden directory");
        std::fs::write(&path, encode_png(image)).expect("write golden");
        return;
    }

    let expected = decode_png(&std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden {}. Regenerate with: UPDATE_GOLDEN=1 cargo test -p svg-raster",
            path.display()
        )
    }));

    assert_eq!(
        (expected.width, expected.height),
        (image.width, image.height),
        "{name}: size changed"
    );

    if expected.pixels == image.pixels {
        return;
    }

    // Say *how* it differs, because "these bytes are not those bytes" is the
    // least useful thing a golden failure can tell you.
    let mut differing = 0usize;
    let mut worst = 0u8;
    for (a, b) in expected.pixels.iter().zip(image.pixels.iter()) {
        if a != b {
            differing += 1;
            worst = worst.max(a.abs_diff(*b));
        }
    }
    panic!(
        "{name} does not match its golden: {differing} of {} channel values differ, \
         worst by {worst}.\n\nRendered:\n{}\nIf this change is intended, regenerate with:\n  \
         UPDATE_GOLDEN=1 cargo test -p svg-raster\n",
        image.pixels.len(),
        ascii_alpha(image)
    );
}

// ---------------------------------------------------------------------------
// Fills
// ---------------------------------------------------------------------------

#[test]
fn golden_fills() {
    for (name, fixture) in [
        ("fills/circle", "basic/circle.svg"),
        ("fills/ellipse", "basic/ellipse.svg"),
        ("fills/rect", "basic/rect.svg"),
        ("fills/polygon", "basic/polygon.svg"),
        ("fills/evenodd", "basic/evenodd.svg"),
        ("fills/multiple-subpaths", "basic/multiple-subpaths.svg"),
        ("fills/offset-viewbox", "basic/offset-viewbox.svg"),
        ("fills/quadratic-and-arc", "basic/quadratic-and-arc.svg"),
        ("fills/transformed-group", "basic/transformed-group.svg"),
    ] {
        for size in [16u32, 24, 32, 64] {
            assert_golden(
                &format!("{name}-{size}"),
                &render_fixture(fixture, size, size),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Strokes
// ---------------------------------------------------------------------------

#[test]
fn golden_strokes() {
    let cases: &[(&str, &str)] = &[
        (
            "strokes/caps-butt",
            r##"<svg viewBox="0 0 32 32"><path d="M6 16 H26" stroke="#000" stroke-width="8"
                 fill="none" stroke-linecap="butt"/></svg>"##,
        ),
        (
            "strokes/caps-round",
            r##"<svg viewBox="0 0 32 32"><path d="M6 16 H26" stroke="#000" stroke-width="8"
                 fill="none" stroke-linecap="round"/></svg>"##,
        ),
        (
            "strokes/caps-square",
            r##"<svg viewBox="0 0 32 32"><path d="M6 16 H26" stroke="#000" stroke-width="8"
                 fill="none" stroke-linecap="square"/></svg>"##,
        ),
        (
            "strokes/join-miter",
            r##"<svg viewBox="0 0 32 32"><path d="M6 6 H26 V26" stroke="#000" stroke-width="8"
                 fill="none" stroke-linejoin="miter"/></svg>"##,
        ),
        (
            "strokes/join-round",
            r##"<svg viewBox="0 0 32 32"><path d="M6 6 H26 V26" stroke="#000" stroke-width="8"
                 fill="none" stroke-linejoin="round"/></svg>"##,
        ),
        (
            "strokes/join-bevel",
            r##"<svg viewBox="0 0 32 32"><path d="M6 6 H26 V26" stroke="#000" stroke-width="8"
                 fill="none" stroke-linejoin="bevel"/></svg>"##,
        ),
        (
            "strokes/miter-limit-clipped",
            r##"<svg viewBox="0 0 32 32"><path d="M2 30 L16 2 L30 30" stroke="#000"
                 stroke-width="6" fill="none" stroke-linejoin="miter"
                 stroke-miterlimit="1.5"/></svg>"##,
        ),
        (
            "strokes/miter-limit-generous",
            r##"<svg viewBox="0 0 32 32"><path d="M2 30 L16 2 L30 30" stroke="#000"
                 stroke-width="6" fill="none" stroke-linejoin="miter"
                 stroke-miterlimit="10"/></svg>"##,
        ),
        (
            "strokes/closed-square",
            r##"<svg viewBox="0 0 32 32"><rect x="8" y="8" width="16" height="16"
                 stroke="#000" stroke-width="4" fill="none"/></svg>"##,
        ),
        (
            "strokes/self-crossing",
            r##"<svg viewBox="0 0 32 32"><path d="M4 4 L28 28 L4 28 L28 4" stroke="#000"
                 stroke-width="5" fill="none"/></svg>"##,
        ),
        (
            "strokes/fill-and-stroke",
            r##"<svg viewBox="0 0 32 32"><rect x="8" y="8" width="16" height="16"
                 fill="#c0c0c0" stroke="#000" stroke-width="4"/></svg>"##,
        ),
        (
            "strokes/paint-order-stroke",
            r##"<svg viewBox="0 0 32 32"><rect x="8" y="8" width="16" height="16"
                 fill="#c0c0c0" stroke="#000" stroke-width="8"
                 paint-order="stroke"/></svg>"##,
        ),
    ];

    for (name, source) in cases {
        for size in [32u32, 64] {
            assert_golden(
                &format!("{name}-{size}"),
                &render_source(source, &RasterOptions::square(size)),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Lucide
// ---------------------------------------------------------------------------

/// The workload that matters. Every one of these is monochrome `currentColor`
/// with round caps and joins, which is exactly the combination the Roblox
/// runtime's tinting fast path depends on.
#[test]
fn golden_lucide() {
    for (name, fixture) in [
        ("lucide/search", "lucide/search.svg"),
        ("lucide/chevron-down", "lucide/chevron-down.svg"),
        ("lucide/git-branch", "lucide/git-branch.svg"),
        ("lucide/settings", "lucide/settings.svg"),
        ("lucide/circle-alert", "lucide/circle-alert.svg"),
        ("lucide/bell", "lucide/bell.svg"),
    ] {
        for size in [16u32, 24, 32, 64] {
            assert_golden(
                &format!("{name}-{size}"),
                &render_fixture(fixture, size, size),
            );
        }
    }
}

/// Stroke width overrides change the picture, so they need their own goldens.
#[test]
fn golden_lucide_stroke_widths() {
    let document = compile_fixture("lucide/search.svg");
    for (label, options) in [
        ("thin", RasterOptions::square(32).with_stroke_width(1.0)),
        ("thick", RasterOptions::square(32).with_stroke_width(3.0)),
        (
            "absolute-2px-at-32",
            RasterOptions::square(32).with_absolute_stroke_width(2.0),
        ),
        (
            "absolute-2px-at-64",
            RasterOptions::square(64).with_absolute_stroke_width(2.0),
        ),
    ] {
        assert_golden(
            &format!("lucide/search-{label}"),
            &render(&document, &options).unwrap(),
        );
    }
}

/// A tintable icon as an alpha mask: what the Roblox backend will actually
/// upload.
#[test]
fn golden_lucide_alpha_mask() {
    let document = compile_fixture("lucide/search.svg");
    for size in [24u32, 48] {
        assert_golden(
            &format!("lucide/search-mask-{size}"),
            &render(
                &document,
                &RasterOptions::square(size).with_mode(RasterMode::AlphaMask),
            )
            .unwrap(),
        );
    }
}

// ---------------------------------------------------------------------------
// Aspect ratio
// ---------------------------------------------------------------------------

/// Rectangular artwork in square and rectangular targets. The three fixtures
/// have identical geometry, so any two of these goldens differing is entirely
/// down to the fitting policy — the thing version 1 of the IR discarded.
#[test]
fn golden_aspect_ratio() {
    for (name, fixture) in [
        ("aspect-ratio/meet", "basic/aspect-meet.svg"),
        ("aspect-ratio/none", "basic/aspect-none.svg"),
        ("aspect-ratio/slice", "basic/aspect-slice.svg"),
    ] {
        for (width, height) in [(64, 64), (96, 24), (32, 64)] {
            assert_golden(
                &format!("{name}-{width}x{height}"),
                &render_fixture(fixture, width, height),
            );
        }
    }
}

/// A square icon in a rectangular target, which is where a renderer that
/// ignored the policy would visibly stretch it.
#[test]
fn golden_lucide_in_rectangular_targets() {
    for (width, height) in [(64, 32), (32, 64)] {
        assert_golden(
            &format!("aspect-ratio/lucide-search-{width}x{height}"),
            &render_fixture("lucide/search.svg", width, height),
        );
    }
}

// ---------------------------------------------------------------------------
// Compositing
// ---------------------------------------------------------------------------

#[test]
fn golden_compositing() {
    let cases: &[(&str, &str)] = &[
        (
            "compositing/overlap-opaque",
            r##"<svg viewBox="0 0 32 32">
                 <rect x="2" y="2" width="20" height="20" fill="#ff0000"/>
                 <rect x="10" y="10" width="20" height="20" fill="#0000ff"/>
               </svg>"##,
        ),
        (
            "compositing/overlap-translucent",
            r##"<svg viewBox="0 0 32 32">
                 <rect x="2" y="2" width="20" height="20" fill="#ff0000"/>
                 <rect x="10" y="10" width="20" height="20" fill="#0000ff"
                       fill-opacity="0.5"/>
               </svg>"##,
        ),
        (
            "compositing/soft-edges",
            r##"<svg viewBox="0 0 32 32">
                 <circle cx="16" cy="16" r="12" fill="#000000" fill-opacity="0.35"/>
                 <circle cx="16" cy="16" r="6" fill="#000000" fill-opacity="0.35"/>
               </svg>"##,
        ),
        (
            "compositing/group-opacity",
            r##"<svg viewBox="0 0 32 32">
                 <g opacity="0.5">
                   <rect x="4" y="4" width="16" height="16" fill="#ff0000"/>
                   <rect x="12" y="12" width="16" height="16" fill="#0000ff"/>
                 </g>
               </svg>"##,
        ),
    ];

    for (name, source) in cases {
        assert_golden(name, &render_source(source, &RasterOptions::square(32)));
    }
}

// ---------------------------------------------------------------------------
// The goldens' own invariants
// ---------------------------------------------------------------------------

/// A golden that round-trips wrongly would silently compare the wrong bytes.
#[test]
fn the_png_helpers_round_trip_exactly() {
    let image = render_fixture("lucide/settings.svg", 37, 23);
    assert_eq!(decode_png(&encode_png(&image)), image);
}
