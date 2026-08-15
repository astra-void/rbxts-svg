/**
 * The render cache.
 *
 * Caching is not an optimization bolted on afterwards — it is why the asset
 * format carries `MONOCHROME` and `USES_CURRENT_COLOR` at all.
 *
 * ```text
 * search.svg, 24x24, strokeWidth 2
 *              │
 *              ▼
 *   one cached alpha raster
 *              │
 *      ┌───────┼───────┐
 *      ▼       ▼       ▼
 *    white    red    blue      (ImageColor3, no re-rasterization)
 * ```
 *
 * # What is in the key
 *
 * Asset identity, pixel size, geometry-affecting overrides, and the renderer
 * version. Colour is *not*, for tintable assets: including it would rasterize
 * the same icon once per colour, which is exactly the cost this design exists
 * to avoid. For a non-tintable asset the colour is already baked into the
 * geometry's paints, so it is covered by the asset identity.
 *
 * # Ownership
 *
 * Every entry is reference-counted. `acquire` hands out a handle and increments;
 * `release` decrements and, at zero, destroys the image immediately. Nothing is
 * left to the garbage collector — an `EditableImage` holds real memory and its
 * lifetime should be deterministic.
 */

import type { SvgAsset } from "../asset";
import { assetData, isTintable, usesCurrentColor } from "../asset";
import { fitLengthScale, viewBoxTransform } from "./fit";
import type {
	ResolvedRenderOptions,
	SvgRenderHandle,
	SvgRenderOptions,
	SvgRenderer,
} from "./types";

/**
 * Applies defaults and snaps the target size to whole pixels.
 *
 * Snapping is what makes the cache effective: an animated size would otherwise
 * produce a fresh key every frame.
 */
/**
 * The `currentColor` default: black, CSS's initial `color`. Shared so that a
 * request that omits the colour and one that passes black explicitly resolve
 * identically — and therefore hash identically.
 */
const DEFAULT_CURRENT_COLOR = new Color3(0, 0, 0);

export function resolveRenderOptions(
	asset: SvgAsset,
	options: SvgRenderOptions,
): ResolvedRenderOptions {
	const pixelWidth = math.max(1, math.round(options.size.X));
	const pixelHeight = math.max(1, math.round(options.size.Y));

	let strokeWidth = options.strokeWidth;
	if (strokeWidth !== undefined && options.absoluteStrokeWidth === true) {
		// An absolute stroke width is specified in pixels, but the asset's
		// geometry lives in view box units, so convert. The scale must be the
		// one the rasterizer actually applies — which depends on the asset's
		// `preserveAspectRatio`, not just on the two sizes — or an asset drawn
		// with `slice` or `none` would get a stroke of the wrong weight.
		const data = assetData(asset);
		const scale = fitLengthScale(
			viewBoxTransform(data.viewBox, data.preserveAspectRatio, pixelWidth, pixelHeight),
		);
		strokeWidth = scale > 0 ? strokeWidth / scale : strokeWidth;
	}

	return {
		pixelWidth,
		pixelHeight,
		currentColor: options.currentColor ?? DEFAULT_CURRENT_COLOR,
		strokeWidth,
	};
}

/**
 * The cache key for one render request.
 *
 * Deliberately a plain string: Luau table keys are compared by identity, so a
 * composite key has to be flattened to something with value equality.
 *
 * # Colour, by asset class
 *
 * - **No `currentColor` in the asset:** the requested colour cannot change a
 *   single pixel, so it is excluded — asking for the same icon in red and in
 *   blue must hit one entry.
 * - **Tintable (monochrome `currentColor`):** the raster is a colour-free
 *   alpha mask and `ImageColor3` applies the tint, so colour is excluded here
 *   too. This is the Lucide fast path.
 * - **Mixed (`currentColor` plus fixed paints):** the resolved colour reaches
 *   the compositor and genuinely changes the RGBA output, so it is included.
 *
 * The colour is serialized as its effective 8-bit channels — the renderer
 * emits 8-bit RGBA, so two colours that quantize identically cannot produce
 * different output and may share an entry. Value-based, never identity-based:
 * two distinct `Color3` objects with the same channels make the same key.
 */
