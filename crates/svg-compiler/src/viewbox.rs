//! View box resolution.
//!
//! # Why this module has to exist
//!
//! usvg does not keep the `viewBox`. It resolves the document to a pixel
//! `Size` and folds the view-box-to-viewport mapping into the root group's
//! transform, so every path's `abs_transform()` already contains it. That is
//! exactly wrong for us: a compiled asset must be resolution-independent, with
//! geometry in *view box* space and the target size supplied at render time.
//!
//! So we parse the view box ourselves, reconstruct the same transform usvg
//! applied, and invert it while baking geometry.
//!
//! # One definition of viewport fitting
//!
//! The reconstruction is *not* written out again here: it is
//! [`svg_core::view_box_transform`], the same function the reference rasterizer
//! uses to map an asset onto a target rectangle. usvg's own
//! `ViewBox::to_transform` is `pub(crate)`, so this has to be reproduced
//! somewhere — but reproducing it once, in the crate every renderer already
//! depends on, is what stops the compiler's idea of "fitting" and a renderer's
//! from drifting apart. The tests below pin the agreement.
//!
//! # The policy is not thrown away
//!
//! Undoing usvg's mapping is only half the job. The authored
//! `preserveAspectRatio` also has to survive into the compiled document,
//! because a renderer handed a target rectangle of a different shape needs to
//! know whether the author meant "stretch" or "letterbox". [`resolve`] returns
//! it and `compile` stores it on the [`svg_core::SvgDocument`].

use std::str::FromStr;

use svg_core::{AspectAlign, AspectScale, PreserveAspectRatio, Transform, ViewBox};
use svgtypes::{Align, AspectRatio, Length, LengthUnit};
use usvg::roxmltree;

use crate::error::CompileError;

/// The root element's coordinate system as authored.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedViewBox {
    pub view_box: ViewBox,
    /// The authored policy, in the framework-neutral representation that is
    /// carried through the IR to every runtime.
    pub aspect: PreserveAspectRatio,
}

/// Reads the coordinate system from the root `<svg>` element.
///
/// Resolution order matches SVG and usvg:
///
/// 1. An explicit `viewBox`.
/// 2. Otherwise `0 0 width height`, if both are absolute lengths.
/// 3. Otherwise a failure — there is no coordinate system to compile into.
pub fn resolve(root: roxmltree::Node<'_, '_>) -> Result<ResolvedViewBox, CompileError> {
    // An unparseable value falls back to the SVG default rather than failing:
    // that is what a browser does, and refusing to compile an icon over a typo
    // in an attribute that only matters for non-matching aspect ratios would be
    // out of proportion.
    let aspect = lower_aspect_ratio(
        root.attribute("preserveAspectRatio")
            .and_then(|s| AspectRatio::from_str(s).ok())
            .unwrap_or_default(),
    );

    if let Some(raw) = root.attribute("viewBox") {
        let parsed =
            svgtypes::ViewBox::from_str(raw).map_err(|_| CompileError::InvalidViewBox {
                reason: format!("could not parse viewBox=\"{raw}\""),
            })?;
        let view_box = ViewBox::new(
            parsed.x as f32,
            parsed.y as f32,
            parsed.w as f32,
            parsed.h as f32,
        )
        .map_err(|_| CompileError::InvalidViewBox {
            reason: format!(
                "viewBox=\"{raw}\" has a non-positive size ({} x {})",
                parsed.w, parsed.h
            ),
        })?;
        return Ok(ResolvedViewBox { view_box, aspect });
    }

    let width = root.attribute("width").and_then(absolute_length);
    let height = root.attribute("height").and_then(absolute_length);

    match (width, height) {
        (Some(w), Some(h)) => {
            let view_box =
                ViewBox::new(0.0, 0.0, w, h).map_err(|_| CompileError::InvalidViewBox {
                    reason: format!("width/height resolve to a non-positive size ({w} x {h})"),
                })?;
            Ok(ResolvedViewBox { view_box, aspect })
        }
        _ => Err(CompileError::InvalidViewBox {
            reason: "the root <svg> element needs either a viewBox, or absolute width and height \
                     attributes"
                .to_string(),
        }),
    }
}

