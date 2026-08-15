/**
 * Device-space vector arithmetic. Port of `svg-raster/src/geom.rs`.
 *
 * Everything under `raster/` works in *device space*: pixels, origin at the
 * top-left, y increasing downwards. The view box → device mapping happens
 * exactly once, at the top of the pipeline.
 *
 * # Representation
 *
 * A `Vec2` is a plain `{ x, y }` table, not a Roblox `Vector2`. This code runs
 * inside a game client, and the rasterizer manufactures thousands of points per
 * icon; a plain table is a cheap Luau allocation where a `Vector2` is a
 * userdata, and the hot scanline loops below this module work on bare numbers
 * anyway. Roblox types appear only at the package boundary.
 */

export interface Vec2 {
	x: number;
	y: number;
}

/**
 * `f32::MIN_POSITIVE`, kept verbatim from the Rust reference so the two
 * implementations agree on when a vector is too short to have a direction.
 */
const MIN_POSITIVE = 1.1754944e-38;

export function vec2(x: number, y: number): Vec2 {
	return { x, y };
}

/** True for a plain, comparable number: not NaN, not ±infinity. */
export function isFiniteNumber(value: number): boolean {
	return value === value && value !== math.huge && value !== -math.huge;
}

export function isFiniteVec(v: Vec2): boolean {
	return isFiniteNumber(v.x) && isFiniteNumber(v.y);
}

/** `a + b * k`, the shape almost every offset calculation takes. */
export function mulAdd(a: Vec2, b: Vec2, k: number): Vec2 {
	return { x: a.x + b.x * k, y: a.y + b.y * k };
}

export function sub(a: Vec2, b: Vec2): Vec2 {
	return { x: a.x - b.x, y: a.y - b.y };
}

export function dot(a: Vec2, b: Vec2): number {
	return a.x * b.x + a.y * b.y;
}

/**
 * The z component of the 3D cross product. Its sign is which way one direction
 * turns to reach another, which is how stroke joins decide which side of a
 * corner is the outside.
 */
export function cross(a: Vec2, b: Vec2): number {
	return a.x * b.y - a.y * b.x;
}

export function lengthSquared(v: Vec2): number {
	return v.x * v.x + v.y * v.y;
}

export function lengthOf(v: Vec2): number {
	return math.sqrt(lengthSquared(v));
}

/**
 * The unit vector in the same direction, or `undefined` for a vector too short
 * to have a well-defined direction.
 *
 * Returning `undefined` rather than dividing by a near-zero length is the whole
 * reason degenerate segments cannot produce NaN coordinates downstream.
 */
export function normalize(v: Vec2): Vec2 | undefined {
	const lsq = lengthSquared(v);
	// Finiteness first, so a NaN takes this branch rather than falling through
	// a comparison it would silently answer `false` to.
	if (!isFiniteNumber(lsq) || lsq <= MIN_POSITIVE) {
		return undefined;
	}
	const length = math.sqrt(lsq);
	return { x: v.x / length, y: v.y / length };
}

/**
 * The direction rotated a quarter turn. Used to offset a segment sideways;
 * the stroker only relies on it being *consistent*, and works out which side
 * is the outside of a corner from {@link cross}.
 */
export function normalOf(v: Vec2): Vec2 {
	return { x: -v.y, y: v.x };
}
