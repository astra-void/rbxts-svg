/**
 * The `<Svg>` component.
 *
 * Deliberately small. The component's whole job is to turn an asset plus a size
 * into an `ImageLabel`; anything more elaborate (theming, animation, layout
 * conventions) belongs in an application's own wrapper, not here.
 *
 * # Layout size vs. raster size
 *
 * These are two different things and the component keeps them apart, because
 * conflating them is how a resolution-dependent rasterizer stops being one.
 *
 * | Props | Layout | Raster before layout | Raster after |
 * | --- | --- | --- | --- |
 * | `size={24}` | 24×24 offset | 24×24 pixels | the observed `AbsoluteSize` |
 * | `Size={...}` | that `UDim2` | nothing drawn | the observed `AbsoluteSize` |
 * | neither | view box, as offset | view box dimensions | the observed `AbsoluteSize` |
 *
 * The last column is the same in every row, and that is the point: a prop says
 * how big the icon should *be*, and only the engine knows how many pixels that
 * became. Under a `UIScale` of 2 a `size={24}` icon occupies 48×48, and drawing
 * its 24×24 raster there would be an upscale — exactly the softness a
 * resolution-dependent rasterizer exists to avoid.
 *
 * The third column is what keeps that free: a component that owns its layout
 * size starts at that size rather than waiting a frame, and when the
 * measurement agrees — which is the ordinary case — it is the same resolution
 * and nothing re-rasterizes.
 *
 * `Size` wins when both are given, matching its documented precedence.
 *
 * A `UDim2` cannot be turned into pixels before the instance has taken part in
 * layout — `UDim2.fromScale(0.1, 0.1)` depends entirely on the parent — so when
 * `Size` is in play there is nothing to draw until the first measurement lands.
 * A numeric `size` has an answer immediately and uses it, then defers to the
 * measurement like everything else.
 */

import React, { useEffect, useRef, useState } from "@rbxts/react";
import {
	getViewBox,
	isTintable,
	measureSvgPixelSize,
	resolveSvgSizing,
	type SvgAsset,
} from "@rbxts/svg";

import { useSvg } from "./useSvg";

export interface SvgProps {
	/** The compiled asset to draw. */
	readonly source: SvgAsset;
	/**
	 * Convenience square size in pixels, as in `<Svg source={Search} size={24} />`.
	 *
	 * A *layout* size. It is also the resolution the icon is first drawn at, so
	 * there is no wait for a layout pass — but the observed `AbsoluteSize` wins
	 * once it arrives, so an icon that the engine actually laid out larger (a
	 * `UIScale`, a scaling ancestor) is rasterized larger rather than upscaled.
	 *
	 * Ignored when `Size` is given; when neither is set, the asset's view box
	 * dimensions are used.
	 */
	readonly size?: number;
	/**
	 * The SVG `currentColor` — what any `currentColor` paint in the asset
	 * resolves to. Not a blanket tint.
	 *
	 * - Tintable asset (monochrome `currentColor`, which is every Lucide
	 *   icon): costs nothing. The raster is a shared alpha mask and the colour
	 *   is applied by `ImageColor3`, so changing it never re-rasterizes.
	 * - Mixed asset (`currentColor` plus fixed paints): the colour reaches the
	 *   rasterizer, so each distinct colour is its own cached raster.
	 * - Asset with no `currentColor` at all: no visual effect.
	 */
	readonly color?: Color3;
	/** Overrides the asset's stroke width, in view box units. */
	readonly strokeWidth?: number;
	/** Interpret `strokeWidth` as pixels rather than view box units. */
	readonly absoluteStrokeWidth?: boolean;

