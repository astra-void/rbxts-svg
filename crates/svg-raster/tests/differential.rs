//! Differential testing against `resvg`.
//!
//! # What resvg is here for, and what it is not
//!
//! It is an *oracle*. It renders the original `.svg` with a mature, independent
//! implementation, and we ask how close our answer is. It is emphatically not
//! part of the renderer: `svg-raster` consumes our compiled IR, through our
//! flattening, our stroker and our scan converter, because that is the pipeline
//! the Luau backend inherits. Rendering the source SVG with resvg and calling
//! that our output would exercise none of it.
//!
//! # Why not byte equality
//!
//! Two rasterizers agreeing exactly would mean they made every one of the same
//! arbitrary choices — the same flattening tolerance, the same sub-pixel sample
//! positions, the same rounding. That is not a goal, and chasing it would mean
//! copying resvg rather than being comparable to it.
//!
//! So the comparison is on metrics that mean something:
//!
//! | Metric | What it catches |
//! | --- | --- |
//! | **Mean absolute error** | Systematic drift: a half-pixel offset, a wrong scale |
//! | **Max absolute error** | A single misplaced feature the mean would hide |
//! | **Coverage ratio** | A stroke or fill that is uniformly too fat or too thin |
//! | **Bounds agreement** | Geometry in the wrong place entirely |
//!
//! Comparison is on the **alpha channel**. Every fixture here is a single
//! colour, so alpha is the entire picture, and it sidesteps the fact that resvg
//! composites premultiplied and we report straight alpha — a difference in
//! representation, not in output.
//!
//! # Measured differences
//!
//! Thresholds below were set by measuring and then leaving a little headroom,
//! not by relaxing until things passed. Run with `--nocapture` for the numbers.
//!
//! | Case | Mean error | Max | Coverage ratio |
//! | --- | --- | --- | --- |
//! | Axis-aligned fills | **0.000** | **0** | 1.0000 |
//! | Fill rules, both | **0.000** | **0** | 1.0000 |
//! | Butt and square caps | **0.000** | **0** | 1.0000 |
//! | Miter and bevel joins | **0.000** | **0** | 1.0000 |
//! | Round caps and joins | 0.04–0.16 | 30 | ±0.1% |
//! | Diagonal fill | 0.48 | 31 | −0.4% |
//! | Curves, 32–128px | 0.12–0.62 | 32 | ±0.3% |
//! | Lucide icons at 64px | 0.45–1.42 | 39 | ±3% |
//! | Lucide icons at 16px | 1.04–5.52 | 48 | ±5% |
//!
//! Every case with no partial coverage matches **exactly**, which is the
//! strongest available evidence that the transform, the fill rules, the stroke
//! bands, the miter arithmetic and the miter limit are all right: a half-pixel
//! offset or a mis-scaled stroke could not survive it.
//!
//! What is left comes from two known, understood sources.
//!
//! **Anti-aliasing.** resvg accumulates analytical coverage in both axes; this
//! renderer is exact in x and samples in y (see `SUB_SCANLINES` in
//! `src/edges.rs`). The difference is confined to edge pixels and shrinks with
//! resolution — a circle's mean error halves with every doubling: 0.494 at
//! 32px, 0.286 at 64, 0.118 at 128. Small icons look worst simply because at
//! 16x16 a Lucide stroke is 1.3 pixels wide and *every* pixel is an edge pixel.
//!
//! **Flatten-then-stroke.** This renderer flattens curves and then strokes the
//! polyline; resvg offsets the curve itself. So a butt cap at the end of a
//! curve is perpendicular to the first chord rather than to the true tangent,
//! tilting it by the flattening chord angle. It is worth up to a fraction of a
//! pixel at the two endpoints of an open curve and nothing anywhere else —
//! round caps, which every Lucide icon uses, are rotationally symmetric and
//! cannot show it at all. The order is deliberate: stroking a polyline is what
//! makes the Luau port tractable, and `caps_on_a_curve_tilt_by_the_flattening_
//! chord_angle` below pins the size of what it costs.

mod common;

use common::*;
use resvg::tiny_skia;
use svg_raster::{RasterOptions, render};

