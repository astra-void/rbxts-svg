/**
 * `@rbxts/svg-vide` — Vide bindings for `@rbxts/svg`.
 *
 * ```text
 *                  SvgAsset
 *                     │
 *                @rbxts/svg
 *          renderer + raster cache
 *                     │
 *          ┌──────────┴──────────┐
 *          ▼                     ▼
 *  @rbxts/svg-react       @rbxts/svg-vide
 * ```
 *
 * The core package owns the asset, the format, the rasterizer and the shared
 * cache. This package owns the one thing Vide adds: binding a
 * reference-counted raster's lifetime to a reactive scope, and choosing which
 * reactive reads are allowed to reach the rasterizer.
 *
 * There is no Vide-specific compiler, rasterizer or cache, and there is no
 * Vide-specific `.svg` import. A React tree and a Vide tree in the same game
 * consume the same `SvgAsset` and share one `EditableImage` per raster.
 *
 * # A renderer must be installed
 *
 * Importing this package installs nothing. Rendering without a renderer is a
 * configuration error, not a loading state, so it surfaces as `renderSvg`'s
 * actionable error rather than as an icon that silently never appears:
 *
 * ```ts
 * import { installEditableImageRenderer } from "@rbxts/svg";
 *
 * installEditableImageRenderer();
 * ```
 *
 * Once, at startup, for the whole application — not once per framework.
 *
 * # Sizing and colour
 *
 * Both are the core's semantics, unchanged: see {@link SvgSpecificProps}. The
 * sizing policy itself lives in `@rbxts/svg` (`resolveSvgSizing`), so React and
 * Vide cannot drift apart on what `size={24}` means.
 */

export { Svg } from "./Svg";
export type { SvgProps, SvgSpecificProps } from "./Svg";
