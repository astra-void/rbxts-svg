//! Encode/decode tests.
//!
//! A serialization format that is only ever written is a format nobody can
//! check. These tests assert three separate things:
//!
//! 1. **Round-trip.** `decode(encode(doc)) == doc`, exactly.
//! 2. **Layout.** Specific bytes land at specific offsets, so the Luau decoder
//!    can be written against the documented layout with confidence.
//! 3. **Robustness.** Corrupt input produces a structured error, never a panic.

use svg_core::{
    AspectAlign, AspectScale, Color, FeatureFlags, Fill, FillRule, LineCap, LineJoin, Opacity,
    Paint, PaintOrder, Path, PathBuilder, PathCommand, Point, PreserveAspectRatio, Shape, Stroke,
    SvgDocument, ViewBox,
};
use svg_ir::format::{
    HEADER_SIZE, PAINT_ENTRY_SIZE, SHAPE_ENTRY_SIZE, aspect_align, aspect_scale, shape_flags,
};
use svg_ir::{DecodeError, MAGIC, SVG_IR_VERSION, decode, decode_header, encode};

fn view_box() -> ViewBox {
    ViewBox::new(0.0, 0.0, 24.0, 24.0).unwrap()
}

fn line() -> Path {
    let mut b = PathBuilder::new();
    b.move_to(Point::new(1.0, 2.0)).unwrap();
    b.line_to(Point::new(3.0, 4.0)).unwrap();
    b.finish()
}

fn curvy() -> Path {
    let mut b = PathBuilder::new();
    b.move_to(Point::new(0.0, 0.0)).unwrap();
    b.cubic_to(
        Point::new(1.0, 2.0),
        Point::new(3.0, 4.0),
        Point::new(5.0, 6.0),
    )
    .unwrap();
    b.close().unwrap();
    b.move_to(Point::new(10.0, 10.0)).unwrap();
    b.line_to(Point::new(20.0, 20.0)).unwrap();
    b.close().unwrap();
    b.finish()
}

fn stroke() -> Stroke {
    Stroke::new(
        Paint::CurrentColor,
        Opacity::OPAQUE,
        2.0,
        LineCap::Round,
        LineJoin::Round,
        4.0,
    )
    .unwrap()
}

fn document(shapes: Vec<Shape>) -> SvgDocument {
    let features = FeatureFlags::HAS_STROKE | FeatureFlags::USES_CURRENT_COLOR;
    SvgDocument::new(view_box(), shapes, features)
}

// ---------------------------------------------------------------------------
// Round-trip
// ---------------------------------------------------------------------------

#[test]
fn empty_document_round_trips() {
    let doc = SvgDocument::new(view_box(), Vec::new(), FeatureFlags::empty());
    assert_eq!(decode(&encode(&doc).unwrap()).unwrap(), doc);
}

#[test]
fn stroked_shape_round_trips_exactly() {
    let doc = document(vec![Shape::new(line(), None, Some(stroke()))]);
    assert_eq!(decode(&encode(&doc).unwrap()).unwrap(), doc);
}

#[test]
fn every_command_kind_round_trips() {
    let doc = document(vec![Shape::new(curvy(), None, Some(stroke()))]);
    let decoded = decode(&encode(&doc).unwrap()).unwrap();
    assert_eq!(decoded, doc);
    assert_eq!(decoded.shapes[0].geometry.subpath_count(), 2);
    assert!(
        decoded.shapes[0]
            .geometry
            .commands()
            .iter()
            .any(|c| matches!(c, PathCommand::CubicTo(..)))
    );
}

#[test]
fn fills_strokes_and_both_round_trip() {
    let fill = Fill::new(
        Paint::Solid(Color::rgb(1, 2, 3)),
        Opacity::new(0.25).unwrap(),
        FillRule::EvenOdd,
    );
    let doc = document(vec![
        Shape::new(line(), Some(fill), None),
        Shape::new(line(), None, Some(stroke())),
        Shape::new(line(), Some(fill), Some(stroke())),
    ]);
    assert_eq!(decode(&encode(&doc).unwrap()).unwrap(), doc);
}

