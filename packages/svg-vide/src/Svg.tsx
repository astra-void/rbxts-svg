/**
 * The `<Svg>` component, for Vide.
 *
 * The rendering semantics here are not Vide's — they are `@rbxts/svg`'s, and
 * they are identical to React's. What this file owns is one thing: tying a
 * reference-counted raster's lifetime to a Vide scope, and doing it so that the
 * *right* state changes reach the rasterizer and the rest do not.
 *
 * # Which changes cost a raster
 *
 * ```text
 * asset ─┐
 * pixels ├─▶ raster effect ──▶ renderSvg ──▶ ImageContent
 * stroke ─┘
 *
 * colour ────▶ ImageColor3          (tintable asset: free)
 * colour ────▶ raster effect        (mixed currentColor: not free)
 * colour ────▶ nothing              (fixed-colour asset)
 * ```
 *
 * That split is the whole point of the design, and under a reactive library it
 * takes deliberate work: Vide tracks whatever a scope reads, so reading the
 * colour inside the raster effect for a Lucide icon would re-rasterize on every
 * theme change. The effect therefore reads the colour *only* when the asset is
 * one whose pixels it can change.
 *
 * # Layout size vs. raster size
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
 * `Size` wins when both are given. `AbsoluteSize` is observed through Vide's
 * own `changed()` action, so the connection belongs to the scope and dies with
 * it.
 *
 * Under a `UDim2` the resolution is *unknown* until that first measurement
 * arrives, and this component acquires nothing meanwhile: no raster, no cache
 * entry, `Content.none`. That is the same thing React's `<Svg>` does, and for
 * the same reason — a placeholder raster at a made-up resolution is a real
 * rasterization and a real cache miss whose only outcome is to be thrown away a
 * frame later.
 *
 * # Handle lifetime
 *
 * A raster is shared and reference-counted, so *when* the old reference is
 * dropped decides whether a shared image survives a change. Every transition
 * therefore goes:
 *
 * ```text
 * effect rerun:        acquire new ─▶ publish new ─▶ release old
 * scope destruction:   cleanup ─▶ release current
 * ```
 *
 * and specifically not "release in a cleanup, acquire on rerun". Vide flushes
 * a scope's cleanups *before* rerunning it, so that ordering would drop an
 * entry's last reference immediately before the rerun asked for the same entry
 * again — destroying and re-rasterizing an image for nothing. Strict mode,
 * which evaluates every reactive scope twice by design, turns that from a
 * latent cost into one that happens on every single mount.
 */

import Vide, { cleanup, derive, effect, read, source } from "@rbxts/vide";
import {
	getViewBox,
	isTintable,
	measureSvgPixelSize,
	renderSvg,
	resolveSvgSizing,
	usesCurrentColor,
	type SvgAsset,
	type SvgRenderHandle,
} from "@rbxts/svg";

/**
 * `ImageColor3`'s identity value.
 *
 * Used for every asset that is not a tintable alpha mask, so that a fixed
 * colour or mixed `currentColor` asset displays exactly the pixels the
 * rasterizer produced. It is also the property's own default, so setting it is
 * indistinguishable from leaving it alone.
 */
const NO_TINT = new Color3(1, 1, 1);

/** The props `<Svg>` interprets itself, rather than passing to the instance. */
export interface SvgSpecificProps {
	/**
	 * The compiled asset to draw.
	 *
	 * Reactive like any other prop: pointing this at a different asset acquires
	 * the new raster, swaps the image, and only then releases the old one.
	 */
	readonly source: Vide.Derivable<SvgAsset>;
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
	readonly size?: Vide.Derivable<number>;
	/**
	 * The SVG `currentColor` — what any `currentColor` paint in the asset
	 * resolves to. Not a blanket tint.
	 *
	 * - Tintable asset (monochrome `currentColor`, which is every Lucide
	 *   icon): costs nothing. The raster is a shared alpha mask and the colour
	 *   is applied by `ImageColor3`, so changing it never re-rasterizes and
	 *   never even wakes the raster effect.
	 * - Mixed asset (`currentColor` plus fixed paints): the colour reaches the
	 *   rasterizer, so each distinct colour is its own cached raster.
	 * - Asset with no `currentColor` at all: no visual effect.
	 */
	readonly color?: Vide.Derivable<Color3>;
	/** Overrides the asset's stroke width, in view box units. */
	readonly strokeWidth?: Vide.Derivable<number>;
	/** Interpret `strokeWidth` as pixels rather than view box units. */
	readonly absoluteStrokeWidth?: Vide.Derivable<boolean>;
}

