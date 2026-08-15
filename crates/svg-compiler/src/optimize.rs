//! Post-normalization cleanup and feature detection.
//!
//! Everything here is conservative: a pass may drop something that provably
//! paints nothing, but it must never change what the asset looks like.

use svg_core::{FeatureFlags, FillRule, Paint, PaintOrder, Shape};

use crate::diagnostics::{Diagnostic, DiagnosticCode};

/// Removes shapes that cannot contribute to the raster.
///
/// Two cases qualify:
///
/// - no visible paint at all (every paint fully transparent, or absent);
/// - no drawing commands, i.e. only `MoveTo`/`Close`. A zero-length subpath can
///   still paint a dot under a round or square cap, so that case is kept.
pub fn drop_invisible_shapes(shapes: &mut Vec<Shape>, diagnostics: &mut Vec<Diagnostic>) {
    let before = shapes.len();
    shapes.retain(|shape| {
        if shape.is_invisible() {
            return false;
        }
        if shape.geometry.has_drawing_commands() {
            return true;
        }
        // Degenerate geometry: only meaningful with a cap that paints at the
        // endpoint.
        shape
            .stroke
            .is_some_and(|s| s.line_cap != svg_core::LineCap::Butt)
    });

    let dropped = before - shapes.len();
    if dropped > 0 {
        diagnostics.push(Diagnostic::info(
            DiagnosticCode::DroppedEmptyShape,
            format!("removed {dropped} shape(s) that paint nothing."),
        ));
    }
}