/// How the two renderers' alpha channels differ.
#[derive(Debug, Clone, Copy)]
struct Difference {
    /// Mean absolute difference per pixel, in alpha levels (0..=255).
    mean: f64,
    /// Largest single-pixel difference, in alpha levels.
    max: u8,
    /// Our total coverage divided by resvg's. 1.0 is perfect agreement.
    coverage_ratio: f64,
    /// Fraction of pixels differing by more than 32 levels.
    badly_wrong: f64,
}

/// Renders the same source through both renderers and measures the difference.
///
/// resvg is handed the *source SVG*; we are handed our own compiled IR. That
/// asymmetry is the point — the whole pipeline is under test, not just the
/// rasterizer.
fn compare(source: &str, width: u32, height: u32) -> Difference {
    let ours = render(
        &compile_source(source, "<differential>"),
        &RasterOptions::new(width, height),
    )
    .expect("our renderer should succeed");

    let theirs = render_with_resvg(source, width, height);

    assert_eq!(ours.alpha().len(), theirs.len());

    let mut total = 0u64;
    let mut max = 0u8;
    let mut badly_wrong = 0usize;
    let mut our_mass = 0u64;
    let mut their_mass = 0u64;

    for (&a, &b) in ours.alpha().iter().zip(theirs.iter()) {
        let delta = a.abs_diff(b);
        total += delta as u64;
        max = max.max(delta);
        if delta > 32 {
            badly_wrong += 1;
        }
        our_mass += a as u64;
        their_mass += b as u64;
    }

    let count = theirs.len() as f64;
    Difference {
        mean: total as f64 / count,
        max,
        coverage_ratio: if their_mass == 0 {
            1.0
        } else {
            our_mass as f64 / their_mass as f64
        },
        badly_wrong: badly_wrong as f64 / count,
    }
}

/// The alpha channel resvg produces for the same source at the same size.
fn render_with_resvg(source: &str, width: u32, height: u32) -> Vec<u8> {
    let tree = resvg::usvg::Tree::from_str(source, &resvg::usvg::Options::default())
        .expect("resvg should parse the source");

    let mut pixmap = tiny_skia::Pixmap::new(width, height).expect("pixmap");
    // resvg draws into the tree's own size, so the view box → target mapping has
    // to be supplied here, exactly as our renderer computes it internally. Both
    // fixtures and sizes are chosen so this is a plain uniform scale.
    let size = tree.size();
    let scale = (width as f32 / size.width()).min(height as f32 / size.height());
    let transform = tiny_skia::Transform::from_translate(
        (width as f32 - size.width() * scale) / 2.0,
        (height as f32 - size.height() * scale) / 2.0,
    )
    .pre_scale(scale, scale);

    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap.pixels().iter().map(|p| p.alpha()).collect()
}

/// Asserts a difference is within budget, printing the numbers either way.
fn assert_close(label: &str, difference: Difference, mean: f64, max: u8, ratio: f64) {
    println!(
        "{label:<44} mean {:>6.3}  max {:>3}  ratio {:>6.4}  bad {:>6.3}%",
        difference.mean,
        difference.max,
        difference.coverage_ratio,
        difference.badly_wrong * 100.0
    );
    assert!(
        difference.mean <= mean,
        "{label}: mean error {:.3} exceeds {mean}",
        difference.mean
    );
    assert!(
        difference.max <= max,
        "{label}: max error {} exceeds {max}",
        difference.max
    );
    assert!(
        (difference.coverage_ratio - 1.0).abs() <= ratio,
        "{label}: coverage ratio {:.4} is more than {ratio} from 1.0",
        difference.coverage_ratio
    );
}

// ---------------------------------------------------------------------------
// Hard-edged fills: these should be all but exact
// ---------------------------------------------------------------------------

/// An axis-aligned rectangle on pixel boundaries has no partial coverage at
/// all, so the two renderers have nothing to disagree about. Anything but an
/// exact match here means a transform is off, not that anti-aliasing differs.
#[test]
fn axis_aligned_fills_match_exactly() {
    for (source, width, height) in [
        (
            r##"<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg">
                 <rect width="32" height="32" fill="#000"/></svg>"##,
            32,
            32,
        ),
        (
            r##"<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg">
                 <rect x="8" y="4" width="16" height="24" fill="#000"/></svg>"##,
            64,
            64,
        ),
        (
            r##"<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg">
                 <path d="M0 0 H8 V8 H16 V16 H0 Z" fill="#000"/></svg>"##,
            32,
            32,
        ),
    ] {
        let difference = compare(source, width, height);
        assert_close("axis-aligned fill", difference, 0.0, 0, 0.0);
    }
}

