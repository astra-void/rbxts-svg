/**
 * Adaptive cubic flattening, and the contour extraction that uses it.
 * Port of `svg-raster/src/flatten.rs`; that file's commentary is the
 * specification and is not repeated here.
 *
 * The one structural difference from Rust: geometry arrives through the IR
 * decoder's visitor rather than as a command slice, so contour extraction is a
 * visitor ({@link PathFlattener}) instead of a loop over commands. The visitor
 * cannot abort iteration, so a non-finite coordinate sets a flag that
 * {@link PathFlattener.finish} reports, where Rust returns `false` early —
 * same outcome, checked at the same boundary.
 */

import type { SvgCommandVisitor } from "../ir/decode";
import type { SvgTransform } from "../render/fit";
import type { Vec2 } from "./geom";
import { isFiniteNumber } from "./geom";

/**
 * Flatness tolerance, in **output pixels**. Identical to the Rust reference —
 * a constant rather than an option on purpose, because a knob that must be set
 * identically in two places is a knob that eventually is not.
 */
export const FLATNESS_TOLERANCE = 0.1;

/** Hard ceiling on how many times one cubic may be halved. */
export const MAX_SUBDIVISION_DEPTH = 12;

/** Points closer together than this are treated as the same point. */
const COINCIDENT_EPSILON = 1e-3;

/** One flattened subpath, in device space. */
export interface Contour {
	/** Points along the subpath, consecutive duplicates already removed. */
	points: Vec2[];
	/**
	 * Whether the subpath was explicitly closed with `Z`. Matters to the
	 * stroker (join vs cap); the filler always treats a contour as closed,
	 * which is applied at the edge builder rather than by editing this.
	 */
	closed: boolean;
}

function coincident(ax: number, ay: number, bx: number, by: number): boolean {
	return math.abs(ax - bx) <= COINCIDENT_EPSILON && math.abs(ay - by) <= COINCIDENT_EPSILON;
}

/** Appends `(x, y)` unless it repeats the previous point. */
function pushUnique(points: Vec2[], x: number, y: number): void {
	const last = points[points.size() - 1];
	if (last !== undefined && coincident(last.x, last.y, x, y)) {
		return;
	}
	points.push({ x, y });
}

/**
 * Appends a cubic's flattened segments to `out`, excluding its start point,
 * which the caller has already emitted. All arguments are device-space
 * coordinates.
 *
 * Iterative binary subdivision against the flatness test, with an explicit
 * stack rather than recursion — the depth limit is then a plain loop bound.
 * The stack holds flat numbers (nine per frame) so subdivision allocates no
 * intermediate tables.
 */
export function flattenCubic(
	p0x: number,
	p0y: number,
	p1x: number,
	p1y: number,
	p2x: number,
	p2y: number,
	p3x: number,
	p3y: number,
	out: Vec2[],
): void {
	// Frames of 9: x0 y0 x1 y1 x2 y2 x3 y3 depth. `top` is the frame count.
	const stack: number[] = [];
	stack[0] = p0x;
	stack[1] = p0y;
	stack[2] = p1x;
	stack[3] = p1y;
	stack[4] = p2x;
	stack[5] = p2y;
	stack[6] = p3x;
	stack[7] = p3y;
	stack[8] = 0;
	let top = 1;

	while (top > 0) {
		top -= 1;
		const base = top * 9;
		const x0 = stack[base];
		const y0 = stack[base + 1];
		const x1 = stack[base + 2];
		const y1 = stack[base + 3];
		const x2 = stack[base + 4];
		const y2 = stack[base + 5];
		const x3 = stack[base + 6];
		const y3 = stack[base + 7];
		const depth = stack[base + 8];

		if (depth >= MAX_SUBDIVISION_DEPTH || cubicIsFlat(x0, y0, x1, y1, x2, y2, x3, y3)) {
			pushUnique(out, x3, y3);
			continue;
		}

		// de Casteljau split at t = 0.5, which is exact.
		const p01x = (x0 + x1) * 0.5;
		const p01y = (y0 + y1) * 0.5;
		const p12x = (x1 + x2) * 0.5;
		const p12y = (y1 + y2) * 0.5;
		const p23x = (x2 + x3) * 0.5;
		const p23y = (y2 + y3) * 0.5;
		const p012x = (p01x + p12x) * 0.5;
		const p012y = (p01y + p12y) * 0.5;
		const p123x = (p12x + p23x) * 0.5;
		const p123y = (p12y + p23y) * 0.5;
		const centreX = (p012x + p123x) * 0.5;
		const centreY = (p012y + p123y) * 0.5;

		// The far half goes on first so the near half pops first: the output
		// has to come out in curve order.
		let at = top * 9;
		stack[at] = centreX;
		stack[at + 1] = centreY;
		stack[at + 2] = p123x;
		stack[at + 3] = p123y;
		stack[at + 4] = p23x;
		stack[at + 5] = p23y;
		stack[at + 6] = x3;
		stack[at + 7] = y3;
		stack[at + 8] = depth + 1;
		at += 9;
		stack[at] = x0;
		stack[at + 1] = y0;
		stack[at + 2] = p01x;
		stack[at + 3] = p01y;
		stack[at + 4] = p012x;
		stack[at + 5] = p012y;
		stack[at + 6] = centreX;
		stack[at + 7] = centreY;
		stack[at + 8] = depth + 1;
		top += 2;
	}
}

