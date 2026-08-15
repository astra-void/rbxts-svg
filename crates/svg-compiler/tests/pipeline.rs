//! End-to-end pipeline tests: source → semantic model → IR → semantic model.
//!
//! Two kinds of golden file back these tests, both regenerated with
//! `UPDATE_GOLDEN=1 cargo test`:
//!
//! - `tests/golden/*.txt` — a readable dump of the semantic model. Reviewable
//!   in a diff, which an opaque blob would not be.
//! - `tests/golden/hashes.txt` — the content hash of every fixture's serialized
//!   IR. This is what pins determinism: if lowering or encoding changes by so
//!   much as a rounding, the hash moves.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use svg_compiler::CompileOptions;
use svg_core::{Paint, PathCommand, Shape, SvgDocument};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn golden_dir() -> PathBuf {
    repo_root().join("crates/svg-compiler/tests/golden")
}

fn updating() -> bool {
    std::env::var("UPDATE_GOLDEN").is_ok_and(|v| v == "1")
}

/// Every fixture, as `dir/name.svg` paths relative to `tests/fixtures`, sorted
/// so the golden files have a stable order.
fn all_compilable_fixtures() -> Vec<String> {
    let root = repo_root().join("tests/fixtures");
    let mut out = Vec::new();
    for dir in ["basic", "lucide"] {
        let mut names: Vec<String> = std::fs::read_dir(root.join(dir))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".svg"))
            .collect();
        names.sort();
        out.extend(names.into_iter().map(|n| format!("{dir}/{n}")));
    }
    out
}

fn compile(relative: &str) -> SvgDocument {
    let source =
        std::fs::read_to_string(repo_root().join("tests/fixtures").join(relative)).unwrap();
    let options = CompileOptions {
        source_name: Some(relative.to_string()),
        ..Default::default()
    };
    svg_compiler::compile(&source, &options)
        .unwrap_or_else(|e| panic!("{relative}:\n{}", e.render(Some(relative))))
        .document
}

/// Compares against a golden file, or rewrites it when `UPDATE_GOLDEN=1`.
fn assert_golden(name: &str, actual: &str) {
    let path = golden_dir().join(name);
    if updating() {
        std::fs::create_dir_all(golden_dir()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden file {}. Regenerate with: UPDATE_GOLDEN=1 cargo test",
            path.display()
        )
    });
    assert_eq!(
        actual,
        expected,
        "\n{} is out of date. If this change is intended, regenerate with:\n  \
         UPDATE_GOLDEN=1 cargo test\n",
        path.display()
    );
}

// ---------------------------------------------------------------------------
// Human-readable semantic snapshots
// ---------------------------------------------------------------------------

fn dump(document: &SvgDocument) -> String {
    let vb = document.view_box;
    let mut out = String::new();
    writeln!(out, "viewBox {} {} {} {}", vb.x, vb.y, vb.width, vb.height).unwrap();
    writeln!(
        out,
        "preserveAspectRatio {:?} {:?}",
        document.preserve_aspect_ratio.align, document.preserve_aspect_ratio.scale
    )
    .unwrap();
    writeln!(out, "features {:?}", document.features).unwrap();
    writeln!(out, "shapes {}", document.shapes.len()).unwrap();
    for (index, shape) in document.shapes.iter().enumerate() {
        writeln!(out, "\nshape {index}").unwrap();
        writeln!(out, "  paintOrder {:?}", shape.paint_order).unwrap();
        dump_paints(&mut out, shape);
        writeln!(out, "  commands {}", shape.geometry.commands().len()).unwrap();
        for command in shape.geometry.commands() {
            writeln!(out, "    {}", dump_command(command)).unwrap();
        }
    }
    out
}

fn dump_paints(out: &mut String, shape: &Shape) {
    match shape.fill {
        Some(f) => writeln!(
            out,
            "  fill {} opacity {:.4} rule {:?}",
            dump_paint(f.paint),
            f.opacity.get(),
            f.rule
        )
        .unwrap(),
        None => writeln!(out, "  fill none").unwrap(),
    }
    match shape.stroke {
        Some(s) => writeln!(
            out,
            "  stroke {} opacity {:.4} width {:.4} cap {:?} join {:?} miter {:.4}",
            dump_paint(s.paint),
            s.opacity.get(),
            s.width,
            s.line_cap,
            s.line_join,
            s.miter_limit
        )
        .unwrap(),
        None => writeln!(out, "  stroke none").unwrap(),
    }
}

fn dump_paint(paint: Paint) -> String {
    match paint {
        Paint::CurrentColor => "currentColor".to_string(),
        Paint::Solid(c) => format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b),
    }
}