// ---- preserveAspectRatio ------------------------------------------------

const EVERY_ALIGN: [AspectAlign; 10] = [
    AspectAlign::None,
    AspectAlign::XMinYMin,
    AspectAlign::XMidYMin,
    AspectAlign::XMaxYMin,
    AspectAlign::XMinYMid,
    AspectAlign::XMidYMid,
    AspectAlign::XMaxYMid,
    AspectAlign::XMinYMax,
    AspectAlign::XMidYMax,
    AspectAlign::XMaxYMax,
];

#[test]
fn every_aspect_ratio_combination_round_trips() {
    for align in EVERY_ALIGN {
        for scale in [AspectScale::Meet, AspectScale::Slice] {
            let aspect = PreserveAspectRatio::new(align, scale);
            let doc = document(vec![Shape::new(line(), None, Some(stroke()))])
                .with_preserve_aspect_ratio(aspect);
            let decoded = decode(&encode(&doc).unwrap()).unwrap();
            assert_eq!(decoded.preserve_aspect_ratio, aspect, "{align:?} {scale:?}");
            assert_eq!(decoded, doc);
        }
    }
}

/// A document built without an explicit policy must encode SVG's default, not
/// a zero byte — `aspect_align` 0 is `none`, which is a different picture.
#[test]
fn the_default_aspect_ratio_is_x_mid_y_mid_meet_not_none() {
    let doc = document(vec![Shape::new(line(), None, Some(stroke()))]);
    let bytes = encode(&doc).unwrap();
    assert_eq!(bytes[32], aspect_align::X_MID_Y_MID);
    assert_eq!(bytes[33], aspect_scale::MEET);
    assert_eq!(
        decode(&bytes).unwrap().preserve_aspect_ratio,
        PreserveAspectRatio::DEFAULT
    );
}

#[test]
fn the_aspect_ratio_bytes_are_where_the_specification_says() {
    let doc = document(vec![Shape::new(line(), None, Some(stroke()))]).with_preserve_aspect_ratio(
        PreserveAspectRatio::new(AspectAlign::XMaxYMin, AspectScale::Slice),
    );
    let bytes = encode(&doc).unwrap();
    assert_eq!(bytes[32], aspect_align::X_MAX_Y_MIN);
    assert_eq!(bytes[33], aspect_scale::SLICE);
    assert_eq!(bytes[34], 0, "reserved byte must stay zero");
    assert_eq!(bytes[35], 0, "reserved byte must stay zero");
}

#[test]
fn an_unknown_aspect_align_discriminant_is_rejected() {
    let doc = document(vec![Shape::new(line(), None, Some(stroke()))]);
    let mut bytes = encode(&doc).unwrap();
    bytes[32] = 42;
    assert!(matches!(
        decode(&bytes),
        Err(DecodeError::InvalidEnum {
            field: "aspect align",
            value: 42
        })
    ));
}

#[test]
fn an_unknown_aspect_scale_discriminant_is_rejected() {
    let doc = document(vec![Shape::new(line(), None, Some(stroke()))]);
    let mut bytes = encode(&doc).unwrap();
    bytes[33] = 7;
    assert!(matches!(
        decode(&bytes),
        Err(DecodeError::InvalidEnum {
            field: "aspect scale",
            value: 7
        })
    ));
}

/// Two documents that differ *only* in their fitting policy must not encode to
/// the same bytes — otherwise the policy is not really being carried.
#[test]
fn the_aspect_ratio_changes_the_encoded_bytes() {
    let shapes = || vec![Shape::new(line(), None, Some(stroke()))];
    let meet = encode(&document(shapes())).unwrap();
    let stretched =
        encode(&document(shapes()).with_preserve_aspect_ratio(PreserveAspectRatio::STRETCH))
            .unwrap();
    assert_ne!(meet, stretched);
    assert_eq!(meet.len(), stretched.len());
}

