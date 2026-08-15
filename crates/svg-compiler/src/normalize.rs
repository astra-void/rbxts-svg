//! usvg tree → flat list of [`Shape`]s.
//!
//! This is where the document stops being a tree. Groups exist in SVG to share
//! transforms, opacity and clipping; once those are resolved there is nothing
//! left for a group to do, and a flat painter's-order list is both smaller and
//! far easier to decode in Luau.
//!
//! Two things are resolved here that usvg leaves on the tree:
//!
//! - **Transforms.** usvg precomputes each path's absolute transform, so we
//!   only have to compose it with the view-box correction and bake the result
//!   into the coordinates.
//! - **Opacity.** Group opacity stays on groups in usvg, so it is accumulated
//!   down the walk and multiplied into each shape's paints.

use svg_core::{
    Fill, FillRule, LineCap, LineJoin, Opacity, Paint, PaintOrder, Shape, Stroke, Transform,
};

use crate::diagnostics::{Diagnostic, DiagnosticCode, feature};
use crate::lower::{lower_path, lower_transform};
use crate::parse::CURRENT_COLOR_SENTINEL;

/// Shared state for one normalization pass.
pub struct Normalizer {
    /// Maps usvg's output space back into view box space.
    to_view_box: Transform,
    /// Whether sentinel-coloured paints should be read back as `currentColor`.
    sentinel_active: bool,
    shapes: Vec<Shape>,
    diagnostics: Vec<Diagnostic>,
    /// Set once per document so a deeply nested skewed group does not produce
    /// one diagnostic per shape.
    reported_stroke_scale: bool,
}

/// The result of flattening a usvg tree.
pub struct Normalized {
    pub shapes: Vec<Shape>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Normalizer {
    pub fn new(to_view_box: Transform, sentinel_active: bool) -> Self {
        Self {
            to_view_box,
            sentinel_active,
            shapes: Vec::new(),
            diagnostics: Vec::new(),
            reported_stroke_scale: false,
        }
    }

    pub fn run(mut self, tree: &usvg::Tree) -> Normalized {
        self.walk(tree.root(), Opacity::OPAQUE);
        Normalized {
            shapes: self.shapes,
            diagnostics: self.diagnostics,
        }
    }

    fn walk(&mut self, group: &usvg::Group, inherited_opacity: Opacity) {
        for node in group.children() {
            match node {
                usvg::Node::Group(child) => {
                    self.check_group_effects(child);
                    let group_opacity = Opacity::clamped(child.opacity().get());
                    self.check_group_opacity(child, group_opacity);
                    self.walk(child, inherited_opacity.multiply(group_opacity));
                }
                usvg::Node::Path(path) => self.emit_path(path, inherited_opacity),
                usvg::Node::Image(image) => {
                    // The source scan already reported `<image>` with a precise
                    // location; this is the backstop for images that reach the
                    // tree by another route (e.g. through `<use>`).
                    self.report_once_unsupported(
                        DiagnosticCode::UnsupportedElement,
                        feature::IMAGE,
                        format!(
                            "embedded raster images are not supported by @rbxts/svg yet (image {:?}).",
                            image.id()
                        ),
                    );
                }
                usvg::Node::Text(text) => {
                    self.report_once_unsupported(
                        DiagnosticCode::UnsupportedElement,
                        feature::TEXT,
                        format!(
                            "text rendering is not supported by @rbxts/svg yet (text {:?}).",
                            text.id()
                        ),
                    );
                }
            }
        }
    }

    /// Group-level effects we cannot reproduce. These duplicate the source scan
    /// for directly authored elements, but catch cases usvg synthesizes.
    fn check_group_effects(&mut self, group: &usvg::Group) {
        if group.clip_path().is_some() {
            self.report_once_unsupported(
                DiagnosticCode::UnsupportedElement,
                feature::CLIP_PATH,
                "clipping paths are not supported by @rbxts/svg yet.".to_string(),
            );
        }
        if group.mask().is_some() {
            self.report_once_unsupported(
                DiagnosticCode::UnsupportedElement,
                feature::MASK,
                "masks are not supported by @rbxts/svg yet.".to_string(),
            );
        }
        if !group.filters().is_empty() {
            self.report_once_unsupported(
                DiagnosticCode::UnsupportedElement,
                feature::FILTER,
                "filter effects are not supported by @rbxts/svg yet.".to_string(),
            );
        }
        if group.blend_mode() != usvg::BlendMode::Normal {
            self.report_once_unsupported(
                DiagnosticCode::UnsupportedBlendMode,
                feature::BLEND_MODE,
                format!(
                    "blend mode `{}` is not supported by @rbxts/svg yet.",
                    group.blend_mode()
                ),
            );
        }
    }

    /// Folding group opacity into each child's paint is exact only when the
    /// children do not overlap. With overlap, true group isolation composites
    /// the group once, whereas we composite each child. Say so rather than
    /// pretending the result is identical.
    fn check_group_opacity(&mut self, group: &usvg::Group, opacity: Opacity) {
        if opacity.is_opaque() {
            return;
        }
        if group.children().len() > 1 {
            self.diagnostics.push(Diagnostic::warning(
                DiagnosticCode::ApproximatedGroupOpacity,
                format!(
                    "group opacity {:.3} was folded into {} children; overlapping shapes will \
                     render slightly differently from an isolated group.",
                    opacity.get(),
                    group.children().len()
                ),
            ));
        }
    }

    fn emit_path(&mut self, path: &usvg::Path, inherited_opacity: Opacity) {
        if !path.is_visible() {
            return;
        }

        // usvg resolves the absolute transform for us; composing it with the
        // view-box correction gives local space -> view box space directly.
        let transform = lower_transform(path.abs_transform()).post_concat(&self.to_view_box);

        let geometry = match lower_path(path.data()) {
            Ok(Some(geometry)) => geometry.transformed(&transform),
            Ok(None) => return,
            Err(e) => {
                self.diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::DroppedEmptyShape,
                    format!("dropped a path with invalid geometry: {e}"),
                ));
                return;
            }
        };

