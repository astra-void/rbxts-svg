//! Node.js bindings for the `@rbxts/svg` compiler.
//!
//! # Scope
//!
//! This crate is a *boundary*, not a layer with opinions. It contains no
//! compilation logic and exposes as little as it can get away with:
//!
//! - [`compile_svg`] — source in, serialized IR out;
//! - [`decode_svg_ir`] — IR in, an inspectable structure out, for tooling and
//!   tests;
//! - [`ir_version`] — the format version this binary speaks.
//!
//! The semantic Rust model is deliberately *not* projected into JavaScript.
//! Doing so would make every internal refactor a breaking change for npm
//! consumers, and JS tooling has no use for it: what tooling needs is the
//! compact blob plus enough metadata to cache and route it.

#![deny(clippy::all)]

use napi::bindgen_prelude::{Buffer, Either};
use napi_derive::napi;

use svg_compiler::{CompileError, CompileOptions, Severity, UnsupportedPolicy};

/// A compiler finding, flattened for JavaScript.
#[napi(object)]
#[derive(Debug)]
pub struct JsDiagnostic {
    /// `"error"`, `"warning"` or `"info"`.
    pub severity: String,
    /// Stable kebab-case identifier, e.g. `"unsupported-element"`.
    pub code: String,
    pub message: String,
    /// Tag name of the offending element, when one is known.
    pub tag: Option<String>,
    /// The offending element's `id`.
    pub id: Option<String>,
    /// Ancestor chain, e.g. `"svg > defs > filter#shadow"`.
    pub path: Option<String>,
    /// 1-based source line.
    pub line: Option<u32>,
    /// 1-based source column.
    pub column: Option<u32>,
    /// The full multi-line rendering, ready to print.
    pub rendered: String,
}

fn to_js_diagnostic(
    diagnostic: &svg_compiler::Diagnostic,
    source_name: Option<&str>,
) -> JsDiagnostic {
    let element = diagnostic.element.as_ref();
    JsDiagnostic {
        severity: diagnostic.severity.as_str().to_string(),
        code: diagnostic.code.as_str().to_string(),
        message: diagnostic.message.clone(),
        tag: element.map(|e| e.tag.clone()),
        id: element.and_then(|e| e.id.clone()),
        path: element.map(|e| e.path.clone()),
        line: element.and_then(|e| e.line),
        column: element.and_then(|e| e.column),
        rendered: diagnostic.render(source_name),
    }
}

/// Options accepted by [`compile_svg`].
#[napi(object)]
#[derive(Debug, Default)]
pub struct JsCompileOptions {
    /// Dots per inch for physical units. Defaults to 96.
    pub dpi: Option<f64>,
    /// When true, unsupported rendering features become warnings instead of
    /// failing the compile. Defaults to false.
    pub allow_unsupported: Option<bool>,
    /// A name used to attribute diagnostics, normally the file path. It never
    /// affects the compiled bytes or the hash.
    pub source_name: Option<String>,
}

/// The result of compiling one SVG.
#[napi(object)]
pub struct JsCompiledSvg {
    /// The serialized IR. Treat as opaque: its layout is versioned and owned
    /// by `svg-ir`, and only `@rbxts/svg` should interpret it.
    pub data: Buffer,
    /// View box width in user units — the asset's intrinsic aspect, not a
    /// pixel size.
    pub width: f64,
    /// View box height in user units.
    pub height: f64,
    /// View box origin, non-zero for a shifted coordinate system.
    pub view_box_x: f64,
    pub view_box_y: f64,
    /// The authored `preserveAspectRatio`, normalized: `"none"`, or an
    /// alignment and a scale keyword such as `"xMidYMid meet"`.
    ///
    /// Carried so a renderer can fit the asset into a target rectangle whose
    /// aspect ratio differs from the view box's. Also encoded into `data`.
    pub preserve_aspect_ratio: String,
    /// `svg_core::FeatureFlags` bits.
    pub flags: u32,
    /// Content hash of `data`, as lowercase hex.
    ///
    /// Deterministic and collision-resistant (BLAKE3). Because it hashes the
    /// compiled output rather than the source, two SVGs that differ only in
    /// formatting share a hash, and a compiler change that does not alter the
    /// output does not invalidate caches.
    pub hash: String,
    /// The IR format version `data` is encoded in.
    pub ir_version: u32,
    /// Number of shapes, for tooling and diagnostics.
    pub shape_count: u32,
    /// Non-fatal findings.
    pub diagnostics: Vec<JsDiagnostic>,
}