#[test]
fn paint_order_round_trips() {
    let fill = Fill::new(Paint::CurrentColor, Opacity::OPAQUE, FillRule::NonZero);
    let mut shape = Shape::new(line(), Some(fill), Some(stroke()));
    shape.paint_order = PaintOrder::StrokeThenFill;

    let decoded = decode(&encode(&document(vec![shape])).unwrap()).unwrap();
    assert_eq!(decoded.shapes[0].paint_order, PaintOrder::StrokeThenFill);
}

#[test]
fn every_cap_and_join_combination_round_trips() {
    for cap in [LineCap::Butt, LineCap::Round, LineCap::Square] {
        for join in [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel] {
            let s = Stroke::new(Paint::CurrentColor, Opacity::OPAQUE, 1.5, cap, join, 8.0).unwrap();
            let doc = document(vec![Shape::new(line(), None, Some(s))]);
            let decoded = decode(&encode(&doc).unwrap()).unwrap();
            let out = decoded.shapes[0].stroke.unwrap();
            assert_eq!(out.line_cap, cap);
            assert_eq!(out.line_join, join);
            assert_eq!(out.miter_limit, 8.0);
        }
    }
}

#[test]
fn negative_and_fractional_view_box_round_trips() {
    let vb = ViewBox::new(-12.5, -0.25, 33.75, 7.125).unwrap();
    let doc = SvgDocument::new(vb, Vec::new(), FeatureFlags::empty());
    assert_eq!(decode(&encode(&doc).unwrap()).unwrap().view_box, vb);
}

#[test]
fn feature_flags_survive_verbatim() {
    let flags = FeatureFlags::USES_CURRENT_COLOR
        | FeatureFlags::MONOCHROME
        | FeatureFlags::HAS_EVEN_ODD_FILL
        | FeatureFlags::HAS_STROKE_FIRST;
    let doc = SvgDocument::new(view_box(), Vec::new(), flags);
    assert_eq!(decode(&encode(&doc).unwrap()).unwrap().features, flags);
}

// ---------------------------------------------------------------------------
// Paint table
// ---------------------------------------------------------------------------

#[test]
fn identical_paints_are_interned_once() {
    let shapes: Vec<Shape> = (0..8)
        .map(|_| Shape::new(line(), None, Some(stroke())))
        .collect();
    let bytes = encode(&document(shapes)).unwrap();
    assert_eq!(decode_header(&bytes).unwrap().paint_count, 1);
}

#[test]
fn paints_differing_only_in_opacity_are_distinct_entries() {
    let a = Fill::new(Paint::CurrentColor, Opacity::OPAQUE, FillRule::NonZero);
    let b = Fill::new(
        Paint::CurrentColor,
        Opacity::new(0.5).unwrap(),
        FillRule::NonZero,
    );
    let doc = document(vec![
        Shape::new(line(), Some(a), None),
        Shape::new(line(), Some(b), None),
    ]);
    assert_eq!(
        decode_header(&encode(&doc).unwrap()).unwrap().paint_count,
        2
    );
}

#[test]
fn current_color_and_black_are_distinct_paints() {
    let current = Fill::new(Paint::CurrentColor, Opacity::OPAQUE, FillRule::NonZero);
    let black = Fill::new(
        Paint::Solid(Color::BLACK),
        Opacity::OPAQUE,
        FillRule::NonZero,
    );
    let doc = document(vec![
        Shape::new(line(), Some(current), None),
        Shape::new(line(), Some(black), None),
    ]);
    let decoded = decode(&encode(&doc).unwrap()).unwrap();
    assert_eq!(decoded.shapes[0].fill.unwrap().paint, Paint::CurrentColor);
    assert_eq!(
        decoded.shapes[1].fill.unwrap().paint,
        Paint::Solid(Color::BLACK)
    );
}

// ---------------------------------------------------------------------------
// Byte layout — what a Luau decoder is written against
// ---------------------------------------------------------------------------