/// Parses a length that needs no context to resolve.
///
/// Percentages and font-relative units depend on a viewport we do not have at
/// this point (that is the very thing we are trying to establish), so they are
/// rejected here and fall through to the "no coordinate system" error.
fn absolute_length(raw: &str) -> Option<f32> {
    let length = Length::from_str(raw).ok()?;
    // usvg's DPI-dependent units (in/cm/mm/pt/pc) would make the view box
    // depend on `Options::dpi`. Icons never use them, and accepting them here
    // would silently couple asset geometry to a compiler option.
    matches!(length.unit, LengthUnit::None | LengthUnit::Px)
        .then_some(length.number as f32)
        .filter(|n| n.is_finite() && *n > 0.0)
}

/// Translates svgtypes' parse result into the framework-neutral model.
///
/// The `defer` keyword is dropped on purpose: it only means anything on
/// `<image>`, where it hands the decision to the referenced content, and is
/// ignored on the root `<svg>` element this describes.
fn lower_aspect_ratio(aspect: AspectRatio) -> PreserveAspectRatio {
    let align = match aspect.align {
        Align::None => AspectAlign::None,
        Align::XMinYMin => AspectAlign::XMinYMin,
        Align::XMidYMin => AspectAlign::XMidYMin,
        Align::XMaxYMin => AspectAlign::XMaxYMin,
        Align::XMinYMid => AspectAlign::XMinYMid,
        Align::XMidYMid => AspectAlign::XMidYMid,
        Align::XMaxYMid => AspectAlign::XMaxYMid,
        Align::XMinYMax => AspectAlign::XMinYMax,
        Align::XMidYMax => AspectAlign::XMidYMax,
        Align::XMaxYMax => AspectAlign::XMaxYMax,
    };
    let scale = if aspect.slice {
        AspectScale::Slice
    } else {
        AspectScale::Meet
    };
    PreserveAspectRatio::new(align, scale)
}

/// Rebuilds the transform usvg baked into the root group.
///
/// This is exactly [`svg_core::view_box_transform`] with usvg's resolved pixel
/// size as the target rectangle, which is the point: the compiler undoes
/// precisely the mapping a renderer will later redo.
pub fn view_box_to_transform(
    view_box: ViewBox,
    aspect: PreserveAspectRatio,
    size: (f32, f32),
) -> Transform {
    svg_core::view_box_transform(view_box, aspect, size.0, size.1)
}

/// The transform that takes usvg's output space back into view box space.
///
/// Returns `None` only for a degenerate (non-invertible) mapping, which cannot
/// arise from a validated [`ViewBox`] and a positive size but is handled rather
/// than asserted.
pub fn to_view_box_space(
    view_box: ViewBox,
    aspect: PreserveAspectRatio,
    size: (f32, f32),
) -> Option<Transform> {
    invert(view_box_to_transform(view_box, aspect, size))
}

/// Inverts an affine transform.
fn invert(t: Transform) -> Option<Transform> {
    let det = t.determinant();
    if !det.is_finite() || det.abs() < f32::EPSILON {
        return None;
    }
    let inv_det = 1.0 / det;
    // Standard 2x2 inverse, with the translation carried through it.
    let sx = t.sy * inv_det;
    let ky = -t.ky * inv_det;
    let kx = -t.kx * inv_det;
    let sy = t.sx * inv_det;
    let tx = (t.kx * t.ty - t.sy * t.tx) * inv_det;
    let ty = (t.ky * t.tx - t.sx * t.ty) * inv_det;
    Some(Transform::from_row(sx, ky, kx, sy, tx, ty))
}

#[cfg(test)]
mod tests {
    use super::*;
    use svg_core::Point;

