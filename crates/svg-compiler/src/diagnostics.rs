//! Structured diagnostics.
//!
//! The compile-time pipeline exists partly so that unsupported SVG constructs
//! are caught *before* anything reaches Roblox. That is only worth anything if
//! the report tells the author which element in which file is the problem, so
//! diagnostics carry a source location rather than just a message string.

use std::fmt;

/// How much a diagnostic matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Rendering would differ from the source. Fails the compile unless
    /// [`crate::UnsupportedPolicy::Warn`] is selected.
    Error,
    /// Output is produced, but it is an approximation of the source.
    Warning,
    /// Something was ignored, and ignoring it does not change the rendering.
    Info,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

/// A stable, machine-readable classification.
///
/// Tooling matches on these; the human-readable `message` may be reworded
/// freely, but a code is part of the compiler's contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    /// An element whose rendering is not implemented.
    UnsupportedElement,
    /// A paint kind that is not implemented (gradient, pattern).
    UnsupportedPaint,
    /// A supported element carrying an unsupported attribute.
    UnsupportedAttribute,
    /// `stroke-dasharray` is present.
    UnsupportedStrokeDash,
    /// A non-`normal` blend mode is present.
    UnsupportedBlendMode,
    /// Group opacity was folded into its children's paints, which differs from
    /// true group isolation where children overlap.
    ApproximatedGroupOpacity,
    /// A shape was baked through a non-uniform or skewed transform, so its
    /// stroke width could only be approximated.
    ApproximatedStrokeScale,
    /// A definition exists but nothing references it, so it was ignored.
    UnreferencedDefinition,
    /// Non-rendering content (editor metadata, unknown elements) was ignored.
    IgnoredMetadata,
    /// A shape contributed nothing to the output and was dropped.
    DroppedEmptyShape,
}

impl DiagnosticCode {
    /// Kebab-case identifier, suitable for logs and lint-style suppression.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedElement => "unsupported-element",
            Self::UnsupportedPaint => "unsupported-paint",
            Self::UnsupportedAttribute => "unsupported-attribute",
            Self::UnsupportedStrokeDash => "unsupported-stroke-dash",
            Self::UnsupportedBlendMode => "unsupported-blend-mode",
            Self::ApproximatedGroupOpacity => "approximated-group-opacity",
            Self::ApproximatedStrokeScale => "approximated-stroke-scale",
            Self::UnreferencedDefinition => "unreferenced-definition",
            Self::IgnoredMetadata => "ignored-metadata",
            Self::DroppedEmptyShape => "dropped-empty-shape",
        }
    }
}

/// Stable keys naming the SVG feature a diagnostic is about.
///
/// The source scan and the normalizer deliberately overlap — the scan has
/// precise source positions, the normalizer catches content that reaches the
/// tree indirectly (through `<use>`, say). Tagging both with the same feature
/// key is what lets one problem be reported once, keeping whichever report
/// carries a source location.
pub mod feature {
    pub const FILTER: &str = "filter";
    pub const MASK: &str = "mask";
    pub const CLIP_PATH: &str = "clip-path";
    pub const PATTERN: &str = "pattern";
    pub const GRADIENT: &str = "gradient";
    pub const TEXT: &str = "text";
    pub const IMAGE: &str = "image";
    pub const FOREIGN_OBJECT: &str = "foreign-object";
    pub const MARKER: &str = "marker";
    pub const ANIMATION: &str = "animation";
    pub const STROKE_DASH: &str = "stroke-dasharray";
    pub const BLEND_MODE: &str = "blend-mode";
}

/// Where in the source document a diagnostic came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementRef {
    /// Tag name as written, e.g. `clipPath`.
    pub tag: String,
    /// The element's `id`, when it has one.
    pub id: Option<String>,
    /// Ancestor chain, e.g. `svg > defs > filter#shadow`.
    pub path: String,
    /// 1-based line in the source.
    pub line: Option<u32>,
    /// 1-based column in the source.
    pub column: Option<u32>,
}

impl ElementRef {
    /// `<tag id="...">`, reconstructed for display.
    pub fn opening_tag(&self) -> String {
        match &self.id {
            Some(id) => format!("<{} id=\"{}\">", self.tag, id),
            None => format!("<{}>", self.tag),
        }
    }
}

