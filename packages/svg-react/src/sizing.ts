/**
 * The sizing policy, now owned by `@rbxts/svg`.
 *
 * None of it was ever React-specific — what `size={24}` means, which prop wins
 * when `Size` is also given, and how a measured `AbsoluteSize` becomes a raster
 * resolution are questions every UI binding has to answer the same way. When
 * `@rbxts/svg-vide` arrived it needed exactly these answers, and the only way
 * for two bindings to be guaranteed to agree is for there to be one
 * implementation, so it moved to `@rbxts/svg/render/sizing`.
 *
 * This module stays as the compatibility surface: the names React users
 * already import keep resolving, and keep meaning what they meant.
 */

export { snapSvgPixelSize as snapToPixels, resolveSvgSizing as svgSizing } from "@rbxts/svg";
export type { SvgSizing } from "@rbxts/svg";
