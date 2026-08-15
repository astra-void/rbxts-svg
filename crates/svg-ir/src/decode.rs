//! Serialized IR → semantic document.
//!
//! This decoder is the executable specification of [`crate::format`]. The Luau
//! runtime decoder and any future DOM/Loom decoder must agree with it exactly,
//! and the round-trip tests here are what keep the format honest.
//!
//! Every read is bounds-checked. The input may come from a stale build cache or
//! a corrupted file, so malformed data must produce a structured error, never a
//! panic.

use std::fmt;

use svg_core::{
    AspectAlign, AspectScale, Color, FeatureFlags, Fill, FillRule, LineCap, LineJoin, Opacity,
    Paint, PaintOrder, Path, PathCommand, Point, PreserveAspectRatio, Shape, Stroke, SvgDocument,
    ViewBox,
};

use crate::format::{
    HEADER_SIZE, MAGIC, PAINT_ENTRY_SIZE, SHAPE_ENTRY_SIZE, SVG_IR_VERSION, aspect_align,
    aspect_scale, line_cap, line_join, paint_kind, shape_flags,
};
use crate::opcode;

/// Why a byte stream could not be interpreted as compiled IR.
#[derive(Debug, Clone, PartialEq)]
pub enum DecodeError {
    /// The magic bytes are wrong: this is not compiled IR at all.
    InvalidMagic { found: [u8; 4] },
    /// Compiled by a different version of the format.
    UnsupportedVersion { found: u16, supported: u16 },
    /// The stream ended before a field that the header said would be there.
    UnexpectedEnd {
        offset: usize,
        needed: usize,
        len: usize,
    },
    /// An enum field held a value this version does not define.
    InvalidEnum { field: &'static str, value: u8 },
    /// A command stream byte was not a known opcode.
    InvalidOpcode { offset: usize, value: u8 },
    /// A shape referenced a paint table slot that does not exist.
    PaintIndexOutOfRange { index: u16, count: u16 },
    /// A shape's declared command range does not lie inside the stream.
    CommandRangeOutOfRange { offset: u32, count: u32 },
    /// A decoded value violated a semantic invariant (degenerate view box,
    /// out-of-range opacity, a path not starting with `MoveTo`, ...).
    InvalidValue(svg_core::CoreError),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic { found } => write!(
                f,
                "not a compiled SVG IR blob: expected magic {:?}, found {found:?}",
                MAGIC
            ),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "IR version {found} is not supported by this decoder (supports {supported}); recompile the asset"
            ),
            Self::UnexpectedEnd {
                offset,
                needed,
                len,
            } => write!(
                f,
                "truncated IR: needed {needed} bytes at offset {offset}, but the blob is {len} bytes"
            ),
            Self::InvalidEnum { field, value } => {
                write!(f, "invalid {field} discriminant {value}")
            }
            Self::InvalidOpcode { offset, value } => {
                write!(
                    f,
                    "invalid opcode {value} at command stream offset {offset}"
                )
            }
            Self::PaintIndexOutOfRange { index, count } => {
                write!(
                    f,
                    "paint index {index} out of range (table has {count} entries)"
                )
            }
            Self::CommandRangeOutOfRange { offset, count } => write!(
                f,
                "shape command range (offset {offset}, count {count}) lies outside the command stream"
            ),
            Self::InvalidValue(e) => write!(f, "IR contains an invalid value: {e}"),
        }
    }
}

impl core::error::Error for DecodeError {}

impl From<svg_core::CoreError> for DecodeError {
    fn from(e: svg_core::CoreError) -> Self {
        Self::InvalidValue(e)
    }
}

/// A bounds-checked little-endian cursor.
struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self
            .offset
            .checked_add(n)
            .ok_or(DecodeError::UnexpectedEnd {
                offset: self.offset,
                needed: n,
                len: self.bytes.len(),
            })?;
        if end > self.bytes.len() {
            return Err(DecodeError::UnexpectedEnd {
                offset: self.offset,
                needed: n,
                len: self.bytes.len(),
            });
        }
        let slice = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, DecodeError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f32(&mut self) -> Result<f32, DecodeError> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn point(&mut self) -> Result<Point, DecodeError> {
        let x = self.f32()?;
        let y = self.f32()?;
        Ok(Point::new(x, y).validate()?)
    }
}

/// Header fields, readable without decoding the body.
///
/// Tooling uses this to answer "how big is this asset and is it tintable?"
/// without paying for a full decode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IrHeader {
    pub version: u16,
    pub features: FeatureFlags,
    pub view_box: ViewBox,
    pub preserve_aspect_ratio: PreserveAspectRatio,
    pub paint_count: u16,
    pub shape_count: u16,
}

