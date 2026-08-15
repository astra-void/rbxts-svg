//! Semantic document → serialized IR.
//!
//! Encoding is *deterministic*: the same [`SvgDocument`] always produces
//! byte-identical output on every machine and in every process. The only place
//! that could go wrong is the paint table, so it is built with a `BTreeMap`
//! keyed on exact bit patterns and ordered by first use, never by hash order.

use std::collections::BTreeMap;
use std::fmt;

use svg_core::{
    AspectAlign, AspectScale, Fill, LineCap, LineJoin, Opacity, Paint, PaintOrder, PathCommand,
    Shape, Stroke, SvgDocument,
};

use crate::format::{
    HEADER_SIZE, MAGIC, MAX_TABLE_ENTRIES, PAINT_ENTRY_SIZE, SHAPE_ENTRY_SIZE, SVG_IR_VERSION,
    aspect_align, aspect_scale, line_cap, line_join, paint_kind, shape_flags,
};
use crate::opcode;

/// Why a document could not be serialized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// More shapes than the `u16` count field can address.
    TooManyShapes(usize),
    /// More distinct paints than the `u16` count field can address.
    TooManyPaints(usize),
    /// The command stream exceeded the `u32` offset space (4 GiB).
    CommandStreamTooLarge(usize),
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyShapes(n) => write!(
                f,
                "document has {n} shapes, the IR format supports at most {MAX_TABLE_ENTRIES}"
            ),
            Self::TooManyPaints(n) => write!(
                f,
                "document has {n} distinct paints, the IR format supports at most {MAX_TABLE_ENTRIES}"
            ),
            Self::CommandStreamTooLarge(n) => {
                write!(
                    f,
                    "command stream is {n} bytes, which exceeds the u32 offset space"
                )
            }
        }
    }
}

impl core::error::Error for EncodeError {}

/// The exact bits of a paint plus its alpha, used as a dedup key.
///
/// Alpha is compared by bit pattern rather than by value so that encoding is a
/// pure function of the input. (`f32` equality would merge `-0.0` and `0.0`,
/// which is harmless, but bit equality is easier to reason about and is what
/// makes "same document ⇒ same bytes" trivially true.)
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct PaintKey {
    kind: u8,
    r: u8,
    g: u8,
    b: u8,
    alpha_bits: u32,
}

/// Interns paints, assigning indices in order of first use.
#[derive(Default)]
struct PaintTable {
    /// Maps a paint to its index. `BTreeMap` rather than `HashMap`: iteration
    /// order never influences the output, but using an ordered map removes the
    /// question entirely.
    indices: BTreeMap<PaintKey, u16>,
    entries: Vec<[u8; PAINT_ENTRY_SIZE]>,
}

impl PaintTable {
    fn intern(&mut self, paint: Paint, opacity: Opacity) -> Result<u16, EncodeError> {
        let (kind, r, g, b) = match paint {
            Paint::CurrentColor => (paint_kind::CURRENT_COLOR, 0, 0, 0),
            Paint::Solid(c) => (paint_kind::SOLID, c.r, c.g, c.b),
        };
        let alpha = opacity.get();
        let key = PaintKey {
            kind,
            r,
            g,
            b,
            alpha_bits: alpha.to_bits(),
        };

        if let Some(&index) = self.indices.get(&key) {
            return Ok(index);
        }

        if self.entries.len() >= MAX_TABLE_ENTRIES {
            return Err(EncodeError::TooManyPaints(self.entries.len() + 1));
        }

        let index = self.entries.len() as u16;
        let mut entry = [0u8; PAINT_ENTRY_SIZE];
        entry[0] = kind;
        entry[1] = r;
        entry[2] = g;
        entry[3] = b;
        entry[4..8].copy_from_slice(&alpha.to_le_bytes());
        self.entries.push(entry);
        self.indices.insert(key, index);
        Ok(index)
    }
}

const fn line_cap_byte(cap: LineCap) -> u8 {
    match cap {
        LineCap::Butt => line_cap::BUTT,
        LineCap::Round => line_cap::ROUND,
        LineCap::Square => line_cap::SQUARE,
    }
}

const fn aspect_align_byte(align: AspectAlign) -> u8 {
    match align {
        AspectAlign::None => aspect_align::NONE,
        AspectAlign::XMinYMin => aspect_align::X_MIN_Y_MIN,
        AspectAlign::XMidYMin => aspect_align::X_MID_Y_MIN,
        AspectAlign::XMaxYMin => aspect_align::X_MAX_Y_MIN,
        AspectAlign::XMinYMid => aspect_align::X_MIN_Y_MID,
        AspectAlign::XMidYMid => aspect_align::X_MID_Y_MID,
        AspectAlign::XMaxYMid => aspect_align::X_MAX_Y_MID,
        AspectAlign::XMinYMax => aspect_align::X_MIN_Y_MAX,
        AspectAlign::XMidYMax => aspect_align::X_MID_Y_MAX,
        AspectAlign::XMaxYMax => aspect_align::X_MAX_Y_MAX,
    }
}

const fn aspect_scale_byte(scale: AspectScale) -> u8 {
    match scale {
        AspectScale::Meet => aspect_scale::MEET,
        AspectScale::Slice => aspect_scale::SLICE,
    }
}

