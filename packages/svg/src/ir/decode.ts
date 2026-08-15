/**
 * The Luau-side IR decoder.
 *
 * # Design
 *
 * The asset is *not* expanded into an object graph at load time. The buffer is
 * kept as-is and read on demand, because an icon that is loaded once and
 * rasterized once should not leave a tree of tables behind for the GC. Only the
 * header — fixed size, needed by everything — is parsed eagerly.
 *
 * Command iteration is a visitor rather than an array of command objects, so a
 * rasterizer can walk a path without allocating anything at all.
 *
 * Every accessor is a fixed-offset read. Bounds are validated once, in
 * {@link decodeAsset}, so the hot path carries no checks.
 */

import type { SvgAssetData, SvgPreserveAspectRatio, SvgViewBox } from "../asset";
import {
	ASPECT_ALIGN_X_MAX_Y_MAX,
	ASPECT_SCALE_SLICE,
	HEADER_SIZE,
	OFFSET_ASPECT_ALIGN,
	OFFSET_ASPECT_SCALE,
	MAGIC_G,
	MAGIC_R,
	MAGIC_S,
	MAGIC_V,
	OFFSET_FLAGS,
	OFFSET_PAINT_COUNT,
	OFFSET_SHAPE_COUNT,
	OFFSET_VERSION,
	OFFSET_VIEW_BOX_HEIGHT,
	OFFSET_VIEW_BOX_WIDTH,
	OFFSET_VIEW_BOX_X,
	OFFSET_VIEW_BOX_Y,
	OP_CLOSE,
	OP_CUBIC_TO,
	OP_LINE_TO,
	OP_MOVE_TO,
	PAINT_ENTRY_SIZE,
	PAINT_KIND_CURRENT_COLOR,
	SHAPE_ENTRY_SIZE,
	SHAPE_FLAG_FILL_RULE_EVEN_ODD,
	SHAPE_FLAG_HAS_FILL,
	SHAPE_FLAG_HAS_STROKE,
	SHAPE_FLAG_STROKE_FIRST,
	SHAPE_OFFSET_COMMAND_COUNT,
	SHAPE_OFFSET_COMMAND_OFFSET,
	SHAPE_OFFSET_FILL_PAINT,
	SHAPE_OFFSET_FLAGS,
	SHAPE_OFFSET_LINE_CAP,
	SHAPE_OFFSET_LINE_JOIN,
	SHAPE_OFFSET_MITER_LIMIT,
	SHAPE_OFFSET_STROKE_PAINT,
	SHAPE_OFFSET_STROKE_WIDTH,
	SVG_IR_VERSION,
} from "./format";

/** A paint table entry. */
export interface SvgPaint {
	/** True when the consumer supplies the colour at render time. */
	readonly isCurrentColor: boolean;
	/** 0-255. Meaningless when `isCurrentColor` is true. */
	readonly r: number;
	readonly g: number;
	readonly b: number;
	/** 0-1. */
	readonly alpha: number;
}

/** A shape table entry. */
export interface SvgShape {
	readonly hasFill: boolean;
	readonly hasStroke: boolean;
	/** True for `fill-rule: evenodd`, false for `nonzero`. */
	readonly evenOdd: boolean;
	/** True when the stroke is painted beneath the fill. */
	readonly strokeFirst: boolean;
	/** Index into the paint table; only meaningful when `hasFill`. */
	readonly fillPaint: number;
	/** Index into the paint table; only meaningful when `hasStroke`. */
	readonly strokePaint: number;
	/** In view box units. */
	readonly strokeWidth: number;
	readonly miterLimit: number;
	/** `LINE_CAP_*` from `./format`. */
	readonly lineCap: number;
	/** `LINE_JOIN_*` from `./format`. */
	readonly lineJoin: number;
	/** Byte offset into the command stream. */
	readonly commandOffset: number;
	readonly commandCount: number;
}

