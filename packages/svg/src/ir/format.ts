/**
 * Constants of the serialized IR, mirroring `crates/svg-ir/src/format.rs`.
 *
 * These two files are a contract. The Rust side is the specification and its
 * tests pin the byte layout; this side must agree exactly. If you change one,
 * change both and bump {@link SVG_IR_VERSION}.
 */

/** ASCII `RSVG`, the format magic. */
export const MAGIC_R = 82;
export const MAGIC_S = 83;
export const MAGIC_V = 86;
export const MAGIC_G = 71;

/** The format version this decoder understands. */
export const SVG_IR_VERSION = 2;

/** Byte offsets within the fixed 36-byte header. */
export const HEADER_SIZE = 36;
export const OFFSET_VERSION = 4;
export const OFFSET_HEADER_SIZE = 6;
export const OFFSET_FLAGS = 8;
export const OFFSET_VIEW_BOX_X = 12;
export const OFFSET_VIEW_BOX_Y = 16;
export const OFFSET_VIEW_BOX_WIDTH = 20;
export const OFFSET_VIEW_BOX_HEIGHT = 24;
export const OFFSET_PAINT_COUNT = 28;
export const OFFSET_SHAPE_COUNT = 30;
export const OFFSET_ASPECT_ALIGN = 32;
export const OFFSET_ASPECT_SCALE = 33;

/**
 * `aspect_align` discriminants.
 *
 * `None` is SVG's `preserveAspectRatio="none"` — stretch independently in X and
 * Y. It is value 0, which is deliberately *not* the default: an asset with no
 * `preserveAspectRatio` attribute encodes `XMidYMid`.
 */
export const ASPECT_ALIGN_NONE = 0;
export const ASPECT_ALIGN_X_MIN_Y_MIN = 1;
export const ASPECT_ALIGN_X_MID_Y_MIN = 2;
export const ASPECT_ALIGN_X_MAX_Y_MIN = 3;
export const ASPECT_ALIGN_X_MIN_Y_MID = 4;
export const ASPECT_ALIGN_X_MID_Y_MID = 5;
export const ASPECT_ALIGN_X_MAX_Y_MID = 6;
export const ASPECT_ALIGN_X_MIN_Y_MAX = 7;
export const ASPECT_ALIGN_X_MID_Y_MAX = 8;
export const ASPECT_ALIGN_X_MAX_Y_MAX = 9;

/** `aspect_scale` discriminants. */
export const ASPECT_SCALE_MEET = 0;
export const ASPECT_SCALE_SLICE = 1;

/** Paint table entry: `kind:u8, r:u8, g:u8, b:u8, alpha:f32`. */
export const PAINT_ENTRY_SIZE = 8;
export const PAINT_KIND_SOLID = 0;
export const PAINT_KIND_CURRENT_COLOR = 1;

/** Shape table entry, 24 bytes. Field offsets are relative to the entry. */
export const SHAPE_ENTRY_SIZE = 24;
export const SHAPE_OFFSET_FLAGS = 0;
export const SHAPE_OFFSET_LINE_CAP = 1;
export const SHAPE_OFFSET_LINE_JOIN = 2;
export const SHAPE_OFFSET_FILL_PAINT = 4;
export const SHAPE_OFFSET_STROKE_PAINT = 6;
export const SHAPE_OFFSET_STROKE_WIDTH = 8;
export const SHAPE_OFFSET_MITER_LIMIT = 12;
export const SHAPE_OFFSET_COMMAND_OFFSET = 16;
export const SHAPE_OFFSET_COMMAND_COUNT = 20;

/** Bits of a shape entry's flags byte. */
export const SHAPE_FLAG_HAS_FILL = 1;
export const SHAPE_FLAG_HAS_STROKE = 2;
export const SHAPE_FLAG_FILL_RULE_EVEN_ODD = 4;
export const SHAPE_FLAG_STROKE_FIRST = 8;

/** Command stream opcodes. */
export const OP_MOVE_TO = 0;
export const OP_LINE_TO = 1;
export const OP_CUBIC_TO = 2;
export const OP_CLOSE = 3;

/** `stroke-linecap` discriminants. */
export const LINE_CAP_BUTT = 0;
export const LINE_CAP_ROUND = 1;
export const LINE_CAP_SQUARE = 2;

/** `stroke-linejoin` discriminants. */
export const LINE_JOIN_MITER = 0;
export const LINE_JOIN_ROUND = 1;
export const LINE_JOIN_BEVEL = 2;