export function renderCacheKey(
	asset: SvgAsset,
	options: ResolvedRenderOptions,
	rendererVersion: number,
): string {
	const stroke = options.strokeWidth ?? -1;
	let colour = "";
	if (usesCurrentColor(asset) && !isTintable(asset)) {
		const c = options.currentColor;
		colour = `|c${math.round(c.R * 255)},${math.round(c.G * 255)},${math.round(c.B * 255)}`;
	}
	return (
		`${assetData(asset).id}|${options.pixelWidth}x${options.pixelHeight}` +
		`|s${stroke}|r${rendererVersion}${colour}`
	);
}

interface CacheEntry {
	readonly image: EditableImage;
	readonly pixelSize: Vector2;
	readonly tintable: boolean;
	referenceCount: number;
}

/** Cache statistics, for diagnostics and tests. */
export interface SvgRenderCacheStats {
	/** Distinct rasters currently held. */
	readonly entryCount: number;
	/** Total outstanding handles across all entries. */
	readonly referenceCount: number;
	readonly hits: number;
	readonly misses: number;
}

/**
 * A reference-counted store of rasterized images.
 *
 * Bounded implicitly: an entry exists only while something holds a handle to
 * it. A future size-bounded variant that retains unreferenced entries for reuse
 * can be layered on top without changing this interface.
 */
export class SvgRenderCache {
	private readonly entries = new Map<string, CacheEntry>();
	private hits = 0;
	private misses = 0;

	constructor(private readonly renderer: SvgRenderer) {}

	/**
	 * Returns a handle to the rendered image, rasterizing only on a miss.
	 */
	acquire(asset: SvgAsset, options: SvgRenderOptions): SvgRenderHandle {
		const resolved = resolveRenderOptions(asset, options);
		const key = renderCacheKey(asset, resolved, this.renderer.version);

		let entry = this.entries.get(key);
		if (entry === undefined) {
			this.misses += 1;
			entry = {
				image: this.renderer.render(asset, resolved),
				pixelSize: new Vector2(resolved.pixelWidth, resolved.pixelHeight),
				tintable: isTintable(asset),
				referenceCount: 0,
			};
			this.entries.set(key, entry);
		} else {
			this.hits += 1;
		}

		entry.referenceCount += 1;
		return this.createHandle(key, entry);
	}

	private createHandle(key: string, entry: CacheEntry): SvgRenderHandle {
		let released = false;
		const cache = this;
		return {
			image: entry.image,
			pixelSize: entry.pixelSize,
			tintable: entry.tintable,
			cacheKey: key,
			release(): void {
				// Idempotent: a component unmounting twice, or releasing in both
				// an effect cleanup and an error path, must not double-decrement
				// and free an image someone else is still using.
				if (released) {
					return;
				}
				released = true;
				cache.release(key);
			},
		};
	}

	private release(key: string): void {
		const entry = this.entries.get(key);
		if (entry === undefined) {
			return;
		}
		entry.referenceCount -= 1;
		if (entry.referenceCount <= 0) {
			this.entries.delete(key);
			this.renderer.destroy(entry.image);
		}
	}

	stats(): SvgRenderCacheStats {
		let referenceCount = 0;
		for (const [, entry] of this.entries) {
			referenceCount += entry.referenceCount;
		}
		return {
			entryCount: this.entries.size(),
			referenceCount,
			hits: this.hits,
			misses: this.misses,
		};
	}

	/**
	 * Destroys every cached image regardless of outstanding references.
	 *
	 * For teardown only. Handles obtained beforehand must not be used after.
	 */
	clear(): void {
		for (const [, entry] of this.entries) {
			this.renderer.destroy(entry.image);
		}
		this.entries.clear();
	}
}