/// A single compiler finding.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
    pub element: Option<ElementRef>,
    /// Which SVG feature this is about; see [`feature`]. Used to recognise two
    /// reports of the same underlying problem.
    pub feature: Option<&'static str>,
}

impl Diagnostic {
    pub fn new(severity: Severity, code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            severity,
            code,
            message: message.into(),
            element: None,
            feature: None,
        }
    }

    pub fn error(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self::new(Severity::Error, code, message)
    }

    pub fn warning(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, code, message)
    }

    pub fn info(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self::new(Severity::Info, code, message)
    }

    pub fn at(mut self, element: ElementRef) -> Self {
        self.element = Some(element);
        self
    }

    pub fn about(mut self, feature: &'static str) -> Self {
        self.feature = Some(feature);
        self
    }

    /// Renders the multi-line developer-facing form:
    ///
    /// ```text
    /// error: <filter> is not supported by @rbxts/svg yet.
    ///   --> assets/logo.svg:4:5
    ///
    /// Element:
    ///   <filter id="shadow">
    ///
    /// Path:
    ///   svg > defs > filter#shadow
    /// ```
    pub fn render(&self, source_name: Option<&str>) -> String {
        let mut out = format!("{}: {}", self.severity.as_str(), self.message);

        let Some(element) = &self.element else {
            if let Some(name) = source_name {
                out.push_str(&format!("\n  --> {name}"));
            }
            return out;
        };

        let location = match (source_name, element.line, element.column) {
            (Some(name), Some(line), Some(col)) => Some(format!("{name}:{line}:{col}")),
            (Some(name), Some(line), None) => Some(format!("{name}:{line}")),
            (Some(name), None, _) => Some(name.to_string()),
            (None, Some(line), Some(col)) => Some(format!("{line}:{col}")),
            (None, Some(line), None) => Some(format!("{line}")),
            (None, None, _) => None,
        };
        if let Some(location) = location {
            out.push_str(&format!("\n  --> {location}"));
        }

        out.push_str(&format!(
            "\n\nElement:\n  {}\n\nPath:\n  {}",
            element.opening_tag(),
            element.path
        ));
        out
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render(None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element() -> ElementRef {
        ElementRef {
            tag: "filter".into(),
            id: Some("shadow".into()),
            path: "svg > defs > filter#shadow".into(),
            line: Some(4),
            column: Some(5),
        }
    }

    #[test]
    fn rendered_diagnostic_names_the_file_element_and_path() {
        let d = Diagnostic::error(
            DiagnosticCode::UnsupportedElement,
            "<filter> is not supported by @rbxts/svg yet.",
        )
        .at(element());

        let text = d.render(Some("assets/logo.svg"));
        assert!(text.contains("assets/logo.svg:4:5"), "{text}");
        assert!(text.contains("<filter id=\"shadow\">"), "{text}");
        assert!(text.contains("svg > defs > filter#shadow"), "{text}");
    }

    #[test]
    fn diagnostic_without_element_still_renders() {
        let d = Diagnostic::warning(DiagnosticCode::IgnoredMetadata, "something");
        assert_eq!(d.render(None), "warning: something");
        assert_eq!(d.render(Some("a.svg")), "warning: something\n  --> a.svg");
    }

    #[test]
    fn codes_are_kebab_case_and_distinct() {
        let codes = [
            DiagnosticCode::UnsupportedElement,
            DiagnosticCode::UnsupportedPaint,
            DiagnosticCode::UnsupportedAttribute,
            DiagnosticCode::UnsupportedStrokeDash,
            DiagnosticCode::UnsupportedBlendMode,
            DiagnosticCode::ApproximatedGroupOpacity,
            DiagnosticCode::ApproximatedStrokeScale,
            DiagnosticCode::UnreferencedDefinition,
            DiagnosticCode::IgnoredMetadata,
            DiagnosticCode::DroppedEmptyShape,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for c in codes {
            assert!(seen.insert(c.as_str()), "duplicate code {}", c.as_str());
            assert!(
                c.as_str()
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch == '-')
            );
        }
    }

    #[test]
    fn severity_orders_error_first() {
        assert!(Severity::Error < Severity::Warning);
        assert!(Severity::Warning < Severity::Info);
    }
}
