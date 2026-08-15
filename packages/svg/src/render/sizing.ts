/**
 * The `size` / `Size` policy shared by every UI binding.
 *
 * This is deliberately *not* in a framework package. React and Vide differ in
 * how they observe a laid-out instance and when they re-render; they do not
 * differ in what `size={24}` means, in which prop wins when both are given, or
 * in how a measured `AbsoluteSize` becomes a raster resolution. Keeping that
 * one answer here is what stops two bindings drifting into two subtly
 * different sizing semantics — and it is the layer the cross-framework tests
 * compare against.
 *
 * It imports no renderer and no Roblox datatype beyond `Vector2`, so the
 * standalone Luau suite can exercise it directly.
 */

import type { SvgViewBox } from "../asset";

/** What a binding should do about sizing, given its props. */
export interface SvgSizing {
	/**
	 * True when the *layout* size comes from a `UDim2` rather than from this
	 * component's own props.
	 *
	 * It no longer decides whether `AbsoluteSize` is observed — it always is,
	 * see {@link initialPixels}. What it decides is whether there is any
	 * sensible resolution to use before the first measurement arrives: a
	 * `UDim2` cannot be resolved to pixels without layout, because
	 * `UDim2.fromScale(0.1, 0.1)` depends entirely on its parent.
	 */
	readonly measureAbsoluteSize: boolean;
	/**
	 * The component's own layout size, in pixels: `size`, or the view box.
	 *
	 * This is what a binding gives the instance as `UDim2.fromOffset(...)`. It
	 * is *not* necessarily the raster resolution — see {@link initialPixels}.
	 *
	 * Always whole pixels, never below 1×1.
	 */
	readonly declaredPixels: Vector2;
	/**
	 * The resolution to rasterize at until the first `AbsoluteSize` arrives,
	 * or `undefined` when there is no honest answer yet.
	 *
	 * `declaredPixels` when this component owns its layout size, so `size={24}`
	 * draws immediately at 24×24 and never waits for a layout pass;
	 * `undefined` under a `UDim2`, so nothing is acquired until the size is
	 * real.
	 *
	 * Once a measurement lands it *supersedes* this in both cases, which is the
	 * point. `size={24}` asks for a 24-pixel-tall icon in layout, not for a
	 * 24-pixel raster whatever the engine then does with it: under a `UIScale`
	 * of 2 that instance is laid out at 48×48, and rasterizing at 24 would mean
	 * displaying a 24×24 image stretched to twice its size. The measurement is
	 * the only thing that knows about `UIScale`, an ancestor's scale-based
	 * layout, or anything else between the prop and the pixels.
	 *
	 * In the ordinary case the measurement equals this value, so nothing
	 * changes and nothing re-rasterizes.
	 */
	readonly initialPixels: Vector2 | undefined;
}

/**
 * Snaps a size to whole pixels, never below 1×1.
 *
 * Snapping is what keeps a resizing layout from rasterizing every frame: the
 * render cache is keyed on integer pixels, so any size that rounds to the same
 * pair reuses the same raster. The 1×1 floor matches the cache's own
 * normalization — a zero-size `EditableImage` is not a valid image.
 */
export function snapSvgPixelSize(size: Vector2): Vector2 {
	return new Vector2(math.max(1, math.round(size.X)), math.max(1, math.round(size.Y)));
}

/**
 * Interprets an observed `AbsoluteSize` as a raster resolution, or `undefined`
 * when there is not one yet.
 *
 * The distinction {@link snapSvgPixelSize} cannot make is between "very small"
 * and "not laid out". Its 1×1 floor is right for the former and actively wrong
 * for the latter: an instance that has not taken part in layout reports
 * `Vector2.zero`, and clamping that to 1×1 buys a real rasterization and a real
 * cache entry whose only future is to be discarded on the next measurement.
 *
 * Both bindings observe an `AbsoluteSize` that is zero at least once. React's
 * effect reads it after mount; Vide's `changed()` action invokes its callback
 * immediately, at instance creation, before the instance is parented — so under
 * Vide the zero measurement is not merely likely but guaranteed. Answering
 * `undefined` for it is what lets a binding hold off until the size is real.
 *
 * Anything that rounds to at least one pixel in both dimensions is a genuine
 * measurement and comes back snapped, identically to `snapSvgPixelSize`.
 */
export function measureSvgPixelSize(absoluteSize: Vector2): Vector2 | undefined {
	const width = math.round(absoluteSize.X);
	const height = math.round(absoluteSize.Y);
	if (width < 1 || height < 1) {
		return undefined;
	}
	return new Vector2(width, height);
}

/**
 * Decides layout and raster dimensions from a binding's size props.
 *
 * | `size` | `Size` | Layout | Raster before layout | Raster after |
 * | --- | --- | --- | --- | --- |
 * | — | — | view box, as offset | view box dimensions | `AbsoluteSize` |
 * | `24` | — | 24×24 offset | 24×24 pixels | `AbsoluteSize` |
 * | any | given | that `UDim2` | nothing drawn | `AbsoluteSize` |
 *
 * `Size` wins when both are given: it is the more specific, Roblox-native
 * property, and that is the precedence every binding documents.
 *
 * # The raster always follows what was laid out
 *
 * The last column is the same in every row, and that is deliberate. A prop says
 * how big the icon should *be*; only the engine knows how many pixels that
 * turned into. A `UIScale`, a scale-based ancestor, or anything else between
 * the two makes those differ, and a raster that ignored the difference would be
 * an image displayed at a size it was not drawn for — which looks exactly as
 * blurry as it is.
 *
 * The middle column is what keeps that from costing anything: a component that
 * owns its layout size starts at that size immediately rather than waiting a
 * frame, and when the measurement agrees — which is the overwhelmingly common
 * case — it is the same resolution and nothing re-rasterizes.
 *
 * @param hasRobloxSize whether a `Size` prop was supplied. The `UDim2` itself
 * is not needed — its *value* never determines the raster resolution, because
 * only layout can.
 */
export function resolveSvgSizing(
	viewBox: SvgViewBox,
	size: number | undefined,
	hasRobloxSize: boolean,
): SvgSizing {
	const declared =
		size !== undefined
			? new Vector2(size, size)
			: new Vector2(viewBox.width, viewBox.height);
	const declaredPixels = snapSvgPixelSize(declared);

	return {
		measureAbsoluteSize: hasRobloxSize,
		declaredPixels,
		initialPixels: hasRobloxSize ? undefined : declaredPixels,
	};
}
