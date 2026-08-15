/**
 * `@rbxts/svg-react` — React bindings for `@rbxts/svg`.
 *
 * This package exists so that React is *not* a dependency of the core runtime.
 * `@rbxts/svg` owns the asset, the format and the render cache; this owns the
 * one thing React actually adds, which is tying a shared raster's lifetime to a
 * component's.
 *
 * # A renderer must be installed
 *
 * Both fail fast if no rasterizer has been installed. A missing renderer is a
 * configuration error, not a loading state, so it surfaces as `renderSvg`'s
 * actionable error rather than as an icon that silently never appears.
 *
 * The production renderer ships in `@rbxts/svg`; install it once at startup:
 *
 * ```ts
 * import { installEditableImageRenderer } from "@rbxts/svg";
 *
 * installEditableImageRenderer();
 * ```
 *
 * Installation is explicit rather than an import side effect, so tooling and
 * tests can use the decoder and cache without a Roblox rendering environment,
 * and can inject fakes through `setSvgRenderer`.
 */

export { Svg } from "./Svg";
export type { SvgProps } from "./Svg";
export { snapToPixels, svgSizing } from "./sizing";
export type { SvgSizing } from "./sizing";
export { useSvg } from "./useSvg";
export type { UseSvgOptions } from "./useSvg";
