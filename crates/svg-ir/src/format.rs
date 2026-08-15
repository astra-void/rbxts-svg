//! The on-the-wire layout of a compiled asset.
//!
//! # Why this is not just "serialize the Rust structs"
//!
//! The semantic model in `svg-core` is shaped for the compiler's convenience.
//! This format is shaped for the *decoder's* convenience, and the decoder that
//! matters is written in Luau and runs inside Roblox. That means:
//!
//! - Everything is little-endian, because Luau's `buffer.readu32` /
//!   `buffer.readf32` are little-endian. No byte swapping on the hot path.
//! - Fixed-size table entries, so element `i` is at `base + i * STRIDE`. No
//!   pointer chasing, no nested object graphs to allocate.
//! - Counts and offsets are up front, so a decoder can validate bounds once
//!   instead of per element.
//!
//! # Layout (version 2)
//!
//! All integers little-endian. All floats IEEE-754 binary32, little-endian.
//!
//! ```text
//! ┌─ header ───────────────────────────────────────── 36 bytes ─┐
//! │  0  [4]  magic            "RSVG" (0x52 0x53 0x56 0x47)      │
//! │  4  u16  version          SVG_IR_VERSION                    │
//! │  6  u16  header_size      byte offset of the paint table    │
//! │  8  u32  feature_flags    svg_core::FeatureFlags bits       │
//! │ 12  f32  view_box.x                                         │
//! │ 16  f32  view_box.y                                         │
//! │ 20  f32  view_box.width   > 0                               │
//! │ 24  f32  view_box.height  > 0                               │
//! │ 28  u16  paint_count                                        │
//! │ 30  u16  shape_count                                        │
//! │ 32  u8   aspect_align     see `aspect_align`                │
//! │ 33  u8   aspect_scale     0 = meet, 1 = slice               │
//! │ 34  u8   reserved         0                                 │
//! │ 35  u8   reserved         0                                 │
//! ├─ paint table ──────────────── paint_count × 8 bytes ────────┤
//! │  0  u8   kind             0 = solid, 1 = currentColor       │
//! │  1  u8   r                                                  │
//! │  2  u8   g                0 for currentColor                │
//! │  3  u8   b                                                  │
//! │  4  f32  alpha            0.0..=1.0                         │
//! ├─ shape table ─────────────── shape_count × 24 bytes ────────┤
//! │  0  u8   flags            see `shape_flags`                 │
//! │  1  u8   line_cap         0 butt, 1 round, 2 square         │
//! │  2  u8   line_join        0 miter, 1 round, 2 bevel         │
//! │  3  u8   reserved         0                                 │
//! │  4  u16  fill_paint       index into the paint table        │
//! │  6  u16  stroke_paint     index into the paint table        │
//! │  8  f32  stroke_width     view box units                    │
//! │ 12  f32  miter_limit      >= 1                              │
//! │ 16  u32  command_offset   byte offset into the command      │
//! │                           stream, relative to its start     │
//! │ 20  u32  command_count    number of commands, not bytes     │
//! ├─ command stream ─────────────────────── variable length ────┤
//! │  u8 opcode, then its operands (see `opcode`)                │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! Shapes appear in painter's order. A shape's commands are contiguous, and
//! shapes' command ranges appear in the same order as the shapes themselves,
//! so a decoder may also stream the whole thing front to back.
//!
//! # Versioning
//!
//! [`SVG_IR_VERSION`] is bumped for *any* change a version-N decoder would
//! misread: new fields, changed strides, a different coordinate encoding
//! (fixed-point, varint deltas), or renumbered enum values. Decoders reject
//! versions they do not implement rather than guessing. Adding a new
//! [`crate::opcode`] value or a new paint `kind` is also a version bump, since
//! an old decoder cannot skip an operand list it does not know the size of.
//!
//! ## Version history
//!
//! | Version | Change |
//! | --- | --- |
//! | 1 | Initial format. 32-byte header. |
//! | 2 | `preserveAspectRatio` added; header grew to 36 bytes. |
//!
//! Version 2 grew the header rather than borrowing spare bits, because there
//! were none: bytes 0..32 were fully assigned in version 1. Growing it moves
//! the paint table, which a version-1 decoder computes as a constant, so the
//! change is unambiguously breaking and the version reflects that. The two
//! trailing bytes *are* reserved, and a future field that fits in them will
//! still need a version bump for the same reason — they are read as zero today
//! and nothing may quietly start meaning something else.

