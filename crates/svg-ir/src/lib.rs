//! The serialization format that connects the `@rbxts/svg` compiler to its
//! runtimes.
//!
//! # Semantic IR vs. serialized IR
//!
//! These are deliberately two different things.
//!
//! - The *semantic IR* is [`svg_core::SvgDocument`]. It is shaped for the
//!   compiler: enums, `Option`s, `Vec`s, whatever is ergonomic in Rust.
//! - The *serialized IR* is what this crate defines. It is shaped for a decoder
//!   written in Luau running inside Roblox: little-endian scalars, fixed-stride
//!   tables, four opcodes.
//!
//! Keeping them separate is what lets the compiler's internals change freely
//! while the runtime format stays a stable, versioned contract — and what lets
//! the format become denser later (fixed-point coordinates, delta-encoded
//! command streams) without touching the compiler or the public asset API.
//!
//! See [`format`] for the byte-level layout.
//!
//! ```
//! # use svg_core::*;
//! # let mut b = PathBuilder::new();
//! # b.move_to(Point::new(0.0, 0.0)).unwrap();
//! # b.line_to(Point::new(4.0, 0.0)).unwrap();
//! # let doc = SvgDocument::new(
//! #     ViewBox::new(0.0, 0.0, 24.0, 24.0).unwrap(),
//! #     vec![Shape::new(b.finish(), None, None)],
//! #     FeatureFlags::empty(),
//! # );
//! let bytes = svg_ir::encode(&doc).unwrap();
//! assert_eq!(svg_ir::decode(&bytes).unwrap(), doc);
//! ```

#![forbid(unsafe_code)]

pub mod decode;
pub mod encode;
pub mod format;
pub mod opcode;

pub use decode::{DecodeError, IrHeader, decode, decode_header};
pub use encode::{EncodeError, encode};
pub use format::{HEADER_SIZE, MAGIC, PAINT_ENTRY_SIZE, SHAPE_ENTRY_SIZE, SVG_IR_VERSION};