        let fill = path
            .fill()
            .and_then(|f| self.convert_fill(f, inherited_opacity));
        let stroke = path
            .stroke()
            .and_then(|s| self.convert_stroke(s, inherited_opacity, &transform));

        if fill.is_none() && stroke.is_none() {
            return;
        }

        let mut shape = Shape::new(geometry, fill, stroke);
        shape.paint_order = match path.paint_order() {
            usvg::PaintOrder::FillAndStroke => PaintOrder::FillThenStroke,
            usvg::PaintOrder::StrokeAndFill => PaintOrder::StrokeThenFill,
        };
        self.shapes.push(shape);
    }

    fn convert_fill(&mut self, fill: &usvg::Fill, inherited: Opacity) -> Option<Fill> {
        let paint = self.convert_paint(fill.paint(), "fill")?;
        let opacity = Opacity::clamped(fill.opacity().get()).multiply(inherited);
        let rule = match fill.rule() {
            usvg::FillRule::NonZero => FillRule::NonZero,
            usvg::FillRule::EvenOdd => FillRule::EvenOdd,
        };
        Some(Fill::new(paint, opacity, rule))
    }

    fn convert_stroke(
        &mut self,
        stroke: &usvg::Stroke,
        inherited: Opacity,
        transform: &Transform,
    ) -> Option<Stroke> {
        if stroke.dasharray().is_some() {
            self.report_once_unsupported(
                DiagnosticCode::UnsupportedStrokeDash,
                feature::STROKE_DASH,
                "`stroke-dasharray` is not supported by @rbxts/svg yet.".to_string(),
            );
            return None;
        }

        let paint = self.convert_paint(stroke.paint(), "stroke")?;
        let opacity = Opacity::clamped(stroke.opacity().get()).multiply(inherited);

        // A stroke is defined in the shape's own user space, so baking the
        // geometry into view box space means scaling the width to match.
        let scale = transform.length_scale();
        if !transform.is_uniform_scale() && !self.reported_stroke_scale {
            self.reported_stroke_scale = true;
            self.diagnostics.push(Diagnostic::warning(
                DiagnosticCode::ApproximatedStrokeScale,
                "a stroked shape sits under a non-uniform or skewed transform; its stroke width \
                 was approximated with the average scale factor."
                    .to_string(),
            ));
        }
        let width = stroke.width().get() * scale;

        match Stroke::new(
            paint,
            opacity,
            width,
            convert_line_cap(stroke.linecap()),
            convert_line_join(stroke.linejoin()),
            stroke.miterlimit().get(),
        ) {
            Ok(stroke) => Some(stroke),
            Err(e) => {
                // A width that collapses to zero under the transform paints
                // nothing; dropping it is correct, but it should be visible.
                self.diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::DroppedEmptyShape,
                    format!("dropped a stroke that does not paint anything: {e}"),
                ));
                None
            }
        }
    }

    fn convert_paint(&mut self, paint: &usvg::Paint, role: &str) -> Option<Paint> {
        match paint {
            usvg::Paint::Color(color) => {
                if self.sentinel_active && *color == CURRENT_COLOR_SENTINEL {
                    Some(Paint::CurrentColor)
                } else {
                    Some(Paint::Solid(svg_core::Color::rgb(
                        color.red,
                        color.green,
                        color.blue,
                    )))
                }
            }
            usvg::Paint::LinearGradient(_) | usvg::Paint::RadialGradient(_) => {
                self.report_once_unsupported(
                    DiagnosticCode::UnsupportedPaint,
                    feature::GRADIENT,
                    format!("gradient {role}s are not supported by @rbxts/svg yet."),
                );
                None
            }
            usvg::Paint::Pattern(_) => {
                self.report_once_unsupported(
                    DiagnosticCode::UnsupportedPaint,
                    feature::PATTERN,
                    format!("pattern {role}s are not supported by @rbxts/svg yet."),
                );
                None
            }
        }
    }

    /// Records an error diagnostic unless an identical one is already present.
    ///
    /// One `<mask>` applied to a group of forty shapes is one problem, not
    /// forty. The source scan is what provides precise per-element locations;
    /// these tree-level reports exist so nothing slips through unreported.
    fn report_once_unsupported(
        &mut self,
        code: DiagnosticCode,
        feature: &'static str,
        message: String,
    ) {
        let already_reported = self
            .diagnostics
            .iter()
            .any(|d| d.code == code && d.feature == Some(feature));
        if !already_reported {
            self.diagnostics
                .push(Diagnostic::error(code, message).about(feature));
        }
    }
}

fn convert_line_cap(cap: usvg::LineCap) -> LineCap {
    match cap {
        usvg::LineCap::Butt => LineCap::Butt,
        usvg::LineCap::Round => LineCap::Round,
        usvg::LineCap::Square => LineCap::Square,
    }
}

fn convert_line_join(join: usvg::LineJoin) -> LineJoin {
    match join {
        usvg::LineJoin::Miter => LineJoin::Miter,
        // `miter-clip` is an SVG 2 refinement of `miter` that changes behaviour
        // only past the miter limit, where we fall back to bevel anyway.
        usvg::LineJoin::MiterClip => LineJoin::Miter,
        usvg::LineJoin::Round => LineJoin::Round,
        usvg::LineJoin::Bevel => LineJoin::Bevel,
    }
}