/// Reads and validates just the header.
pub fn decode_header(bytes: &[u8]) -> Result<IrHeader, DecodeError> {
    let mut r = Reader::new(bytes);

    let magic = r.take(4)?;
    if magic != MAGIC {
        return Err(DecodeError::InvalidMagic {
            found: [magic[0], magic[1], magic[2], magic[3]],
        });
    }

    let version = r.u16()?;
    if version != SVG_IR_VERSION {
        return Err(DecodeError::UnsupportedVersion {
            found: version,
            supported: SVG_IR_VERSION,
        });
    }

    let _header_size = r.u16()?;
    // `FeatureFlags::from_bits_retain` keeps bits this build does not know
    // about. Within a single version that cannot happen, but retaining rather
    // than discarding means a forward-compatible reader never silently drops
    // information it merely does not interpret.
    let features = FeatureFlags::from_bits_retain(r.u32()?);

    let x = r.f32()?;
    let y = r.f32()?;
    let width = r.f32()?;
    let height = r.f32()?;
    let view_box = ViewBox::new(x, y, width, height)?;

    let paint_count = r.u16()?;
    let shape_count = r.u16()?;

    let preserve_aspect_ratio =
        PreserveAspectRatio::new(decode_aspect_align(r.u8()?)?, decode_aspect_scale(r.u8()?)?);
    // The two trailing bytes are reserved and must read as zero. They are not
    // validated: a future version may assign them, and a decoder that rejected
    // a non-zero value here would fail for the wrong reason.
    let _reserved = r.take(2)?;

    Ok(IrHeader {
        version,
        features,
        view_box,
        preserve_aspect_ratio,
        paint_count,
        shape_count,
    })
}

fn decode_aspect_align(value: u8) -> Result<AspectAlign, DecodeError> {
    match value {
        aspect_align::NONE => Ok(AspectAlign::None),
        aspect_align::X_MIN_Y_MIN => Ok(AspectAlign::XMinYMin),
        aspect_align::X_MID_Y_MIN => Ok(AspectAlign::XMidYMin),
        aspect_align::X_MAX_Y_MIN => Ok(AspectAlign::XMaxYMin),
        aspect_align::X_MIN_Y_MID => Ok(AspectAlign::XMinYMid),
        aspect_align::X_MID_Y_MID => Ok(AspectAlign::XMidYMid),
        aspect_align::X_MAX_Y_MID => Ok(AspectAlign::XMaxYMid),
        aspect_align::X_MIN_Y_MAX => Ok(AspectAlign::XMinYMax),
        aspect_align::X_MID_Y_MAX => Ok(AspectAlign::XMidYMax),
        aspect_align::X_MAX_Y_MAX => Ok(AspectAlign::XMaxYMax),
        value => Err(DecodeError::InvalidEnum {
            field: "aspect align",
            value,
        }),
    }
}

fn decode_aspect_scale(value: u8) -> Result<AspectScale, DecodeError> {
    match value {
        aspect_scale::MEET => Ok(AspectScale::Meet),
        aspect_scale::SLICE => Ok(AspectScale::Slice),
        value => Err(DecodeError::InvalidEnum {
            field: "aspect scale",
            value,
        }),
    }
}

/// A resolved paint table entry: paint plus its alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PaintEntry {
    paint: Paint,
    opacity: Opacity,
}

/// Fully decodes a blob back into the semantic model.
///
/// `decode(encode(doc)) == doc` for every document the encoder accepts; see the
/// round-trip tests in `crates/svg-ir/tests/`.
pub fn decode(bytes: &[u8]) -> Result<SvgDocument, DecodeError> {
    let header = decode_header(bytes)?;

    let paint_table_start = HEADER_SIZE;
    let shape_table_start = paint_table_start + header.paint_count as usize * PAINT_ENTRY_SIZE;
    let command_stream_start = shape_table_start + header.shape_count as usize * SHAPE_ENTRY_SIZE;

    if command_stream_start > bytes.len() {
        return Err(DecodeError::UnexpectedEnd {
            offset: bytes.len(),
            needed: command_stream_start - bytes.len(),
            len: bytes.len(),
        });
    }
    let command_stream = &bytes[command_stream_start..];

    let paints = decode_paint_table(
        &bytes[paint_table_start..shape_table_start],
        header.paint_count,
    )?;

    let mut shapes = Vec::with_capacity(header.shape_count as usize);
    let mut shape_reader = Reader::new(&bytes[shape_table_start..command_stream_start]);
    for _ in 0..header.shape_count {
        shapes.push(decode_shape(&mut shape_reader, &paints, command_stream)?);
    }

    Ok(SvgDocument::new(header.view_box, shapes, header.features)
        .with_preserve_aspect_ratio(header.preserve_aspect_ratio))
}