/**
 * True when both control points lie within {@link FLATNESS_TOLERANCE} of the
 * chord: the curve lies inside the convex hull of its control points, so no
 * point of it is further from the chord than they are.
 */
function cubicIsFlat(
	x0: number,
	y0: number,
	x1: number,
	y1: number,
	x2: number,
	y2: number,
	x3: number,
	y3: number,
): boolean {
	const chordX = x3 - x0;
	const chordY = y3 - y0;
	const chordLengthSquared = chordX * chordX + chordY * chordY;

	if (chordLengthSquared <= COINCIDENT_EPSILON * COINCIDENT_EPSILON) {
		// A closed loop: the chord is a point, so measure from that point —
		// the curve is flat only if the whole hull has collapsed onto it.
		const d1 = math.sqrt((x1 - x0) * (x1 - x0) + (y1 - y0) * (y1 - y0));
		const d2 = math.sqrt((x2 - x0) * (x2 - x0) + (y2 - y0) * (y2 - y0));
		return math.max(d1, d2) <= FLATNESS_TOLERANCE;
	}

	// |cross| / |chord| is the perpendicular distance; comparing squares keeps
	// the square root out of the inner loop.
	const d1 = chordX * (y1 - y0) - chordY * (x1 - x0);
	const d2 = chordX * (y2 - y0) - chordY * (x2 - x0);
	const worst = math.max(math.abs(d1), math.abs(d2));
	return worst * worst <= FLATNESS_TOLERANCE * FLATNESS_TOLERANCE * chordLengthSquared;
}

/**
 * Builds device-space contours from a shape's command stream.
 *
 * ```ts
 * const flattener = new PathFlattener(transform, contours);
 * forEachCommand(asset, shape, flattener);
 * if (!flattener.finish()) { ... non-finite geometry ... }
 * ```
 */
export class PathFlattener implements SvgCommandVisitor {
	private open: Contour | undefined;
	private currentX = 0;
	private currentY = 0;
	private startX = 0;
	private startY = 0;
	private nonFinite = false;

	private readonly sx: number;
	private readonly kx: number;
	private readonly ky: number;
	private readonly sy: number;
	private readonly tx: number;
	private readonly ty: number;

	/** Existing contents of `out` are replaced. */
	constructor(
		transform: SvgTransform,
		private readonly out: Contour[],
	) {
		this.sx = transform.sx;
		this.kx = transform.kx;
		this.ky = transform.ky;
		this.sy = transform.sy;
		this.tx = transform.tx;
		this.ty = transform.ty;
		out.clear();
	}

	private mapX(x: number, y: number): number {
		return this.sx * x + this.kx * y + this.tx;
	}

