/**
 * Stroke expansion: an outline turned into the area it covers.
 * Port of `svg-raster/src/stroke.rs`; see that file for the full rationale.
 *
 * The stroke is emitted as a *set* of simple polygons — one quadrilateral per
 * segment, one wedge per join, one per cap — all wound the same way and filled
 * with the non-zero rule. Non-zero over same-wound polygons is exactly their
 * union, so overlap is what the fill rule is for, and no outline stitching is
 * needed.
 *
 * Curves were flattened before reaching here; the polyline is what gets
 * stroked. That is the reference renderer's deliberate trade (see
 * `docs/ROADMAP.md`), reproduced rather than "improved".
 */

import {
	LINE_CAP_BUTT,
	LINE_CAP_ROUND,
	LINE_CAP_SQUARE,
	LINE_JOIN_BEVEL,
	LINE_JOIN_MITER,
	LINE_JOIN_ROUND,
} from "../ir/format";
import type { Contour } from "./flatten";
import type { Vec2 } from "./geom";
import { isFiniteNumber, isFiniteVec } from "./geom";

/**
 * How a stroke is drawn. Width is in **device pixels**, unlike the IR's shape
 * table, whose width is in view box units.
 */
export interface StrokeStyle {
	/** Total width in device pixels. The pen's radius is half this. */
	width: number;
	/** A `LINE_CAP_*` value from `../ir/format`. */
	cap: number;
	/** A `LINE_JOIN_*` value from `../ir/format`. */
	join: number;
	/** SVG's `stroke-miterlimit`. */
	miterLimit: number;
}

/**
 * Below this, two unit directions count as parallel and their turn as no turn.
 * Their cross product is the sine of the turn angle: about a thousandth of a
 * degree.
 */
const PARALLEL_EPSILON = 1e-5;

/**
 * Arc chord tolerance for round joins and caps, in device pixels. Deliberately
 * the same scale as `FLATNESS_TOLERANCE`, and identical to the Rust reference.
 */
export const ARC_TOLERANCE = 0.1;

/** Ceiling on the segments in one arc. */
const MAX_ARC_SEGMENTS = 256;

/** One segment of a contour, with its unit direction and offset normal. */
interface Segment {
	startX: number;
	startY: number;
	endX: number;
	endY: number;
	directionX: number;
	directionY: number;
	normalX: number;
	normalY: number;
}

/** Scratch reused across calls; the expansion is single-threaded. */
const segmentScratch: Segment[] = [];

/**
 * Expands `contours` into the polygons their stroke covers.
 *
 * The polygons are implicitly closed, wound consistently, and must be filled
 * with the **non-zero** rule. Existing contents of `out` are replaced.
 */
export function expand(contours: Contour[], style: StrokeStyle, out: Vec2[][]): void {
	out.clear();

	const radius = style.width * 0.5;
	// Finiteness first, so a NaN width takes this branch rather than falling
	// through a comparison that would silently answer `false`.
	if (!isFiniteNumber(radius) || radius <= 0) {
		return;
	}

	const segments = segmentScratch;

	for (const contour of contours) {
		buildSegments(contour.points, contour.closed, segments);

		if (segments.size() === 0) {
			// No length at all. A cap with area still paints here — SVG says
			// so explicitly, and it is how a single-point subpath draws a dot.
			const point = contour.points[0];
			if (point !== undefined && !contour.closed) {
				emitDegenerateCap(out, point.x, point.y, radius, style.cap);
			}
			continue;
		}

		// One quadrilateral per segment: the pen swept along it.
		for (const segment of segments) {
			pushPolygon(out, [
				{ x: segment.startX + segment.normalX * radius, y: segment.startY + segment.normalY * radius },
				{ x: segment.endX + segment.normalX * radius, y: segment.endY + segment.normalY * radius },
				{ x: segment.endX - segment.normalX * radius, y: segment.endY - segment.normalY * radius },
				{ x: segment.startX - segment.normalX * radius, y: segment.startY - segment.normalY * radius },
			]);
		}

		// One wedge per interior vertex, filling the gap the bend opens on the
		// outside of the corner. A closed contour bends at its start point too.
		const count = segments.size();
		const firstJoin = contour.closed ? 0 : 1;
		for (let index = firstJoin; index < count; index++) {
			const current = segments[index];
			const previous = segments[(index + count - 1) % count];
			emitJoin(out, current.startX, current.startY, previous, current, radius, style);
		}

		if (!contour.closed) {
			const last = segments[count - 1];
			const first = segments[0];
			emitCap(out, last.endX, last.endY, last.directionX, last.directionY, radius, style.cap);
			emitCap(out, first.startX, first.startY, -first.directionX, -first.directionY, radius, style.cap);
		}
	}
}

