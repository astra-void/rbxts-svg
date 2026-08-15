//! The compiled semantic document.
//!
//! An [`SvgDocument`] is a *flat, resolved* description of vector graphics. It
//! is deliberately not a model of the SVG DOM: by the time a document exists,
//! groups, `use` references, inherited properties, primitive shapes and
//! transforms have all been resolved away. What remains is the minimum needed
//! to draw the picture.

use crate::aspect::PreserveAspectRatio;
use crate::features::FeatureFlags;
use crate::geometry::ViewBox;
use crate::paint::{Fill, Paint, Stroke};
use crate::path::Path;
use crate::transform::Transform;

/// Whether a shape's fill or its stroke is painted first. Mirrors SVG's
/// `paint-order`. It only matters when both exist and are not fully opaque or
/// do not fully overlap, but getting it wrong is visible, so it is carried
/// through rather than assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaintOrder {
    /// Fill, then stroke (the SVG default).
    #[default]
    FillThenStroke,
    /// Stroke, then fill.
    StrokeThenFill,
}

/// One drawable shape: canonical geometry plus how to paint it.
///
/// Geometry and paint are kept in separate fields so that a future renderer can
/// consume the outline without caring about colour (stroke expansion, hit
/// testing, bounds) and vice versa.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    /// Canonical geometry in view box space, with all transforms baked in.
    pub geometry: Path,
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
    pub paint_order: PaintOrder,
}

impl Shape {
    pub fn new(geometry: Path, fill: Option<Fill>, stroke: Option<Stroke>) -> Self {
        Self {
            geometry,
            fill,
            stroke,
            paint_order: PaintOrder::default(),
        }
    }

    /// True when the shape contributes nothing to the raster: no paint at all,
    /// or every paint fully transparent.
    pub fn is_invisible(&self) -> bool {
        let fill_visible = self.fill.is_some_and(|f| !f.opacity.is_fully_transparent());
        let stroke_visible = self
            .stroke
            .is_some_and(|s| !s.opacity.is_fully_transparent());
        !(fill_visible || stroke_visible)
    }

    /// Every paint this shape actually uses, in paint order.
    pub fn paints(&self) -> impl Iterator<Item = Paint> + '_ {
        let (first, second) = match self.paint_order {
            PaintOrder::FillThenStroke => {
                (self.fill.map(|f| f.paint), self.stroke.map(|s| s.paint))
            }
            PaintOrder::StrokeThenFill => {
                (self.stroke.map(|s| s.paint), self.fill.map(|f| f.paint))
            }
        };
        first.into_iter().chain(second)
    }
}

/// A fully compiled, framework-neutral vector asset.
///
/// This is the hand-off point between the compiler and every consumer: the
/// serializer in `svg-ir`, a future reference rasterizer, and (via the
/// serialized form) the Roblox and DOM runtimes.
#[derive(Debug, Clone, PartialEq)]
pub struct SvgDocument {
    /// The coordinate system every shape's geometry is expressed in.
    pub view_box: ViewBox,
    /// How the view box is fitted into a target rectangle whose aspect ratio
    /// differs from its own.
    ///
    /// Without this a renderer has no way to tell "stretch me" from "letterbox
    /// me", and would have to guess — which is wrong for one of the two.
    pub preserve_aspect_ratio: PreserveAspectRatio,
    /// Shapes in painter's order: index 0 is drawn first, i.e. furthest back.
    pub shapes: Vec<Shape>,
    /// Cheap facts about the document, computed once at compile time.
    pub features: FeatureFlags,
}

impl SvgDocument {
    /// Builds a document with SVG's default `xMidYMid meet` fitting policy. Use
    /// [`Self::with_preserve_aspect_ratio`] to override it.
    pub fn new(view_box: ViewBox, shapes: Vec<Shape>, features: FeatureFlags) -> Self {
        Self {
            view_box,
            preserve_aspect_ratio: PreserveAspectRatio::DEFAULT,
            shapes,
            features,
        }
    }

    /// Sets the fitting policy, as authored on the root `<svg>` element.
    #[must_use]
    pub fn with_preserve_aspect_ratio(mut self, aspect: PreserveAspectRatio) -> Self {
        self.preserve_aspect_ratio = aspect;
        self
    }

