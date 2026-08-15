//! Compile-time facts about an asset that the runtime can act on cheaply.
//!
//! These flags exist so the runtime never has to walk the command stream to
//! answer questions it needs *before* rasterizing — most importantly "can I
//! rasterize this once and recolour it?".

use bitflags::bitflags;

bitflags! {
    /// A bitset describing what a compiled asset contains.
    ///
    /// Encoded verbatim into the serialized IR header as a little-endian `u32`.
    /// Bit positions are part of the format contract: never renumber an
    /// existing bit, only allocate unused ones.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct FeatureFlags: u32 {
        /// At least one paint is `currentColor`, so the asset expects the
        /// consumer to supply a colour.
        const USES_CURRENT_COLOR = 1 << 0;
        /// At least one shape has a fill.
        const HAS_FILL = 1 << 1;
        /// At least one shape has a stroke.
        const HAS_STROKE = 1 << 2;
        /// At least one fill uses the even-odd rule. A renderer that only
        /// implements non-zero can detect up front that it cannot draw this.
        const HAS_EVEN_ODD_FILL = 1 << 3;
        /// Every visible paint in the asset is the *same* paint. Combined with
        /// the raster cache this is the tinting fast path: rasterize a single
        /// alpha mask, then set `ImageColor3` per instance instead of
        /// re-rasterizing per colour. See `MONOCHROME` notes in
        /// `docs/ARCHITECTURE.md`.
        const MONOCHROME = 1 << 4;
        /// Some paint is drawn at less than full opacity, so the rasterizer
        /// must blend rather than write coverage directly.
        const HAS_TRANSPARENCY = 1 << 5;
        /// At least one shape paints its stroke beneath its fill
        /// (`paint-order: stroke`).
        const HAS_STROKE_FIRST = 1 << 6;

        // ---- Reserved for features that are designed for but not implemented.
        // Declared now so their bit positions are committed and a future
        // compiler can set them without a format version bump.
        /// Reserved: asset contains a gradient paint.
        const HAS_GRADIENT = 1 << 16;
        /// Reserved: asset contains a clip path.
        const HAS_CLIP = 1 << 17;
        /// Reserved: asset contains a mask.
        const HAS_MASK = 1 << 18;
        /// Reserved: asset contains dashed strokes.
        const HAS_DASH = 1 << 19;
    }
}

impl FeatureFlags {
    /// True when the asset can be rasterized once as an alpha mask and then
    /// recoloured per instance.
    ///
    /// This is the property the render cache keys on: for a tintable asset the
    /// colour is deliberately *excluded* from the raster cache key.
    pub fn is_tintable(self) -> bool {
        self.contains(Self::MONOCHROME) && self.contains(Self::USES_CURRENT_COLOR)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tintable_requires_both_monochrome_and_current_color() {
        assert!(!FeatureFlags::MONOCHROME.is_tintable());
        assert!(!FeatureFlags::USES_CURRENT_COLOR.is_tintable());
        assert!((FeatureFlags::MONOCHROME | FeatureFlags::USES_CURRENT_COLOR).is_tintable());
    }

    #[test]
    fn bit_positions_are_stable() {
        // These values are part of the serialized format. Changing one is a
        // breaking change to every already-compiled asset, so pin them here.
        assert_eq!(FeatureFlags::USES_CURRENT_COLOR.bits(), 1);
        assert_eq!(FeatureFlags::HAS_FILL.bits(), 2);
        assert_eq!(FeatureFlags::HAS_STROKE.bits(), 4);
        assert_eq!(FeatureFlags::HAS_EVEN_ODD_FILL.bits(), 8);
        assert_eq!(FeatureFlags::MONOCHROME.bits(), 16);
        assert_eq!(FeatureFlags::HAS_TRANSPARENCY.bits(), 32);
        assert_eq!(FeatureFlags::HAS_STROKE_FIRST.bits(), 64);
        assert_eq!(FeatureFlags::HAS_GRADIENT.bits(), 1 << 16);
    }
}
