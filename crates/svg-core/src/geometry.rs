//! Basic geometric primitives.

use crate::error::CoreError;

/// A point in user space.
///
/// Which user space depends on where the point appears: inside a
/// [`crate::document::SvgDocument`] every coordinate is expressed in *view box
/// space* (see [`ViewBox`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Returns an error unless both components are finite.
    pub fn validate(self) -> Result<Self, CoreError> {
        if self.x.is_finite() && self.y.is_finite() {
            Ok(self)
        } else {
            Err(CoreError::NonFiniteCoordinate {
                x: self.x,
                y: self.y,
            })
        }
    }
}

/// The coordinate system a document's geometry is expressed in.
///
/// This is the SVG `viewBox` attribute, and it is the *only* sizing information
/// the compiled asset carries. The source `width`/`height` attributes are
/// deliberately dropped: they describe a default presentation size, and
/// consumers always supply their own target size at render time.
///
/// Invariant: `width > 0`, `height > 0`, all fields finite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ViewBox {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Result<Self, CoreError> {
        if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
            return Err(CoreError::InvalidViewBox { width, height });
        }
        if !(x.is_finite() && y.is_finite()) {
            return Err(CoreError::NonFiniteCoordinate { x, y });
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }

    /// Aspect ratio (`width / height`). Always finite and positive.
    #[inline]
    pub fn aspect_ratio(&self) -> f32 {
        self.width / self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_box_rejects_degenerate_sizes() {
        assert!(ViewBox::new(0.0, 0.0, 0.0, 24.0).is_err());
        assert!(ViewBox::new(0.0, 0.0, 24.0, -1.0).is_err());
        assert!(ViewBox::new(0.0, 0.0, f32::NAN, 24.0).is_err());
        assert!(ViewBox::new(0.0, 0.0, 24.0, 24.0).is_ok());
    }

    #[test]
    fn view_box_rejects_non_finite_origin() {
        assert!(ViewBox::new(f32::INFINITY, 0.0, 24.0, 24.0).is_err());
    }

    #[test]
    fn point_validation() {
        assert!(Point::new(1.0, 2.0).validate().is_ok());
        assert!(Point::new(f32::NAN, 2.0).validate().is_err());
    }
}
