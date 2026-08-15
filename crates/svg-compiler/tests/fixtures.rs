//! Fixture-driven compiler tests.
//!
//! Fixtures live in `tests/fixtures/` at the repository root so that the Rust
//! and TypeScript suites exercise the exact same corpus. Cargo requires
//! integration tests to live inside their crate, hence the relative path.

use std::path::{Path, PathBuf};

use svg_compiler::{CompileError, CompileOptions, DiagnosticCode, Severity, UnsupportedPolicy};
use svg_core::{
    AspectAlign, AspectScale, FeatureFlags, FillRule, LineCap, LineJoin, Paint, PathCommand, Point,
    PreserveAspectRatio,
};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .canonicalize()
        .expect("fixtures directory should exist")
}

fn read(relative: &str) -> String {
    let path = fixture_dir().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn compile(relative: &str) -> svg_compiler::CompileOutput {
    let source = read(relative);
    let options = CompileOptions {
        source_name: Some(relative.to_string()),
        ..Default::default()
    };
    svg_compiler::compile(&source, &options)
        .unwrap_or_else(|e| panic!("{relative} should compile:\n{}", e.render(Some(relative))))
}

fn compile_err(relative: &str) -> CompileError {
    let source = read(relative);
    let options = CompileOptions {
        source_name: Some(relative.to_string()),
        ..Default::default()
    };
    svg_compiler::compile(&source, &options)
        .err()
        .unwrap_or_else(|| panic!("{relative} should have failed to compile"))
}

// ---------------------------------------------------------------------------
// View box
// ---------------------------------------------------------------------------

#[test]
fn view_box_is_preserved_not_replaced_by_width_and_height() {
    // The file declares width/height 96 with a 24-unit view box. The compiled
    // asset must describe the 24-unit coordinate system: the pixel size is the
    // consumer's decision, made at render time.
    let out = compile("basic/offset-viewbox.svg");
    assert_eq!(out.document.view_box.width, 24.0);
    assert_eq!(out.document.view_box.height, 24.0);
    assert_eq!(out.document.view_box.x, -12.0);
    assert_eq!(out.document.view_box.y, -12.0);
}

// ---------------------------------------------------------------------------
// preserveAspectRatio
//
// The view box alone does not say how an asset should fill a target rectangle
// of a different shape. These pin that the authored policy survives compilation
// and produces the right target transform.
// ---------------------------------------------------------------------------

#[test]
fn an_unannotated_document_compiles_to_the_svg_default_policy() {
    let out = compile("lucide/search.svg");
    assert_eq!(
        out.document.preserve_aspect_ratio,
        PreserveAspectRatio::DEFAULT
    );
}

#[test]
fn an_authored_meet_policy_survives_compilation() {
    let out = compile("basic/aspect-meet.svg");
    assert_eq!(
        out.document.preserve_aspect_ratio.align,
        AspectAlign::XMidYMid
    );
    assert_eq!(out.document.preserve_aspect_ratio.scale, AspectScale::Meet);
}

#[test]
fn an_authored_none_policy_survives_compilation() {
    let out = compile("basic/aspect-none.svg");
    assert_eq!(out.document.preserve_aspect_ratio.align, AspectAlign::None);
}

#[test]
fn an_authored_slice_policy_survives_compilation() {
    let out = compile("basic/aspect-slice.svg");
    assert_eq!(
        out.document.preserve_aspect_ratio.align,
        AspectAlign::XMinYMin
    );
    assert_eq!(out.document.preserve_aspect_ratio.scale, AspectScale::Slice);
}

/// The three fixtures share their geometry and differ only in policy, so the
/// target transform is the only thing that can distinguish them — which is
/// exactly the information version 1 of the IR threw away.
#[test]
fn the_compiled_policy_drives_the_target_transform() {
    let corner = |fixture: &str| {
        compile(fixture)
            .document
            .target_transform(100.0, 100.0)
            .map_point(Point::new(0.0, 0.0))
    };

    // 24x12 letterboxed into 100x100: 100x50 centred vertically.
    assert_eq!(corner("basic/aspect-meet.svg"), Point::new(0.0, 25.0));
    // Stretched: the view box corner lands on the target corner.
    assert_eq!(corner("basic/aspect-none.svg"), Point::new(0.0, 0.0));
    // Sliced from the top-left: 200x100, overflowing to the right.
    assert_eq!(corner("basic/aspect-slice.svg"), Point::new(0.0, 0.0));
    assert_eq!(
        compile("basic/aspect-slice.svg")
            .document
            .target_transform(100.0, 100.0)
            .map_point(Point::new(24.0, 12.0)),
        Point::new(200.0, 100.0)
    );
}

/// The whole point of the field is that it reaches a renderer, which means it
/// has to survive serialization too.
#[test]
fn the_policy_survives_compile_encode_decode() {
    for (fixture, expected) in [
        (
            "basic/aspect-meet.svg",
            PreserveAspectRatio::new(AspectAlign::XMidYMid, AspectScale::Meet),
        ),
        (
            "basic/aspect-none.svg",
            PreserveAspectRatio::new(AspectAlign::None, AspectScale::Meet),
        ),
        (
            "basic/aspect-slice.svg",
            PreserveAspectRatio::new(AspectAlign::XMinYMin, AspectScale::Slice),
        ),
        ("lucide/search.svg", PreserveAspectRatio::DEFAULT),
    ] {
        let document = compile(fixture).document;
        let bytes = svg_ir::encode(&document).unwrap();
        let decoded = svg_ir::decode(&bytes).unwrap();
        assert_eq!(decoded.preserve_aspect_ratio, expected, "{fixture}");
        assert_eq!(decoded, document, "{fixture}");
    }
}

/// The heart of the view-box correction: usvg would have scaled this circle by
/// 4x and shifted it. Geometry must come back in view box space.
#[test]
fn geometry_is_returned_to_view_box_space() {
    let out = compile("basic/offset-viewbox.svg");
    let shape = &out.document.shapes[0];

    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    for command in shape.geometry.commands() {
        if let Some(p) = command.end_point() {
            min_x = min_x.min(p.x);
            max_x = max_x.max(p.x);
        }
    }
    // A radius-10 circle centred on the view box origin (0,0).
    assert!((min_x + 10.0).abs() < 0.05, "min_x = {min_x}");
    assert!((max_x - 10.0).abs() < 0.05, "max_x = {max_x}");
}

#[test]
fn missing_coordinate_system_is_a_structured_error() {
    assert!(matches!(
        compile_err("unsupported/no-viewbox.svg"),
        CompileError::InvalidViewBox { .. }
    ));
}

#[test]
fn malformed_xml_does_not_panic() {
    assert!(matches!(
        compile_err("unsupported/malformed.svg"),
        CompileError::Parse(_)
    ));
}

// ---------------------------------------------------------------------------
// Primitive lowering
// ---------------------------------------------------------------------------

/// Every primitive must arrive as generic path geometry, so the runtime needs
/// no separate implementation for any of them.
#[test]
fn all_primitives_lower_to_paths() {
    for fixture in [
        "basic/circle.svg",
        "basic/rect.svg",
        "basic/ellipse.svg",
        "basic/line.svg",
        "basic/polyline.svg",
        "basic/polygon.svg",
        "basic/simple-path.svg",
    ] {
        let out = compile(fixture);
        assert_eq!(out.document.shapes.len(), 1, "{fixture}");
        assert!(
            out.document.shapes[0].geometry.has_drawing_commands(),
            "{fixture} produced no drawing commands"
        );
    }
}

/// The runtime command set is exactly four opcodes. Anything else reaching the
/// IR would mean the lowering has a hole.
#[test]
fn only_canonical_commands_are_produced() {
    for entry in std::fs::read_dir(fixture_dir().join("basic")).unwrap() {
        let path = entry.unwrap().path();
        let name = format!("basic/{}", path.file_name().unwrap().to_string_lossy());
        let out = compile(&name);
        for shape in &out.document.shapes {
            for command in shape.geometry.commands() {
                assert!(
                    matches!(
                        command,
                        PathCommand::MoveTo(_)
                            | PathCommand::LineTo(_)
                            | PathCommand::CubicTo(..)
                            | PathCommand::Close
                    ),
                    "{name} produced a non-canonical command"
                );
            }
        }
    }
}

#[test]
fn quadratics_and_arcs_become_cubics() {
    let out = compile("basic/quadratic-and-arc.svg");
    let has_cubic = out
        .document
        .shapes
        .iter()
        .flat_map(|s| s.geometry.commands())
        .any(|c| matches!(c, PathCommand::CubicTo(..)));
    assert!(has_cubic, "Q/T/A should lower into cubic segments");
}

#[test]
fn shorthand_commands_are_expanded() {
    let out = compile("basic/shorthand-commands.svg");
    assert!(out.document.shapes[0].geometry.has_drawing_commands());
}

#[test]
fn subpaths_are_preserved() {
    let out = compile("basic/multiple-subpaths.svg");
    assert_eq!(out.document.shapes[0].geometry.subpath_count(), 2);
}

#[test]
fn rounded_rect_produces_curves() {
    let out = compile("basic/rect.svg");
    let has_cubic = out.document.shapes[0]
        .geometry
        .commands()
        .iter()
        .any(|c| matches!(c, PathCommand::CubicTo(..)));
    assert!(
        has_cubic,
        "rx should produce rounded corners, not square ones"
    );
}

// ---------------------------------------------------------------------------
// Paint
// ---------------------------------------------------------------------------

#[test]
fn current_color_survives_usvg_normalization() {
    let out = compile("basic/current-color.svg");
    let stroke = out.document.shapes[0].stroke.expect("expected a stroke");
    assert_eq!(stroke.paint, Paint::CurrentColor);
    assert!(
        out.document
            .features
            .contains(FeatureFlags::USES_CURRENT_COLOR)
    );
}

#[test]
fn fixed_colours_are_not_mistaken_for_current_color() {
    let out = compile("basic/simple-path.svg");
    let fill = out.document.shapes[0].fill.expect("expected a fill");
    assert_eq!(
        fill.paint,
        Paint::Solid(svg_core::Color::rgb(0x33, 0x66, 0x99))
    );
}

#[test]
fn fill_rule_is_preserved() {
    let out = compile("basic/evenodd.svg");
    let fill = out.document.shapes[0].fill.expect("expected a fill");
    assert_eq!(fill.rule, FillRule::EvenOdd);
    assert!(
        out.document
            .features
            .contains(FeatureFlags::HAS_EVEN_ODD_FILL)
    );
}

#[test]
fn non_zero_is_the_default_fill_rule() {
    let out = compile("basic/simple-path.svg");
    assert_eq!(out.document.shapes[0].fill.unwrap().rule, FillRule::NonZero);
}

#[test]
fn stroke_caps_and_joins_are_preserved() {
    let out = compile("basic/stroke-round.svg");
    let stroke = out.document.shapes[0].stroke.expect("expected a stroke");
    assert_eq!(stroke.line_cap, LineCap::Round);
    assert_eq!(stroke.line_join, LineJoin::Round);
    assert_eq!(stroke.width, 2.0);
}

// ---------------------------------------------------------------------------
// Transforms and opacity
// ---------------------------------------------------------------------------

/// The group scales by 2 and translates by 4. Baking that into geometry means
/// the stroke width has to scale with it.
#[test]
fn group_transforms_are_baked_into_geometry_and_stroke_width() {
    let out = compile("basic/transformed-group.svg");
    assert_eq!(out.document.shapes.len(), 2);

    let rect = &out.document.shapes[0];
    let start = rect.geometry.commands()[0].end_point().unwrap();
    assert!((start.x - 4.0).abs() < 1e-4, "{start:?}");
    assert!((start.y - 4.0).abs() < 1e-4, "{start:?}");

    let stroked = out.document.shapes[1].stroke.expect("expected a stroke");
    assert!(
        (stroked.width - 2.0).abs() < 1e-4,
        "stroke-width 1 under scale(2) should become 2, got {}",
        stroked.width
    );
}

#[test]
fn group_opacity_is_folded_into_children_with_a_warning() {
    let out = compile("basic/group-opacity.svg");
    for shape in &out.document.shapes {
        let fill = shape.fill.expect("expected a fill");
        assert!((fill.opacity.get() - 0.5).abs() < 1e-4);
    }
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::ApproximatedGroupOpacity),
        "folding group opacity is an approximation and must be reported"
    );
    assert!(
        out.document
            .features
            .contains(FeatureFlags::HAS_TRANSPARENCY)
    );
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[test]
fn unsupported_features_fail_rather_than_render_wrongly() {
    for (fixture, code) in [
        (
            "unsupported/gradient-fill.svg",
            DiagnosticCode::UnsupportedElement,
        ),
        ("unsupported/filter.svg", DiagnosticCode::UnsupportedElement),
        ("unsupported/text.svg", DiagnosticCode::UnsupportedElement),
        (
            "unsupported/clip-path.svg",
            DiagnosticCode::UnsupportedElement,
        ),
    ] {
        let error = compile_err(fixture);
        let CompileError::UnsupportedFeature { diagnostics } = &error else {
            panic!("{fixture}: expected UnsupportedFeature, got {error:?}");
        };
        assert!(
            diagnostics.iter().any(|d| d.code == code),
            "{fixture}: {diagnostics:?}"
        );
        assert!(diagnostics.iter().all(|d| d.severity == Severity::Error));
    }
}