function buildSegments(points: Vec2[], closed: boolean, out: Segment[]): void {
	out.clear();
	const pointCount = points.size();
	if (pointCount < 2) {
		return;
	}
	const count = closed ? pointCount : pointCount - 1;
	for (let index = 0; index < count; index++) {
		const start = points[index];
		const finish = points[(index + 1) % pointCount];
		// Flattening already collapsed coincident points, but a directly
		// constructed contour might not have, and a zero-length segment has no
		// direction to offset along.
		const dx = finish.x - start.x;
		const dy = finish.y - start.y;
		const lsq = dx * dx + dy * dy;
		if (!isFiniteNumber(lsq) || lsq <= 1.1754944e-38) {
			continue;
		}
		const length = math.sqrt(lsq);
		const directionX = dx / length;
		const directionY = dy / length;
		out.push({
			startX: start.x,
			startY: start.y,
			endX: finish.x,
			endY: finish.y,
			directionX,
			directionY,
			// The quarter-turn normal, (-y, x).
			normalX: -directionY,
			normalY: directionX,
		});
	}
}

/**
 * Adds a polygon, normalising its winding: the non-zero union only works if
 * every piece is wound the same way, so each polygon's signed area is measured
 * and it is flipped if need be.
 */
function pushPolygon(out: Vec2[][], points: Vec2[]): void {
	if (points.size() < 3) {
		return;
	}
	for (const point of points) {
		if (!isFiniteVec(point)) {
			return;
		}
	}
	if (signedArea(points) < 0) {
		reverseInPlace(points);
	}
	out.push(points);
}

function reverseInPlace(points: Vec2[]): void {
	let low = 0;
	let high = points.size() - 1;
	while (low < high) {
		const temp = points[low];
		points[low] = points[high];
		points[high] = temp;
		low += 1;
		high -= 1;
	}
}

/** Twice the signed area — the sign is all that is needed. */
export function signedArea(points: Vec2[]): number {
	let total = 0;
	const count = points.size();
	for (let index = 0; index < count; index++) {
		const a = points[index];
		const b = points[(index + 1) % count];
		total += a.x * b.y - a.y * b.x;
	}
	return total;
}

/**
 * Fills the wedge a bend opens on the outside of a corner. Only the outside
 * needs anything: on the inside the two segment quadrilaterals already overlap
 * and the non-zero rule merges them.
 */
function emitJoin(
	out: Vec2[][],
	vertexX: number,
	vertexY: number,
	previous: Segment,
	current: Segment,
	radius: number,
	style: StrokeStyle,
): void {
	const turn = previous.directionX * current.directionY - previous.directionY * current.directionX;
	const straight = previous.directionX * current.directionX + previous.directionY * current.directionY;

	if (math.abs(turn) <= PARALLEL_EPSILON) {
		if (straight >= 0) {
			// Collinear and continuing: the quadrilaterals already meet flush.
			return;
		}
		// A cusp — the path doubles back. Both sides are "outside" and the
		// miter is infinite by definition, so only a round join has anything
		// to add: the disc the pen sweeps as it pivots through half a turn on
		// each side, which together is a full one.
		if (style.join === LINE_JOIN_ROUND) {
			emitDegenerateCap(out, vertexX, vertexY, radius, LINE_CAP_ROUND);
		}
		return;
	}

	// The bend turns towards the normal side when the cross product is
	// positive, which puts the *outside* on the other one.
	const side = turn > 0 ? -1 : 1;
	const fromX = previous.normalX * side;
	const fromY = previous.normalY * side;
	const toX = current.normalX * side;
	const toY = current.normalY * side;

	const wedge: Vec2[] = [
		{ x: vertexX, y: vertexY },
		{ x: vertexX + fromX * radius, y: vertexY + fromY * radius },
	];

	if (style.join === LINE_JOIN_MITER) {
		miterApex(wedge, vertexX, vertexY, fromX, fromY, toX, toY, radius, style.miterLimit);
		// Beyond the limit SVG falls back to a bevel, which is this wedge
		// without its apex: a triangle straight across the corner.
	} else if (style.join === LINE_JOIN_ROUND) {
		emitArc(wedge, vertexX, vertexY, fromX, fromY, toX, toY, radius);
	}
	// LINE_JOIN_BEVEL adds nothing between the two offset points.

	wedge.push({ x: vertexX + toX * radius, y: vertexY + toY * radius });
	pushPolygon(out, wedge);
}

/**
 * Appends the miter apex — where the two outer offset lines meet — unless that
 * point is further out than `miterLimit` allows.
 *
 * SVG defines the limit as `miterLength / strokeWidth`; for an interior angle
 * `θ` the ratio is `1 / sin(θ/2)`, and the bisector of the two outer normals
 * is a unit vector whose dot product with either is exactly `sin(θ/2)`.
 */
function miterApex(
	wedge: Vec2[],
	vertexX: number,
	vertexY: number,
	fromX: number,
	fromY: number,
	toX: number,
	toY: number,
	radius: number,
	miterLimit: number,
): void {
	// A cusp makes the two normals opposite and the bisector vanish. That is
	// the infinite-miter case, so bevelling is the correct answer anyway.
	const sumX = fromX + toX;
	const sumY = fromY + toY;
	const sumLsq = sumX * sumX + sumY * sumY;
	if (!isFiniteNumber(sumLsq) || sumLsq <= 1.1754944e-38) {
		return;
	}
	const sumLength = math.sqrt(sumLsq);
	const bisectorX = sumX / sumLength;
	const bisectorY = sumY / sumLength;

	const halfAngleSine = bisectorX * fromX + bisectorY * fromY;
	if (halfAngleSine <= 1.1754944e-38) {
		return;
	}
	const ratio = 1 / halfAngleSine;
	if (!isFiniteNumber(miterLimit) || ratio > miterLimit) {
		return;
	}

	const apexX = vertexX + bisectorX * radius * ratio;
	const apexY = vertexY + bisectorY * radius * ratio;
	if (isFiniteNumber(apexX) && isFiniteNumber(apexY)) {
		wedge.push({ x: apexX, y: apexY });
	}
}

