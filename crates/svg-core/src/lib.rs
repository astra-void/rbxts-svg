//! Framework-neutral semantic model for compiled vector graphics.
//!
//! `svg-core` is the vocabulary the rest of `@rbxts/svg` is written in. It
//! describes *vector graphics*, not SVG documents: by the time something is
//! expressed in these types, XML, CSS, groups, `use` references, inherited
//! presentation attributes and primitive shapes have all been resolved away.
//!
//! # What lives here
//!
//! - [`geometry`] — points and the view box coordinate system
//! - [`transform`] — affine transforms, used while baking geometry
//! - [`aspect`] — `preserveAspectRatio` and the view box → target mapping
//! - [`path`] — the four canonical path commands
//! - [`paint`] — colours, opacity, fills and strokes
//! - [`document`] — shapes and the finished [`SvgDocument`]
//! - [`features`] — compile-time facts the runtime can act on cheaply
//!
//! # What must never live here
//!
//! Node.js, napi-rs, TypeScript, Roblox, `EditableImage`, React, and the
//! serialized wire format. Those belong to `svg-node`, the TypeScript packages
//! and `svg-ir` respectively. `svg-core` has exactly one dependency
//! (`bitflags`), and that is intended to stay true.

#![deny(missing_debug_implementations)]
#![forbid(unsafe_code)]

pub mod aspect;
pub mod document;
pub mod error;
pub mod features;
pub mod geometry;
pub mod paint;
pub mod path;
pub mod transform;

pub use aspect::{AspectAlign, AspectScale, PreserveAspectRatio, view_box_transform};
pub use document::{PaintOrder, Shape, SvgDocument};
pub use error::CoreError;
pub use features::FeatureFlags;
pub use geometry::{Point, ViewBox};
pub use paint::{Color, Fill, LineCap, LineJoin, Opacity, Paint, Stroke};
pub use path::{FillRule, Path, PathBuilder, PathCommand};
pub use transform::Transform;