/**
 * Receives a shape's geometry.
 *
 * The four methods are the entire runtime command set: the compiler lowers all
 * twenty SVG path commands into these, so a renderer never sees an arc, a
 * quadratic or a shorthand.
 */
export interface SvgCommandVisitor {
	moveTo(x: number, y: number): void;
	lineTo(x: number, y: number): void;
	cubicTo(
		c1x: number,
		c1y: number,
		c2x: number,
		c2y: number,
		x: number,
		y: number,
	): void;
	close(): void;
}

function fail(message: string): never {
	return error(`@rbxts/svg: ${message}`);
}

/**
 * Validates a blob and computes its section offsets.
 *
 * Called once per asset, at module load. Everything it checks is something the
 * accessors are then allowed to assume.
 */
export function decodeAsset(id: string, data: buffer): SvgAssetData {
	const length = buffer.len(data);
	if (length < HEADER_SIZE) {
		fail(`compiled asset is ${length} bytes, shorter than the ${HEADER_SIZE}-byte header`);
	}

	if (
		buffer.readu8(data, 0) !== MAGIC_R ||
		buffer.readu8(data, 1) !== MAGIC_S ||
		buffer.readu8(data, 2) !== MAGIC_V ||
		buffer.readu8(data, 3) !== MAGIC_G
	) {
		fail("data is not a compiled SVG asset (bad magic)");
	}

	const version = buffer.readu16(data, OFFSET_VERSION);
	if (version !== SVG_IR_VERSION) {
		fail(
			`asset was compiled for IR version ${version} but this runtime speaks ` +
				`version ${SVG_IR_VERSION}. Recompile your .svg files with a matching ` +
				`version of @rbxts/svg-compiler.`,
		);
	}

	const features = buffer.readu32(data, OFFSET_FLAGS);
	const viewBox: SvgViewBox = {
		x: buffer.readf32(data, OFFSET_VIEW_BOX_X),
		y: buffer.readf32(data, OFFSET_VIEW_BOX_Y),
		width: buffer.readf32(data, OFFSET_VIEW_BOX_WIDTH),
		height: buffer.readf32(data, OFFSET_VIEW_BOX_HEIGHT),
	};
	if (!(viewBox.width > 0) || !(viewBox.height > 0)) {
		fail(`asset has a degenerate view box (${viewBox.width} x ${viewBox.height})`);
	}

	// Validated here rather than trusted, because a renderer indexes on these:
	// an out-of-range discriminant would otherwise become a silently wrong fit.
	const align = buffer.readu8(data, OFFSET_ASPECT_ALIGN);
	if (align > ASPECT_ALIGN_X_MAX_Y_MAX) {
		fail(`asset has an unknown preserveAspectRatio alignment (${align})`);
	}
	const scale = buffer.readu8(data, OFFSET_ASPECT_SCALE);
	if (scale > ASPECT_SCALE_SLICE) {
		fail(`asset has an unknown preserveAspectRatio scale (${scale})`);
	}
	const preserveAspectRatio: SvgPreserveAspectRatio = { align, scale };

	const paintCount = buffer.readu16(data, OFFSET_PAINT_COUNT);
	const shapeCount = buffer.readu16(data, OFFSET_SHAPE_COUNT);

	const paintTableOffset = HEADER_SIZE;
	const shapeTableOffset = paintTableOffset + paintCount * PAINT_ENTRY_SIZE;
	const commandStreamOffset = shapeTableOffset + shapeCount * SHAPE_ENTRY_SIZE;

	if (commandStreamOffset > length) {
		fail(
			`asset is truncated: its tables need ${commandStreamOffset} bytes but the ` +
				`blob is ${length} bytes`,
		);
	}

	return {
		id,
		data,
		viewBox,
		preserveAspectRatio,
		features,
		paintCount,
		shapeCount,
		paintTableOffset,
		shapeTableOffset,
		commandStreamOffset,
	};
}

