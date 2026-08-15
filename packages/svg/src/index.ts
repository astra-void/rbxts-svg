/**
 * `@rbxts/svg` — framework-neutral compiled SVG assets for roblox-ts.
 *
 * ```text
 *                          SvgAsset
 *                             │
 *                    renderer + raster cache
 *                             │
 *         ┌───────────────────┼───────────────────┐
 *         ▼                   ▼                   ▼
 *  @rbxts/svg-react    @rbxts/svg-vide           Loom
 *         │                   │                   │
 *         └─── EditableImage ─┘                DOM SVG
 * ```
 *
 * This package owns the asset, the render cache, and the production
 * `EditableImage` renderer. It knows nothing about React or Vide — the arrows
 * only ever point inwards, and a binding is a lifetime adapter rather than a
 * second renderer. Two bindings in one game share this package's single cache,
 * so the same icon at the same size is rasterized once for both.
 *
 * Rendering is opt-in: call {@link installEditableImageRenderer} once at
 * startup. Nothing installs it automatically, so tooling and tests can use
 * the decoder and cache without a Roblox rendering environment, and tests can
 * install fakes through {@link setSvgRenderer}.
 *
 * # Getting an `SvgAsset`
 *
 * ```ts
 * import Search from "./icons/search.svg";
 * ```
 *
 * compiled ahead of time by `rbxts-svg build` and pointed at its generated
 * module by `@rbxts/svg-transformer`. See `docs/SVG-IMPORTS.md`.
 */

export {
	SvgAspectAlign,
	SvgAspectScale,
	SvgFeature,
	getFeatures,
	getPreserveAspectRatio,
	getShapeCount,
	getViewBox,
	isTintable,
	usesCurrentColor,
} from "./asset";
export type { SvgAsset, SvgPreserveAspectRatio, SvgViewBox } from "./asset";

export type { SvgCommandVisitor, SvgPaint, SvgShape } from "./ir/decode";

export { fitLengthScale, viewBoxTransform } from "./render/fit";
export type { SvgTransform } from "./render/fit";

export { measureSvgPixelSize, resolveSvgSizing, snapSvgPixelSize } from "./render/sizing";
export type { SvgSizing } from "./render/sizing";

export { SvgRenderCache, renderCacheKey, resolveRenderOptions } from "./render/cache";
export type { SvgRenderCacheStats } from "./render/cache";
export {
	getSvgRenderCache,
	getSvgRenderer,
	renderSvg,
	setSvgRenderer,
} from "./render/renderer";
export type {
	ResolvedRenderOptions,
	SvgRenderHandle,
	SvgRenderOptions,
	SvgRenderer,
} from "./render/types";

export {
	EDITABLE_IMAGE_MAX_DIMENSION,
	installEditableImageRenderer,
} from "./raster/editableImageRenderer";

export { unstable_internal } from "./internal";
