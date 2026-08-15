/**
 * Viewport fitting: view box + `preserveAspectRatio` + target rectangle → a
 * transform.
 *
 * # Why this exists as its own module
 *
 * A compiled asset is resolution independent: its geometry is in view box space
 * and the target rectangle arrives at render time. Every renderer therefore has
 * to perform this mapping, and a renderer that derives its own scale by hand is
 * a renderer that quietly disagrees with the others about what an asset looks
 * like.
 *
 * So the mapping is defined once per language and the two are held to each
 * other: `svg_core::view_box_transform` is the specification, this is its port,
 * and `tests/luau/spec.luau` checks this against values the Rust side produced.
 *
 * # The mapping
 *
 * ```text
 * sx = targetWidth  / viewBox.width
 * sy = targetHeight / viewBox.height
 *
 * none   →  (sx, sy)                 non-uniform, fills the target exactly
 * meet   →  (min(sx, sy), same)      whole view box visible, letterboxed
 * slice  →  (max(sx, sy), same)      target fully covered, view box cropped
 * ```
 *
 * The leftover space — negative under `slice`, which is what turns the same
 * arithmetic from letterboxing into cropping — is distributed according to the
 * alignment, and the view box origin is subtracted so that
 * `(viewBox.x, viewBox.y)` lands on the aligned corner.
 */

import type { SvgPreserveAspectRatio, SvgViewBox } from "../asset";
import { SvgAspectAlign, SvgAspectScale } from "../asset";

/**
 * A 2D affine transform, laid out like SVG's `matrix(a b c d e f)`.
 *
 * Only the scale and translation components are ever non-trivial here — a
 * viewport fit never rotates or skews — but the full shape is kept so that a
 * rasterizer can compose it with anything else without a special case.
 */
export interface SvgTransform {
	readonly sx: number;
	readonly ky: number;
	readonly kx: number;
	readonly sy: number;
	readonly tx: number;
	readonly ty: number;
}

/** The fraction of leftover horizontal space placed *before* the view box. */
function xFraction(align: number): number {
	if (
		align === SvgAspectAlign.XMidYMin ||
		align === SvgAspectAlign.XMidYMid ||
		align === SvgAspectAlign.XMidYMax
	) {
		return 0.5;
	}
	if (
		align === SvgAspectAlign.XMaxYMin ||
		align === SvgAspectAlign.XMaxYMid ||
		align === SvgAspectAlign.XMaxYMax
	) {
		return 1;
	}
	return 0;
}

/** The fraction of leftover vertical space placed *above* the view box. */
function yFraction(align: number): number {
	if (
		align === SvgAspectAlign.XMinYMid ||
		align === SvgAspectAlign.XMidYMid ||
		align === SvgAspectAlign.XMaxYMid
	) {
		return 0.5;
	}
	if (
		align === SvgAspectAlign.XMinYMax ||
		align === SvgAspectAlign.XMidYMax ||
		align === SvgAspectAlign.XMaxYMax
	) {
		return 1;
	}
	return 0;
}

/**
 * Maps view box space onto a `targetWidth` × `targetHeight` rectangle whose
 * top-left corner is the origin.
 *
 * `targetWidth` and `targetHeight` are expected to be positive; callers that
 * take them from a consumer snap and clamp first (see `resolveRenderOptions`).
 */
export function viewBoxTransform(
	viewBox: SvgViewBox,
	aspect: SvgPreserveAspectRatio,
	targetWidth: number,
	targetHeight: number,
): SvgTransform {
	let sx = targetWidth / viewBox.width;
	let sy = targetHeight / viewBox.height;

	if (aspect.align !== SvgAspectAlign.None) {
		const uniform =
			aspect.scale === SvgAspectScale.Slice
				? math.max(sx, sy)
				: math.min(sx, sy);
		sx = uniform;
		sy = uniform;
	}

	const leftoverX = targetWidth - viewBox.width * sx;
	const leftoverY = targetHeight - viewBox.height * sy;

	return {
		sx,
		ky: 0,
		kx: 0,
		sy,
		tx: -viewBox.x * sx + leftoverX * xFraction(aspect.align),
		ty: -viewBox.y * sy + leftoverY * yFraction(aspect.align),
	};
}

/**
 * The factor by which a viewport fit scales *lengths*, used to convert a stroke
 * width between view box units and output pixels.
 *
 * Under `meet` and `slice` the fit is a uniform scale, so this is exact. Under
 * `none` the two axes differ and no single number is correct — a circular pen
 * becomes an elliptical one — so the geometric mean `sqrt(sx * sy)` is used,
 * matching `svg_core::Transform::length_scale` and the approximation the
 * compiler already makes for skewed transforms.
 */
export function fitLengthScale(transform: SvgTransform): number {
	const determinant = transform.sx * transform.sy - transform.kx * transform.ky;
	return math.sqrt(math.abs(determinant));
}