/**
 * The instance properties `<Svg>` is authoritative over.
 *
 * `Image`/`ImageContent` *are* the rendered SVG; letting a caller set them
 * would silently replace the thing the component exists to draw. `ImageColor3`
 * is how `color` is applied to a tintable asset, and `AbsoluteSizeChanged` is
 * how a `UDim2` layout reaches the rasterizer — a caller's handler would
 * displace the measurement and freeze the resolution.
 *
 * A raw final-image tint, independent of SVG `currentColor`, would be a
 * separate explicit prop if it is ever wanted. It is not this one.
 */
type OwnedInstanceProps = "Image" | "ImageContent" | "ImageColor3" | "AbsoluteSizeChanged";

/**
 * `Omit`, but tolerant of keys the source type may not have.
 *
 * roblox-ts's lib constrains `Omit`'s key parameter to `keyof T`, which makes
 * the composition below depend on the exact shape of Vide's generated attribute
 * type. That is a detail of two libraries' typings, not of this component.
 */
type Without<T, K extends string> = Pick<T, Exclude<keyof T, K>>;

/**
 * `<Svg>`'s props: the SVG-specific ones, plus everything an `imagelabel`
 * accepts under Vide — reactive property sources, events and `changed`
 * callbacks alike — except what the component owns.
 *
 * Composed from Vide's own JSX attribute type rather than an enumerated list,
 * so `Position`, `LayoutOrder`, `Visible`, `MouseButton1Click` and the rest
 * work without this package tracking the Roblox API.
 */
export type SvgProps = SvgSpecificProps &
	Without<Vide.InstanceAttributes<ImageLabel>, keyof SvgSpecificProps | OwnedInstanceProps>;

/**
 * The props this component consumes rather than forwards.
 *
 * They have to be removed by name, not merely ignored: Vide assigns every
 * string key of a props table onto the instance, so an `SvgAsset` left under
 * `source` would be assigned to `ImageLabel.source` and fail there — a long way
 * from the code that put it there. `children` is excluded for a different
 * reason: Vide parents children from *numeric* keys, so it is passed on as a
 * JSX child instead of as a property.
 */
const CONSUMED_PROPS = [
	"source",
	"size",
	"color",
	"strokeWidth",
	"absoluteStrokeWidth",
	"children",
] as const;

/** Everything in `props` that belongs to the `ImageLabel`, and nothing else. */
function forwardedProps(props: SvgProps): Vide.InstanceAttributes<ImageLabel> {
	const forwarded = table.clone(props) as unknown as Record<string, unknown>;
	for (const name of CONSUMED_PROPS) {
		forwarded[name] = undefined;
	}
	return forwarded as unknown as Vide.InstanceAttributes<ImageLabel>;
}

/**
 * Renders a compiled SVG asset.
 *
 * ```tsx
 * import Search from "./search.svg";
 * import { Svg } from "@rbxts/svg-vide";
 *
 * <Svg source={Search} size={24} color={Color3.fromRGB(255, 255, 255)} />
 * ```
 *
 * Every prop may also be a source, so `size={iconSize}` and `color={theme}`
 * reactively update the one label.
 *
 * A renderer must have been installed first — `installEditableImageRenderer()`
 * from `@rbxts/svg`, once, at startup. Importing this package does not install
 * one, and rendering without one throws rather than showing an empty label.
 */
