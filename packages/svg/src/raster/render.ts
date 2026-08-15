/**
 * The software rasterization pipeline: a decoded asset in, an RGBA8 buffer
 * out. Port of `svg-raster/src/render.rs`.
 *
 * ```text
 * SvgAssetData
 *     ├─ view box + preserveAspectRatio + target size -> transform  (render/fit)
 *     └─ for each shape, in painter's order:
 *            command stream -> adaptive flattening -> device contours
 *                 ├─ fill edges (implicitly closed)
 *                 └─ stroke expansion -> stroke edges
 *            scanline coverage -> source-over compositing
 * ```
 *
 * Contours are flattened **once per shape** and used by both the fill and the
 * stroke, and both converge on the same scan conversion — which is what makes
 * caps, joins, self-overlap and anti-aliasing behave identically for both.
 *
 * This module knows nothing about Roblox: no `EditableImage`, no `Color3`, no
 * instances. It is what the standalone Luau test suite exercises directly, and
 * what a future scheduler would call — no frame timing belongs in here.
 */

import type { SvgAssetData } from "../asset";
import { forEachCommand, readPaint, readShape, type SvgShape } from "../ir/decode";
import { fitLengthScale, viewBoxTransform } from "../render/fit";
import { ScanlineSupersampler } from "./coverage";
import { EdgeSet } from "./edges";
import { PathFlattener, type Contour } from "./flatten";
import type { Vec2 } from "./geom";
import { isFiniteNumber } from "./geom";
import { Canvas } from "./image";
import { expand, type StrokeStyle } from "./stroke";

/**
 * A rasterization request, with every unit already resolved.
 *
 * Colour channels are bare 0-255 numbers rather than `Color3` on purpose: the
 * compositor works in numbers, and converting per pixel would put a userdata
 * call in the hottest loop.
 */
export interface SoftwareRasterRequest {
	readonly pixelWidth: number;
	readonly pixelHeight: number;
	/** Coverage-only white alpha mask when true; full colour when false. */
	readonly alphaMask: boolean;
	/** What `currentColor` paints resolve to. Ignored under `alphaMask`. */
	readonly currentColorR: number;
	readonly currentColorG: number;
	readonly currentColorB: number;
	/**
	 * Replaces every shape's stroke width, in **view box units** — the single
	 * unit-conversion boundary is `resolveRenderOptions`, which has already
	 * folded `absoluteStrokeWidth` in. The pipeline multiplies by the fit's
	 * length scale exactly once, mirroring the reference's
	 * `device_stroke_width`.
	 */
	readonly strokeWidth?: number;
}

/**
 * Lightweight counters for diagnostics and benchmarks. Plain increments —
 * cheap enough to stay on unconditionally.
 */
export interface SoftwareRasterStats {
	/** Rasterizations performed since load. */
	rasterCount: number;
	/** Flattened contour points produced by the most recent rasterization. */
	lastPointCount: number;
	/** Directed edges produced by the most recent rasterization. */
	lastEdgeCount: number;
}

const stats: SoftwareRasterStats = { rasterCount: 0, lastPointCount: 0, lastEdgeCount: 0 };

export function getSoftwareRasterStats(): SoftwareRasterStats {
	return stats;
}

/**
 * Scratch reused across every shape and every rasterization; the pipeline is
 * single-threaded. A document is dozens of shapes and each would otherwise
 * allocate a contour list, an edge set and a coverage row per layer.
 */
const scratchContours: Contour[] = [];
const scratchPolygons: Vec2[][] = [];
const scratchEdges = new EdgeSet();
const scratchSampler = new ScanlineSupersampler();

function fail(message: string): never {
	return error(`@rbxts/svg: ${message}`);
}

/**
 * Rasterizes an asset into a straight RGBA8 buffer of exactly
 * `pixelWidth * pixelHeight * 4` bytes.
 *
 * Errors on unusable dimensions and on geometry that becomes non-finite under
 * the requested transform — reported rather than clamped, because a silently
 * relocated coordinate is a silently wrong picture.
 */