	// ---- Standard Roblox layout properties, passed through unchanged.
	/**
	 * The Roblox-native layout size. Takes precedence over {@link SvgProps.size}.
	 *
	 * The raster resolution comes from the instance's `AbsoluteSize` either way;
	 * what this changes is that there is no resolution at all until the first
	 * measurement, so nothing is drawn for a frame rather than a guess being
	 * drawn and discarded.
	 */
	readonly Size?: UDim2;
	readonly Position?: UDim2;
	readonly AnchorPoint?: Vector2;
	readonly LayoutOrder?: number;
	readonly ZIndex?: number;
	readonly Visible?: boolean;
	readonly BackgroundTransparency?: number;
	readonly ImageTransparency?: number;
}

/**
 * Renders a compiled SVG asset.
 *
 * ```tsx
 * import Search from "./search.svg";
 * import { Svg } from "@rbxts/svg-react";
 *
 * <Svg source={Search} size={24} color={Color3.fromRGB(255, 255, 255)} />
 * ```
 *
 * The `.svg` import is compiled by `rbxts-svg build` and pointed at its
 * generated module by `@rbxts/svg-transformer`; see `docs/SVG-IMPORTS.md`. For
 * Lucide there is no import to wire up at all — `@rbxts/lucide-react` ships the
 * icons precompiled.
 */
export function Svg(props: SvgProps): React.Element {
	const ref = useRef<ImageLabel>();

	const sizing = resolveSvgSizing(
		getViewBox(props.source),
		props.size,
		props.Size !== undefined,
	);
	const declaredPixels = sizing.declaredPixels;

	const [measuredPixels, setMeasuredPixels] = useState<Vector2 | undefined>(undefined);

	// Always, whatever decided the layout size. A `size={24}` icon under a
	// `UIScale` of 2 is laid out at 48×48, and only the instance knows that.
	useEffect(() => {
		const instance = ref.current;
		if (instance === undefined) {
			return;
		}

		const update = (): void => {
			// `undefined` for an instance that has not been laid out. A
			// zero AbsoluteSize is not a very small size, and clamping it up
			// to 1×1 would rasterize a placeholder that the next measurement
			// immediately discards.
			const snapped = measureSvgPixelSize(instance.AbsoluteSize);
			setMeasuredPixels((previous) =>
				// Identity-stable when the snapped size is unchanged, so React
				// bails out and no new raster is acquired. Without this a size
				// animation would rasterize on every frame it moved at all.
				previous !== undefined &&
				snapped !== undefined &&
				previous.X === snapped.X &&
				previous.Y === snapped.Y
					? previous
					: snapped,
			);
		};

		update();
		const connection = instance.GetPropertyChangedSignal("AbsoluteSize").Connect(update);
		return () => connection.Disconnect();
	}, []);

	// The measurement wins once it exists, because it is the only value that
	// knows what the engine actually laid out. Before it arrives, a component
	// that owns its layout size uses that size — so `size={24}` draws on the
	// first frame rather than after one — and a `UDim2`-driven one draws
	// nothing rather than guessing.
	const rasterPixels = measuredPixels ?? sizing.initialPixels;

	const handle = useSvg(props.source, {
		size: rasterPixels,
		currentColor: props.color,
		strokeWidth: props.strokeWidth,
		absoluteStrokeWidth: props.absoluteStrokeWidth,
	});

	// Only a tintable asset routes its colour through ImageColor3; for every
	// other asset the colours are in the raster itself (with `color` already
	// delivered to the rasterizer as currentColor above), and multiplying
	// them here would silently change the artwork.
	const tint = isTintable(props.source) ? props.color : undefined;

	return (
		<imagelabel
			ref={ref}
			Size={props.Size ?? UDim2.fromOffset(declaredPixels.X, declaredPixels.Y)}
			Position={props.Position}
			AnchorPoint={props.AnchorPoint}
			LayoutOrder={props.LayoutOrder}
			ZIndex={props.ZIndex}
			Visible={props.Visible}
			BackgroundTransparency={props.BackgroundTransparency ?? 1}
			ImageTransparency={props.ImageTransparency}
			ImageColor3={tint}
			ImageContent={
				handle !== undefined ? Content.fromObject(handle.image) : Content.none
			}
		/>
	);
}
