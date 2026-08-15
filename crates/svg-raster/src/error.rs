//! Structured failures.
//!
//! A rasterizer fed compiled IR is fed *user-controlled* geometry, arriving
//! from whatever `.svg` someone dropped into their project. Every failure it
//! can have is therefore a normal outcome to report, not a bug to panic over:
//! nothing in this crate panics on input, and the fuzz-shaped tests in
//! `tests/robustness.rs` are what keeps that true.

use core::fmt;

/// The maximum width or height of a raster, in pixels.
///
/// A limit exists so that a bad `width` cannot ask for an allocation measured
/// in gigabytes. It is generous — far beyond any icon, and beyond what
/// `EditableImage` accepts — because its job is to catch nonsense, not to
/// second-guess a legitimate request.
pub const MAX_DIMENSION: u32 = 8192;

/// Why an asset could not be rasterized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RasterError {
    /// A raster dimension was zero, or larger than [`MAX_DIMENSION`].
    InvalidDimensions { width: u32, height: u32 },
    /// A coordinate reached the rasterizer as NaN or infinity.
    ///
    /// The compiler rejects non-finite coordinates, and the IR decoder
    /// re-checks them, so this means the *transform* produced one — an extreme
    /// scale overflowing `f32`, for instance. Reported rather than clamped,
    /// because a silently relocated coordinate is a silently wrong picture.
    NonFiniteGeometry,
}

impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions { width, height } => write!(
                f,
                "raster size {width}x{height} is not usable: both dimensions must be \
                 between 1 and {MAX_DIMENSION}"
            ),
            Self::NonFiniteGeometry => f.write_str(
                "geometry became non-finite while mapping the view box onto the target size; \
                 the requested scale is too extreme for this asset",
            ),
        }
    }
}

impl core::error::Error for RasterError {}
