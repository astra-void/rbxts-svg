//! The reference rasterizer for `@rbxts/svg`.
//!
//! ```text
//! .svg ──▶ svg-compiler ──▶ SvgDocument ──▶ svg-ir ──┬──▶ svg-raster  (this crate, Rust)
//!                                                    │
//!                                                    └──▶ Luau rasterizer ──▶ EditableImage
//! ```
//!
//! # What this crate is for
//!
//! It is **not** the renderer that runs inside Roblox. It is the definition of
//! what that renderer is supposed to produce.
//!
//! The eventual Luau rasterizer has to be written against something. Written
//! against a prose specification it would be approximately right; written
//! against a working implementation whose output can be diffed pixel by pixel,
//! "approximately" becomes measurable. So every decision here — the flattening
//! tolerance, the coverage scheme, the alpha convention, the stroke
//! construction — is made to be *reproducible in Luau*, not to be as fast or as
//! clever as a CPU rasterizer could be.
//!
//! That is also why it consumes [`svg_core::SvgDocument`] rather than SVG.
//! Running the original file through a third-party renderer would produce
//! prettier pictures and prove nothing: it would exercise none of our
//! compilation, none of our IR, and none of the architecture the Luau backend
//! inherits. `resvg` appears in this crate's dev-dependencies as a *judge*, and
//! nowhere else.
//!
//! # Pipeline
//!
//! ```text
//! view box + preserveAspectRatio + target size  ──▶  transform
//!                                                        │
//! path commands ─────────────────────────────────────────┤
//!                                                        ▼
//!                                          adaptive cubic flattening
//!                                                        │
//!                              ┌─────────────────────────┴────────────┐
//!                              ▼                                      ▼
//!                    fill contours (closed)                  stroke expansion
//!                              │                                      │
//!                              └────────────────┬─────────────────────┘
//!                                               ▼
//!                                     directed edge set
//!                                               ▼
//!                              scanline coverage, nonzero / evenodd
//!                                               ▼
//!                                  source-over compositing
//!                                               ▼
//!                                       RGBA or alpha mask
//! ```
//!
//! Each stage has its own module and its own tests; see [`flatten`], [`edges`],
//! [`stroke`] and [`image`]. The tests that matter most are the ones that check
//! geometry without looking at pixels at all — a golden image tells you
//! *something* changed, and those tell you what.
//!
//! # Conventions, in one place
//!
//! - **Device space** is pixels, origin top-left, y downwards.
//! - **Tolerances** are in device pixels, so they mean the same thing at every
//!   output size ([`flatten::FLATNESS_TOLERANCE`]).
//! - **Compositing** is premultiplied `f32` internally; the returned
//!   [`RasterImage`] is straight (non-premultiplied) RGBA8 in sRGB. See
//!   [`image`] for why both.
//! - **Determinism** is a requirement, not an accident: the same document and
//!   options produce byte-identical output on every run and every machine.
//!
//! # Example
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let document: svg_core::SvgDocument = unimplemented!();
//! use svg_raster::{RasterMode, RasterOptions, render};
//!
//! // A tintable icon: rasterize the coverage once, colour it per instance.
//! let options = RasterOptions::square(24).with_mode(RasterMode::AlphaMask);
//! let image = render(&document, &options)?;
//! assert_eq!(image.pixels.len(), 24 * 24 * 4);
//! # Ok(())
//! # }
//! ```
//!
//! # Not implemented
//!
//! Gradients, clip paths, masks and dashed strokes. The compiler rejects all
//! four with a diagnostic, so they cannot reach this crate; what would change
//! here when they arrive is a new paint resolution step and, for dashes, a
//! segmentation pass in front of [`stroke::expand`]. Nothing about the stage
//! boundaries would have to move.

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod edges;
pub mod error;
pub mod flatten;
pub mod geom;
pub mod image;
pub mod options;
pub mod render;
pub mod stroke;

pub use error::{MAX_DIMENSION, RasterError};
pub use flatten::FLATNESS_TOLERANCE;
pub use geom::Vec2;
pub use image::RasterImage;
pub use options::{RasterMode, RasterOptions};
pub use render::render;