/**
 * Appends the interior points of an arc of `radius` about the centre, sweeping
 * from `from` to `to` (both unit vectors) the short way round.
 */
function emitArc(
	out: Vec2[],
	centreX: number,
	centreY: number,
	fromX: number,
	fromY: number,
	toX: number,
	toY: number,
	radius: number,
): void {
	const sweep = math.atan2(fromX * toY - fromY * toX, fromX * toX + fromY * toY);
	emitArcSweep(out, centreX, centreY, fromX, fromY, sweep, radius);
}

/**
 * Appends the interior points of an arc starting at `from` and turning through
 * `sweep` radians. Separate from {@link emitArc} because a half turn's
 * direction cannot be recovered from its endpoints.
 */
function emitArcSweep(
	out: Vec2[],
	centreX: number,
	centreY: number,
	fromX: number,
	fromY: number,
	sweep: number,
	radius: number,
): void {
	if (!isFiniteNumber(sweep) || math.abs(sweep) <= PARALLEL_EPSILON) {
		return;
	}

	// A chord subtending angle `d` on a circle of radius `r` falls short of
	// the arc by `r * (1 - cos(d / 2))`; solving for the tolerance gives the
	// largest step that stays within it.
	let step: number;
	if (radius > ARC_TOLERANCE) {
		step = 2 * math.acos(math.clamp(1 - ARC_TOLERANCE / radius, -1, 1));
	} else {
		// The whole arc is already smaller than the tolerance: one chord.
		step = math.abs(sweep);
	}

	let segments: number;
	if (step > 0) {
		segments = math.clamp(math.ceil(math.abs(sweep) / step), 1, MAX_ARC_SEGMENTS);
	} else {
		segments = 1;
	}

	const stepAngle = sweep / segments;
	const sinStep = math.sin(stepAngle);
	const cosStep = math.cos(stepAngle);
	let directionX = fromX;
	let directionY = fromY;
	for (let index = 1; index < segments; index++) {
		const rotatedX = directionX * cosStep - directionY * sinStep;
		const rotatedY = directionX * sinStep + directionY * cosStep;
		directionX = rotatedX;
		directionY = rotatedY;
		out.push({ x: centreX + directionX * radius, y: centreY + directionY * radius });
	}
}

/** Closes off an open end. `direction` points *out* of the path at this end. */
function emitCap(
	out: Vec2[][],
	endX: number,
	endY: number,
	directionX: number,
	directionY: number,
	radius: number,
	cap: number,
): void {
	const normalX = -directionY;
	const normalY = directionX;
	if (cap === LINE_CAP_SQUARE) {
		const extendedX = endX + directionX * radius;
		const extendedY = endY + directionY * radius;
		pushPolygon(out, [
			{ x: endX + normalX * radius, y: endY + normalY * radius },
			{ x: extendedX + normalX * radius, y: extendedY + normalY * radius },
			{ x: extendedX - normalX * radius, y: extendedY - normalY * radius },
			{ x: endX - normalX * radius, y: endY - normalY * radius },
		]);
	} else if (cap === LINE_CAP_ROUND) {
		// A half turn from `normal` round to `-normal`, swept the way that
		// carries the arc out over `direction` rather than back across the
		// path.
		const halfDisc: Vec2[] = [{ x: endX + normalX * radius, y: endY + normalY * radius }];
		emitArcSweep(halfDisc, endX, endY, normalX, normalY, -math.pi, radius);
		halfDisc.push({ x: endX - normalX * radius, y: endY - normalY * radius });
		pushPolygon(out, halfDisc);
	}
	// LINE_CAP_BUTT adds nothing: the segment's own quadrilateral already
	// stops exactly at the endpoint, which is what `butt` means.
}

/**
 * The area a cap paints where the path has no length at all: nothing under
 * butt, a full circle under round, an axis-aligned square under square.
 */
function emitDegenerateCap(out: Vec2[][], x: number, y: number, radius: number, cap: number): void {
	if (cap === LINE_CAP_SQUARE) {
		pushPolygon(out, [
			{ x: x - radius, y: y - radius },
			{ x: x + radius, y: y - radius },
			{ x: x + radius, y: y + radius },
			{ x: x - radius, y: y + radius },
		]);
	} else if (cap === LINE_CAP_ROUND) {
		const circle: Vec2[] = [{ x: x + radius, y: y }];
		emitArcSweep(circle, x, y, 1, 0, 2 * math.pi, radius);
		pushPolygon(out, circle);
	}
	// LINE_CAP_BUTT paints nothing.
}