/// Compiles SVG source into the serialized IR.
///
/// Accepts a UTF-8 string or a `Buffer`. Throws with the compiler's rendered
/// diagnostics when the document uses unsupported rendering features, unless
/// `allowUnsupported` is set.
#[napi]
pub fn compile_svg(
    source: Either<String, Buffer>,
    options: Option<JsCompileOptions>,
) -> napi::Result<JsCompiledSvg> {
    let options = options.unwrap_or_default();
    let source_name = options.source_name.clone();

    let compile_options = CompileOptions {
        dpi: options.dpi.unwrap_or(96.0) as f32,
        unsupported: if options.allow_unsupported.unwrap_or(false) {
            UnsupportedPolicy::Warn
        } else {
            UnsupportedPolicy::Error
        },
        source_name: source_name.clone(),
    };

    let output = match &source {
        Either::A(text) => svg_compiler::compile(text, &compile_options),
        Either::B(bytes) => svg_compiler::compile_bytes(bytes.as_ref(), &compile_options),
    }
    .map_err(|e| to_napi_error(e, source_name.as_deref()))?;

    let data = svg_ir::encode(&output.document)
        .map_err(|e| napi::Error::from_reason(format!("failed to serialize compiled SVG: {e}")))?;
    let hash = blake3::hash(&data).to_hex().to_string();

    let view_box = output.document.view_box;
    Ok(JsCompiledSvg {
        width: view_box.width as f64,
        height: view_box.height as f64,
        view_box_x: view_box.x as f64,
        view_box_y: view_box.y as f64,
        preserve_aspect_ratio: format_aspect_ratio(output.document.preserve_aspect_ratio),
        flags: output.document.features.bits(),
        ir_version: svg_ir::SVG_IR_VERSION as u32,
        shape_count: output.document.shapes.len() as u32,
        diagnostics: output
            .diagnostics
            .iter()
            .filter(|d| d.severity != Severity::Error)
            .map(|d| to_js_diagnostic(d, source_name.as_deref()))
            .collect(),
        data: data.into(),
        hash,
    })
}

/// Renders a fitting policy in SVG's own attribute syntax, so a build log or a
/// generated module header reads the way the source did.
fn format_aspect_ratio(aspect: svg_core::PreserveAspectRatio) -> String {
    use svg_core::{AspectAlign, AspectScale};

    let align = match aspect.align {
        AspectAlign::None => return "none".to_string(),
        AspectAlign::XMinYMin => "xMinYMin",
        AspectAlign::XMidYMin => "xMidYMin",
        AspectAlign::XMaxYMin => "xMaxYMin",
        AspectAlign::XMinYMid => "xMinYMid",
        AspectAlign::XMidYMid => "xMidYMid",
        AspectAlign::XMaxYMid => "xMaxYMid",
        AspectAlign::XMinYMax => "xMinYMax",
        AspectAlign::XMidYMax => "xMidYMax",
        AspectAlign::XMaxYMax => "xMaxYMax",
    };
    let scale = match aspect.scale {
        AspectScale::Meet => "meet",
        AspectScale::Slice => "slice",
    };
    format!("{align} {scale}")
}

/// Turns a compile failure into a JS error whose message is the rendered
/// diagnostic report, so a build tool can print it verbatim.
fn to_napi_error(error: CompileError, source_name: Option<&str>) -> napi::Error {
    napi::Error::from_reason(error.render(source_name))
}

// ---------------------------------------------------------------------------
// Decoding — exposed for tooling and tests.
//
// This exists so the IR format can be asserted on from TypeScript rather than
// only from Rust. It is not on any hot path, and it is not how the Roblox
// runtime reads assets: that decoder is written in Luau against the same
// format specification.
// ---------------------------------------------------------------------------

/// A paint table entry.
#[napi(object)]
#[derive(Debug)]
pub struct JsPaint {
    /// `"solid"` or `"currentColor"`.
    pub kind: String,
    pub r: u32,
    pub g: u32,
    pub b: u32,
    pub alpha: f64,
}

