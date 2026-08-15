//! The output image, and the alpha convention behind it.
//!
//! # Two conventions, stated once
//!
//! **Internally** the canvas holds *premultiplied* RGBA as `f32`. Premultiplied
//! is what makes source-over a single expression — `dst = src + dst * (1 - a)`
//! applies to every channel identically — with no per-pixel division and no
//! special case for a transparent destination. `f32` because coverage is
//! fractional and a document is composited shape by shape; rounding to eight
//! bits between shapes would accumulate visible error on soft edges.
//!
//! **Externally** [`RasterImage`] is *straight* (non-premultiplied) RGBA8, in
//! sRGB, row-major from the top-left, four bytes per pixel. That is what
//! `EditableImage.WritePixelsBuffer` wants, and it is what makes the alpha-mask
//! path meaningful: a mask is only tintable if its RGB survives independently
//! of its alpha.
//!
//! # Colour space
//!
//! Blending happens on the sRGB-encoded values, not on linearised ones. That is
//! not because it is more correct — it is not — but because it is what SVG
//! renderers actually do, so it is what a differential comparison against resvg
//! and what a user's expectations are both calibrated to. Choosing correctness
//! here would make every one of our outputs disagree with every other renderer.

use svg_core::Color;

/// A rendered image: straight (non-premultiplied) RGBA8, row-major, sRGB.
///
/// `pixels` is exactly `width * height * 4` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RasterImage {
    /// The four bytes of pixel `(x, y)`, or `None` if it is outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = ((y * self.width + x) * 4) as usize;
        Some([
            self.pixels[at],
            self.pixels[at + 1],
            self.pixels[at + 2],
            self.pixels[at + 3],
        ])
    }

    /// The alpha channel on its own.
    ///
    /// The tintable path's output is entirely in here, and comparing two
    /// renders' alpha is how "the same coverage regardless of colour" is
    /// actually checked.
    pub fn alpha(&self) -> Vec<u8> {
        self.pixels.iter().skip(3).step_by(4).copied().collect()
    }
}

/// A premultiplied RGBA accumulation buffer.
#[derive(Debug)]
pub struct Canvas {
    width: u32,
    height: u32,
    /// Premultiplied `r, g, b, a` per pixel, each in `0.0..=1.0`.
    pixels: Vec<f32>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0.0; (width as usize) * (height as usize) * 4],
        }
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Composites a run of one row with `colour` at `alpha × coverage`.
    ///
    /// `coverage` holds the whole row; `start..end` is the part worth touching,
    /// which the rasterizer already knows and would otherwise be rediscovered
    /// by scanning zeros.
    pub fn blend_row(
        &mut self,
        y: u32,
        coverage: &[f32],
        start: usize,
        end: usize,
        colour: Color,
        alpha: f32,
    ) {
        if y >= self.height || alpha <= 0.0 {
            return;
        }
        let red = colour.r as f32 / 255.0;
        let green = colour.g as f32 / 255.0;
        let blue = colour.b as f32 / 255.0;

        let row_base = (y as usize) * (self.width as usize) * 4;
        let last = end.min(self.width as usize);
        for (x, &covered) in coverage.iter().enumerate().take(last).skip(start) {
            // Coverage can exceed 1 only through floating-point slop in the
            // span accumulator; clamping here is what keeps alpha a probability.
            let source_alpha = (covered * alpha).clamp(0.0, 1.0);
            if source_alpha <= 0.0 {
                continue;
            }
            let at = row_base + x * 4;
            let inverse = 1.0 - source_alpha;
            self.pixels[at] = red * source_alpha + self.pixels[at] * inverse;
            self.pixels[at + 1] = green * source_alpha + self.pixels[at + 1] * inverse;
            self.pixels[at + 2] = blue * source_alpha + self.pixels[at + 2] * inverse;
            self.pixels[at + 3] = source_alpha + self.pixels[at + 3] * inverse;
        }
    }

    /// Composites coverage into the alpha channel alone, leaving RGB untouched.
    ///
    /// The mask path. Colour is not merely ignored — it must not be recorded at
    /// all, or the result would no longer be tintable.
    pub fn blend_row_alpha(
        &mut self,
        y: u32,
        coverage: &[f32],
        start: usize,
        end: usize,
        alpha: f32,
    ) {
        if y >= self.height || alpha <= 0.0 {
            return;
        }
        let row_base = (y as usize) * (self.width as usize) * 4;
        let last = end.min(self.width as usize);
        for (x, &covered) in coverage.iter().enumerate().take(last).skip(start) {
            let source_alpha = (covered * alpha).clamp(0.0, 1.0);
            if source_alpha <= 0.0 {
                continue;
            }
            let at = row_base + x * 4 + 3;
            self.pixels[at] = source_alpha + self.pixels[at] * (1.0 - source_alpha);
        }
    }

    /// Converts to straight RGBA8.
    ///
    /// `mask` replaces every colour with white, so that `ImageColor3` — which
    /// multiplies — reproduces any tint exactly.
    pub fn finish(self, mask: bool) -> RasterImage {
        let mut pixels = Vec::with_capacity(self.pixels.len());

        for chunk in self.pixels.chunks_exact(4) {
            let alpha = chunk[3].clamp(0.0, 1.0);
            if mask {
                pixels.extend_from_slice(&[255, 255, 255, to_u8(alpha)]);
                continue;
            }
            if alpha <= 0.0 {
                pixels.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            // Undo the premultiplication. Values can exceed the alpha only by
            // rounding, so clamping is a guard rather than a correction.
            pixels.extend_from_slice(&[
                to_u8(chunk[0] / alpha),
                to_u8(chunk[1] / alpha),
                to_u8(chunk[2] / alpha),
                to_u8(alpha),
            ]);
        }

        RasterImage {
            width: self.width,
            height: self.height,
            pixels,
        }
    }
}