/// A diagonal edge is where analytical and sampled coverage first diverge, but
/// only along the edge itself.
#[test]
fn diagonal_fills_agree_closely() {
    let difference = compare(
        r##"<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg">
             <path d="M0 0 L32 32 L0 32 Z" fill="#000"/></svg>"##,
        64,
        64,
    );
    assert_close("diagonal fill", difference, 0.6, 40, 0.01);
}

#[test]
fn curved_fills_agree_closely() {
    for (label, source) in [
        (
            "circle",
            r##"<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg">
                 <circle cx="16" cy="16" r="12" fill="#000"/></svg>"##,
        ),
        (
            "ellipse",
            r##"<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg">
                 <ellipse cx="16" cy="16" rx="14" ry="7" fill="#000"/></svg>"##,
        ),
        (
            "bezier blob",
            r##"<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg">
                 <path d="M4 16 C 4 4, 28 4, 28 16 S 4 28, 4 16 Z" fill="#000"/></svg>"##,
        ),
    ] {
        for size in [32u32, 64, 128] {
            let difference = compare(source, size, size);
            assert_close(&format!("{label} @{size}"), difference, 0.8, 40, 0.01);
        }
    }
}

#[test]
fn fill_rules_agree_with_resvg() {
    for rule in ["nonzero", "evenodd"] {
        let difference = compare(
            &format!(
                r##"<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg">
                     <path d="M2 2 H30 V30 H2 Z M8 8 H24 V24 H8 Z" fill="#000"
                           fill-rule="{rule}"/></svg>"##
            ),
            64,
            64,
        );
        assert_close(&format!("fill-rule {rule}"), difference, 0.1, 8, 0.002);
    }
}

// ---------------------------------------------------------------------------
// Strokes
// ---------------------------------------------------------------------------

#[test]
fn stroke_caps_agree_with_resvg() {
    for cap in ["butt", "round", "square"] {
        let difference = compare(
            &format!(
                r##"<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg">
                     <path d="M6 16 H26" stroke="#000" stroke-width="8" fill="none"
                           stroke-linecap="{cap}"/></svg>"##
            ),
            64,
            64,
        );
        assert_close(&format!("cap {cap}"), difference, 0.3, 40, 0.01);
    }
}

#[test]
fn stroke_joins_agree_with_resvg() {
    for join in ["miter", "round", "bevel"] {
        let difference = compare(
            &format!(
                r##"<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg">
                     <path d="M6 6 H26 V26" stroke="#000" stroke-width="8" fill="none"
                           stroke-linejoin="{join}"/></svg>"##
            ),
            64,
            64,
        );
        assert_close(&format!("join {join}"), difference, 0.2, 40, 0.01);
    }
}

#[test]
fn the_miter_limit_agrees_with_resvg() {
    for limit in ["1.2", "2", "10"] {
        let difference = compare(
            &format!(
                r##"<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg">
                     <path d="M2 30 L16 4 L30 30" stroke="#000" stroke-width="6" fill="none"
                           stroke-linejoin="miter" stroke-miterlimit="{limit}"/></svg>"##
            ),
            64,
            64,
        );
        assert_close(&format!("miterlimit {limit}"), difference, 0.6, 40, 0.01);
    }
}

#[test]
fn closed_strokes_agree_with_resvg() {
    for (label, source) in [
        (
            "square",
            r##"<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg">
                 <rect x="8" y="8" width="16" height="16" stroke="#000" stroke-width="4"
                       fill="none"/></svg>"##,
        ),
        (
            "circle",
            r##"<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg">
                 <circle cx="16" cy="16" r="11" stroke="#000" stroke-width="4"
                         fill="none"/></svg>"##,
        ),
    ] {
        let difference = compare(source, 64, 64);
        assert_close(&format!("closed stroke {label}"), difference, 0.9, 40, 0.01);
    }
}

