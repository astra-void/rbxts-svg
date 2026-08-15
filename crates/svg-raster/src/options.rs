//! What a caller asks for.

use svg_core::Color;

/// What kind of image to produce.
///
/// The two modes share every stage of the pipeline except the last: geometry is
/// traversed, flattened, expanded and scan-converted once, and only the
/// compositing step differs. Duplicating the traversal to produce a mask would
/// mean two code paths that must agree pixel for pixel, which is precisely the
/// kind of agreement that does not survive a year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RasterMode {
    /// Full colour. Every shape's own paint is composited.
    #[default]
    Color,
    /// Coverage only: white RGB, with the composited alpha in the alpha channel.
    ///
    /// The tinting fast path. For an asset whose every paint is the same
    /// `currentColor` — which is every Lucide icon — one mask serves every
    /// colour, because `ImageColor3` multiplies white by the tint exactly.
    ///
    /// Paint *opacity* still applies: it is part of how much of the shape is
    /// there, not of what colour it is. Only the hue is discarded.
    AlphaMask,
}

/// A rasterization request.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterOptions {
    /// Output width in pixels. Must be between 1 and
    /// [`crate::MAX_DIMENSION`].
    pub width: u32,
    /// Output height in pixels.
    pub height: u32,
    /// Colour or mask.
    pub mode: RasterMode,
    /// The colour `currentColor` paints resolve to.
    ///
    /// Defaults to black, matching CSS's initial `color`, but is a genuine
    /// parameter: forcing `currentColor` to black inside the renderer would
    /// make the colour path unusable for exactly the assets — tintable ones —
    /// that the whole design is built around.
    ///
    /// Ignored under [`RasterMode::AlphaMask`], where no colour is recorded.
    pub current_color: Color,
    /// Replaces every shape's stroke width.
    ///
    /// This is Lucide's `strokeWidth` prop. `None` keeps each shape's own
    /// width, which is the common case.
    pub stroke_width_override: Option<f32>,
    /// Interprets [`Self::stroke_width_override`] as **output pixels** rather
    /// than view box units.
    ///
    /// Mirrors Lucide's `absoluteStrokeWidth`: an icon keeps the same apparent
    /// line weight whatever size it is drawn at, instead of the stroke growing
    /// with the artwork.
    ///
    /// # Under a non-uniform fit
    ///
    /// `preserveAspectRatio="none"` scales x and y differently, so no single
    /// number is the scale and a circular pen becomes an elliptical one. Rather
    /// than pretend otherwise, this renderer strokes with a circular pen whose
    /// width is converted using the geometric mean of the two scales — the same
    /// approximation `svg_core::Transform::length_scale` makes, and the same
    /// one the compiler already applies to strokes under a skewed transform.
    /// The result is a stroke of the right *weight* but without the
    /// directional thick-and-thin a true elliptical pen would give.
    ///
    /// Note that this only affects how a width in one unit is expressed in
    /// another. An absolute width is already in pixels, so it is used as-is and
    /// the approximation does not arise at all.
    pub absolute_stroke_width: bool,
}

impl RasterOptions {
    /// A colour render at `width` × `height`, with everything else defaulted.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            mode: RasterMode::Color,
            current_color: Color::BLACK,
            stroke_width_override: None,
            absolute_stroke_width: false,
        }
    }

    /// A square colour render.
    pub fn square(size: u32) -> Self {
        Self::new(size, size)
    }

    #[must_use]
    pub fn with_mode(mut self, mode: RasterMode) -> Self {
        self.mode = mode;
        self
    }

    #[must_use]
    pub fn with_current_color(mut self, color: Color) -> Self {
        self.current_color = color;
        self
    }

    /// Sets a stroke width override in view box units.
    #[must_use]
    pub fn with_stroke_width(mut self, width: f32) -> Self {
        self.stroke_width_override = Some(width);
        self.absolute_stroke_width = false;
        self
    }

    /// Sets a stroke width override in output pixels.
    #[must_use]
    pub fn with_absolute_stroke_width(mut self, pixels: f32) -> Self {
        self.stroke_width_override = Some(pixels);
        self.absolute_stroke_width = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_a_black_colour_render() {
        let options = RasterOptions::square(24);
        assert_eq!(options.width, 24);
        assert_eq!(options.height, 24);
        assert_eq!(options.mode, RasterMode::Color);
        assert_eq!(options.current_color, Color::BLACK);
        assert_eq!(options.stroke_width_override, None);
        assert!(!options.absolute_stroke_width);
    }

    /// The two stroke setters are mutually exclusive: setting one must clear
    /// the other's interpretation, or a builder chain would silently keep a
    /// stale unit.
    #[test]
    fn the_stroke_setters_are_mutually_exclusive() {
        let absolute = RasterOptions::square(24)
            .with_stroke_width(2.0)
            .with_absolute_stroke_width(1.5);
        assert_eq!(absolute.stroke_width_override, Some(1.5));
        assert!(absolute.absolute_stroke_width);

        let relative = RasterOptions::square(24)
            .with_absolute_stroke_width(1.5)
            .with_stroke_width(2.0);
        assert_eq!(relative.stroke_width_override, Some(2.0));
        assert!(!relative.absolute_stroke_width);
    }
}