/// Format magic: ASCII `RSVG`, for "Roblox SVG".
pub const MAGIC: [u8; 4] = *b"RSVG";

/// The format version this crate reads and writes.
pub const SVG_IR_VERSION: u16 = 2;

/// Size of the fixed header, and therefore the offset of the paint table.
pub const HEADER_SIZE: usize = 36;

/// Bytes per paint table entry.
pub const PAINT_ENTRY_SIZE: usize = 8;

/// Bytes per shape table entry.
pub const SHAPE_ENTRY_SIZE: usize = 24;

/// A paint table entry's `kind` discriminant.
///
/// Values are part of the format. Gradients will take 2 and 3.
pub mod paint_kind {
    pub const SOLID: u8 = 0;
    pub const CURRENT_COLOR: u8 = 1;
}

/// Bits of a shape entry's `flags` byte.
pub mod shape_flags {
    /// The shape has a fill; `fill_paint` is meaningful.
    pub const HAS_FILL: u8 = 1 << 0;
    /// The shape has a stroke; `stroke_paint`, `stroke_width`, `miter_limit`,
    /// `line_cap` and `line_join` are meaningful.
    pub const HAS_STROKE: u8 = 1 << 1;
    /// Fill uses the even-odd rule; otherwise non-zero.
    pub const FILL_RULE_EVEN_ODD: u8 = 1 << 2;
    /// Stroke is painted beneath the fill (`paint-order: stroke`).
    pub const STROKE_FIRST: u8 = 1 << 3;
}

/// `aspect_align` discriminants, matching [`svg_core::AspectAlign`]
/// declaration order.
///
/// `NONE` is SVG's `preserveAspectRatio="none"`: stretch independently in X and
/// Y. The nine remaining values are the alignment grid.
pub mod aspect_align {
    pub const NONE: u8 = 0;
    pub const X_MIN_Y_MIN: u8 = 1;
    pub const X_MID_Y_MIN: u8 = 2;
    pub const X_MAX_Y_MIN: u8 = 3;
    pub const X_MIN_Y_MID: u8 = 4;
    pub const X_MID_Y_MID: u8 = 5;
    pub const X_MAX_Y_MID: u8 = 6;
    pub const X_MIN_Y_MAX: u8 = 7;
    pub const X_MID_Y_MAX: u8 = 8;
    pub const X_MAX_Y_MAX: u8 = 9;
}

/// `aspect_scale` discriminants, matching [`svg_core::AspectScale`].
pub mod aspect_scale {
    pub const MEET: u8 = 0;
    pub const SLICE: u8 = 1;
}

/// `line_cap` discriminants, matching [`svg_core::LineCap`] declaration order.
pub mod line_cap {
    pub const BUTT: u8 = 0;
    pub const ROUND: u8 = 1;
    pub const SQUARE: u8 = 2;
}

/// `line_join` discriminants, matching [`svg_core::LineJoin`] declaration order.
pub mod line_join {
    pub const MITER: u8 = 0;
    pub const ROUND: u8 = 1;
    pub const BEVEL: u8 = 2;
}

/// Largest number of paints or shapes a single asset may contain, imposed by
/// the `u16` count fields. Icons use a handful; the limit exists to keep the
/// header small and is checked rather than assumed.
pub const MAX_TABLE_ENTRIES: usize = u16::MAX as usize;