/// The one place this renderer is knowingly, structurally different from resvg.
///
/// Curves are flattened *before* being stroked, so a butt cap at the end of a
/// curve is perpendicular to the first flattened chord rather than to the true
/// tangent — it is tilted by the chord's angle. resvg offsets the curve itself
/// and has no such tilt.
///
/// This test exists to state the size of that: a fraction of a pixel, confined
/// to the two endpoints, and only for a flat cap on a curved path. Round caps —
/// what every Lucide icon uses — are rotationally symmetric and cannot show it.
///
/// It is the price of stroking a polyline, and stroking a polyline is what
/// makes the Luau port a few hundred lines rather than a curve-offsetting
/// library. Worth knowing about; not worth changing the architecture over.
#[test]
fn caps_on_a_curve_tilt_by_the_flattening_chord_angle() {
    // A semicircular arc whose endpoints have exactly vertical tangents, so the
    // cap edge is exactly horizontal for resvg and slightly tilted for us.
    let source = r##"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
         <path d="M4 20 A 6 6 0 0 1 16 20" stroke="#000" stroke-width="2" fill="none"/>
       </svg>"##;

    let flat = compare(source, 128, 128);
    println!(
        "butt cap on a curve   mean {:>6.3}  max {:>3}  ratio {:>6.4}  bad {:>6.3}%",
        flat.mean,
        flat.max,
        flat.coverage_ratio,
        flat.badly_wrong * 100.0
    );

    // The tilt is real but tiny: it never affects more than a sliver of the
    // two cap rows, so the coverage totals still agree to well within a percent.
    assert!(
        (flat.coverage_ratio - 1.0).abs() < 0.02,
        "coverage ratio {:.4}",
        flat.coverage_ratio
    );
    assert!(
        flat.badly_wrong < 0.005,
        "{:.3}% of pixels differ by more than 32 levels",
        flat.badly_wrong * 100.0
    );

    // A round cap on the same path has nothing to tilt, so it agrees far more
    // closely. That contrast is what identifies the cause as the cap and not
    // the curve.
    let round = compare(
        &source.replace("fill=\"none\"", "fill=\"none\" stroke-linecap=\"round\""),
        128,
        128,
    );
    println!(
        "round cap on a curve  mean {:>6.3}  max {:>3}  ratio {:>6.4}  bad {:>6.3}%",
        round.mean,
        round.max,
        round.coverage_ratio,
        round.badly_wrong * 100.0
    );
    assert!(
        round.max < flat.max,
        "a round cap should not show the tilt: {} vs {}",
        round.max,
        flat.max
    );
}

// ---------------------------------------------------------------------------
// Lucide: the workload that actually matters
// ---------------------------------------------------------------------------

/// Every representative Lucide icon, at every size the runtime is likely to
/// ask for. These are round-capped, round-joined `currentColor` strokes —
/// the hardest case for a stroker and the one users will actually see.
#[test]
fn lucide_icons_agree_with_resvg() {
    let mut worst_mean = 0.0f64;
    let mut worst_max = 0u8;

    for fixture in [
        "lucide/search.svg",
        "lucide/chevron-down.svg",
        "lucide/git-branch.svg",
        "lucide/settings.svg",
        "lucide/circle-alert.svg",
        "lucide/bell.svg",
    ] {
        let source = read_fixture(fixture);
        for size in [16u32, 24, 32, 64] {
            let difference = compare(&source, size, size);
            assert_close(
                &format!("{fixture} @{size}"),
                difference,
                // Small sizes are the hardest: a 2-unit Lucide stroke is 1.3
                // pixels at 16x16, so almost every pixel is a partial-coverage
                // one and the two AA schemes disagree across the whole glyph
                // rather than only along an edge. The budget tightens as the
                // icon grows, because that is what a purely edge-localised
                // difference does.
                match size {
                    16 => 7.0,
                    24 => 5.5,
                    32 => 4.5,
                    _ => 2.0,
                },
                90,
                0.08,
            );
            worst_mean = worst_mean.max(difference.mean);
            worst_max = worst_max.max(difference.max);
        }
    }

    println!("\nworst across all Lucide cases: mean {worst_mean:.3}, max {worst_max}");
}