#[test]
fn header_layout_matches_the_specification() {
    let doc = document(vec![Shape::new(line(), None, Some(stroke()))]);
    let bytes = encode(&doc).unwrap();

    assert_eq!(&bytes[0..4], &MAGIC);
    assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), SVG_IR_VERSION);
    assert_eq!(
        u16::from_le_bytes([bytes[6], bytes[7]]) as usize,
        HEADER_SIZE
    );
    assert_eq!(
        u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
        doc.features.bits()
    );
    assert_eq!(f32::from_le_bytes(bytes[12..16].try_into().unwrap()), 0.0);
    assert_eq!(f32::from_le_bytes(bytes[16..20].try_into().unwrap()), 0.0);
    assert_eq!(f32::from_le_bytes(bytes[20..24].try_into().unwrap()), 24.0);
    assert_eq!(f32::from_le_bytes(bytes[24..28].try_into().unwrap()), 24.0);
    assert_eq!(u16::from_le_bytes([bytes[28], bytes[29]]), 1); // paints
    assert_eq!(u16::from_le_bytes([bytes[30], bytes[31]]), 1); // shapes
    assert_eq!(bytes[32], aspect_align::X_MID_Y_MID);
    assert_eq!(bytes[33], aspect_scale::MEET);
    assert_eq!(&bytes[34..36], &[0, 0]);
    assert_eq!(HEADER_SIZE, 36);
}

#[test]
fn total_size_is_exactly_the_sum_of_its_sections() {
    let doc = document(vec![Shape::new(line(), None, Some(stroke()))]);
    let bytes = encode(&doc).unwrap();
    // 1 paint, 1 shape, commands = MoveTo(9) + LineTo(9)
    let expected = HEADER_SIZE + PAINT_ENTRY_SIZE + SHAPE_ENTRY_SIZE + 18;
    assert_eq!(bytes.len(), expected);
}

