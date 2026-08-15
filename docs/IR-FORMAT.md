# Serialized IR format, version 2

The bytes that travel from the compiler to a runtime. Written by
[`crates/svg-ir`](../crates/svg-ir), read by both that crate and the Luau
decoder in [`packages/svg/src/ir`](../packages/svg/src/ir).

Two implementations must agree, so this file is the specification and
`crates/svg-ir/tests/roundtrip.rs` and `tests/luau/spec.luau` are what hold them
to it.

## Principles

- **Little-endian, always.** Luau's `buffer.readu32` / `buffer.readf32` are
  little-endian, so a Roblox decoder does no byte swapping.
- **Fixed-stride tables.** Element *i* of a table is at `base + i * STRIDE`. No
  pointer chasing, no per-element size decoding.
- **Counts up front.** Bounds are validated once, not per element.
- **Four opcodes.** A Luau command-stream decoder is a four-arm branch.

## Layout

All integers little-endian. All floats IEEE-754 binary32, little-endian.

```text
┌─ header ───────────────────────────────────────── 36 bytes ─┐
│  0  [4]  magic            "RSVG" (0x52 0x53 0x56 0x47)      │
│  4  u16  version          2                                 │
│  6  u16  header_size      36; byte offset of the paint table│
│  8  u32  feature_flags    see below                         │
│ 12  f32  view_box.x                                         │
│ 16  f32  view_box.y                                         │
│ 20  f32  view_box.width   > 0                               │
│ 24  f32  view_box.height  > 0                               │
│ 28  u16  paint_count                                        │
│ 30  u16  shape_count                                        │
│ 32  u8   aspect_align     see below                         │
│ 33  u8   aspect_scale     0 = meet, 1 = slice               │
│ 34  u8   reserved         0                                 │
│ 35  u8   reserved         0                                 │
├─ paint table ──────────────── paint_count × 8 bytes ────────┤
│  0  u8   kind             0 = solid, 1 = currentColor       │
│  1  u8   r                                                  │
│  2  u8   g                0 for currentColor                │
│  3  u8   b                                                  │
│  4  f32  alpha            0.0..=1.0                         │
├─ shape table ─────────────── shape_count × 24 bytes ────────┤
│  0  u8   flags            see below                         │
│  1  u8   line_cap         0 butt, 1 round, 2 square         │
│  2  u8   line_join        0 miter, 1 round, 2 bevel         │
│  3  u8   reserved         0                                 │
│  4  u16  fill_paint       index into the paint table        │
│  6  u16  stroke_paint     index into the paint table        │
│  8  f32  stroke_width     view box units                    │
│ 12  f32  miter_limit      >= 1                              │
│ 16  u32  command_offset   byte offset into the command      │
│                           stream, relative to its start     │
│ 20  u32  command_count    number of commands, not bytes     │
├─ command stream ─────────────────────── variable length ────┤
│  u8 opcode, then its operands                               │
└─────────────────────────────────────────────────────────────┘
```

Section offsets follow directly:

```text
paint_table    = 36
shape_table    = 36 + paint_count * 8
command_stream = 36 + paint_count * 8 + shape_count * 24
```

### Shape flags

| Bit | Meaning |
| --- | --- |
| 0 | `HAS_FILL` — `fill_paint` is meaningful |
| 1 | `HAS_STROKE` — `stroke_paint`, `stroke_width`, `miter_limit`, `line_cap`, `line_join` are meaningful |
| 2 | `FILL_RULE_EVEN_ODD` — otherwise non-zero |
| 3 | `STROKE_FIRST` — stroke is painted beneath the fill |

### preserveAspectRatio

How the asset fills a target rectangle whose aspect ratio differs from the view
box's. The view box alone cannot answer that — a 24×12 asset drawn into a
100×100 square is either letterboxed or stretched, and only the source document
knows which — so the authored policy travels with the geometry.

| `aspect_align` | Meaning |
| --- | --- |
| 0 | `none` — scale X and Y independently, stretching to fit |
| 1 | `xMinYMin` |
| 2 | `xMidYMin` |
| 3 | `xMaxYMin` |
| 4 | `xMinYMid` |
| 5 | `xMidYMid` *(the SVG default)* |
| 6 | `xMaxYMid` |
| 7 | `xMinYMax` |
| 8 | `xMidYMax` |
| 9 | `xMaxYMax` |

`aspect_scale` is `0` for `meet` (fit the whole view box inside the target,
leaving unused space) and `1` for `slice` (cover the target and crop the
overflow). It has no effect when `aspect_align` is `none`.

A document with no `preserveAspectRatio` attribute encodes SVG's default,
`xMidYMid meet` — align `5`, scale `0`. Note that align `0` is **not** the
default: it is `none`, a genuinely different picture.

