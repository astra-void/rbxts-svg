//! Compiles SVG source into the framework-neutral semantic model.
//!
//! ```text
//! SVG bytes ──▶ roxmltree ──┬──▶ source scan  (diagnostics, currentColor)
//!                           │
//!                           └──▶ usvg  ──▶ normalize ──▶ optimize ──▶ SvgDocument
//! ```
//!
//! The XML is parsed once and shared: the scan needs source positions that usvg
//! discards, and usvg needs the tree that the scan already built.
//!
//! # Determinism
//!
//! The same source, compiler version and options produce the same document on
//! every machine. Nothing here reads the filesystem, the clock, the environment
//! or a random source, no ordering depends on hashing, and system font
//! enumeration is disabled at the dependency level.
//!
//! # Robustness
//!
//! Malformed input is a normal outcome, not a bug. Nothing in this crate calls
//! `unwrap()` on user-controlled data; every failure path produces a
//! [`CompileError`].

#![forbid(unsafe_code)]

pub mod diagnostics;
pub mod error;
pub mod lower;
pub mod normalize;
pub mod optimize;
pub mod parse;
pub mod viewbox;

pub use diagnostics::{Diagnostic, DiagnosticCode, ElementRef, Severity};
pub use error::{CompileError, ParseError};

use svg_core::SvgDocument;

/// How to treat constructs whose absence would change the rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnsupportedPolicy {
    /// Fail the compile. The default, and the reason a build-time pipeline is
    /// worth having: an unsupported construct is caught before it becomes a
    /// silently wrong picture inside Roblox.
    #[default]
    Error,
    /// Downgrade to a warning and compile what is left. An explicit opt-in for
    /// "I know this file has a gradient and I accept losing it".
    Warn,
}

/// Options for a single compile.
#[derive(Debug, Clone)]
pub struct CompileOptions {
    /// Dots per inch, used only to resolve physical units (`mm`, `pt`, ...).
    /// Icons never use them; the field exists because usvg requires a value.
    pub dpi: f32,
    /// What to do about unsupported rendering features.
    pub unsupported: UnsupportedPolicy,
    /// A name for the source, used to attribute diagnostics. Never affects the
    /// compiled output — a file compiled under two different names produces
    /// identical bytes.
    pub source_name: Option<String>,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            dpi: 96.0,
            unsupported: UnsupportedPolicy::default(),
            source_name: None,
        }
    }
}

/// A successful compile.
#[derive(Debug)]
pub struct CompileOutput {
    pub document: SvgDocument,
    /// Non-fatal findings: approximations, ignored metadata, dropped shapes.
    /// Under [`UnsupportedPolicy::Warn`] this also carries the downgraded
    /// unsupported-feature reports.
    pub diagnostics: Vec<Diagnostic>,
}

/// Compiles SVG bytes.
///
/// Accepts UTF-8 SVG. Gzip-compressed `.svgz` is rejected here rather than
/// silently accepted, because the source scan needs the decompressed text.
pub fn compile_bytes(
    source: &[u8],
    options: &CompileOptions,
) -> Result<CompileOutput, CompileError> {
    let text = std::str::from_utf8(source).map_err(|_| CompileError::Parse(ParseError::NotUtf8))?;
    compile(text, options)
}

/// Compiles SVG source text.
pub fn compile(source: &str, options: &CompileOptions) -> Result<CompileOutput, CompileError> {
    let xml = parse::parse_xml(source)?;
    let scan = parse::scan(&xml, source);

    // Resolve the coordinate system before handing anything to usvg: without a
    // view box there is nothing to compile into, and failing here gives a much
    // better message than usvg's generic `InvalidSize`.
    let resolved = viewbox::resolve(xml.root_element())?;

    let usvg_options = parse::usvg_options(options.dpi, scan.current_color_sentinel_active);
    let tree = usvg::Tree::from_xmltree(&xml, &usvg_options)
        .map_err(|e| CompileError::Parse(ParseError::Usvg(e)))?;

    // usvg folded the view-box-to-viewport mapping into the tree. Undo it, so
    // the compiled geometry is in view box space and stays resolution
    // independent. See `viewbox` for why this has to be reconstructed.
    let size = (tree.size().width(), tree.size().height());
    let to_view_box = viewbox::to_view_box_space(resolved.view_box, resolved.aspect, size)
        .ok_or_else(|| CompileError::InvalidViewBox {
            reason: format!(
                "viewBox {}x{} cannot be mapped onto the resolved size {}x{}",
                resolved.view_box.width, resolved.view_box.height, size.0, size.1
            ),
        })?;

    let normalized =
        normalize::Normalizer::new(to_view_box, scan.current_color_sentinel_active).run(&tree);

    let mut diagnostics = scan.diagnostics;
    diagnostics.extend(normalized.diagnostics);

    // The source scan and the tree walk deliberately overlap so that nothing is
    // missed, which means one problem can be reported twice. Collapse those:
    // the scan's report carries a source location and is strictly more useful,
    // so a location-less report about a feature already covered is dropped.
    let located_features: Vec<&'static str> = diagnostics
        .iter()
        .filter(|d| d.element.is_some())
        .filter_map(|d| d.feature)
        .collect();
    diagnostics.retain(|d| {
        d.element.is_some()
            || match d.feature {
                Some(feature) => !located_features.contains(&feature),
                None => true,
            }
    });
    diagnostics.dedup_by(|a, b| a.code == b.code && a.message == b.message);

    if options.unsupported == UnsupportedPolicy::Error {
        let errors: Vec<Diagnostic> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .cloned()
            .collect();
        if !errors.is_empty() {
            return Err(CompileError::UnsupportedFeature {
                diagnostics: errors,
            });
        }
    } else {
        for diagnostic in &mut diagnostics {
            if diagnostic.severity == Severity::Error {
                diagnostic.severity = Severity::Warning;
            }
        }
    }

    let mut shapes = normalized.shapes;
    optimize::drop_invisible_shapes(&mut shapes, &mut diagnostics);
    let features = optimize::detect_features(&shapes);

    // Cheap end-of-pipeline check: a transform with an extreme scale can push a
    // coordinate to infinity, and that must not reach the encoder.
    for shape in &shapes {
        shape.geometry.validate()?;
    }

    Ok(CompileOutput {
        // The authored `preserveAspectRatio` travels with the document: it is
        // the only thing that tells a renderer how to fill a target rectangle
        // whose shape differs from the view box's.
        document: SvgDocument::new(resolved.view_box, shapes, features)
            .with_preserve_aspect_ratio(resolved.aspect),
        diagnostics,
    })
}
