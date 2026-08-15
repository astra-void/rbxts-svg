//! Shared helpers for the raster test suites.
//!
//! Every suite renders through the *real* pipeline — parse, compile, encode,
//! decode, rasterize — rather than hand-building documents. A renderer tested
//! against geometry written to suit it proves only that it agrees with itself;
//! going through the compiler and the IR is what makes these tests say anything
//! about the system the Luau backend will inherit.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use svg_compiler::CompileOptions;
use svg_core::SvgDocument;
use svg_raster::{RasterImage, RasterOptions, render};

/// The repository root, found relative to this crate.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root should exist")
}

pub fn fixture_path(relative: &str) -> PathBuf {
    repo_root().join("tests/fixtures").join(relative)
}

pub fn read_fixture(relative: &str) -> String {
    let path = fixture_path(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Compiles a fixture, then round-trips it through the serialized IR.
///
/// The round trip is not incidental. What the Roblox runtime will hand a
/// rasterizer is a *decoded* document, so rendering the compiler's in-memory
/// one would leave the encoder and decoder untested by every raster assertion
/// in this crate.
pub fn compile_fixture(relative: &str) -> SvgDocument {
    compile_source(&read_fixture(relative), relative)
}

/// Compiles SVG source, round-tripping through the IR as [`compile_fixture`]
/// does.
pub fn compile_source(source: &str, label: &str) -> SvgDocument {
    let options = CompileOptions {
        source_name: Some(label.to_string()),
        ..Default::default()
    };
    let output = svg_compiler::compile(source, &options)
        .unwrap_or_else(|e| panic!("{label} should compile:\n{}", e.render(Some(label))));

    let bytes =
        svg_ir::encode(&output.document).unwrap_or_else(|e| panic!("{label} should encode: {e}"));
    let decoded = svg_ir::decode(&bytes).unwrap_or_else(|e| panic!("{label} should decode: {e}"));
    assert_eq!(decoded, output.document, "{label} did not survive the IR");
    decoded
}

/// Renders a fixture at `width` x `height` with default options.
pub fn render_fixture(relative: &str, width: u32, height: u32) -> RasterImage {
    render(
        &compile_fixture(relative),
        &RasterOptions::new(width, height),
    )
    .unwrap_or_else(|e| panic!("{relative} should rasterize: {e}"))
}

/// Renders SVG source at `width` x `height` with the given options.
pub fn render_source(source: &str, options: &RasterOptions) -> RasterImage {
    render(&compile_source(source, "<inline>"), options)
        .unwrap_or_else(|e| panic!("inline source should rasterize: {e}"))
}

/// The alpha of pixel `(x, y)`.
pub fn alpha_at(image: &RasterImage, x: u32, y: u32) -> u8 {
    image.pixel(x, y).expect("pixel should be inside")[3]
}

/// The RGB of pixel `(x, y)`.
pub fn rgb_at(image: &RasterImage, x: u32, y: u32) -> [u8; 3] {
    let [r, g, b, _] = image.pixel(x, y).expect("pixel should be inside");
    [r, g, b]
}

/// Total alpha across the image, normalised to "pixels fully covered".
///
/// A single number that moves when geometry moves. Much better than eyeballing
/// a picture when what you want to know is "did this get bigger".
pub fn alpha_mass(image: &RasterImage) -> f64 {
    image.alpha().iter().map(|&a| a as f64 / 255.0).sum()
}

/// The bounding box of everything with non-zero alpha, as
/// `(min_x, min_y, max_x, max_y)` inclusive, or `None` if the image is empty.
pub fn painted_bounds(image: &RasterImage) -> Option<(u32, u32, u32, u32)> {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for y in 0..image.height {
        for x in 0..image.width {
            if alpha_at(image, x, y) == 0 {
                continue;
            }
            bounds = Some(match bounds {
                None => (x, y, x, y),
                Some((min_x, min_y, max_x, max_y)) => {
                    (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
                }
            });
        }
    }
    bounds
}

/// A one-character-per-pixel rendering of the alpha channel.
///
/// Assertion failures in this crate are about shapes, and a shape is far easier
/// to recognise as a picture than as a list of byte values.
pub fn ascii_alpha(image: &RasterImage) -> String {
    const RAMP: &[u8] = b" .:-=+*#%@";
    let mut out = String::with_capacity(((image.width + 1) * image.height) as usize);
    for y in 0..image.height {
        for x in 0..image.width {
            let alpha = alpha_at(image, x, y) as usize;
            let index = (alpha * (RAMP.len() - 1)) / 255;
            out.push(RAMP[index] as char);
        }
        out.push('\n');
    }
    out
}
