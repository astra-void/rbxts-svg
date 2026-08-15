//! Canonical path geometry.
//!
//! The whole point of this module is that it is *small*. The SVG path grammar
//! has 20 commands; a renderer that has to understand all of them is a renderer
//! that is painful to reimplement in Luau. So the compiler lowers everything
//! into four commands and the runtime only ever sees those four.
//!
//! | SVG            | canonical form                    |
//! |----------------|-----------------------------------|
//! | `M`/`m`        | [`PathCommand::MoveTo`]           |
//! | `L`/`l`/`H`/`V`| [`PathCommand::LineTo`]           |
//! | `C`/`c`/`S`/`s`| [`PathCommand::CubicTo`]          |
//! | `Q`/`q`/`T`/`t`| [`PathCommand::CubicTo`] (exact)  |
//! | `A`/`a`        | [`PathCommand::CubicTo`] (approx) |
//! | `Z`/`z`        | [`PathCommand::Close`]            |
//!
//! Curves stay curves. Flattening a cubic into line segments requires knowing
//! the output resolution, which the compiler does not know and must not guess,
//! so it belongs to the rasterizer.

use crate::error::CoreError;
use crate::geometry::Point;
use crate::transform::Transform;

/// How overlapping subpaths are combined when filling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

/// A single canonical drawing command.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCommand {
    /// Begins a new subpath at the given point.
    MoveTo(Point),
    /// Straight segment from the current point.
    LineTo(Point),
    /// Cubic Bézier from the current point: `(control1, control2, end)`.
    CubicTo(Point, Point, Point),
    /// Closes the current subpath back to its starting point.
    Close,
}

impl PathCommand {
    /// The command's endpoint, if it has one. `Close` has none: it returns to
    /// the subpath's start, which the reader already knows.
    pub fn end_point(&self) -> Option<Point> {
        match self {
            Self::MoveTo(p) | Self::LineTo(p) => Some(*p),
            Self::CubicTo(_, _, p) => Some(*p),
            Self::Close => None,
        }
    }

    fn map_points(&self, f: impl Fn(Point) -> Point) -> Self {
        match *self {
            Self::MoveTo(p) => Self::MoveTo(f(p)),
            Self::LineTo(p) => Self::LineTo(f(p)),
            Self::CubicTo(a, b, c) => Self::CubicTo(f(a), f(b), f(c)),
            Self::Close => Self::Close,
        }
    }

    fn validate(&self) -> Result<(), CoreError> {
        match *self {
            Self::MoveTo(p) | Self::LineTo(p) => {
                p.validate()?;
            }
            Self::CubicTo(a, b, c) => {
                a.validate()?;
                b.validate()?;
                c.validate()?;
            }
            Self::Close => {}
        }
        Ok(())
    }
}

/// A canonical path: an ordered stream of [`PathCommand`]s, possibly spanning
/// several subpaths.
///
/// # Invariants
///
/// A `Path` obtained through [`PathBuilder`] or [`Path::from_commands`] always
/// satisfies:
///
/// 1. The stream is either empty, or its first command is `MoveTo`.
/// 2. Every `LineTo`, `CubicTo` and `Close` is preceded by a `MoveTo`, so the
///    current point is always defined.
/// 3. Every coordinate is finite.
///
/// Subpath structure is preserved exactly as authored — a later `MoveTo` starts
/// a new subpath rather than replacing the previous one. Fill rules depend on
/// this, so subpaths must never be merged or reordered.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Path {
    commands: Vec<PathCommand>,
}

impl Path {
    /// Builds a path from a command stream, checking the invariants above.
    pub fn from_commands(commands: Vec<PathCommand>) -> Result<Self, CoreError> {
        if let Some(first) = commands.first()
            && !matches!(first, PathCommand::MoveTo(_))
        {
            return Err(CoreError::PathMissingInitialMoveTo);
        }
        for cmd in &commands {
            cmd.validate()?;
        }
        Ok(Self { commands })
    }