/// Rounds a `0.0..=1.0` value to a byte, half away from zero.
#[inline]
fn to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_row(width: usize) -> Vec<f32> {
        vec![1.0; width]
    }

    #[test]
    fn an_untouched_canvas_is_fully_transparent() {
        let image = Canvas::new(2, 2).finish(false);
        assert_eq!(image.pixels, vec![0; 16]);
    }

    #[test]
    fn an_opaque_fill_survives_the_premultiplication_round_trip() {
        let mut canvas = Canvas::new(2, 1);
        canvas.blend_row(0, &solid_row(2), 0, 2, Color::rgb(255, 128, 64), 1.0);
        let image = canvas.finish(false);
        assert_eq!(image.pixel(0, 0), Some([255, 128, 64, 255]));
    }

    #[test]
    fn a_half_covered_pixel_keeps_its_colour_and_halves_its_alpha() {
        let mut canvas = Canvas::new(1, 1);
        canvas.blend_row(0, &[0.5], 0, 1, Color::rgb(200, 100, 50), 1.0);
        let image = canvas.finish(false);
        let [r, g, b, a] = image.pixel(0, 0).unwrap();
        assert_eq!(a, 128);
        // Colour comes back essentially unchanged: the alpha carried the
        // coverage, not the channels.
        assert!(r.abs_diff(200) <= 1, "{r}");
        assert!(g.abs_diff(100) <= 1, "{g}");
        assert!(b.abs_diff(50) <= 1, "{b}");
    }

    #[test]
    fn source_over_puts_the_later_shape_on_top() {
        let mut canvas = Canvas::new(1, 1);
        canvas.blend_row(0, &[1.0], 0, 1, Color::rgb(255, 0, 0), 1.0);
        canvas.blend_row(0, &[1.0], 0, 1, Color::rgb(0, 0, 255), 1.0);
        assert_eq!(canvas.finish(false).pixel(0, 0), Some([0, 0, 255, 255]));
    }

    /// Half-covering blue over opaque red gives the even mix, and stays opaque.
    #[test]
    fn a_translucent_shape_blends_with_what_is_beneath_it() {
        let mut canvas = Canvas::new(1, 1);
        canvas.blend_row(0, &[1.0], 0, 1, Color::rgb(255, 0, 0), 1.0);
        canvas.blend_row(0, &[1.0], 0, 1, Color::rgb(0, 0, 255), 0.5);
        let [r, g, b, a] = canvas.finish(false).pixel(0, 0).unwrap();
        assert_eq!(a, 255);
        assert!(r.abs_diff(128) <= 1, "{r}");
        assert_eq!(g, 0);
        assert!(b.abs_diff(128) <= 1, "{b}");
    }

    /// Two half-transparent layers compose to 1 - 0.5^2 = 0.75.
    #[test]
    fn stacked_transparency_accumulates_correctly() {
        let mut canvas = Canvas::new(1, 1);
        canvas.blend_row(0, &[1.0], 0, 1, Color::rgb(255, 255, 255), 0.5);
        canvas.blend_row(0, &[1.0], 0, 1, Color::rgb(255, 255, 255), 0.5);
        let [.., a] = canvas.finish(false).pixel(0, 0).unwrap();
        assert_eq!(a, 191); // round(0.75 * 255)
    }

    #[test]
    fn mask_mode_records_coverage_and_discards_colour() {
        let mut red = Canvas::new(1, 1);
        red.blend_row_alpha(0, &[0.5], 0, 1, 1.0);

        let mut blue = Canvas::new(1, 1);
        blue.blend_row_alpha(0, &[0.5], 0, 1, 1.0);

        assert_eq!(red.finish(true), blue.finish(true));
        // White RGB, so ImageColor3 multiplies to exactly the requested tint.
        assert_eq!(Canvas::new(1, 1).finish(true).pixels[0..3], [255, 255, 255]);
    }

    #[test]
    fn blending_outside_the_canvas_is_ignored() {
        let mut canvas = Canvas::new(2, 2);
        canvas.blend_row(9, &solid_row(2), 0, 2, Color::WHITE, 1.0);
        canvas.blend_row(0, &solid_row(2), 0, 99, Color::WHITE, 1.0);
        let image = canvas.finish(false);
        assert_eq!(image.pixel(0, 0), Some([255, 255, 255, 255]));
        assert_eq!(image.pixel(0, 1), Some([0, 0, 0, 0]));
    }

    #[test]
    fn zero_alpha_paints_nothing() {
        let mut canvas = Canvas::new(1, 1);
        canvas.blend_row(0, &[1.0], 0, 1, Color::WHITE, 0.0);
        assert_eq!(canvas.finish(false).pixel(0, 0), Some([0, 0, 0, 0]));
    }

    #[test]
    fn alpha_extracts_one_byte_per_pixel() {
        let mut canvas = Canvas::new(2, 2);
        canvas.blend_row(1, &solid_row(2), 0, 2, Color::WHITE, 1.0);
        let alpha = canvas.finish(false).alpha();
        assert_eq!(alpha, vec![0, 0, 255, 255]);
    }
}
