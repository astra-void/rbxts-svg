//! The paint model: how geometry is coloured.
//!
//! Paint is modelled separately from geometry so that a shape's outline and its
//! appearance can evolve independently. In particular [`Paint`] is an enum with
//! room for gradients, which are not implemented yet but must remain possible
//! without reshaping anything around them.

use crate::error::CoreError;
use crate::path::FillRule;

/// An opaque sRGB colour. Alpha lives in [`Opacity`], mirroring SVG, where
/// `fill` carries the colour and `fill-opacity` carries the alpha. An
/// eight-digit `#RRGGBBAA` colour is split into both on the way in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    #[inline]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// A normalized opacity in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Opacity(f32);

impl Opacity {
    pub const OPAQUE: Self = Self(1.0);
    pub const TRANSPARENT: Self = Self(0.0);

    pub fn new(value: f32) -> Result<Self, CoreError> {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(CoreError::InvalidOpacity { value })
        }
    }

    /// Clamps instead of failing. Used where SVG itself specifies clamping.
    pub fn clamped(value: f32) -> Self {
        if value.is_nan() {
            Self::OPAQUE
        } else {
            Self(value.clamp(0.0, 1.0))
        }
    }

    #[inline]
    pub fn get(self) -> f32 {
        self.0
    }

    #[inline]
    pub fn is_opaque(self) -> bool {
        self.0 >= 1.0
    }

    #[inline]
    pub fn is_fully_transparent(self) -> bool {
        self.0 <= 0.0
    }

    /// Composes two opacities, e.g. an inherited group opacity with a shape's
    /// own `fill-opacity`.
    #[inline]
    pub fn multiply(self, other: Self) -> Self {
        Self(self.0 * other.0)
    }
}

impl Default for Opacity {
    fn default() -> Self {
        Self::OPAQUE
    }
}

/// What a fill or stroke is painted with.
///
/// `CurrentColor` is kept as a distinct variant rather than being resolved to
/// black at compile time. That single distinction is what makes an icon
/// *tintable*: the runtime can rasterize one alpha mask and recolour it with
/// `ImageColor3` instead of re-rasterizing per colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Paint {
    /// Defers to a colour supplied by the consumer at render time.
    CurrentColor,
    /// A fixed colour baked into the asset.
    Solid(Color),
    // Future variants — `LinearGradient(GradientId)`, `RadialGradient(..)` —
    // slot in here. The serialized IR stores paints in an indexed table with a
    // `kind` tag precisely so new kinds can be added without moving anything.
}

/// Stroke cap style. Values mirror SVG's `stroke-linecap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

/// Stroke join style. Values mirror SVG's `stroke-linejoin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// The interior paint of a shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fill {
    pub paint: Paint,
    pub opacity: Opacity,
    pub rule: FillRule,
}

impl Fill {
    pub fn new(paint: Paint, opacity: Opacity, rule: FillRule) -> Self {
        Self {
            paint,
            opacity,
            rule,
        }
    }
}

/// The outline paint of a shape.
///
/// `width` is expressed in view box units, matching the document's geometry.
/// Consumers that want a resolution-independent stroke scale it themselves at
/// render time; see `SvgRenderOptions.absoluteStrokeWidth` in `@rbxts/svg`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    pub paint: Paint,
    pub opacity: Opacity,
    pub width: f32,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub miter_limit: f32,
}

impl Stroke {
    pub fn new(
        paint: Paint,
        opacity: Opacity,
        width: f32,
        line_cap: LineCap,
        line_join: LineJoin,
        miter_limit: f32,
    ) -> Result<Self, CoreError> {
        if !width.is_finite() || width <= 0.0 {
            return Err(CoreError::InvalidScalar {
                what: "stroke width",
                value: width,
            });
        }
        // SVG requires stroke-miterlimit >= 1.
        if !miter_limit.is_finite() || miter_limit < 1.0 {
            return Err(CoreError::InvalidScalar {
                what: "stroke miter limit",
                value: miter_limit,
            });
        }
        Ok(Self {
            paint,
            opacity,
            width,
            line_cap,
            line_join,
            miter_limit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_rejects_out_of_range_values() {
        assert!(Opacity::new(-0.1).is_err());
        assert!(Opacity::new(1.1).is_err());
        assert!(Opacity::new(f32::NAN).is_err());
        assert!(Opacity::new(0.5).is_ok());
    }

    #[test]
    fn opacity_clamping_matches_svg_semantics() {
        assert_eq!(Opacity::clamped(-2.0), Opacity::TRANSPARENT);
        assert_eq!(Opacity::clamped(7.0), Opacity::OPAQUE);
        assert_eq!(Opacity::clamped(f32::NAN), Opacity::OPAQUE);
    }

    #[test]
    fn opacity_multiplies() {
        let a = Opacity::new(0.5).unwrap();
        let b = Opacity::new(0.5).unwrap();
        assert_eq!(a.multiply(b).get(), 0.25);
    }

    #[test]
    fn stroke_rejects_non_positive_width() {
        let r = Stroke::new(
            Paint::CurrentColor,
            Opacity::OPAQUE,
            0.0,
            LineCap::Butt,
            LineJoin::Miter,
            4.0,
        );
        assert!(r.is_err());
    }

    #[test]
    fn stroke_rejects_miter_limit_below_one() {
        let r = Stroke::new(
            Paint::CurrentColor,
            Opacity::OPAQUE,
            2.0,
            LineCap::Butt,
            LineJoin::Miter,
            0.5,
        );
        assert!(r.is_err());
    }

    #[test]
    fn current_color_and_solid_are_distinct() {
        assert_ne!(Paint::CurrentColor, Paint::Solid(Color::BLACK));
    }
}