    #[inline]
    pub fn commands(&self) -> &[PathCommand] {
        &self.commands
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Number of subpaths, i.e. the number of `MoveTo` commands.
    pub fn subpath_count(&self) -> usize {
        self.commands
            .iter()
            .filter(|c| matches!(c, PathCommand::MoveTo(_)))
            .count()
    }

    /// True when no subpath contains an actual drawing command. Such a path
    /// covers no area and produces no stroke except under round/square caps,
    /// which the compiler handles explicitly rather than dropping blindly.
    pub fn has_drawing_commands(&self) -> bool {
        self.commands
            .iter()
            .any(|c| !matches!(c, PathCommand::MoveTo(_) | PathCommand::Close))
    }

    /// Returns a copy with `transform` applied to every coordinate.
    ///
    /// Because all four canonical commands are affine-invariant (a cubic Bézier
    /// maps to a cubic Bézier under an affine transform, with its control
    /// points transformed pointwise), this is exact — no re-fitting needed.
    pub fn transformed(&self, transform: &Transform) -> Self {
        if transform.is_identity() {
            return self.clone();
        }
        Self {
            commands: self
                .commands
                .iter()
                .map(|c| c.map_points(|p| transform.map_point(p)))
                .collect(),
        }
    }

    /// Re-checks the invariants. Cheap enough to call at pipeline boundaries.
    pub fn validate(&self) -> Result<(), CoreError> {
        Self::from_commands(self.commands.clone()).map(|_| ())
    }
}

/// Incrementally builds a [`Path`] while maintaining its invariants.
///
/// Drawing commands issued with no open subpath are rejected rather than
/// silently repaired, so malformed lowering shows up as an error instead of as
/// subtly wrong geometry.
#[derive(Debug, Default)]
pub struct PathBuilder {
    commands: Vec<PathCommand>,
    has_open_subpath: bool,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(n: usize) -> Self {
        Self {
            commands: Vec::with_capacity(n),
            has_open_subpath: false,
        }
    }

    pub fn move_to(&mut self, p: Point) -> Result<&mut Self, CoreError> {
        p.validate()?;
        self.commands.push(PathCommand::MoveTo(p));
        self.has_open_subpath = true;
        Ok(self)
    }

    pub fn line_to(&mut self, p: Point) -> Result<&mut Self, CoreError> {
        self.require_open()?;
        p.validate()?;
        self.commands.push(PathCommand::LineTo(p));
        Ok(self)
    }

    pub fn cubic_to(&mut self, c1: Point, c2: Point, end: Point) -> Result<&mut Self, CoreError> {
        self.require_open()?;
        c1.validate()?;
        c2.validate()?;
        end.validate()?;
        self.commands.push(PathCommand::CubicTo(c1, c2, end));
        Ok(self)
    }

    /// Appends a quadratic Bézier, exactly elevated to a cubic.
    ///
    /// Degree elevation is lossless: a quadratic with control point `q` and
    /// endpoints `p0`, `p1` is the same curve as the cubic with control points
    /// `p0 + 2/3 (q - p0)` and `p1 + 2/3 (q - p1)`. `from` must be the current
    /// point, which the caller tracks during lowering.
    pub fn quad_to(
        &mut self,
        from: Point,
        ctrl: Point,
        end: Point,
    ) -> Result<&mut Self, CoreError> {
        const TWO_THIRDS: f32 = 2.0 / 3.0;
        let c1 = Point::new(
            from.x + TWO_THIRDS * (ctrl.x - from.x),
            from.y + TWO_THIRDS * (ctrl.y - from.y),
        );
        let c2 = Point::new(
            end.x + TWO_THIRDS * (ctrl.x - end.x),
            end.y + TWO_THIRDS * (ctrl.y - end.y),
        );
        self.cubic_to(c1, c2, end)
    }