The mapping from these fields to a transform is defined once, in
`svg_core::view_box_transform`, and is the function every renderer is expected
to use rather than deriving its own scale.

### Feature flags

| Bit | Name | Meaning |
| --- | --- | --- |
| 0 | `USES_CURRENT_COLOR` | Some paint defers to a consumer-supplied colour |
| 1 | `HAS_FILL` | Some shape has a fill |
| 2 | `HAS_STROKE` | Some shape has a stroke |
| 3 | `HAS_EVEN_ODD_FILL` | Some fill uses the even-odd rule |
| 4 | `MONOCHROME` | Every visible paint is the *same* paint |
| 5 | `HAS_TRANSPARENCY` | Some paint is below full opacity |
| 6 | `HAS_STROKE_FIRST` | Some shape uses `paint-order: stroke` |
| 16 | `HAS_GRADIENT` | *Reserved* — not yet produced |
| 17 | `HAS_CLIP` | *Reserved* |
| 18 | `HAS_MASK` | *Reserved* |
| 19 | `HAS_DASH` | *Reserved* |

Reserved bits are declared now so their positions are committed and a future
compiler can set them without a format version bump.

### Opcodes

| Value | Command | Operands |
| --- | --- | --- |
| 0 | `MoveTo` | `x, y` |
| 1 | `LineTo` | `x, y` |
| 2 | `CubicTo` | `c1x, c1y, c2x, c2y, x, y` |
| 3 | `Close` | none |

Coordinates are in **view box space**, so an asset is resolution independent.
Curves are never flattened: that requires knowing the output resolution, which
belongs to the rasterizer.

## Invariants

A decoder may assume all of these, because the encoder guarantees them and the
decoder validates them once at load:

1. `magic == "RSVG"` and `version` is one the decoder implements.
2. `view_box.width > 0` and `view_box.height > 0`.
3. Every `fill_paint` / `stroke_paint` index is `< paint_count`, and only read
   when the corresponding flag bit is set.
4. Every alpha is within `0.0..=1.0`.
5. `miter_limit >= 1`, and `stroke_width > 0` whenever `HAS_STROKE` is set.
6. Each shape's command range lies entirely within the command stream.
7. Every command stream begins a subpath with `MoveTo` before any drawing
   command, so the current point is always defined.
8. Shapes are in painter's order — index 0 is drawn first, furthest back.
9. Shapes' command ranges appear in the same order as the shapes, so a decoder
   may also stream the file front to back.

## Ordering and determinism

The paint table is interned in **order of first use**, deduplicated on the exact
bit pattern of `(kind, r, g, b, alpha)`. Interning goes through a `BTreeMap`, so
no hash iteration order can reach the output. The same document always produces
the same bytes.

## Limits

`paint_count` and `shape_count` are `u16`, so at most 65,535 of each. The
command stream is addressed by `u32`. Exceeding either is an explicit
`EncodeError`, never a silent truncation.

## Versioning

### History

| Version | Change |
| --- | --- |
| 1 | Initial format. 32-byte header. |
| 2 | `preserveAspectRatio` added; header grew to 36 bytes. |

Version 2 grew the header rather than borrowing spare bits, because there were
none: bytes 0..32 were fully assigned in version 1. Growing the header moves the
paint table, which a version-1 decoder computes as the constant `32`, so the
change is unambiguously breaking and the version number says so. The two
trailing bytes are reserved and read as zero; a future field that fits in them
will still need a version bump, for exactly the same reason.

### Rules

`SVG_IR_VERSION` is bumped for **any** change an older decoder would misread:

- new or reordered header fields, or a different `header_size`
- a changed table stride
- a different coordinate encoding (fixed-point, delta encoding, compression)
- renumbered enum values or flag bits
- a new opcode or paint `kind` — an old decoder cannot skip an operand list
  whose length it does not know

Decoders reject unknown versions with an actionable message rather than
attempting to interpret them:

```text
asset was compiled for IR version 3 but this runtime speaks version 2.
Recompile your .svg files with a matching version of @rbxts/svg-compiler.
```

## Worked example

`tests/fixtures/lucide/search.svg` — a line plus a circle, both stroked with
`currentColor` — compiles to **220 bytes**:

```text
36   header       viewBox 0 0 24 24, xMidYMid meet, flags 21, 1 paint, 2 shapes
8    paint table  currentColor, alpha 1.0
48   shape table  2 × 24 bytes
128  commands     MoveTo + LineTo, then MoveTo + 4×CubicTo + Close
```

Flags `21` = `USES_CURRENT_COLOR | HAS_STROKE | MONOCHROME`, i.e. tintable.