#[test]
fn shape_entry_flags_are_where_the_specification_says() {
    let fill = Fill::new(Paint::CurrentColor, Opacity::OPAQUE, FillRule::EvenOdd);
    let doc = document(vec![Shape::new(line(), Some(fill), Some(stroke()))]);
    let bytes = encode(&doc).unwrap();

    let paint_count = decode_header(&bytes).unwrap().paint_count as usize;
    let shape_start = HEADER_SIZE + paint_count * PAINT_ENTRY_SIZE;
    let flags = bytes[shape_start];

    assert_ne!(flags & shape_flags::HAS_FILL, 0);
    assert_ne!(flags & shape_flags::HAS_STROKE, 0);
    assert_ne!(flags & shape_flags::FILL_RULE_EVEN_ODD, 0);
    assert_eq!(flags & shape_flags::STROKE_FIRST, 0);
    assert_eq!(bytes[shape_start + 3], 0, "reserved byte must stay zero");
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn encoding_is_byte_for_byte_reproducible() {
    let doc = document(vec![
        Shape::new(curvy(), None, Some(stroke())),
        Shape::new(line(), None, Some(stroke())),
    ]);
    let first = encode(&doc).unwrap();
    for _ in 0..16 {
        assert_eq!(encode(&doc).unwrap(), first);
    }
}

#[test]
fn paint_indices_follow_first_use_order() {
    let red = Fill::new(
        Paint::Solid(Color::rgb(255, 0, 0)),
        Opacity::OPAQUE,
        FillRule::NonZero,
    );
    let blue = Fill::new(
        Paint::Solid(Color::rgb(0, 0, 255)),
        Opacity::OPAQUE,
        FillRule::NonZero,
    );

    let bytes = encode(&document(vec![
        Shape::new(line(), Some(red), None),
        Shape::new(line(), Some(blue), None),
    ]))
    .unwrap();

    // Entry 0 must be red, entry 1 blue: the order shapes first used them.
    assert_eq!(&bytes[HEADER_SIZE + 1..HEADER_SIZE + 4], &[255, 0, 0]);
    assert_eq!(
        &bytes[HEADER_SIZE + PAINT_ENTRY_SIZE + 1..HEADER_SIZE + PAINT_ENTRY_SIZE + 4],
        &[0, 0, 255]
    );
}

// ---------------------------------------------------------------------------
// Malformed input
// ---------------------------------------------------------------------------

#[test]
fn wrong_magic_is_rejected() {
    let mut bytes = encode(&document(vec![])).unwrap();
    bytes[0] = b'X';
    assert!(matches!(
        decode(&bytes),
        Err(DecodeError::InvalidMagic { .. })
    ));
}

#[test]
fn a_future_version_is_rejected_rather_than_guessed_at() {
    let mut bytes = encode(&document(vec![])).unwrap();
    bytes[4..6].copy_from_slice(&(SVG_IR_VERSION + 1).to_le_bytes());
    match decode(&bytes) {
        Err(DecodeError::UnsupportedVersion { found, supported }) => {
            assert_eq!(found, SVG_IR_VERSION + 1);
            assert_eq!(supported, SVG_IR_VERSION);
        }
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
}

#[test]
fn truncation_at_every_length_is_an_error_and_never_a_panic() {
    let bytes = encode(&document(vec![Shape::new(curvy(), None, Some(stroke()))])).unwrap();
    for len in 0..bytes.len() {
        assert!(
            decode(&bytes[..len]).is_err(),
            "a {len}-byte prefix should not decode"
        );
    }
    assert!(decode(&bytes).is_ok());
}

#[test]
fn a_bad_opcode_is_reported() {
    let doc = document(vec![Shape::new(line(), None, Some(stroke()))]);
    let mut bytes = encode(&doc).unwrap();
    let command_start = HEADER_SIZE + PAINT_ENTRY_SIZE + SHAPE_ENTRY_SIZE;
    bytes[command_start] = 99;
    assert!(matches!(
        decode(&bytes),
        Err(DecodeError::InvalidOpcode { value: 99, .. })
    ));
}

#[test]
fn a_bad_line_cap_discriminant_is_reported() {
    let doc = document(vec![Shape::new(line(), None, Some(stroke()))]);
    let mut bytes = encode(&doc).unwrap();
    let shape_start = HEADER_SIZE + PAINT_ENTRY_SIZE;
    bytes[shape_start + 1] = 7;
    assert!(matches!(
        decode(&bytes),
        Err(DecodeError::InvalidEnum {
            field: "line cap",
            value: 7
        })
    ));
}

#[test]
fn an_out_of_range_paint_index_is_reported() {
    let doc = document(vec![Shape::new(line(), None, Some(stroke()))]);
    let mut bytes = encode(&doc).unwrap();
    let shape_start = HEADER_SIZE + PAINT_ENTRY_SIZE;
    bytes[shape_start + 6..shape_start + 8].copy_from_slice(&99u16.to_le_bytes());
    assert!(matches!(
        decode(&bytes),
        Err(DecodeError::PaintIndexOutOfRange { index: 99, .. })
    ));
}

#[test]
fn a_command_range_past_the_end_is_reported() {
    let doc = document(vec![Shape::new(line(), None, Some(stroke()))]);
    let mut bytes = encode(&doc).unwrap();
    let shape_start = HEADER_SIZE + PAINT_ENTRY_SIZE;
    bytes[shape_start + 16..shape_start + 20].copy_from_slice(&9999u32.to_le_bytes());
    assert!(matches!(
        decode(&bytes),
        Err(DecodeError::CommandRangeOutOfRange { .. })
    ));
}

#[test]
fn an_out_of_range_opacity_is_rejected() {
    let doc = document(vec![Shape::new(line(), None, Some(stroke()))]);
    let mut bytes = encode(&doc).unwrap();
    // Paint alpha sits at offset 4 of the first paint entry.
    bytes[HEADER_SIZE + 4..HEADER_SIZE + 8].copy_from_slice(&5.0f32.to_le_bytes());
    assert!(matches!(decode(&bytes), Err(DecodeError::InvalidValue(_))));
}

#[test]
fn arbitrary_garbage_does_not_panic() {
    for seed in 0u32..512 {
        // Deterministic pseudo-random bytes; no RNG, so failures reproduce.
        let bytes: Vec<u8> = (0..64u32)
            .map(|i| (seed.wrapping_mul(2654435761).wrapping_add(i * 97) >> 3) as u8)
            .collect();
        let _ = decode(&bytes);
    }
}

#[test]
fn empty_input_is_an_error() {
    assert!(decode(&[]).is_err());
    assert!(decode_header(&[]).is_err());
}
