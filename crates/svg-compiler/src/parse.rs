//! Source-level parsing: XML, the `currentColor` sentinel, and the scan that
//! finds unsupported constructs while their source positions are still known.
//!
//! usvg is the SVG parser — we never touch the XML grammar ourselves. But usvg
//! *normalizes away* the two things we most need for good behaviour:
//!
//! - it resolves `currentColor` to a concrete colour, losing the fact that the
//!   colour was meant to be supplied by the consumer;
//! - it drops unsupported content silently, along with its source position.
//!
//! Both are recovered here, from the same `roxmltree::Document` that is then
//! handed to usvg. The document is parsed exactly once.

use std::collections::BTreeSet;

use usvg::roxmltree;

use crate::diagnostics::{Diagnostic, DiagnosticCode, ElementRef, Severity, feature};
use crate::error::ParseError;

/// The colour `currentColor` is temporarily resolved to so we can recognise it
/// again on the other side of usvg.
///
/// # Why a sentinel
///
/// usvg resolves `currentColor` against the inherited `color` property and
/// hands back a plain `Paint::Color`, with no record that it was ever
/// `currentColor`. There is no hook to intercept that. So before parsing we
/// inject the stylesheet `svg{color:#...}` (usvg's `Options::style_sheet`), and
/// afterwards any paint equal to this colour must have come from a
/// `currentColor`.
///
/// # When this is wrong
///
/// A document that *both* mentions `currentColor` *and* separately paints
/// something with this exact colour would have that paint misread as tintable.
/// The value is chosen to make that essentially impossible in practice, and the
/// substitution is skipped entirely for documents that never say
/// `currentColor` — so a document that cannot benefit also cannot be harmed.
pub const CURRENT_COLOR_SENTINEL: usvg::Color = usvg::Color {
    red: 0x7B,
    green: 0x2D,
    blue: 0xF1,
};

const SENTINEL_STYLE_SHEET: &str = "svg{color:#7B2DF1}";

/// What the source-level scan learned before usvg normalized anything away.
#[derive(Debug)]
pub struct SourceScan {
    /// Whether the `currentColor` sentinel was injected, and therefore whether
    /// sentinel-coloured paints should be read back as `currentColor`.
    pub current_color_sentinel_active: bool,
    /// Findings about elements we cannot render.
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses the source as XML.
pub fn parse_xml(source: &str) -> Result<roxmltree::Document<'_>, ParseError> {
    // `allow_dtd` matches what usvg itself uses, so a document that parses here
    // also parses there.
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    roxmltree::Document::parse_with_options(source, options).map_err(ParseError::Xml)
}

/// Builds the usvg options for a compile, injecting the sentinel stylesheet
/// when the document can benefit from it.
pub fn usvg_options(dpi: f32, sentinel_active: bool) -> usvg::Options<'static> {
    let mut options = usvg::Options {
        dpi,
        ..Default::default()
    };
    if sentinel_active {
        options.style_sheet = Some(SENTINEL_STYLE_SHEET.to_string());
    }
    options
}

/// Elements whose rendering `@rbxts/svg` does not implement: the tag, the
/// human-readable feature name used in the message, and the stable feature key
/// used to deduplicate against the normalizer's reports.
const UNSUPPORTED_ELEMENTS: &[(&str, &str, &str)] = &[
    ("filter", "filter effects", feature::FILTER),
    ("mask", "masks", feature::MASK),
    ("clipPath", "clipping paths", feature::CLIP_PATH),
    ("pattern", "pattern fills", feature::PATTERN),
    ("linearGradient", "linear gradients", feature::GRADIENT),
    ("radialGradient", "radial gradients", feature::GRADIENT),
    ("text", "text", feature::TEXT),
    ("image", "embedded raster images", feature::IMAGE),
    ("foreignObject", "foreign objects", feature::FOREIGN_OBJECT),
    ("marker", "markers", feature::MARKER),
    ("animate", "animation", feature::ANIMATION),
    ("animateMotion", "animation", feature::ANIMATION),
    ("animateTransform", "animation", feature::ANIMATION),
    ("set", "animation", feature::ANIMATION),
];

