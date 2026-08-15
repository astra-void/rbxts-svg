//! Errors raised when a semantic model invariant would be violated.
//!
//! These are *construction* errors: they are produced when someone tries to
//! build a document that the rest of the pipeline could not meaningfully
//! encode or render. Parsing and feature-support errors live in `svg-compiler`.

use core::fmt;

/// A violated invariant of the semantic model.
#[derive(Debug, Clone, PartialEq)]
pub enum CoreError {
    /// A view box must have a strictly positive, finite width and height.
    InvalidViewBox { width: f32, height: f32 },
    /// Every coordinate in the model must be finite; NaN/inf cannot be
    /// rasterized and cannot be encoded into the runtime IR.
    NonFiniteCoordinate { x: f32, y: f32 },
    /// A finite, non-negative scalar (stroke width, miter limit, ...) was
    /// required but something else was supplied.
    InvalidScalar { what: &'static str, value: f32 },
    /// Opacity is a normalized value in `0.0..=1.0`.
    InvalidOpacity { value: f32 },
    /// A path's command stream must open a subpath with `MoveTo` before any
    /// drawing command. See [`crate::path::Path`] for the full invariant list.
    PathMissingInitialMoveTo,
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidViewBox { width, height } => write!(
                f,
                "view box must have a positive finite size, got {width}x{height}"
            ),
            Self::NonFiniteCoordinate { x, y } => {
                write!(f, "coordinate ({x}, {y}) is not finite")
            }
            Self::InvalidScalar { what, value } => {
                write!(f, "{what} must be finite and non-negative, got {value}")
            }
            Self::InvalidOpacity { value } => {
                write!(f, "opacity must be within 0.0..=1.0, got {value}")
            }
            Self::PathMissingInitialMoveTo => {
                f.write_str("path command stream must begin a subpath with MoveTo")
            }
        }
    }
}

impl core::error::Error for CoreError {}