#[test]
fn stroke_dasharray_is_reported() {
    let error = compile_err("unsupported/stroke-dasharray.svg");
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|d| d.code == DiagnosticCode::UnsupportedStrokeDash)
    );
}

#[test]
fn unsupported_errors_name_the_file_element_and_path() {
    let error = compile_err("unsupported/filter.svg");
    let rendered = error.render(Some("unsupported/filter.svg"));

    assert!(
        rendered.contains("Unsupported SVG feature in unsupported/filter.svg"),
        "{rendered}"
    );
    assert!(rendered.contains("<filter id=\"shadow\">"), "{rendered}");
    assert!(
        rendered.contains("svg > defs > filter#shadow"),
        "{rendered}"
    );
}

#[test]
fn unsupported_features_can_be_downgraded_to_warnings() {
    let source = read("unsupported/gradient-fill.svg");
    let options = CompileOptions {
        unsupported: UnsupportedPolicy::Warn,
        source_name: Some("gradient-fill.svg".into()),
        ..Default::default()
    };
    let out = svg_compiler::compile(&source, &options).expect("Warn policy should not fail");

    assert!(
        out.diagnostics
            .iter()
            .all(|d| d.severity != Severity::Error)
    );
    // The gradient-filled rect had no other paint, so nothing is left to draw.
    assert!(out.document.shapes.is_empty());
}

#[test]
fn metadata_and_unreferenced_definitions_do_not_fail_a_compile() {
    let out = compile("basic/metadata-and-title.svg");
    assert_eq!(out.document.shapes.len(), 1);
    assert!(
        out.diagnostics
            .iter()
            .all(|d| d.severity != Severity::Error)
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::UnreferencedDefinition),
        "an unused gradient should be reported as ignored, not as an error"
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::IgnoredMetadata),
        "editor namespaces should be reported as ignored"
    );
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn compilation_is_deterministic_across_runs() {
    for entry in std::fs::read_dir(fixture_dir().join("basic")).unwrap() {
        let path = entry.unwrap().path();
        let name = format!("basic/{}", path.file_name().unwrap().to_string_lossy());
        let first = compile(&name);
        let second = compile(&name);
        assert_eq!(
            first.document, second.document,
            "{name} is not deterministic"
        );
    }
}

#[test]
fn the_source_name_option_does_not_affect_the_output() {
    let source = read("basic/stroke-round.svg");
    let a = svg_compiler::compile(
        &source,
        &CompileOptions {
            source_name: Some("a.svg".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let b = svg_compiler::compile(
        &source,
        &CompileOptions {
            source_name: Some("some/other/path/b.svg".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(a.document, b.document);
}
