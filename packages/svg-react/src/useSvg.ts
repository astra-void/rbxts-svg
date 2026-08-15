/**
 * The hook that binds an asset's lifetime to a component's.
 *
 * All the React-specific work lives here: acquiring a shared raster when the
 * asset or its render parameters change, and releasing it on unmount. The
 * component is a thin wrapper over this, so anyone wanting a different
 * component API can build one without reimplementing the lifecycle.
 */

import { useEffect, useState } from "@rbxts/react";
import {
	isTintable,
	renderSvg,
	usesCurrentColor,
	type SvgAsset,
	type SvgRenderHandle,
} from "@rbxts/svg";

export interface UseSvgOptions {
	/**
	 * Target size in pixels.
	 *
	 * This is the resolution the asset is *rasterized* at, not a layout size.
	 * A caller whose layout is driven by a `UDim2` should pass the instance's
	 * observed `AbsoluteSize`, which is what `<Svg>` does — rasterizing at the
	 * view box size and then stretching the result would throw away the whole
	 * point of a resolution-dependent rasterizer.
	 *
	 * `undefined` means "not known yet": nothing is acquired and the hook
	 * returns `undefined`. That is the honest state for a layout-driven size
	 * before its first measurement, and it is better than rasterizing at a
	 * placeholder resolution only to throw that raster away a frame later.
	 */
	readonly size: Vector2 | undefined;
	/**
	 * The SVG `currentColor`. For a tintable asset this never re-rasterizes —
	 * the shared alpha mask is coloured by `ImageColor3` at the component
	 * layer — so it only forces a new raster for assets that mix
	 * `currentColor` with fixed paints.
	 */
	readonly currentColor?: Color3;
	/** Overrides the asset's stroke width, in view box units. */
	readonly strokeWidth?: number;
	/** Interpret `strokeWidth` as pixels rather than view box units. */
	readonly absoluteStrokeWidth?: boolean;
}

/**
 * Returns a render handle for `asset`, or `undefined` until one is acquired.
 *
 * The handle is reference-counted: several components rendering the same icon
 * at the same size share one rasterized image, and it is freed when the last of
 * them unmounts.
 *
 * # When this returns `undefined`
 *
 * Two transient cases, both of them "no target size to rasterize at yet":
 * before the effect has run (React commits the first render before effects
 * fire), and while `options.size` is `undefined`.
 *
 * It is specifically *not* what happens when no renderer is installed. That is
 * a configuration error, and `renderSvg` throws an actionable message for it
 * which this hook deliberately does not catch: an icon that silently renders as
 * nothing is far harder to diagnose than one that fails loudly at startup.
 */
export function useSvg(
	asset: SvgAsset,
	options: UseSvgOptions,
): SvgRenderHandle | undefined {
	const [handle, setHandle] = useState<SvgRenderHandle | undefined>(undefined);

	// Vector2/Color3 have value semantics but are distinct objects each
	// render, so the dependency list uses their components. Otherwise every
	// render would look like a change and re-acquire.
	const width = options.size?.X;
	const height = options.size?.Y;
	const strokeWidth = options.strokeWidth;
	const absolute = options.absoluteStrokeWidth;
	const currentColor = options.currentColor;

	// The colour is a dependency only when it can actually change the raster:
	// a mixed-currentColor asset. For a tintable or fixed-colour asset a
	// colour change must NOT re-run the effect — the release/acquire pair
	// would momentarily drop the entry's last reference and destroy the very
	// image the "same raster, new ImageColor3" fast path exists to keep.
	const colorAffectsRaster = usesCurrentColor(asset) && !isTintable(asset);
	const colorKey =
		colorAffectsRaster && currentColor !== undefined
			? `${math.round(currentColor.R * 255)},${math.round(currentColor.G * 255)},${math.round(
					currentColor.B * 255,
				)}`
			: "";

	useEffect(() => {
		if (width === undefined || height === undefined) {
			// Nothing acquired means nothing to release, and any handle from a
			// previous size was already released by this effect's own cleanup.
			setHandle(undefined);
			return;
		}

		const acquired = renderSvg(asset, {
			size: new Vector2(width, height),
			currentColor,
			strokeWidth,
			absoluteStrokeWidth: absolute,
		});
		setHandle(acquired);

		return () => {
			setHandle(undefined);
			acquired.release();
		};
	}, [asset, width, height, strokeWidth, absolute, colorKey]);

	return handle;
}