/// One canonical path command.
#[napi(object)]
#[derive(Debug)]
pub struct JsPathCommand {
    /// `"moveTo"`, `"lineTo"`, `"cubicTo"` or `"close"`.
    pub op: String,
    /// Flat `x, y` pairs: 2 values for a move/line, 6 for a cubic, none for a
    /// close.
    pub points: Vec<f64>,
}

/// A decoded shape.
#[napi(object)]
#[derive(Debug)]
pub struct JsShape {
    pub fill: Option<JsPaint>,
    /// `"nonzero"` or `"evenodd"`; present only when there is a fill.
    pub fill_rule: Option<String>,
    pub stroke: Option<JsPaint>,
    pub stroke_width: Option<f64>,
    /// `"butt"`, `"round"` or `"square"`.
    pub line_cap: Option<String>,
    /// `"miter"`, `"round"` or `"bevel"`.
    pub line_join: Option<String>,
    pub miter_limit: Option<f64>,
    /// `"fillThenStroke"` or `"strokeThenFill"`.
    pub paint_order: String,
    pub commands: Vec<JsPathCommand>,
}

/// A fully decoded asset.
#[napi(object)]
#[derive(Debug)]
pub struct JsDecodedSvg {
    pub ir_version: u32,
    pub view_box_x: f64,
    pub view_box_y: f64,
    pub width: f64,
    pub height: f64,
    /// `"none"`, or an alignment and scale keyword such as `"xMidYMid meet"`.
    pub preserve_aspect_ratio: String,
    pub flags: u32,
    pub shapes: Vec<JsShape>,
}

/// Decodes serialized IR back into an inspectable structure.
#[napi]
pub fn decode_svg_ir(data: Buffer) -> napi::Result<JsDecodedSvg> {
    let document = svg_ir::decode(data.as_ref())
        .map_err(|e| napi::Error::from_reason(format!("failed to decode SVG IR: {e}")))?;

    let view_box = document.view_box;
    Ok(JsDecodedSvg {
        ir_version: svg_ir::SVG_IR_VERSION as u32,
        view_box_x: view_box.x as f64,
        view_box_y: view_box.y as f64,
        width: view_box.width as f64,
        height: view_box.height as f64,
        preserve_aspect_ratio: format_aspect_ratio(document.preserve_aspect_ratio),
        flags: document.features.bits(),
        shapes: document.shapes.iter().map(to_js_shape).collect(),
    })
}

fn to_js_shape(shape: &svg_core::Shape) -> JsShape {
    JsShape {
        fill: shape.fill.map(|f| to_js_paint(f.paint, f.opacity)),
        fill_rule: shape.fill.map(|f| {
            match f.rule {
                svg_core::FillRule::NonZero => "nonzero",
                svg_core::FillRule::EvenOdd => "evenodd",
            }
            .to_string()
        }),
        stroke: shape.stroke.map(|s| to_js_paint(s.paint, s.opacity)),
        stroke_width: shape.stroke.map(|s| s.width as f64),
        line_cap: shape.stroke.map(|s| {
            match s.line_cap {
                svg_core::LineCap::Butt => "butt",
                svg_core::LineCap::Round => "round",
                svg_core::LineCap::Square => "square",
            }
            .to_string()
        }),
        line_join: shape.stroke.map(|s| {
            match s.line_join {
                svg_core::LineJoin::Miter => "miter",
                svg_core::LineJoin::Round => "round",
                svg_core::LineJoin::Bevel => "bevel",
            }
            .to_string()
        }),
        miter_limit: shape.stroke.map(|s| s.miter_limit as f64),
        paint_order: match shape.paint_order {
            svg_core::PaintOrder::FillThenStroke => "fillThenStroke",
            svg_core::PaintOrder::StrokeThenFill => "strokeThenFill",
        }
        .to_string(),
        commands: shape
            .geometry
            .commands()
            .iter()
            .map(to_js_command)
            .collect(),
    }
}

fn to_js_paint(paint: svg_core::Paint, opacity: svg_core::Opacity) -> JsPaint {
    let (kind, color) = match paint {
        svg_core::Paint::CurrentColor => ("currentColor", svg_core::Color::BLACK),
        svg_core::Paint::Solid(c) => ("solid", c),
    };
    JsPaint {
        kind: kind.to_string(),
        r: color.r as u32,
        g: color.g as u32,
        b: color.b as u32,
        alpha: opacity.get() as f64,
    }
}

