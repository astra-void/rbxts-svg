/**
 * Renderer registration and the public `renderSvg` entry point.
 *
 * The rasterizer is a *plug-in* rather than a hard dependency of this package,
 * for three reasons: `@rbxts/svg` must stay usable in contexts with no Roblox
 * rendering at all (tooling, tests, a future DOM target); the reference
 * renderer and the production renderer must be interchangeable so their output
 * can be compared; and the compiler/asset boundary must not drag `EditableImage`
 * into every consumer.
 *
 * The production backend is installed with `installEditableImageRenderer()`.
 * Until a renderer is registered, {@link renderSvg} fails with an explanation
 * rather than returning something that looks like it worked.
 *
 * # Fail fast, and only once
 *
 * A missing renderer is a programming or configuration error, not a normal
 * loading state, so it throws. `@rbxts/svg-react` deliberately does not catch
 * it: an icon that silently renders as nothing is far harder to diagnose than
 * one that fails loudly at startup. The error is raised at exactly one layer —
 * here — so nothing above it reports the same problem a second time.
 */

import type { SvgAsset } from "../asset";
import { SvgRenderCache } from "./cache";
import type { SvgRenderHandle, SvgRenderOptions, SvgRenderer } from "./types";

let activeRenderer: SvgRenderer | undefined;
let activeCache: SvgRenderCache | undefined;

/**
 * Installs the rasterizer.
 *
 * Called once at startup by whichever renderer package is in use. Replacing a
 * renderer clears the cache, because entries rasterized by the old one are not
 * valid for the new one.
 */
export function setSvgRenderer(renderer: SvgRenderer): void {
	if (activeCache !== undefined) {
		activeCache.clear();
	}
	activeRenderer = renderer;
	activeCache = new SvgRenderCache(renderer);
}

/** The active renderer, or `undefined` if none is installed. */
export function getSvgRenderer(): SvgRenderer | undefined {
	return activeRenderer;
}

/** The cache backing {@link renderSvg}. Exposed for diagnostics and tests. */
export function getSvgRenderCache(): SvgRenderCache | undefined {
	return activeCache;
}

/**
 * Renders an asset, reusing a cached raster when one matches.
 *
 * The returned handle must be released when the consumer is done with it;
 * `@rbxts/svg-react` does this on unmount.
 */
export function renderSvg(
	asset: SvgAsset,
	options: SvgRenderOptions,
): SvgRenderHandle {
	if (activeCache === undefined) {
		error(
			"@rbxts/svg: no renderer is installed, so renderSvg cannot rasterize " +
				"anything.\n" +
				"Call installEditableImageRenderer() once at startup to use the " +
				"production EditableImage backend, or install a custom one with " +
				"setSvgRenderer().",
		);
	}
	return activeCache.acquire(asset, options);
}