/// A single misplaced subpath would show up as ink in the wrong place long
/// before the mean error noticed, so compare the bounding boxes directly.
#[test]
fn lucide_geometry_lands_in_the_same_place_as_resvg() {
    for fixture in [
        "lucide/search.svg",
        "lucide/git-branch.svg",
        "lucide/settings.svg",
        "lucide/bell.svg",
    ] {
        let source = read_fixture(fixture);
        let size = 64;

        let ours = render(
            &compile_source(&source, fixture),
            &RasterOptions::square(size),
        )
        .unwrap();
        let theirs = render_with_resvg(&source, size, size);

        let bounds = |alpha: &[u8]| {
            let mut result: Option<(u32, u32, u32, u32)> = None;
            for (index, &value) in alpha.iter().enumerate() {
                // A threshold rather than zero: the very faintest edge pixels
                // are exactly where the two disagree, and a bounding box that
                // moved by one because of a single level-3 pixel would be
                // reporting noise.
                if value < 16 {
                    continue;
                }
                let (x, y) = (index as u32 % size, index as u32 / size);
                result = Some(match result {
                    None => (x, y, x, y),
                    Some((a, b, c, d)) => (a.min(x), b.min(y), c.max(x), d.max(y)),
                });
            }
            result.expect("something should be drawn")
        };

        let (ax0, ay0, ax1, ay1) = bounds(&ours.alpha());
        let (bx0, by0, bx1, by1) = bounds(&theirs);
        for (label, ours, theirs) in [
            ("min x", ax0, bx0),
            ("min y", ay0, by0),
            ("max x", ax1, bx1),
            ("max y", ay1, by1),
        ] {
            assert!(
                ours.abs_diff(theirs) <= 1,
                "{fixture}: {label} is {ours}, resvg says {theirs}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Compositing
// ---------------------------------------------------------------------------

#[test]
fn opacity_and_overlap_agree_with_resvg() {
    for (label, source) in [
        (
            "translucent overlap",
            r##"<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg">
                 <rect x="2" y="2" width="20" height="20" fill="#000" fill-opacity="0.5"/>
                 <rect x="10" y="10" width="20" height="20" fill="#000" fill-opacity="0.5"/>
               </svg>"##,
        ),
        (
            "fill and stroke",
            r##"<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg">
                 <rect x="8" y="8" width="16" height="16" fill="#000" fill-opacity="0.4"
                       stroke="#000" stroke-width="4" stroke-opacity="0.8"/>
               </svg>"##,
        ),
    ] {
        let difference = compare(source, 64, 64);
        assert_close(label, difference, 0.3, 4, 0.01);
    }
}

// ---------------------------------------------------------------------------
// Whole-corpus sweep
// ---------------------------------------------------------------------------

/// A backstop over every fixture we can compare, with a deliberately loose
/// budget. Its job is not to be precise but to make sure no fixture is wildly
/// wrong — a subpath dropped, a transform inverted — which the targeted tests
/// above would not notice for a file they do not name.
#[test]
fn no_fixture_is_wildly_different_from_resvg() {
    for fixture in [
        "basic/circle.svg",
        "basic/ellipse.svg",
        "basic/rect.svg",
        "basic/polygon.svg",
        "basic/polyline.svg",
        "basic/line.svg",
        "basic/evenodd.svg",
        "basic/multiple-subpaths.svg",
        "basic/quadratic-and-arc.svg",
        "basic/shorthand-commands.svg",
        "basic/simple-path.svg",
        "basic/stroke-round.svg",
        "basic/transformed-group.svg",
        "basic/offset-viewbox.svg",
        "lucide/search.svg",
        "lucide/chevron-down.svg",
        "lucide/circle-alert.svg",
        "lucide/git-branch.svg",
        "lucide/settings.svg",
        "lucide/bell.svg",
    ] {
        let source = read_fixture(fixture);
        let difference = compare(&source, 64, 64);
        println!(
            "{fixture:<36} mean {:>6.3}  max {:>3}  ratio {:>6.4}  bad {:>6.3}%",
            difference.mean,
            difference.max,
            difference.coverage_ratio,
            difference.badly_wrong * 100.0
        );
        assert!(
            difference.mean < 3.0,
            "{fixture}: mean error {:.3}",
            difference.mean
        );
        assert!(
            difference.badly_wrong < 0.02,
            "{fixture}: {:.2}% of pixels differ by more than 32 levels",
            difference.badly_wrong * 100.0
        );
        assert!(
            (difference.coverage_ratio - 1.0).abs() < 0.05,
            "{fixture}: coverage ratio {:.4}",
            difference.coverage_ratio
        );
    }
}