    fn parse_root(xml: &str) -> roxmltree::Document<'_> {
        roxmltree::Document::parse(xml).unwrap()
    }

    fn vb(x: f32, y: f32, w: f32, h: f32) -> ViewBox {
        ViewBox::new(x, y, w, h).unwrap()
    }

    #[test]
    fn explicit_view_box_wins_over_width_and_height() {
        let doc = parse_root(r#"<svg width="96" height="96" viewBox="0 0 24 24"/>"#);
        let r = resolve(doc.root_element()).unwrap();
        assert_eq!(r.view_box, vb(0.0, 0.0, 24.0, 24.0));
    }

    #[test]
    fn width_and_height_are_the_fallback() {
        let doc = parse_root(r#"<svg width="32" height="16"/>"#);
        let r = resolve(doc.root_element()).unwrap();
        assert_eq!(r.view_box, vb(0.0, 0.0, 32.0, 16.0));
    }

    #[test]
    fn px_suffix_is_accepted() {
        let doc = parse_root(r#"<svg width="32px" height="16px"/>"#);
        assert_eq!(
            resolve(doc.root_element()).unwrap().view_box,
            vb(0.0, 0.0, 32.0, 16.0)
        );
    }

    #[test]
    fn percentage_sizes_do_not_establish_a_coordinate_system() {
        let doc = parse_root(r#"<svg width="100%" height="100%"/>"#);
        assert!(matches!(
            resolve(doc.root_element()),
            Err(CompileError::InvalidViewBox { .. })
        ));
    }

    #[test]
    fn missing_view_box_and_size_is_an_error() {
        let doc = parse_root(r#"<svg/>"#);
        assert!(matches!(
            resolve(doc.root_element()),
            Err(CompileError::InvalidViewBox { .. })
        ));
    }

    #[test]
    fn degenerate_view_box_is_an_error() {
        let doc = parse_root(r#"<svg viewBox="0 0 0 24"/>"#);
        assert!(matches!(
            resolve(doc.root_element()),
            Err(CompileError::InvalidViewBox { .. })
        ));
    }

    #[test]
    fn malformed_view_box_is_an_error() {
        let doc = parse_root(r#"<svg viewBox="nonsense"/>"#);
        assert!(matches!(
            resolve(doc.root_element()),
            Err(CompileError::InvalidViewBox { .. })
        ));
    }

    // ---- preserveAspectRatio --------------------------------------------

    #[test]
    fn absent_preserve_aspect_ratio_is_the_svg_default() {
        let doc = parse_root(r#"<svg viewBox="0 0 24 24"/>"#);
        assert_eq!(
            resolve(doc.root_element()).unwrap().aspect,
            PreserveAspectRatio::DEFAULT
        );
    }

    #[test]
    fn every_alignment_and_scale_keyword_is_lowered() {
        let cases: &[(&str, AspectAlign, AspectScale)] = &[
            ("none", AspectAlign::None, AspectScale::Meet),
            ("xMinYMin meet", AspectAlign::XMinYMin, AspectScale::Meet),
            ("xMidYMin meet", AspectAlign::XMidYMin, AspectScale::Meet),
            ("xMaxYMin meet", AspectAlign::XMaxYMin, AspectScale::Meet),
            ("xMinYMid slice", AspectAlign::XMinYMid, AspectScale::Slice),
            ("xMidYMid slice", AspectAlign::XMidYMid, AspectScale::Slice),
            ("xMaxYMid slice", AspectAlign::XMaxYMid, AspectScale::Slice),
            ("xMinYMax meet", AspectAlign::XMinYMax, AspectScale::Meet),
            ("xMidYMax slice", AspectAlign::XMidYMax, AspectScale::Slice),
            ("xMaxYMax slice", AspectAlign::XMaxYMax, AspectScale::Slice),
            // The keyword is optional and defaults to `meet`.
            ("xMinYMin", AspectAlign::XMinYMin, AspectScale::Meet),
        ];
        for (raw, align, scale) in cases {
            let xml = format!(r#"<svg viewBox="0 0 24 12" preserveAspectRatio="{raw}"/>"#);
            let doc = parse_root(&xml);
            let resolved = resolve(doc.root_element()).unwrap();
            assert_eq!(resolved.aspect.align, *align, "{raw}");
            assert_eq!(resolved.aspect.scale, *scale, "{raw}");
        }
    }

    /// A typo in an attribute that only matters for non-matching aspect ratios
    /// must not fail the compile; browsers fall back to the default too.
    #[test]
    fn unparseable_preserve_aspect_ratio_falls_back_to_the_default() {
        let doc = parse_root(r#"<svg viewBox="0 0 24 24" preserveAspectRatio="nonsense"/>"#);
        assert_eq!(
            resolve(doc.root_element()).unwrap().aspect,
            PreserveAspectRatio::DEFAULT
        );
    }

    /// `defer` is only meaningful on `<image>`; on the root it is noise.
    #[test]
    fn defer_is_ignored_on_the_root_element() {
        let doc =
            parse_root(r#"<svg viewBox="0 0 24 24" preserveAspectRatio="defer xMinYMax slice"/>"#);
        let resolved = resolve(doc.root_element()).unwrap();
        assert_eq!(resolved.aspect.align, AspectAlign::XMinYMax);
        assert_eq!(resolved.aspect.scale, AspectScale::Slice);
    }

    #[test]
    fn offset_view_box_is_parsed() {
        let doc = parse_root(r#"<svg viewBox="-4 -8 24 24"/>"#);
        assert_eq!(
            resolve(doc.root_element()).unwrap().view_box,
            vb(-4.0, -8.0, 24.0, 24.0)
        );
    }

    /// The identity case that matters most: when usvg's resolved size equals
    /// the view box size and the origin is zero, no correction is needed.
    #[test]
    fn matching_size_yields_identity() {
        let t = view_box_to_transform(
            vb(0.0, 0.0, 24.0, 24.0),
            PreserveAspectRatio::DEFAULT,
            (24.0, 24.0),
        );
        assert!(t.is_identity());
    }

    #[test]
    fn offset_view_box_maps_its_origin_to_zero() {
        let t = view_box_to_transform(
            vb(-4.0, -8.0, 24.0, 24.0),
            PreserveAspectRatio::DEFAULT,
            (24.0, 24.0),
        );
        assert_eq!(t.map_point(Point::new(-4.0, -8.0)), Point::new(0.0, 0.0));
    }

    /// A 24-unit view box drawn at 96px scales by 4, and the inverse undoes it.
    #[test]
    fn inverse_returns_usvg_output_to_view_box_space() {
        let view_box = vb(0.0, 0.0, 24.0, 24.0);
        let aspect = PreserveAspectRatio::DEFAULT;
        let size = (96.0, 96.0);

        let forward = view_box_to_transform(view_box, aspect, size);
        assert_eq!(
            forward.map_point(Point::new(12.0, 6.0)),
            Point::new(48.0, 24.0)
        );

        let back = to_view_box_space(view_box, aspect, size).unwrap();
        let round_tripped = back.map_point(forward.map_point(Point::new(12.0, 6.0)));
        assert!((round_tripped.x - 12.0).abs() < 1e-4);
        assert!((round_tripped.y - 6.0).abs() < 1e-4);
    }

    #[test]
    fn aspect_ratio_none_allows_non_uniform_scale() {
        let t = view_box_to_transform(
            vb(0.0, 0.0, 24.0, 24.0),
            PreserveAspectRatio::STRETCH,
            (48.0, 24.0),
        );
        assert_eq!(t.map_point(Point::new(24.0, 24.0)), Point::new(48.0, 24.0));
    }

    #[test]
    fn meet_centres_the_view_box_in_a_wider_viewport() {
        // XMidYMid meet: uniform scale 1, centred horizontally in 48x24.
        let t = view_box_to_transform(
            vb(0.0, 0.0, 24.0, 24.0),
            PreserveAspectRatio::DEFAULT,
            (48.0, 24.0),
        );
        assert_eq!(t.map_point(Point::new(0.0, 0.0)), Point::new(12.0, 0.0));
    }

    #[test]
    fn inversion_round_trips_through_an_aligned_transform() {
        let view_box = vb(2.0, 3.0, 24.0, 12.0);
        let aspect = PreserveAspectRatio::DEFAULT;
        let size = (100.0, 40.0);
        let forward = view_box_to_transform(view_box, aspect, size);
        let back = to_view_box_space(view_box, aspect, size).unwrap();

        for p in [
            Point::new(2.0, 3.0),
            Point::new(26.0, 15.0),
            Point::new(10.0, 7.0),
        ] {
            let out = back.map_point(forward.map_point(p));
            assert!((out.x - p.x).abs() < 1e-3, "{out:?} vs {p:?}");
            assert!((out.y - p.y).abs() < 1e-3, "{out:?} vs {p:?}");
        }
    }

    #[test]
    fn degenerate_transform_cannot_be_inverted() {
        assert!(invert(Transform::from_row(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)).is_none());
    }
}