/// Computes the document's [`FeatureFlags`].
///
/// # `MONOCHROME`
///
/// Set when every visible paint in the document is the *same* [`Paint`] value.
/// That is the precise condition under which the asset can be rasterized once
/// as a coverage mask and recoloured afterwards: with a single paint, colour
/// only ever scales the whole image uniformly, so `ImageColor3` reproduces any
/// tint exactly. Opacity may still vary between shapes — that lives in the
/// mask's alpha, not in the tint.
pub fn detect_features(shapes: &[Shape]) -> FeatureFlags {
    let mut flags = FeatureFlags::empty();
    let mut distinct_paint: Option<Paint> = None;
    let mut monochrome = true;

    for shape in shapes {
        if let Some(fill) = shape.fill {
            flags |= FeatureFlags::HAS_FILL;
            if fill.rule == FillRule::EvenOdd {
                flags |= FeatureFlags::HAS_EVEN_ODD_FILL;
            }
            if !fill.opacity.is_opaque() {
                flags |= FeatureFlags::HAS_TRANSPARENCY;
            }
        }
        if let Some(stroke) = shape.stroke {
            flags |= FeatureFlags::HAS_STROKE;
            if !stroke.opacity.is_opaque() {
                flags |= FeatureFlags::HAS_TRANSPARENCY;
            }
        }
        if shape.paint_order == PaintOrder::StrokeThenFill {
            flags |= FeatureFlags::HAS_STROKE_FIRST;
        }

        for paint in shape.paints() {
            if paint == Paint::CurrentColor {
                flags |= FeatureFlags::USES_CURRENT_COLOR;
            }
            match distinct_paint {
                None => distinct_paint = Some(paint),
                Some(seen) if seen != paint => monochrome = false,
                Some(_) => {}
            }
        }
    }

    // An empty document is trivially uniform, but calling it monochrome would
    // advertise a tinting fast path for something with nothing to tint.
    if monochrome && distinct_paint.is_some() {
        flags |= FeatureFlags::MONOCHROME;
    }

    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use svg_core::{Color, Fill, LineCap, LineJoin, Opacity, Path, PathBuilder, Point, Stroke};

    fn line() -> Path {
        let mut b = PathBuilder::new();
        b.move_to(Point::new(0.0, 0.0)).unwrap();
        b.line_to(Point::new(1.0, 1.0)).unwrap();
        b.finish()
    }

    fn dot() -> Path {
        let mut b = PathBuilder::new();
        b.move_to(Point::new(0.0, 0.0)).unwrap();
        b.finish()
    }

    fn fill(paint: Paint) -> Fill {
        Fill::new(paint, Opacity::OPAQUE, FillRule::NonZero)
    }

    fn stroke_with(paint: Paint, cap: LineCap) -> Stroke {
        Stroke::new(paint, Opacity::OPAQUE, 2.0, cap, LineJoin::Round, 4.0).unwrap()
    }

    #[test]
    fn unpainted_shapes_are_dropped() {
        let mut shapes = vec![Shape::new(line(), None, None)];
        let mut diagnostics = Vec::new();
        drop_invisible_shapes(&mut shapes, &mut diagnostics);
        assert!(shapes.is_empty());
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn zero_length_subpath_survives_under_a_round_cap() {
        let mut shapes = vec![Shape::new(
            dot(),
            None,
            Some(stroke_with(Paint::CurrentColor, LineCap::Round)),
        )];
        let mut diagnostics = Vec::new();
        drop_invisible_shapes(&mut shapes, &mut diagnostics);
        assert_eq!(shapes.len(), 1, "a round cap paints a dot");
    }

    #[test]
    fn zero_length_subpath_is_dropped_under_a_butt_cap() {
        let mut shapes = vec![Shape::new(
            dot(),
            None,
            Some(stroke_with(Paint::CurrentColor, LineCap::Butt)),
        )];
        let mut diagnostics = Vec::new();
        drop_invisible_shapes(&mut shapes, &mut diagnostics);
        assert!(shapes.is_empty());
    }

    #[test]
    fn nothing_is_reported_when_nothing_is_dropped() {
        let mut shapes = vec![Shape::new(line(), Some(fill(Paint::CurrentColor)), None)];
        let mut diagnostics = Vec::new();
        drop_invisible_shapes(&mut shapes, &mut diagnostics);
        assert_eq!(shapes.len(), 1);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn current_color_only_document_is_monochrome_and_tintable() {
        let shapes = vec![
            Shape::new(
                line(),
                None,
                Some(stroke_with(Paint::CurrentColor, LineCap::Round)),
            ),
            Shape::new(
                line(),
                None,
                Some(stroke_with(Paint::CurrentColor, LineCap::Round)),
            ),
        ];
        let flags = detect_features(&shapes);
        assert!(flags.contains(FeatureFlags::USES_CURRENT_COLOR));
        assert!(flags.contains(FeatureFlags::MONOCHROME));
        assert!(flags.contains(FeatureFlags::HAS_STROKE));
        assert!(!flags.contains(FeatureFlags::HAS_FILL));
        assert!(flags.is_tintable());
    }

    #[test]
    fn two_different_colours_are_not_monochrome() {
        let shapes = vec![
            Shape::new(line(), Some(fill(Paint::Solid(Color::BLACK))), None),
            Shape::new(line(), Some(fill(Paint::Solid(Color::WHITE))), None),
        ];
        assert!(!detect_features(&shapes).contains(FeatureFlags::MONOCHROME));
    }

    #[test]
    fn a_single_solid_colour_is_monochrome_but_not_tintable() {
        let shapes = vec![Shape::new(
            line(),
            Some(fill(Paint::Solid(Color::BLACK))),
            None,
        )];
        let flags = detect_features(&shapes);
        assert!(flags.contains(FeatureFlags::MONOCHROME));
        assert!(
            !flags.is_tintable(),
            "no currentColor means nothing to tint"
        );
    }

    #[test]
    fn mixing_current_color_with_a_fixed_colour_is_not_monochrome() {
        let shapes = vec![Shape::new(
            line(),
            Some(fill(Paint::Solid(Color::WHITE))),
            Some(stroke_with(Paint::CurrentColor, LineCap::Round)),
        )];
        let flags = detect_features(&shapes);
        assert!(flags.contains(FeatureFlags::USES_CURRENT_COLOR));
        assert!(!flags.contains(FeatureFlags::MONOCHROME));
    }

    #[test]
    fn empty_document_is_not_monochrome() {
        assert!(!detect_features(&[]).contains(FeatureFlags::MONOCHROME));
    }

    #[test]
    fn varying_opacity_does_not_break_monochrome() {
        // Opacity lands in the mask's alpha channel, so it does not stop the
        // asset from being tintable.
        let mut a = Shape::new(line(), Some(fill(Paint::CurrentColor)), None);
        a.fill = Some(Fill::new(
            Paint::CurrentColor,
            Opacity::new(0.5).unwrap(),
            FillRule::NonZero,
        ));
        let b = Shape::new(line(), Some(fill(Paint::CurrentColor)), None);

        let flags = detect_features(&[a, b]);
        assert!(flags.contains(FeatureFlags::MONOCHROME));
        assert!(flags.contains(FeatureFlags::HAS_TRANSPARENCY));
    }

    #[test]
    fn even_odd_is_flagged() {
        let mut shape = Shape::new(line(), Some(fill(Paint::CurrentColor)), None);
        shape.fill = Some(Fill::new(
            Paint::CurrentColor,
            Opacity::OPAQUE,
            FillRule::EvenOdd,
        ));
        assert!(detect_features(&[shape]).contains(FeatureFlags::HAS_EVEN_ODD_FILL));
    }

    #[test]
    fn paint_order_is_flagged() {
        let mut shape = Shape::new(
            line(),
            Some(fill(Paint::CurrentColor)),
            Some(stroke_with(Paint::CurrentColor, LineCap::Round)),
        );
        shape.paint_order = PaintOrder::StrokeThenFill;
        assert!(detect_features(&[shape]).contains(FeatureFlags::HAS_STROKE_FIRST));
    }
}