    /// The transform that maps this document's geometry onto a target
    /// rectangle, honouring its `preserveAspectRatio`.
    ///
    /// Every renderer goes through this rather than deriving its own scale, so
    /// that the Rust reference rasterizer and the Luau one cannot disagree
    /// about what an asset is supposed to look like.
    pub fn target_transform(&self, target_width: f32, target_height: f32) -> Transform {
        crate::aspect::view_box_transform(
            self.view_box,
            self.preserve_aspect_ratio,
            target_width,
            target_height,
        )
    }

    /// Total number of canonical path commands across all shapes. Used for
    /// capacity hints and for size diagnostics.
    pub fn command_count(&self) -> usize {
        self.shapes
            .iter()
            .map(|s| s.geometry.commands().len())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;
    use crate::paint::{Color, LineCap, LineJoin, Opacity};
    use crate::path::{FillRule, PathBuilder};

    fn square() -> Path {
        let mut b = PathBuilder::new();
        b.move_to(Point::new(0.0, 0.0)).unwrap();
        b.line_to(Point::new(1.0, 0.0)).unwrap();
        b.line_to(Point::new(1.0, 1.0)).unwrap();
        b.close().unwrap();
        b.finish()
    }

    #[test]
    fn shape_with_no_paint_is_invisible() {
        let s = Shape::new(square(), None, None);
        assert!(s.is_invisible());
    }

    #[test]
    fn shape_with_fully_transparent_paint_is_invisible() {
        let fill = Fill::new(Paint::CurrentColor, Opacity::TRANSPARENT, FillRule::NonZero);
        assert!(Shape::new(square(), Some(fill), None).is_invisible());
    }

    #[test]
    fn paints_follow_paint_order() {
        let fill = Fill::new(
            Paint::Solid(Color::WHITE),
            Opacity::OPAQUE,
            FillRule::NonZero,
        );
        let stroke = Stroke::new(
            Paint::CurrentColor,
            Opacity::OPAQUE,
            2.0,
            LineCap::Round,
            LineJoin::Round,
            4.0,
        )
        .unwrap();

        let mut s = Shape::new(square(), Some(fill), Some(stroke));
        assert_eq!(
            s.paints().collect::<Vec<_>>(),
            vec![Paint::Solid(Color::WHITE), Paint::CurrentColor]
        );

        s.paint_order = PaintOrder::StrokeThenFill;
        assert_eq!(
            s.paints().collect::<Vec<_>>(),
            vec![Paint::CurrentColor, Paint::Solid(Color::WHITE)]
        );
    }

    #[test]
    fn documents_default_to_the_svg_aspect_ratio_policy() {
        let doc = SvgDocument::new(
            ViewBox::new(0.0, 0.0, 24.0, 24.0).unwrap(),
            vec![],
            FeatureFlags::empty(),
        );
        assert_eq!(
            doc.preserve_aspect_ratio,
            crate::aspect::PreserveAspectRatio::DEFAULT
        );
    }

    #[test]
    fn target_transform_honours_the_documents_aspect_policy() {
        use crate::aspect::{AspectAlign, AspectScale};

        let view_box = ViewBox::new(0.0, 0.0, 24.0, 12.0).unwrap();
        let meet = SvgDocument::new(view_box, vec![], FeatureFlags::empty());
        // Letterboxed: 100x50 centred vertically in a 100x100 target.
        assert_eq!(
            meet.target_transform(100.0, 100.0)
                .map_point(Point::new(0.0, 0.0)),
            Point::new(0.0, 25.0)
        );

        let stretched = meet
            .clone()
            .with_preserve_aspect_ratio(PreserveAspectRatio::new(
                AspectAlign::None,
                AspectScale::Meet,
            ));
        assert_eq!(
            stretched
                .target_transform(100.0, 100.0)
                .map_point(Point::new(24.0, 12.0)),
            Point::new(100.0, 100.0)
        );
    }

    #[test]
    fn command_count_sums_all_shapes() {
        let doc = SvgDocument::new(
            ViewBox::new(0.0, 0.0, 24.0, 24.0).unwrap(),
            vec![
                Shape::new(square(), None, None),
                Shape::new(square(), None, None),
            ],
            FeatureFlags::empty(),
        );
        assert_eq!(doc.command_count(), 8);
    }
}