fn dump_command(command: &PathCommand) -> String {
    // Four decimals: enough to catch a real geometry regression, coarse enough
    // that the last bit of f32 noise does not churn the golden files.
    match *command {
        PathCommand::MoveTo(p) => format!("M {:.4} {:.4}", p.x, p.y),
        PathCommand::LineTo(p) => format!("L {:.4} {:.4}", p.x, p.y),
        PathCommand::CubicTo(a, b, c) => format!(
            "C {:.4} {:.4} {:.4} {:.4} {:.4} {:.4}",
            a.x, a.y, b.x, b.y, c.x, c.y
        ),
        PathCommand::Close => "Z".to_string(),
    }
}

#[test]
fn lucide_search_semantic_snapshot() {
    assert_golden("lucide-search.txt", &dump(&compile("lucide/search.svg")));
}

#[test]
fn primitive_lowering_snapshot() {
    let mut out = String::new();
    for fixture in [
        "basic/rect.svg",
        "basic/circle.svg",
        "basic/ellipse.svg",
        "basic/line.svg",
        "basic/polyline.svg",
        "basic/polygon.svg",
    ] {
        writeln!(out, "=== {fixture}").unwrap();
        out.push_str(&dump(&compile(fixture)));
        out.push('\n');
    }
    assert_golden("primitives.txt", &out);
}

#[test]
fn transformed_group_snapshot() {
    assert_golden(
        "transformed-group.txt",
        &dump(&compile("basic/transformed-group.svg")),
    );
}

// ---------------------------------------------------------------------------
// Serialized IR: stable content hashes
// ---------------------------------------------------------------------------

#[test]
fn serialized_ir_hashes_are_stable() {
    let mut out = String::new();
    writeln!(out, "# ir-version {}", svg_ir::SVG_IR_VERSION).unwrap();
    writeln!(
        out,
        "# BLAKE3 of the serialized IR. Regenerate with UPDATE_GOLDEN=1 cargo test."
    )
    .unwrap();
    for fixture in all_compilable_fixtures() {
        let bytes = svg_ir::encode(&compile(&fixture)).unwrap();
        writeln!(
            out,
            "{}  {}  {} bytes",
            blake3::hash(&bytes).to_hex(),
            fixture,
            bytes.len()
        )
        .unwrap();
    }
    assert_golden("hashes.txt", &out);
}

// ---------------------------------------------------------------------------
// The full round trip
// ---------------------------------------------------------------------------

#[test]
fn every_fixture_survives_encode_then_decode() {
    for fixture in all_compilable_fixtures() {
        let document = compile(&fixture);
        let bytes =
            svg_ir::encode(&document).unwrap_or_else(|e| panic!("{fixture} failed to encode: {e}"));
        let decoded =
            svg_ir::decode(&bytes).unwrap_or_else(|e| panic!("{fixture} failed to decode: {e}"));
        assert_eq!(
            decoded, document,
            "{fixture} did not survive the round trip"
        );
    }
}

#[test]
fn encoding_the_same_source_twice_produces_identical_bytes() {
    for fixture in all_compilable_fixtures() {
        let a = svg_ir::encode(&compile(&fixture)).unwrap();
        let b = svg_ir::encode(&compile(&fixture)).unwrap();
        assert_eq!(a, b, "{fixture}");
    }
}

/// Formatting-only differences must not change a single byte, so a reformatted
/// SVG does not invalidate a build cache.
#[test]
fn insignificant_whitespace_does_not_change_the_output() {
    let compact = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 12 L20 12"/></svg>"#;
    let spaced = r#"
<svg
  xmlns="http://www.w3.org/2000/svg"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
>
  <path d="M4 12 L20 12" />
</svg>
"#;

    let options = CompileOptions::default();
    let a = svg_ir::encode(&svg_compiler::compile(compact, &options).unwrap().document).unwrap();
    let b = svg_ir::encode(&svg_compiler::compile(spaced, &options).unwrap().document).unwrap();
    assert_eq!(a, b);
}

/// The vertical slice from the specification, end to end in Rust. The
/// TypeScript equivalent lives in `tests/integration/`.
#[test]
fn lucide_search_vertical_slice() {
    let document = compile("lucide/search.svg");
    assert_eq!(document.view_box.width, 24.0);
    assert_eq!(document.view_box.height, 24.0);

    let bytes = svg_ir::encode(&document).unwrap();
    assert!(!bytes.is_empty());

    let decoded = svg_ir::decode(&bytes).unwrap();
    assert_eq!(decoded.shapes.len(), 2);

    // Meaningful content, not just "some bytes came out".
    let stroke = decoded.shapes[0].stroke.expect("expected a stroke");
    assert_eq!(stroke.paint, Paint::CurrentColor);
    assert_eq!(stroke.width, 2.0);
    assert_eq!(stroke.line_cap, svg_core::LineCap::Round);
    assert!(decoded.shapes[1].geometry.commands().len() > 4);
    assert!(decoded.features.is_tintable());
}