export function rasterize(asset: SvgAssetData, request: SoftwareRasterRequest): buffer {
	const width = request.pixelWidth;
	const height = request.pixelHeight;
	if (width !== math.floor(width) || height !== math.floor(height) || width < 1 || height < 1) {
		fail(`cannot rasterize ${width}x${height} pixels; both dimensions must be whole and at least 1`);
	}

	// The single definition of viewport fitting, shared with the compiler and
	// the Rust reference. Deriving a scale here instead would be how the two
	// renderers start disagreeing.
	const transform = viewBoxTransform(asset.viewBox, asset.preserveAspectRatio, width, height);
	if (
		!(
			isFiniteNumber(transform.sx) &&
			isFiniteNumber(transform.sy) &&
			isFiniteNumber(transform.kx) &&
			isFiniteNumber(transform.ky) &&
			isFiniteNumber(transform.tx) &&
			isFiniteNumber(transform.ty)
		)
	) {
		fail(`encountered non-finite geometry while rasterizing asset ${asset.id}`);
	}

	// How the fit scales lengths — what converts a stroke width in view box
	// units into pixels. Exact for `meet`/`slice`; the geometric mean under
	// `none`, matching `svg_core::Transform::length_scale`.
	const lengthScale = fitLengthScale(transform);

	const canvas = new Canvas(width, height);

	stats.rasterCount += 1;
	stats.lastPointCount = 0;
	stats.lastEdgeCount = 0;

	for (let index = 0; index < asset.shapeCount; index++) {
		const shape = readShape(asset, index);

		const flattener = new PathFlattener(transform, scratchContours);
		forEachCommand(asset, shape, flattener);
		if (!flattener.finish()) {
			fail(`encountered non-finite geometry while rasterizing asset ${asset.id}`);
		}
		if (scratchContours.size() === 0) {
			continue;
		}
		for (const contour of scratchContours) {
			stats.lastPointCount += contour.points.size();
		}

		if (shape.strokeFirst) {
			drawStroke(canvas, shape, asset, request, lengthScale);
			drawFill(canvas, shape, asset, request);
		} else {
			drawFill(canvas, shape, asset, request);
			drawStroke(canvas, shape, asset, request, lengthScale);
		}
	}

	return canvas.finish(request.alphaMask);
}

function drawFill(
	canvas: Canvas,
	shape: SvgShape,
	asset: SvgAssetData,
	request: SoftwareRasterRequest,
): void {
	if (!shape.hasFill) {
		return;
	}
	const paint = readPaint(asset, shape.fillPaint);
	if (paint.alpha <= 0) {
		return;
	}

	scratchEdges.clear();
	for (const contour of scratchContours) {
		// Every contour is closed here, whether or not the author wrote `Z`:
		// SVG closes fill contours implicitly, applied at the edge builder so
		// the canonical geometry — which the stroker still needs to see as
		// open — stays untouched.
		scratchEdges.addPolygon(contour.points);
	}
	scratchEdges.finish();

	composite(canvas, shape.evenOdd, request, paint.isCurrentColor, paint.r, paint.g, paint.b, paint.alpha);
}

function drawStroke(
	canvas: Canvas,
	shape: SvgShape,
	asset: SvgAssetData,
	request: SoftwareRasterRequest,
	lengthScale: number,
): void {
	if (!shape.hasStroke) {
		return;
	}
	const paint = readPaint(asset, shape.strokePaint);
	if (paint.alpha <= 0) {
		return;
	}

	// The stroke width to use, in device pixels: the override when one was
	// given, the shape's own width otherwise — both in view box units, both
	// scaled by the fit.
	const viewBoxWidth = request.strokeWidth ?? shape.strokeWidth;
	const width = viewBoxWidth * lengthScale;
	if (!isFiniteNumber(width) || width <= 0) {
		return;
	}

	const style: StrokeStyle = {
		width,
		cap: shape.lineCap,
		join: shape.lineJoin,
		miterLimit: shape.miterLimit,
	};
	expand(scratchContours, style, scratchPolygons);
	if (scratchPolygons.size() === 0) {
		return;
	}

	scratchEdges.clear();
	for (const polygon of scratchPolygons) {
		scratchEdges.addPolygon(polygon);
	}
	scratchEdges.finish();

	// Always non-zero: a stroke outline overlaps itself wherever the path
	// does, and even-odd would punch holes through exactly those places. The
	// shape's own fill rule governs its interior, not its outline.
	composite(canvas, false, request, paint.isCurrentColor, paint.r, paint.g, paint.b, paint.alpha);
}

function composite(
	canvas: Canvas,
	evenOdd: boolean,
	request: SoftwareRasterRequest,
	isCurrentColor: boolean,
	paintR: number,
	paintG: number,
	paintB: number,
	alpha: number,
): void {
	if (scratchEdges.isEmpty()) {
		return;
	}
	stats.lastEdgeCount += scratchEdges.edges.size();

	let r = paintR;
	let g = paintG;
	let b = paintB;
	if (isCurrentColor) {
		r = request.currentColorR;
		g = request.currentColorG;
		b = request.currentColorB;
	}

	const mask = request.alphaMask;
	scratchSampler.rasterize(
		scratchEdges,
		evenOdd,
		request.pixelWidth,
		request.pixelHeight,
		(y, row, startX, endX) => {
			if (mask) {
				canvas.blendRowAlpha(y, row, startX, endX, alpha);
			} else {
				canvas.blendRow(y, row, startX, endX, r, g, b, alpha);
			}
		},
	);
}