/// Elements that are silently accepted and whose *contents* are not SVG
/// rendering content.
///
/// The scan does not descend into these. `<desc>` and `<metadata>` may legally
/// contain arbitrary XML — including an XHTML `<text>` element — and rejecting
/// a file over a description would be plainly wrong. `<defs>` is deliberately
/// absent: its contents do matter, and are handled by the reference check.
const OPAQUE_ELEMENTS: &[&str] = &["metadata", "title", "desc", "style", "script"];

/// Elements that only *define* content; they render solely through a reference.
const DEFINITION_ELEMENTS: &[&str] = &[
    "filter",
    "mask",
    "clipPath",
    "pattern",
    "linearGradient",
    "radialGradient",
    "marker",
    "symbol",
];

/// Walks the source document and reports what cannot be compiled.
pub fn scan(document: &roxmltree::Document<'_>, source: &str) -> SourceScan {
    let referenced = collect_referenced_ids(document);
    let mut diagnostics = Vec::new();
    scan_node(
        document.root_element(),
        document,
        &referenced,
        &mut diagnostics,
    );

    SourceScan {
        current_color_sentinel_active: should_inject_sentinel(document, source),
        diagnostics,
    }
}

/// The sentinel is injected only when it can help and cannot mislead.
fn should_inject_sentinel(document: &roxmltree::Document<'_>, source: &str) -> bool {
    if !mentions_current_color(source) {
        return false;
    }
    // If the root already sets `color`, our injected stylesheet would override
    // an intentional author decision (CSS beats presentation attributes), and
    // we would report a genuinely fixed colour as tintable. Stand down.
    let root = document.root_element();
    if root.attribute("color").is_some() {
        return false;
    }
    if let Some(style) = root.attribute("style")
        && style.to_ascii_lowercase().contains("color")
    {
        return false;
    }
    true
}

/// Case-insensitive search for the `currentColor` keyword.
///
/// A false positive (the word inside a comment) only means we inject a
/// stylesheet that changes nothing, so this deliberately does not try to be
/// clever about where the token appears.
fn mentions_current_color(source: &str) -> bool {
    const NEEDLE: &[u8] = b"currentcolor";
    source
        .as_bytes()
        .windows(NEEDLE.len())
        .any(|w| w.eq_ignore_ascii_case(NEEDLE))
}

/// All ids reachable through `url(#id)`, `href="#id"` or `xlink:href="#id"`.
fn collect_referenced_ids(document: &roxmltree::Document<'_>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for node in document.descendants().filter(|n| n.is_element()) {
        for attribute in node.attributes() {
            collect_ids_from_value(attribute.value(), &mut out);
        }
    }
    out
}

fn collect_ids_from_value(value: &str, out: &mut BTreeSet<String>) {
    // `url(#name)`, possibly several per value (e.g. a fallback list).
    let mut rest = value;
    while let Some(start) = rest.find("url(") {
        rest = &rest[start + 4..];
        let Some(end) = rest.find(')') else { break };
        let inner = rest[..end].trim().trim_matches(['\'', '"']);
        if let Some(id) = inner.strip_prefix('#') {
            out.insert(id.trim().to_string());
        }
        rest = &rest[end + 1..];
    }

    // A bare local reference, as used by `href`/`xlink:href`.
    if let Some(id) = value.trim().strip_prefix('#')
        && !id.is_empty()
    {
        out.insert(id.to_string());
    }
}

fn scan_node(
    node: roxmltree::Node<'_, '_>,
    document: &roxmltree::Document<'_>,
    referenced: &BTreeSet<String>,
    out: &mut Vec<Diagnostic>,
) {
    for child in node.children().filter(|c| c.is_element()) {
        let tag = child.tag_name().name();

        if OPAQUE_ELEMENTS.contains(&tag) {
            continue;
        }

        if let Some((_, label, key)) = UNSUPPORTED_ELEMENTS.iter().find(|(t, _, _)| *t == tag) {
            let live = is_live(child, referenced);
            let diagnostic = if live {
                Diagnostic::error(
                    DiagnosticCode::UnsupportedElement,
                    format!("<{tag}> is not supported by @rbxts/svg yet ({label})."),
                )
                .about(key)
            } else {
                Diagnostic::info(
                    DiagnosticCode::UnreferencedDefinition,
                    format!("<{tag}> is defined but never referenced, so it was ignored."),
                )
                .about(key)
            };
            out.push(diagnostic.at(element_ref(child, document)));
            // Do not descend: reporting `<tspan>` under an already-reported
            // `<text>` adds noise, not information.
            continue;
        }

        if is_foreign_namespace(child) {
            out.push(
                Diagnostic::info(
                    DiagnosticCode::IgnoredMetadata,
                    format!(
                        "<{}> is not in the SVG namespace and does not affect rendering; ignored.",
                        child.tag_name().name()
                    ),
                )
                .at(element_ref(child, document)),
            );
            continue;
        }

        scan_node(child, document, referenced, out);
    }
}

