/**
 * The public asset type.
 *
 * `SvgAsset` is intentionally opaque. Consumers receive one, hold it, and hand
 * it to a renderer; they cannot read its bytes, and nothing they write can come
 * to depend on how it is encoded. That freedom is the whole point — the
 * serialized representation is expected to change (packed bytes today,
 * fixed-point or a denser command stream later) and none of those changes
 * should be visible here.
 */

declare const svgAssetBrand: unique symbol;

/**
 * A compiled SVG asset.
 *
 * Produced at build time from a `.svg` file and consumed by a renderer. It
 * carries no framework, no Roblox instance and no React binding: the same asset
 * is used by the Roblox renderer, by `@rbxts/svg-react`, and eventually by a
 * DOM/Loom renderer.
 */
export interface SvgAsset {
	readonly [svgAssetBrand]: true;
}

/** The coordinate system an asset's geometry is expressed in. */
export interface SvgViewBox {
	readonly x: number;
	readonly y: number;
	readonly width: number;
	readonly height: number;
}

/**
 * Where the scaled view box sits inside the target rectangle.
 *
 * Mirrors `svg_core::AspectAlign`, and the values are the serialized
 * discriminants. `None` is SVG's `preserveAspectRatio="none"`: stretch
 * independently in X and Y.
 */
export const SvgAspectAlign = {
	None: 0,
	XMinYMin: 1,
	XMidYMin: 2,
	XMaxYMin: 3,
	XMinYMid: 4,
	/** The SVG default. */
	XMidYMid: 5,
	XMaxYMid: 6,
	XMinYMax: 7,
	XMidYMax: 8,
	XMaxYMax: 9,
} as const;

/** Whether the view box is fitted inside the target or made to cover it. */
export const SvgAspectScale = {
	/** Fit the whole view box inside the target; space may be left over. */
	Meet: 0,
	/** Cover the target; the view box overflows and is cropped. */
	Slice: 1,
} as const;

/**
 * An asset's viewport-fitting policy, i.e. SVG's `preserveAspectRatio`.
 *
 * Carried because it is not recoverable from the view box: a 24×12 asset drawn
 * into a 100×100 square is letterboxed under `xMidYMid meet` and stretched
 * under `none`, and only the source document knows which the author meant.
 */
export interface SvgPreserveAspectRatio {
	/** An `SvgAspectAlign` value. */
	readonly align: number;
	/** An `SvgAspectScale` value. Ignored when `align` is `None`. */
	readonly scale: number;
}

/**
 * Compile-time facts about an asset, mirroring `svg_core::FeatureFlags`.
 *
 * Bit values are part of the serialized format and never change.
 */
export const SvgFeature = {
	UsesCurrentColor: 1,
	HasFill: 2,
	HasStroke: 4,
	HasEvenOddFill: 8,
	Monochrome: 16,
	HasTransparency: 32,
	HasStrokeFirst: 64,
} as const;

/**
 * The runtime representation behind an [`SvgAsset`].
 *
 * Internal: reachable only through `$internal`, never re-exported from the
 * package root. Table offsets are precomputed at load time so that reading a
 * shape is a fixed-offset buffer read rather than a walk.
 */
export interface SvgAssetData {
	/**
	 * A stable identity for cache keys. The content hash when one is known
	 * (which is what lets two copies of the same icon share a raster), and
	 * otherwise a unique per-instance id.
	 */
	readonly id: string;
	readonly data: buffer;
	readonly viewBox: SvgViewBox;
	readonly preserveAspectRatio: SvgPreserveAspectRatio;
	readonly features: number;
	readonly paintCount: number;
	readonly shapeCount: number;
	readonly paintTableOffset: number;
	readonly shapeTableOffset: number;
	readonly commandStreamOffset: number;
}

/**
 * Reinterprets an asset as its runtime data.
 *
 * The brand exists to stop consumers doing this; internal code may, because
 * internal code is what created the value in the first place.
 */
export function assetData(asset: SvgAsset): SvgAssetData {
	return asset as unknown as SvgAssetData;
}

/** The inverse of {@link assetData}. */
export function asAsset(data: SvgAssetData): SvgAsset {
	return data as unknown as SvgAsset;
}

/** The asset's intrinsic coordinate system. */
export function getViewBox(asset: SvgAsset): SvgViewBox {
	return assetData(asset).viewBox;
}

/**
 * How the asset should fill a target rectangle whose aspect ratio differs from
 * its view box's.
 *
 * Feed it, the view box and the target size to {@link viewBoxTransform} rather
 * than deriving a scale by hand — that function is the single definition of
 * viewport fitting, shared with the Rust reference rasterizer.
 */
export function getPreserveAspectRatio(asset: SvgAsset): SvgPreserveAspectRatio {
	return assetData(asset).preserveAspectRatio;
}

/** The asset's `SvgFeature` bitset. */
export function getFeatures(asset: SvgAsset): number {
	return assetData(asset).features;
}

/** How many shapes the asset contains. */
export function getShapeCount(asset: SvgAsset): number {
	return assetData(asset).shapeCount;
}

/**
 * Whether the asset can be rasterized once and recoloured per instance.
 *
 * True when every paint in the asset is the same `currentColor`. Such an asset
 * rasterizes to a single alpha mask that any `ImageColor3` tints correctly, so
 * the render cache deliberately leaves colour out of its key.
 */
export function isTintable(asset: SvgAsset): boolean {
	const features = assetData(asset).features;
	return (
		(features & SvgFeature.Monochrome) !== 0 &&
		(features & SvgFeature.UsesCurrentColor) !== 0
	);
}

/**
 * Whether any paint in the asset is `currentColor`.
 *
 * This is what decides whether a render request's `currentColor` can change
 * the output at all. Together with {@link isTintable} it splits assets into
 * the three colour paths the cache distinguishes:
 *
 * - `false` — colour cannot affect pixels, so it is never part of a cache key;
 * - `true` and tintable — one shared alpha mask, colour applied by
 *   `ImageColor3`, colour still not part of the raster cache key;
 * - `true` and *not* tintable — the resolved colour reaches the compositor, so
 *   it must be part of the cache key.
 */
export function usesCurrentColor(asset: SvgAsset): boolean {
	return (assetData(asset).features & SvgFeature.UsesCurrentColor) !== 0;
}