fn to_js_command(command: &svg_core::PathCommand) -> JsPathCommand {
    use svg_core::PathCommand as C;
    let (op, points) = match *command {
        C::MoveTo(p) => ("moveTo", vec![p.x as f64, p.y as f64]),
        C::LineTo(p) => ("lineTo", vec![p.x as f64, p.y as f64]),
        C::CubicTo(a, b, c) => (
            "cubicTo",
            vec![
                a.x as f64, a.y as f64, b.x as f64, b.y as f64, c.x as f64, c.y as f64,
            ],
        ),
        C::Close => ("close", Vec::new()),
    };
    JsPathCommand {
        op: op.to_string(),
        points,
    }
}

/// The serialized IR format version this binary reads and writes.
///
/// Build tooling compares this against the version recorded in its cache to
/// decide whether previously generated assets are still valid.
#[napi]
pub fn ir_version() -> u32 {
    svg_ir::SVG_IR_VERSION as u32
}

// ---------------------------------------------------------------------------
// Reference rasterization — exposed for golden fixtures and tooling.
//
// This is `svg-raster` behind the boundary: the executable specification the
// Luau renderer is compared against. The Luau test bundler calls it to embed
// expected RGBA output next to each compiled fixture, which is what makes the
// cross-language comparison a generated artifact rather than bytes maintained
// by hand.
// ---------------------------------------------------------------------------

/// Options accepted by [`render_svg_ir`]. Mirrors `svg_raster::RasterOptions`.
#[napi(object)]
#[derive(Debug, Default)]
pub struct JsRasterOptions {
    /// Produce a white-RGB alpha mask instead of full colour.
    pub alpha_mask: Option<bool>,
    /// `[r, g, b]`, each 0-255: what `currentColor` paints resolve to.
    /// Defaults to black, CSS's initial `color`.
    pub current_color: Option<Vec<u32>>,
    /// Replaces every shape's stroke width.
    pub stroke_width: Option<f64>,
    /// Interpret `strokeWidth` as output pixels rather than view box units.
    pub absolute_stroke_width: Option<bool>,
}

/// A reference-rendered image: straight RGBA8, row-major from the top-left.
#[napi(object)]
pub struct JsRasterImage {
    pub width: u32,
    pub height: u32,
    /// Exactly `width * height * 4` bytes.
    pub pixels: Buffer,
}

/// Renders serialized IR through the reference rasterizer.
#[napi]
pub fn render_svg_ir(
    data: Buffer,
    width: u32,
    height: u32,
    options: Option<JsRasterOptions>,
) -> napi::Result<JsRasterImage> {
    let document = svg_ir::decode(data.as_ref())
        .map_err(|e| napi::Error::from_reason(format!("failed to decode SVG IR: {e}")))?;

    let options = options.unwrap_or_default();
    let mut raster_options = svg_raster::RasterOptions::new(width, height);
    if options.alpha_mask.unwrap_or(false) {
        raster_options = raster_options.with_mode(svg_raster::RasterMode::AlphaMask);
    }
    if let Some(channels) = options.current_color {
        if channels.len() != 3 || channels.iter().any(|&c| c > 255) {
            return Err(napi::Error::from_reason(
                "currentColor must be [r, g, b] with each channel in 0..=255".to_string(),
            ));
        }
        raster_options = raster_options.with_current_color(svg_core::Color::rgb(
            channels[0] as u8,
            channels[1] as u8,
            channels[2] as u8,
        ));
    }
    if let Some(stroke_width) = options.stroke_width {
        raster_options = if options.absolute_stroke_width.unwrap_or(false) {
            raster_options.with_absolute_stroke_width(stroke_width as f32)
        } else {
            raster_options.with_stroke_width(stroke_width as f32)
        };
    }

    let image = svg_raster::render(&document, &raster_options)
        .map_err(|e| napi::Error::from_reason(format!("failed to rasterize SVG IR: {e}")))?;

    Ok(JsRasterImage {
        width: image.width,
        height: image.height,
        pixels: image.pixels.into(),
    })
}