/// True when an element can affect the rendered output.
///
/// Content inside `<defs>`, and definition elements generally, only render via
/// a reference. If nothing points at them they are dead weight, and flagging
/// them as errors would reject perfectly renderable files.
fn is_live(node: roxmltree::Node<'_, '_>, referenced: &BTreeSet<String>) -> bool {
    let tag = node.tag_name().name();
    let in_defs = node
        .ancestors()
        .any(|a| a.is_element() && a.tag_name().name() == "defs");

    if in_defs || DEFINITION_ELEMENTS.contains(&tag) {
        return node
            .attribute("id")
            .is_some_and(|id| referenced.contains(id));
    }
    true
}

fn is_foreign_namespace(node: roxmltree::Node<'_, '_>) -> bool {
    const SVG_NS: &str = "http://www.w3.org/2000/svg";
    match node.tag_name().namespace() {
        Some(ns) => ns != SVG_NS,
        // No namespace at all: treat as SVG, matching how browsers and usvg
        // handle namespace-less documents.
        None => false,
    }
}

/// Captures where an element sits, for diagnostics.
pub fn element_ref(
    node: roxmltree::Node<'_, '_>,
    document: &roxmltree::Document<'_>,
) -> ElementRef {
    let position = document.text_pos_at(node.range().start);
    ElementRef {
        tag: node.tag_name().name().to_string(),
        id: node.attribute("id").map(str::to_string),
        path: element_path(node),
        line: Some(position.row),
        column: Some(position.col),
    }
}

/// `svg > defs > filter#shadow`
fn element_path(node: roxmltree::Node<'_, '_>) -> String {
    let mut segments: Vec<String> = node
        .ancestors()
        .filter(|n| n.is_element())
        .map(|n| match n.attribute("id") {
            Some(id) => format!("{}#{}", n.tag_name().name(), id),
            None => n.tag_name().name().to_string(),
        })
        .collect();
    // `ancestors()` yields the node itself first, then upwards.
    segments.reverse();
    segments.join(" > ")
}