const fn line_join_byte(join: LineJoin) -> u8 {
    match join {
        LineJoin::Miter => line_join::MITER,
        LineJoin::Round => line_join::ROUND,
        LineJoin::Bevel => line_join::BEVEL,
    }
}

/// Serializes a document into the versioned IR.
pub fn encode(document: &SvgDocument) -> Result<Vec<u8>, EncodeError> {
    let shape_count = document.shapes.len();
    if shape_count > MAX_TABLE_ENTRIES {
        return Err(EncodeError::TooManyShapes(shape_count));
    }

    let mut paints = PaintTable::default();
    let mut shape_entries: Vec<[u8; SHAPE_ENTRY_SIZE]> = Vec::with_capacity(shape_count);
    let mut commands: Vec<u8> = Vec::with_capacity(document.command_count() * 9);

    for shape in &document.shapes {
        let command_offset = commands.len();
        if command_offset > u32::MAX as usize {
            return Err(EncodeError::CommandStreamTooLarge(command_offset));
        }
        encode_commands(shape.geometry.commands(), &mut commands);

        shape_entries.push(encode_shape(
            shape,
            &mut paints,
            command_offset as u32,
            shape.geometry.commands().len() as u32,
        )?);
    }

    if commands.len() > u32::MAX as usize {
        return Err(EncodeError::CommandStreamTooLarge(commands.len()));
    }

    let paint_count = paints.entries.len();
    let mut out = Vec::with_capacity(
        HEADER_SIZE
            + paint_count * PAINT_ENTRY_SIZE
            + shape_count * SHAPE_ENTRY_SIZE
            + commands.len(),
    );

    // ---- header
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&SVG_IR_VERSION.to_le_bytes());
    out.extend_from_slice(&(HEADER_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&document.features.bits().to_le_bytes());
    out.extend_from_slice(&document.view_box.x.to_le_bytes());
    out.extend_from_slice(&document.view_box.y.to_le_bytes());
    out.extend_from_slice(&document.view_box.width.to_le_bytes());
    out.extend_from_slice(&document.view_box.height.to_le_bytes());
    out.extend_from_slice(&(paint_count as u16).to_le_bytes());
    out.extend_from_slice(&(shape_count as u16).to_le_bytes());
    out.push(aspect_align_byte(document.preserve_aspect_ratio.align));
    out.push(aspect_scale_byte(document.preserve_aspect_ratio.scale));
    out.push(0); // reserved
    out.push(0); // reserved
    debug_assert_eq!(out.len(), HEADER_SIZE);

    // ---- tables and command stream
    for entry in &paints.entries {
        out.extend_from_slice(entry);
    }
    for entry in &shape_entries {
        out.extend_from_slice(entry);
    }
    out.extend_from_slice(&commands);

    Ok(out)
}

fn encode_shape(
    shape: &Shape,
    paints: &mut PaintTable,
    command_offset: u32,
    command_count: u32,
) -> Result<[u8; SHAPE_ENTRY_SIZE], EncodeError> {
    let mut flags = 0u8;
    let mut fill_index = 0u16;
    let mut stroke_index = 0u16;
    let mut stroke_width = 0f32;
    let mut miter_limit = 0f32;
    let mut cap = line_cap::BUTT;
    let mut join = line_join::MITER;

    if let Some(Fill {
        paint,
        opacity,
        rule,
    }) = shape.fill
    {
        flags |= shape_flags::HAS_FILL;
        if rule == svg_core::FillRule::EvenOdd {
            flags |= shape_flags::FILL_RULE_EVEN_ODD;
        }
        fill_index = paints.intern(paint, opacity)?;
    }

    if let Some(Stroke {
        paint,
        opacity,
        width,
        line_cap,
        line_join,
        miter_limit: ml,
    }) = shape.stroke
    {
        flags |= shape_flags::HAS_STROKE;
        stroke_index = paints.intern(paint, opacity)?;
        stroke_width = width;
        miter_limit = ml;
        cap = line_cap_byte(line_cap);
        join = line_join_byte(line_join);
    }

    if shape.paint_order == PaintOrder::StrokeThenFill {
        flags |= shape_flags::STROKE_FIRST;
    }

    let mut entry = [0u8; SHAPE_ENTRY_SIZE];
    entry[0] = flags;
    entry[1] = cap;
    entry[2] = join;
    entry[3] = 0; // reserved
    entry[4..6].copy_from_slice(&fill_index.to_le_bytes());
    entry[6..8].copy_from_slice(&stroke_index.to_le_bytes());
    entry[8..12].copy_from_slice(&stroke_width.to_le_bytes());
    entry[12..16].copy_from_slice(&miter_limit.to_le_bytes());
    entry[16..20].copy_from_slice(&command_offset.to_le_bytes());
    entry[20..24].copy_from_slice(&command_count.to_le_bytes());
    Ok(entry)
}

fn encode_commands(commands: &[PathCommand], out: &mut Vec<u8>) {
    fn push_point(out: &mut Vec<u8>, p: svg_core::Point) {
        out.extend_from_slice(&p.x.to_le_bytes());
        out.extend_from_slice(&p.y.to_le_bytes());
    }

    for command in commands {
        out.push(opcode::opcode_of(command));
        match *command {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => push_point(out, p),
            PathCommand::CubicTo(a, b, c) => {
                push_point(out, a);
                push_point(out, b);
                push_point(out, c);
            }
            PathCommand::Close => {}
        }
    }
}