fn decode_paint_table(bytes: &[u8], count: u16) -> Result<Vec<PaintEntry>, DecodeError> {
    let mut r = Reader::new(bytes);
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let kind = r.u8()?;
        let red = r.u8()?;
        let green = r.u8()?;
        let blue = r.u8()?;
        let alpha = r.f32()?;

        let paint = match kind {
            paint_kind::SOLID => Paint::Solid(Color::rgb(red, green, blue)),
            paint_kind::CURRENT_COLOR => Paint::CurrentColor,
            value => {
                return Err(DecodeError::InvalidEnum {
                    field: "paint kind",
                    value,
                });
            }
        };

        out.push(PaintEntry {
            paint,
            opacity: Opacity::new(alpha)?,
        });
    }
    Ok(out)
}

fn decode_shape(
    r: &mut Reader<'_>,
    paints: &[PaintEntry],
    command_stream: &[u8],
) -> Result<Shape, DecodeError> {
    let flags = r.u8()?;
    let cap = decode_line_cap(r.u8()?)?;
    let join = decode_line_join(r.u8()?)?;
    let _reserved = r.u8()?;
    let fill_index = r.u16()?;
    let stroke_index = r.u16()?;
    let stroke_width = r.f32()?;
    let miter_limit = r.f32()?;
    let command_offset = r.u32()?;
    let command_count = r.u32()?;

    let fill = if flags & shape_flags::HAS_FILL != 0 {
        let entry = paint_at(paints, fill_index)?;
        let rule = if flags & shape_flags::FILL_RULE_EVEN_ODD != 0 {
            FillRule::EvenOdd
        } else {
            FillRule::NonZero
        };
        Some(Fill::new(entry.paint, entry.opacity, rule))
    } else {
        None
    };

    let stroke = if flags & shape_flags::HAS_STROKE != 0 {
        let entry = paint_at(paints, stroke_index)?;
        Some(Stroke::new(
            entry.paint,
            entry.opacity,
            stroke_width,
            cap,
            join,
            miter_limit,
        )?)
    } else {
        None
    };

    let geometry = decode_commands(command_stream, command_offset, command_count)?;

    let paint_order = if flags & shape_flags::STROKE_FIRST != 0 {
        PaintOrder::StrokeThenFill
    } else {
        PaintOrder::FillThenStroke
    };

    Ok(Shape {
        geometry,
        fill,
        stroke,
        paint_order,
    })
}

fn paint_at(paints: &[PaintEntry], index: u16) -> Result<PaintEntry, DecodeError> {
    paints
        .get(index as usize)
        .copied()
        .ok_or(DecodeError::PaintIndexOutOfRange {
            index,
            count: paints.len() as u16,
        })
}

fn decode_line_cap(value: u8) -> Result<LineCap, DecodeError> {
    match value {
        line_cap::BUTT => Ok(LineCap::Butt),
        line_cap::ROUND => Ok(LineCap::Round),
        line_cap::SQUARE => Ok(LineCap::Square),
        value => Err(DecodeError::InvalidEnum {
            field: "line cap",
            value,
        }),
    }
}

fn decode_line_join(value: u8) -> Result<LineJoin, DecodeError> {
    match value {
        line_join::MITER => Ok(LineJoin::Miter),
        line_join::ROUND => Ok(LineJoin::Round),
        line_join::BEVEL => Ok(LineJoin::Bevel),
        value => Err(DecodeError::InvalidEnum {
            field: "line join",
            value,
        }),
    }
}

fn decode_commands(command_stream: &[u8], offset: u32, count: u32) -> Result<Path, DecodeError> {
    let start = offset as usize;
    if start > command_stream.len() {
        return Err(DecodeError::CommandRangeOutOfRange { offset, count });
    }

    let mut r = Reader::new(&command_stream[start..]);
    let mut commands = Vec::with_capacity(count as usize);

    for _ in 0..count {
        let op_offset = start + r.offset;
        let op = r
            .u8()
            .map_err(|_| DecodeError::CommandRangeOutOfRange { offset, count })?;
        let command = match op {
            opcode::MOVE_TO => PathCommand::MoveTo(r.point()?),
            opcode::LINE_TO => PathCommand::LineTo(r.point()?),
            opcode::CUBIC_TO => {
                let a = r.point()?;
                let b = r.point()?;
                let c = r.point()?;
                PathCommand::CubicTo(a, b, c)
            }
            opcode::CLOSE => PathCommand::Close,
            value => {
                return Err(DecodeError::InvalidOpcode {
                    offset: op_offset,
                    value,
                });
            }
        };
        commands.push(command);
    }

    Ok(Path::from_commands(commands)?)
}
