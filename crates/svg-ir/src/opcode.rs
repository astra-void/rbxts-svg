//! Command stream opcodes.
//!
//! One byte of opcode followed by a fixed number of `f32` operands. There are
//! four opcodes and there will only ever be four, because the compiler lowers
//! the entire SVG path grammar into them (see [`svg_core::path`]). A Luau
//! decoder for this stream is a four-arm `if` chain.

use svg_core::PathCommand;

/// Begin a subpath. Operands: `x, y`.
pub const MOVE_TO: u8 = 0;
/// Straight segment. Operands: `x, y`.
pub const LINE_TO: u8 = 1;
/// Cubic Bézier. Operands: `c1x, c1y, c2x, c2y, x, y`.
pub const CUBIC_TO: u8 = 2;
/// Close the current subpath. No operands.
pub const CLOSE: u8 = 3;

/// Number of `f32` operands that follow each opcode.
pub const fn operand_count(opcode: u8) -> Option<usize> {
    match opcode {
        MOVE_TO | LINE_TO => Some(2),
        CUBIC_TO => Some(6),
        CLOSE => Some(0),
        _ => None,
    }
}

/// Total encoded size of a command in bytes: the opcode plus its operands.
pub const fn encoded_size(opcode: u8) -> Option<usize> {
    match operand_count(opcode) {
        Some(n) => Some(1 + n * 4),
        None => None,
    }
}

/// The opcode a semantic command encodes as.
pub const fn opcode_of(command: &PathCommand) -> u8 {
    match command {
        PathCommand::MoveTo(_) => MOVE_TO,
        PathCommand::LineTo(_) => LINE_TO,
        PathCommand::CubicTo(..) => CUBIC_TO,
        PathCommand::Close => CLOSE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use svg_core::Point;

    #[test]
    fn every_opcode_has_a_known_size() {
        assert_eq!(encoded_size(MOVE_TO), Some(9));
        assert_eq!(encoded_size(LINE_TO), Some(9));
        assert_eq!(encoded_size(CUBIC_TO), Some(25));
        assert_eq!(encoded_size(CLOSE), Some(1));
    }

    #[test]
    fn unknown_opcodes_are_rejected() {
        assert_eq!(operand_count(4), None);
        assert_eq!(encoded_size(255), None);
    }

    #[test]
    fn opcode_mapping_covers_every_command() {
        let p = Point::new(0.0, 0.0);
        assert_eq!(opcode_of(&PathCommand::MoveTo(p)), MOVE_TO);
        assert_eq!(opcode_of(&PathCommand::LineTo(p)), LINE_TO);
        assert_eq!(opcode_of(&PathCommand::CubicTo(p, p, p)), CUBIC_TO);
        assert_eq!(opcode_of(&PathCommand::Close), CLOSE);
    }
}
