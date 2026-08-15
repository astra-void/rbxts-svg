/**
 * The renderer boundary.
 *
 * These types define the seam the rasterizer slots into. The production
 * `EditableImage` renderer implements it (see `../raster`), and tests inject
 * fakes through exactly the same interface.
 */

import type { SvgAsset } from "../asset";

/** What a consumer asks for when rendering an asset. */
export interface SvgRenderOptions {
	/** Target size in pixels. */
	readonly size: Vector2;
	/**
	 * The colour that `currentColor` paints resolve to, defaulting to black —
	 * CSS's initial `color`, and what the Rust reference renderer defaults to.
	 *
	 * For a tintable asset this never re-rasterizes: the raster is a colour-free
	 * alpha mask and the tint is applied by `ImageColor3`. It only reaches the
	 * compositor — and the cache key — for an asset that mixes `currentColor`
	 * with fixed paints.
	 */
	readonly currentColor?: Color3;
	/**
	 * Overrides the stroke width baked into the asset, in view box units.
	 *
	 * This is Lucide's `strokeWidth` prop: `<Search strokeWidth={1.5} />`.
	 */
	readonly strokeWidth?: number;
	/**
	 * Treat `strokeWidth` as pixels rather than view box units, so a stroke
	 * keeps its apparent thickness at any size.
	 *
	 * Mirrors Lucide's `absoluteStrokeWidth`.
	 */
	readonly absoluteStrokeWidth?: boolean;
}

/**
 * Render options with defaults applied and sizes snapped.
 *
 * Resolving before caching matters: two requests that differ only in a
 * defaulted field must produce the same cache key, or the cache silently
 * duplicates rasters.
 */
export interface ResolvedRenderOptions {
	readonly pixelWidth: number;
	readonly pixelHeight: number;
	/** The concrete `currentColor`, after the black default is applied. */
	readonly currentColor: Color3;
	/**
	 * Effective stroke width in **view box units**, after any override.
	 *
	 * This is the single unit-conversion boundary for `absoluteStrokeWidth`:
	 * an absolute width in pixels is divided by the fit's length scale *here*,
	 * and the rasterizer always multiplies by that same scale to get back to
	 * device pixels — the Rust reference's `device_stroke_width` contract,
	 * expressed once instead of in every backend.
	 */
	readonly strokeWidth: number | undefined;
}

/**
 * A consumer's claim on a rendered image.
 *
 * Handles are reference-counted: several components rendering the same icon at
 * the same size share one image, and it is released only when the last of them
 * calls {@link SvgRenderHandle.release}.
 */
export interface SvgRenderHandle {
	/** The shared rasterized image. */
	readonly image: EditableImage;
	/** The size actually rasterized, in pixels. */
	readonly pixelSize: Vector2;
	/**
	 * True when `image` is an alpha mask that any `ImageColor3` tints
	 * correctly. Colour is deliberately absent from the cache key for these.
	 */
	readonly tintable: boolean;
	/** The cache key this handle holds a reference to. */
	readonly cacheKey: string;
	/** Releases this handle's reference. Calling twice is a no-op. */
	release(): void;
}

/**
 * A rasterizer.
 *
 * Implemented by the Roblox `EditableImage` renderer, and by a future reference
 * renderer used for golden-image comparison. Both consume exactly the same
 * [`SvgAsset`], which is what makes comparing them meaningful.
 */
export interface SvgRenderer {
	/**
	 * Bumped whenever output changes for the same input.
	 *
	 * Part of every cache key, so a renderer upgrade invalidates cached rasters
	 * instead of leaving stale ones behind.
	 */
	readonly version: number;
	/** Rasterizes an asset. */
	render(asset: SvgAsset, options: ResolvedRenderOptions): EditableImage;
	/** Releases an image this renderer produced. */
	destroy(image: EditableImage): void;
}