export function Svg(props: SvgProps): Vide.Node {
	const assetProp = props.source;
	const sizeProp = props.size;
	const colorProp = props.color;
	const strokeWidthProp = props.strokeWidth;
	const absoluteStrokeWidthProp = props.absoluteStrokeWidth;
	const instanceProps = forwardedProps(props);

	const asset = (): SvgAsset => read(assetProp);

	// Whether a `Size` was supplied is a fact about the props table, not a
	// reactive value: its *presence* decides whether there is any resolution to
	// draw at before layout has run, and only its value can change over a
	// component's life.
	const measuresAbsoluteSize = instanceProps.Size !== undefined;

	const sizing = () =>
		resolveSvgSizing(getViewBox(asset()), read(sizeProp), measuresAbsoluteSize);

	// The size known without measuring. Derived rather than recomputed inline
	// because Vector2 has value equality, so a recompute that lands on the same
	// pixel dimensions stops here instead of reaching the raster effect.
	const declaredPixels = derive(() => sizing().declaredPixels);
	const initialPixels = derive(() => sizing().initialPixels);

	// Under a `UDim2` layout the resolution follows what Roblox laid out, and
	// until layout has run there is no resolution at all. `undefined` says
	// exactly that: not 1×1, not the view box size, but *unknown*. A
	// placeholder raster at either of those resolutions would be a wrong
	// answer that costs a real rasterization and a real cache entry, only to
	// be discarded on the first `AbsoluteSizeChanged` — which is precisely
	// what React declines to do, and what this now matches.
	const measuredPixels = source<Vector2 | undefined>(undefined);

	// The measurement wins once it exists, because it is the only value that
	// knows what the engine actually laid out — a `size={24}` icon under a
	// `UIScale` of 2 occupies 48×48. `initialPixels` is only *read* while
	// there is no measurement, so it is a dependency of the raster effect only
	// while it is the answer.
	const rasterPixels = (): Vector2 | undefined => measuredPixels() ?? initialPixels();

	const handle = source<SvgRenderHandle | undefined>(undefined);

	// The one thing Vide cannot infer: a shared, reference-counted raster.
	//
	// Handing over explicitly — acquire, publish, then release — rather than
	// releasing from `cleanup()` is what keeps a shared cache entry alive
	// across a change. Vide flushes a scope's cleanups *before* rerunning it,
	// so a cleanup-based release would drop the last reference to an entry the
	// new request is about to ask for again, destroying and re-rasterizing an
	// image for nothing. It matters most in strict mode, where every effect
	// evaluation runs twice by design.
	let active: SvgRenderHandle | undefined;
	const adopt = (acquired: SvgRenderHandle | undefined): void => {
		const previous = active;
		active = acquired;
		handle(acquired);
		if (previous !== undefined) {
			previous.release();
		}
	};

	effect(() => {
		const current = asset();
		const pixels = rasterPixels();

		if (pixels === undefined) {
			// No measurement yet, so there is no resolution to rasterize at.
			// Acquiring nothing is the whole point: `ImageContent` stays
			// `Content.none` for the frame or two before layout resolves,
			// exactly as it does under React, and the first raster this
			// component ever causes is one at a real size. `adopt` still runs
			// so that a component switching *back* to an unmeasured state
			// releases what it was holding.
			adopt(undefined);
			return;
		}

		// Colour is read here — and so becomes a dependency — only for an
		// asset whose pixels it actually changes. For a tintable or
		// fixed-colour asset it is deliberately not read at all, which is what
		// makes recolouring a Lucide icon cost nothing but an ImageColor3
		// write. The cache key excludes it for those assets too, so passing
		// `undefined` cannot split an entry.
		const affectsRaster = usesCurrentColor(current) && !isTintable(current);

		adopt(
			renderSvg(current, {
				size: pixels,
				currentColor: affectsRaster ? read(colorProp) : undefined,
				strokeWidth: read(strokeWidthProp),
				absoluteStrokeWidth: read(absoluteStrokeWidthProp),
			}),
		);
	});

	// Runs once, when the scope holding this component is destroyed — including
	// the scope a `<Show>` or `<Switch>` creates per branch. The effect above
	// owns every transition; this owns the end.
	cleanup(() => adopt(undefined));

	return (
		<imagelabel
			{...instanceProps}
			Size={
				instanceProps.Size ??
				(() => UDim2.fromOffset(declaredPixels().X, declaredPixels().Y))
			}
			BackgroundTransparency={instanceProps.BackgroundTransparency ?? 1}
			ImageContent={() => {
				const acquired = handle();
				return acquired !== undefined ? Content.fromObject(acquired.image) : Content.none;
			}}
			ImageColor3={() => {
				// Only a tintable asset routes its colour through ImageColor3.
				// For every other asset the colours are in the raster already
				// (with `color` delivered to the rasterizer as currentColor
				// above, where it applies), and multiplying them here would
				// silently change the artwork.
				const current = asset();
				return isTintable(current) ? read(colorProp) ?? NO_TINT : NO_TINT;
			}}
			AbsoluteSizeChanged={(absoluteSize: Vector2): void => {
				// Attached unconditionally: the raster follows what was laid
				// out, whether or not this component chose the layout size.
				//
				// Interpreted before it is stored, and the two halves of that
				// matter for different reasons.
				//
				// Snapping keeps subpixel layout noise away from the source:
				// Vide compares a source's new value against its old one, and
				// two AbsoluteSizes that round to the same integers produce an
				// equal Vector2 and no update at all. That is also what makes
				// this free in the ordinary case — a `size={24}` icon laid out
				// at exactly 24×24 measures the resolution it is already
				// drawing at, so the source never changes.
				//
				// The `undefined` case keeps the *first* call cheap. Vide's
				// `changed()` action fires its callback immediately, at
				// creation time, before the instance has been parented — so
				// this handler always sees `Vector2.zero` first, and reporting
				// that as a 1×1 measurement is exactly the placeholder raster
				// this component exists not to make.
				measuredPixels(measureSvgPixelSize(absoluteSize));
			}}
		>
			{props.children}
		</imagelabel>
	);
}