	private mapY(x: number, y: number): number {
		return this.ky * x + this.sy * y + this.ty;
	}

	private pushOpen(): void {
		if (this.open !== undefined) {
			this.out.push(this.open);
			this.open = undefined;
		}
	}

	/**
	 * Returns the open contour, starting a fresh one at the current point if
	 * the previous was ended by a `Z` — `M 0 0 L 5 0 Z L 9 9` is legal.
	 */
	private reopen(): Contour {
		if (this.open === undefined) {
			this.open = { points: [{ x: this.currentX, y: this.currentY }], closed: false };
		}
		return this.open;
	}

	moveTo(x: number, y: number): void {
		if (this.nonFinite) {
			return;
		}
		this.pushOpen();
		const dx = this.mapX(x, y);
		const dy = this.mapY(x, y);
		if (!(isFiniteNumber(dx) && isFiniteNumber(dy))) {
			this.nonFinite = true;
			return;
		}
		this.currentX = dx;
		this.currentY = dy;
		this.startX = dx;
		this.startY = dy;
		this.open = { points: [{ x: dx, y: dy }], closed: false };
	}

	lineTo(x: number, y: number): void {
		if (this.nonFinite) {
			return;
		}
		const dx = this.mapX(x, y);
		const dy = this.mapY(x, y);
		if (!(isFiniteNumber(dx) && isFiniteNumber(dy))) {
			this.nonFinite = true;
			return;
		}
		pushUnique(this.reopen().points, dx, dy);
		this.currentX = dx;
		this.currentY = dy;
	}

	cubicTo(c1x: number, c1y: number, c2x: number, c2y: number, x: number, y: number): void {
		if (this.nonFinite) {
			return;
		}
		const d1x = this.mapX(c1x, c1y);
		const d1y = this.mapY(c1x, c1y);
		const d2x = this.mapX(c2x, c2y);
		const d2y = this.mapY(c2x, c2y);
		const dx = this.mapX(x, y);
		const dy = this.mapY(x, y);
		if (
			!(
				isFiniteNumber(d1x) &&
				isFiniteNumber(d1y) &&
				isFiniteNumber(d2x) &&
				isFiniteNumber(d2y) &&
				isFiniteNumber(dx) &&
				isFiniteNumber(dy)
			)
		) {
			this.nonFinite = true;
			return;
		}
		flattenCubic(
			this.currentX,
			this.currentY,
			d1x,
			d1y,
			d2x,
			d2y,
			dx,
			dy,
			this.reopen().points,
		);
		this.currentX = dx;
		this.currentY = dy;
	}

	close(): void {
		if (this.nonFinite) {
			return;
		}
		if (this.open !== undefined) {
			this.open.closed = true;
		}
		this.pushOpen();
		// SVG puts the current point back at the subpath's start, so a drawing
		// command after `Z` continues from there.
		this.currentX = this.startX;
		this.currentY = this.startY;
	}

	/**
	 * Flushes the trailing contour and normalises the output. Returns `false`
	 * if any coordinate went non-finite, in which case the partial output must
	 * not be used.
	 */
	finish(): boolean {
		this.pushOpen();
		if (this.nonFinite) {
			return false;
		}
		// A closed contour's final point repeats its first; the segment
		// builders wrap around instead, so carrying the duplicate would only
		// produce zero-length edges. Also drop empty contours.
		const out = this.out;
		let writeIndex = 0;
		for (let index = 0; index < out.size(); index++) {
			const contour = out[index];
			const points = contour.points;
			const count = points.size();
			if (contour.closed && count > 1) {
				const first = points[0];
				const last = points[count - 1];
				if (coincident(first.x, first.y, last.x, last.y)) {
					points.pop();
				}
			}
			if (points.size() > 0) {
				out[writeIndex] = contour;
				writeIndex += 1;
			}
		}
		// Truncate anything left over from the compaction.
		for (let index = out.size() - 1; index >= writeIndex; index--) {
			out.pop();
		}
		return true;
	}
}
