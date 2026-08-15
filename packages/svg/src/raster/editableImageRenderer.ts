/**
 * The production Roblox backend: the software rasterizer's RGBA buffer written
 * into an `EditableImage`.
 *
 * This file is deliberately thin. Everything about *what* the pixels are lives
 * in `./render`; everything here is Roblox plumbing — allocation, its failure
 * modes, and the platform's own size limit, which is a property of
 * `EditableImage` and not of the rasterizer. The Rust reference keeps its much
 * larger `MAX_DIMENSION` for exactly that reason: reference-renderer
 * capability and Roblox backend capability are different numbers on purpose.
 */

import type { SvgAsset } from "../asset";
import { assetData, isTintable } from "../asset";
import { setSvgRenderer } from "../render/renderer";
import type { ResolvedRenderOptions, SvgRenderer } from "../render/types";
import { rasterize } from "./render";

/**
 * The largest width or height `AssetService.CreateEditableImage` accepts.
 *
 * Enforced here rather than clamped: silently rendering a 1500px request at
 * 1024px would change the visual resolution, the cache identity and the
 * meaning of `absoluteStrokeWidth` without telling anyone. A future explicit
 * max-resolution/downsampling policy belongs above this layer.
 */
export const EDITABLE_IMAGE_MAX_DIMENSION = 1024;

/**
 * Bumped whenever this backend's output changes for the same input, so stale
 * rasters fall out of the cache instead of surviving an upgrade.
 */
export const EDITABLE_IMAGE_RENDERER_VERSION = 1;

/**
 * The allocation seam, internal on purpose.
 *
 * Editable-image memory is device-budgeted and allocation genuinely fails in
 * the field; this is the boundary that lets tests exercise that failure
 * deterministically instead of trying to exhaust a real device. It is not
 * public API — applications install the renderer, they do not construct it.
 */
export interface EditableImageFactory {
	/** Returns a fresh image, or `undefined` when the budget is exhausted. */
	create(size: Vector2): EditableImage | undefined;
}

const assetServiceFactory: EditableImageFactory = {
	create(size: Vector2): EditableImage | undefined {
		// Resolved lazily so that merely loading this module — which the
		// package root does — never touches a Roblox service. Tooling and the
		// standalone Luau suite import the package without a `game` global.
		const [ok, image] = pcall(() =>
			game.GetService("AssetService").CreateEditableImage({ Size: size }),
		);
		// The engine reports budget exhaustion inconsistently — sometimes nil,
		// sometimes an error — so both collapse to the same answer here.
		if (!ok || image === undefined) {
			return undefined;
		}
		return image as EditableImage;
	},
};

/**
 * Builds the production renderer. Exposed (rather than only
 * {@link installEditableImageRenderer}) so tests can inject a fake
 * {@link EditableImageFactory}; applications have no reason to call it.
 */
export function createEditableImageRenderer(
	factory: EditableImageFactory = assetServiceFactory,
): SvgRenderer {
	return {
		version: EDITABLE_IMAGE_RENDERER_VERSION,

		render(asset: SvgAsset, options: ResolvedRenderOptions): EditableImage {
			const width = options.pixelWidth;
			const height = options.pixelHeight;
			const data = assetData(asset);

			if (width < 1 || height < 1) {
				error(`@rbxts/svg: cannot rasterize ${width}x${height} pixels (asset ${data.id})`);
			}
			if (width > EDITABLE_IMAGE_MAX_DIMENSION || height > EDITABLE_IMAGE_MAX_DIMENSION) {
				error(
					`@rbxts/svg: requested SVG raster size ${width}x${height} exceeds the ` +
						`EditableImage backend limit of ${EDITABLE_IMAGE_MAX_DIMENSION}x` +
						`${EDITABLE_IMAGE_MAX_DIMENSION} (asset ${data.id}). Render at a smaller ` +
						`size, or downsample explicitly.`,
				);
			}

			// A tintable asset rasterizes as a colour-free alpha mask that
			// ImageColor3 tints; everything else gets full colour, with
			// currentColor resolved to the requested colour.
			const mask = isTintable(asset);
			const colour = options.currentColor;
			const pixels = rasterize(data, {
				pixelWidth: width,
				pixelHeight: height,
				alphaMask: mask,
				currentColorR: math.round(colour.R * 255),
				currentColorG: math.round(colour.G * 255),
				currentColorB: math.round(colour.B * 255),
				strokeWidth: options.strokeWidth,
			});

			const size = new Vector2(width, height);
			const image = factory.create(size);
			if (image === undefined) {
				error(
					`@rbxts/svg: failed to allocate an EditableImage for a ${width}x${height} ` +
						`SVG raster (asset ${data.id}). The client may have exhausted its ` +
						`editable-image memory budget.`,
				);
			}

			// One full-image write: the raster is immutable once cached, so
			// there is nothing to gain from incremental updates.
			image.WritePixelsBuffer(Vector2.zero, size, pixels);
			return image;
		},

		destroy(image: EditableImage): void {
			image.Destroy();
		},
	};
}

/**
 * Installs the production `EditableImage` renderer.
 *
 * ```ts
 * import { installEditableImageRenderer } from "@rbxts/svg";
 * installEditableImageRenderer();
 * ```
 *
 * Call once at startup, before anything renders. Calling again follows
 * `setSvgRenderer`'s documented semantics — the cache is cleared and a fresh
 * renderer takes over — so a repeated call is safe, if pointless. Nothing
 * installs this automatically: importing `@rbxts/svg` must stay side-effect
 * free for tooling, tests and non-Roblox targets, and React deliberately does
 * not own renderer initialization.
 */
export function installEditableImageRenderer(): void {
	setSvgRenderer(createEditableImageRenderer());
}