/// Convenience: does this scan contain anything that must fail the compile?
pub fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|d| d.severity == Severity::Error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_source(source: &str) -> SourceScan {
        let document = parse_xml(source).unwrap();
        scan(&document, source)
    }

    #[test]
    fn plain_icon_produces_no_diagnostics() {
        let scan = scan_source(
            r#"<svg viewBox="0 0 24 24"><path d="M0 0 L1 1"/><circle cx="1" cy="1" r="1"/></svg>"#,
        );
        assert!(scan.diagnostics.is_empty(), "{:?}", scan.diagnostics);
    }

    #[test]
    fn referenced_filter_is_an_error_with_a_source_location() {
        let source = r#"<svg viewBox="0 0 24 24">
  <defs>
    <filter id="shadow"><feGaussianBlur stdDeviation="2"/></filter>
  </defs>
  <rect width="10" height="10" filter="url(#shadow)"/>
</svg>"#;
        let scan = scan_source(source);
        let d = scan
            .diagnostics
            .iter()
            .find(|d| d.code == DiagnosticCode::UnsupportedElement)
            .expect("expected an unsupported-element diagnostic");

        assert_eq!(d.severity, Severity::Error);
        let element = d.element.as_ref().unwrap();
        assert_eq!(element.tag, "filter");
        assert_eq!(element.id.as_deref(), Some("shadow"));
        assert_eq!(element.path, "svg > defs > filter#shadow");
        assert_eq!(element.line, Some(3));
    }

    #[test]
    fn unreferenced_definition_is_only_informational() {
        let source = r#"<svg viewBox="0 0 24 24">
  <defs><linearGradient id="unused"><stop offset="0"/></linearGradient></defs>
  <path d="M0 0 L1 1"/>
</svg>"#;
        let scan = scan_source(source);
        assert!(!has_errors(&scan.diagnostics), "{:?}", scan.diagnostics);
        assert!(
            scan.diagnostics
                .iter()
                .any(|d| d.code == DiagnosticCode::UnreferencedDefinition)
        );
    }

    #[test]
    fn referenced_gradient_is_an_error() {
        let source = r#"<svg viewBox="0 0 24 24">
  <defs><linearGradient id="g"><stop offset="0"/></linearGradient></defs>
  <rect width="10" height="10" fill="url(#g)"/>
</svg>"#;
        assert!(has_errors(&scan_source(source).diagnostics));
    }

    #[test]
    fn text_is_reported_once_not_per_tspan() {
        let source = r#"<svg viewBox="0 0 24 24"><text x="0" y="0"><tspan>a</tspan><tspan>b</tspan></text></svg>"#;
        let scan = scan_source(source);
        let errors: Vec<_> = scan
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].element.as_ref().unwrap().tag, "text");
    }

    #[test]
    fn editor_namespaced_elements_are_ignored_not_rejected() {
        let source = r#"<svg xmlns="http://www.w3.org/2000/svg"
     xmlns:sodipodi="http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd"
     viewBox="0 0 24 24">
  <sodipodi:namedview id="nv"/>
  <path d="M0 0 L1 1"/>
</svg>"#;
        let scan = scan_source(source);
        assert!(!has_errors(&scan.diagnostics), "{:?}", scan.diagnostics);
        assert!(
            scan.diagnostics
                .iter()
                .any(|d| d.code == DiagnosticCode::IgnoredMetadata)
        );
    }

    #[test]
    fn title_and_desc_are_accepted_silently() {
        let source = r#"<svg viewBox="0 0 24 24"><title>Icon</title><desc>An icon</desc><path d="M0 0 L1 1"/></svg>"#;
        assert!(scan_source(source).diagnostics.is_empty());
    }

    /// `<desc>` may contain arbitrary XML. A `<text>` element in a description
    /// is prose, not something to render, and must not fail the compile.
    #[test]
    fn opaque_element_contents_are_not_scanned() {
        let source = r#"<svg viewBox="0 0 24 24">
  <desc><text xmlns="http://www.w3.org/1999/xhtml">a description</text></desc>
  <path d="M0 0 L1 1"/>
</svg>"#;
        assert!(scan_source(source).diagnostics.is_empty());
    }

    #[test]
    fn sentinel_activates_only_when_current_color_is_used() {
        assert!(
            !scan_source(r#"<svg viewBox="0 0 1 1"><path fill="red" d="M0 0"/></svg>"#)
                .current_color_sentinel_active
        );
        assert!(
            scan_source(r#"<svg viewBox="0 0 1 1"><path fill="currentColor" d="M0 0"/></svg>"#)
                .current_color_sentinel_active
        );
    }

    #[test]
    fn sentinel_is_case_insensitive() {
        assert!(mentions_current_color("stroke:CURRENTCOLOR"));
        assert!(mentions_current_color("stroke=\"currentcolor\""));
        assert!(!mentions_current_color("stroke=\"current-color\""));
    }

    #[test]
    fn sentinel_stands_down_when_the_root_pins_color() {
        let source =
            r#"<svg viewBox="0 0 1 1" color="red"><path fill="currentColor" d="M0 0"/></svg>"#;
        assert!(!scan_source(source).current_color_sentinel_active);

        let styled = r#"<svg viewBox="0 0 1 1" style="color:red"><path fill="currentColor" d="M0 0"/></svg>"#;
        assert!(!scan_source(styled).current_color_sentinel_active);
    }

    #[test]
    fn reference_collection_understands_url_and_href_forms() {
        let mut ids = BTreeSet::new();
        collect_ids_from_value("url(#a)", &mut ids);
        collect_ids_from_value("url('#b') url(\"#c\")", &mut ids);
        collect_ids_from_value("#d", &mut ids);
        collect_ids_from_value("none", &mut ids);
        assert_eq!(
            ids.into_iter().collect::<Vec<_>>(),
            vec!["a", "b", "c", "d"]
        );
    }

    #[test]
    fn malformed_xml_is_an_error_not_a_panic() {
        assert!(parse_xml("<svg><path></svg>").is_err());
        assert!(parse_xml("").is_err());
        assert!(parse_xml("not xml at all").is_err());
    }
}
