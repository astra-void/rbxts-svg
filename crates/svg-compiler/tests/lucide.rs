//! Lucide compatibility.
//!
//! Lucide is the first large real-world SVG corpus `@rbxts/svg` targets, so
//! these tests run against unmodified icons from `lucide-static@1.30.0` rather
//! than hand-written approximations of them. Between them the six fixtures
//! cover paths, circles, lines, arcs, rounded caps and joins, multiple
//! subpaths, and `currentColor`.

use std::path::{Path, PathBuf};

use svg_compiler::{CompileOptions, Severity};
use svg_core::{FeatureFlags, LineCap, LineJoin, Paint, PathCommand};

const ICONS: &[&str] = &[
    "search",       // path + circle
    "settings",     // arcs, many subpaths
    "chevron-down", // a single open polyline-ish path
    "circle-alert", // circle + two lines, one degenerate
    "bell",         // two paths, arcs
    "git-branch",   // path + two circles
];

fn lucide_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/lucide")
        .canonicalize()
        .expect("lucide fixtures should exist")
}

fn compile(icon: &str) -> svg_compiler::CompileOutput {
    let name = format!("{icon}.svg");
    let source = std::fs::read_to_string(lucide_dir().join(&name))
        .unwrap_or_else(|e| panic!("cannot read {name}: {e}"));
    let options = CompileOptions {
        source_name: Some(name.clone()),
        ..Default::default()
    };
    svg_compiler::compile(&source, &options)
        .unwrap_or_else(|e| panic!("{name} should compile:\n{}", e.render(Some(&name))))
}

#[test]
fn every_icon_compiles_without_unsupported_features() {
    for icon in ICONS {
        let out = compile(icon);
        let errors: Vec<_> = out
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{icon}: {errors:?}");
        assert!(!out.document.shapes.is_empty(), "{icon} produced no shapes");
    }
}

#[test]
fn every_icon_uses_the_24_unit_lucide_coordinate_system() {
    for icon in ICONS {
        let out = compile(icon);
        assert_eq!(out.document.view_box.x, 0.0, "{icon}");
        assert_eq!(out.document.view_box.y, 0.0, "{icon}");
        assert_eq!(out.document.view_box.width, 24.0, "{icon}");
        assert_eq!(out.document.view_box.height, 24.0, "{icon}");
    }
}

/// The property the whole tinting fast path depends on: a Lucide icon is
/// stroke-only, `currentColor`, and therefore rasterizable once and recoloured
/// per instance.
#[test]
fn every_icon_is_monochrome_current_color_and_tintable() {
    for icon in ICONS {
        let flags = compile(icon).document.features;
        assert!(
            flags.contains(FeatureFlags::USES_CURRENT_COLOR),
            "{icon} should use currentColor"
        );
        assert!(flags.contains(FeatureFlags::MONOCHROME), "{icon}");
        assert!(flags.contains(FeatureFlags::HAS_STROKE), "{icon}");
        assert!(
            !flags.contains(FeatureFlags::HAS_FILL),
            "{icon}: Lucide sets fill=\"none\""
        );
        assert!(flags.is_tintable(), "{icon} should be tintable");
    }
}

#[test]
fn every_icon_carries_lucides_stroke_style() {
    for icon in ICONS {
        let out = compile(icon);
        for (index, shape) in out.document.shapes.iter().enumerate() {
            let stroke = shape
                .stroke
                .unwrap_or_else(|| panic!("{icon} shape {index} has no stroke"));
            assert_eq!(stroke.paint, Paint::CurrentColor, "{icon}");
            assert_eq!(stroke.width, 2.0, "{icon}");
            assert_eq!(stroke.line_cap, LineCap::Round, "{icon}");
            assert_eq!(stroke.line_join, LineJoin::Round, "{icon}");
            assert!(shape.fill.is_none(), "{icon}");
        }
    }
}

#[test]
fn every_icon_lowers_to_canonical_commands_only() {
    for icon in ICONS {
        for shape in &compile(icon).document.shapes {
            for command in shape.geometry.commands() {
                assert!(
                    matches!(
                        command,
                        PathCommand::MoveTo(_)
                            | PathCommand::LineTo(_)
                            | PathCommand::CubicTo(..)
                            | PathCommand::Close
                    ),
                    "{icon}"
                );
            }
        }
    }
}

/// `search.svg` is `<path d="m21 21-4.34-4.34"/>` plus `<circle r="8"/>`: one
/// line segment and one circle lowered to four cubic arcs.
#[test]
fn search_has_the_expected_structure() {
    let out = compile("search");
    assert_eq!(out.document.shapes.len(), 2);

    let line = &out.document.shapes[0];
    assert_eq!(line.geometry.subpath_count(), 1);
    assert!(matches!(
        line.geometry.commands()[0],
        PathCommand::MoveTo(_)
    ));
    assert!(matches!(
        line.geometry.commands()[1],
        PathCommand::LineTo(_)
    ));

    let circle = &out.document.shapes[1];
    let cubics = circle
        .geometry
        .commands()
        .iter()
        .filter(|c| matches!(c, PathCommand::CubicTo(..)))
        .count();
    assert_eq!(cubics, 4, "a circle lowers to four cubic quadrants");
}

/// `circle-alert` contains `<line x1="12" x2="12.01" y1="16" y2="16"/>`, a
/// near-zero-length segment that only paints because the cap is round. It must
/// survive the "drop shapes that paint nothing" pass.
#[test]
fn degenerate_dot_segments_survive_because_the_cap_paints_them() {
    let out = compile("circle-alert");
    assert_eq!(
        out.document.shapes.len(),
        3,
        "circle + two lines, including the dot of the exclamation mark"
    );
}

#[test]
fn multi_subpath_icons_keep_their_subpaths() {
    // `settings` is a gear outline (one long subpath) plus a circle.
    let out = compile("settings");
    let total: usize = out
        .document
        .shapes
        .iter()
        .map(|s| s.geometry.subpath_count())
        .sum();
    assert!(total >= 2, "expected at least two subpaths, got {total}");
}

#[test]
fn geometry_stays_inside_the_view_box() {
    // Lucide icons are drawn within their 24x24 box. A view-box correction
    // applied in the wrong direction would blow this up by 1x or 4x, so this
    // is a cheap guard against the geometry silently landing in pixel space.
    for icon in ICONS {
        for shape in &compile(icon).document.shapes {
            for command in shape.geometry.commands() {
                if let Some(p) = command.end_point() {
                    assert!(
                        (-1.0..=25.0).contains(&p.x) && (-1.0..=25.0).contains(&p.y),
                        "{icon}: point {p:?} lies outside the 24-unit view box"
                    );
                }
            }
        }
    }
}

#[test]
fn compilation_is_deterministic() {
    for icon in ICONS {
        assert_eq!(compile(icon).document, compile(icon).document, "{icon}");
    }
}
