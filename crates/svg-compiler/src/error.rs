//! Compilation failures.
//!
//! Malformed user SVG must never panic. Everything that can go wrong while
//! reading someone's file is represented here, and the `usvg`/`roxmltree`
//! errors are wrapped rather than stringified so callers can branch on them.

use std::fmt;

use svg_core::CoreError;

use crate::diagnostics::Diagnostic;

/// The source could not be read as XML/SVG at all.
#[derive(Debug)]
pub enum ParseError {
    /// The bytes are not valid UTF-8.
    NotUtf8,
    /// XML is malformed, or the document exceeds usvg's element limit.
    Xml(usvg::roxmltree::Error),
    /// usvg rejected the document.
    Usvg(usvg::Error),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotUtf8 => f.write_str("SVG source is not valid UTF-8"),
            Self::Xml(e) => write!(f, "malformed XML: {e}"),
            Self::Usvg(e) => write!(f, "{e}"),
        }
    }
}

impl core::error::Error for ParseError {}

/// Why a compile did not produce a document.
#[derive(Debug)]
pub enum CompileError {
    /// The input is not parseable SVG.
    Parse(ParseError),
    /// No usable coordinate system: the root has neither a `viewBox` nor a
    /// pair of absolute `width`/`height` attributes, or they are degenerate.
    InvalidViewBox { reason: String },
    /// The document uses features whose absence would change the rendering.
    /// Carries every offending diagnostic, not just the first, so one compile
    /// reports the full list.
    UnsupportedFeature { diagnostics: Vec<Diagnostic> },
    /// Geometry survived parsing but violates a semantic invariant, e.g. a
    /// coordinate that is not finite after transform baking.
    InvalidGeometry(CoreError),
}

impl CompileError {
    /// Renders the error the way a build tool should show it, attributing it to
    /// `source_name` when one is known.
    pub fn render(&self, source_name: Option<&str>) -> String {
        match self {
            Self::UnsupportedFeature { diagnostics } => {
                let header = match source_name {
                    Some(name) => format!("Unsupported SVG feature in {name}:"),
                    None => "Unsupported SVG feature:".to_string(),
                };
                let body = diagnostics
                    .iter()
                    .map(|d| d.render(source_name))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                format!("{header}\n\n{body}")
            }
            other => match source_name {
                Some(name) => format!("{other} ({name})"),
                None => other.to_string(),
            },
        }
    }

    /// The diagnostics that caused this failure, if any.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        match self {
            Self::UnsupportedFeature { diagnostics } => diagnostics,
            _ => &[],
        }
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "failed to parse SVG: {e}"),
            Self::InvalidViewBox { reason } => {
                write!(f, "cannot determine the SVG coordinate system: {reason}")
            }
            Self::UnsupportedFeature { diagnostics } => {
                write!(f, "SVG uses {} unsupported feature(s)", diagnostics.len())
            }
            Self::InvalidGeometry(e) => write!(f, "invalid geometry: {e}"),
        }
    }
}

impl core::error::Error for CompileError {}

impl From<CoreError> for CompileError {
    fn from(e: CoreError) -> Self {
        Self::InvalidGeometry(e)
    }
}

impl From<ParseError> for CompileError {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}