/** Reads paint table entry `index`. */
export function readPaint(asset: SvgAssetData, index: number): SvgPaint {
	if (index < 0 || index >= asset.paintCount) {
		fail(`paint index ${index} is out of range (0..${asset.paintCount - 1})`);
	}
	const at = asset.paintTableOffset + index * PAINT_ENTRY_SIZE;
	return {
		isCurrentColor: buffer.readu8(asset.data, at) === PAINT_KIND_CURRENT_COLOR,
		r: buffer.readu8(asset.data, at + 1),
		g: buffer.readu8(asset.data, at + 2),
		b: buffer.readu8(asset.data, at + 3),
		alpha: buffer.readf32(asset.data, at + 4),
	};
}

/** Reads shape table entry `index`. Shapes are in painter's order. */
export function readShape(asset: SvgAssetData, index: number): SvgShape {
	if (index < 0 || index >= asset.shapeCount) {
		fail(`shape index ${index} is out of range (0..${asset.shapeCount - 1})`);
	}
	const at = asset.shapeTableOffset + index * SHAPE_ENTRY_SIZE;
	const flags = buffer.readu8(asset.data, at + SHAPE_OFFSET_FLAGS);

	return {
		hasFill: (flags & SHAPE_FLAG_HAS_FILL) !== 0,
		hasStroke: (flags & SHAPE_FLAG_HAS_STROKE) !== 0,
		evenOdd: (flags & SHAPE_FLAG_FILL_RULE_EVEN_ODD) !== 0,
		strokeFirst: (flags & SHAPE_FLAG_STROKE_FIRST) !== 0,
		fillPaint: buffer.readu16(asset.data, at + SHAPE_OFFSET_FILL_PAINT),
		strokePaint: buffer.readu16(asset.data, at + SHAPE_OFFSET_STROKE_PAINT),
		strokeWidth: buffer.readf32(asset.data, at + SHAPE_OFFSET_STROKE_WIDTH),
		miterLimit: buffer.readf32(asset.data, at + SHAPE_OFFSET_MITER_LIMIT),
		lineCap: buffer.readu8(asset.data, at + SHAPE_OFFSET_LINE_CAP),
		lineJoin: buffer.readu8(asset.data, at + SHAPE_OFFSET_LINE_JOIN),
		commandOffset: buffer.readu32(asset.data, at + SHAPE_OFFSET_COMMAND_OFFSET),
		commandCount: buffer.readu32(asset.data, at + SHAPE_OFFSET_COMMAND_COUNT),
	};
}

/**
 * Walks a shape's geometry, calling back for each command.
 *
 * Coordinates are in view box space; scaling to pixels is the renderer's job,
 * which is what keeps a compiled asset resolution-independent.
 */
export function forEachCommand(
	asset: SvgAssetData,
	shape: SvgShape,
	visitor: SvgCommandVisitor,
): void {
	const data = asset.data;
	const limit = buffer.len(data);
	let at = asset.commandStreamOffset + shape.commandOffset;

	for (let i = 0; i < shape.commandCount; i++) {
		if (at >= limit) {
			fail("command stream ended early; the asset is truncated");
		}
		const opcode = buffer.readu8(data, at);
		at += 1;

		if (opcode === OP_MOVE_TO) {
			visitor.moveTo(buffer.readf32(data, at), buffer.readf32(data, at + 4));
			at += 8;
		} else if (opcode === OP_LINE_TO) {
			visitor.lineTo(buffer.readf32(data, at), buffer.readf32(data, at + 4));
			at += 8;
		} else if (opcode === OP_CUBIC_TO) {
			visitor.cubicTo(
				buffer.readf32(data, at),
				buffer.readf32(data, at + 4),
				buffer.readf32(data, at + 8),
				buffer.readf32(data, at + 12),
				buffer.readf32(data, at + 16),
				buffer.readf32(data, at + 20),
			);
			at += 24;
		} else if (opcode === OP_CLOSE) {
			visitor.close();
		} else {
			fail(`unknown opcode ${opcode} at byte ${at - 1}`);
		}
	}
}