    pub fn close(&mut self) -> Result<&mut Self, CoreError> {
        self.require_open()?;
        self.commands.push(PathCommand::Close);
        Ok(self)
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn finish(self) -> Path {
        // Every mutator upheld the invariants, so no re-validation is needed.
        Path {
            commands: self.commands,
        }
    }

    fn require_open(&self) -> Result<(), CoreError> {
        if self.has_open_subpath {
            Ok(())
        } else {
            Err(CoreError::PathMissingInitialMoveTo)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn builder_rejects_drawing_before_move_to() {
        let mut b = PathBuilder::new();
        assert_eq!(
            b.line_to(p(1.0, 1.0)).unwrap_err(),
            CoreError::PathMissingInitialMoveTo
        );
    }

    #[test]
    fn builder_rejects_non_finite_coordinates() {
        let mut b = PathBuilder::new();
        assert!(b.move_to(p(f32::NAN, 0.0)).is_err());
    }

    #[test]
    fn from_commands_rejects_stream_not_starting_with_move_to() {
        let err = Path::from_commands(vec![PathCommand::LineTo(p(1.0, 1.0))]).unwrap_err();
        assert_eq!(err, CoreError::PathMissingInitialMoveTo);
    }

    #[test]
    fn empty_command_stream_is_valid() {
        assert!(Path::from_commands(vec![]).unwrap().is_empty());
    }

    #[test]
    fn quad_to_elevation_preserves_the_curve_midpoint() {
        // Quadratic B(0.5) = 0.25*p0 + 0.5*q + 0.25*p1
        let (from, ctrl, end) = (p(0.0, 0.0), p(2.0, 4.0), p(4.0, 0.0));
        let expected = Point::new(
            0.25 * from.x + 0.5 * ctrl.x + 0.25 * end.x,
            0.25 * from.y + 0.5 * ctrl.y + 0.25 * end.y,
        );

        let mut b = PathBuilder::new();
        b.move_to(from).unwrap();
        b.quad_to(from, ctrl, end).unwrap();
        let path = b.finish();

        let PathCommand::CubicTo(c1, c2, e) = path.commands()[1] else {
            panic!("expected a cubic");
        };
        // Cubic B(0.5) = (p0 + 3*c1 + 3*c2 + p1) / 8
        let mid = Point::new(
            (from.x + 3.0 * c1.x + 3.0 * c2.x + e.x) / 8.0,
            (from.y + 3.0 * c1.y + 3.0 * c2.y + e.y) / 8.0,
        );
        assert!((mid.x - expected.x).abs() < 1e-5);
        assert!((mid.y - expected.y).abs() < 1e-5);
    }

    #[test]
    fn subpaths_are_counted_and_preserved() {
        let mut b = PathBuilder::new();
        b.move_to(p(0.0, 0.0)).unwrap();
        b.line_to(p(1.0, 0.0)).unwrap();
        b.close().unwrap();
        b.move_to(p(5.0, 5.0)).unwrap();
        b.line_to(p(6.0, 5.0)).unwrap();
        let path = b.finish();

        assert_eq!(path.subpath_count(), 2);
        assert_eq!(path.commands().len(), 5);
    }

    #[test]
    fn transform_is_applied_to_every_control_point() {
        let mut b = PathBuilder::new();
        b.move_to(p(0.0, 0.0)).unwrap();
        b.cubic_to(p(1.0, 0.0), p(2.0, 1.0), p(3.0, 1.0)).unwrap();
        let path = b.finish();

        let t = Transform::from_row(2.0, 0.0, 0.0, 2.0, 10.0, 0.0);
        let out = path.transformed(&t);

        assert_eq!(out.commands()[0], PathCommand::MoveTo(p(10.0, 0.0)));
        assert_eq!(
            out.commands()[1],
            PathCommand::CubicTo(p(12.0, 0.0), p(14.0, 2.0), p(16.0, 2.0))
        );
    }

    #[test]
    fn move_only_path_has_no_drawing_commands() {
        let mut b = PathBuilder::new();
        b.move_to(p(1.0, 1.0)).unwrap();
        assert!(!b.finish().has_drawing_commands());
    }

    #[test]
    fn end_point_of_close_is_none() {
        assert_eq!(PathCommand::Close.end_point(), None);
        assert_eq!(
            PathCommand::MoveTo(p(1.0, 2.0)).end_point(),
            Some(p(1.0, 2.0))
        );
    }
}
