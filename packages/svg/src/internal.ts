/**
 * Internal entry points.
 *
 * Everything here is called by generated code and by other `@rbxts/svg-*`
 * packages, never by application code. It is exported under the `unstable_internal`
 * name — rather than as ordinary named exports — precisely so that nobody
 * reaches for it by accident, and so that a review can see at a glance when
 * something has.
 *
 * No stability promise applies to anything in this module.
 */

import type { SvgAsset, SvgAssetData } from "./asset";
import { asAsset, assetData } from "./asset";
import { decodeBase64 } from "./ir/base64";
import {
	decodeAsset,
	forEachCommand,
	readPaint,
	readShape,
	type SvgCommandVisitor,
	type SvgPaint,
	type SvgShape,
} from "./ir/decode";

/**
 * Distinguishes assets that arrived without a content hash.
 *
 * An asset's id is only ever used as a cache key, so uniqueness is what
 * matters; a hash additionally lets two copies of the same icon share a raster.
 */
let anonymousAssetCounter = 0;

/**
 * Builds an asset from a base64-encoded IR blob.
 *
 * This is what a generated `.svg` module calls:
 *
 * ```ts
 * import { unstable_internal } from "@rbxts/svg";
 * export default unstable_internal.createAssetFromBase64("UlNWRw...", "a1b2c3...");
 * ```
 *
 * @param hash the compiled content hash. Optional, but supplying it lets
 * identical assets share cached rasters.
 */
function createAssetFromBase64(encoded: string, hash?: string): SvgAsset {
	return createAsset(decodeBase64(encoded), hash);
}

/** Builds an asset from raw serialized IR. */
function createAsset(data: buffer, hash?: string): SvgAsset {
	let id = hash;
	if (id === undefined) {
		anonymousAssetCounter += 1;
		id = `anon:${anonymousAssetCounter}`;
	}
	return asAsset(decodeAsset(id, data));
}

/** The runtime data behind an asset. */
function inspect(asset: SvgAsset): SvgAssetData {
	return assetData(asset);
}

/** Reads shape `index` of an asset. */
function shapeAt(asset: SvgAsset, index: number): SvgShape {
	return readShape(assetData(asset), index);
}

/** Reads paint `index` of an asset. */
function paintAt(asset: SvgAsset, index: number): SvgPaint {
	return readPaint(assetData(asset), index);
}

/** Walks a shape's geometry. */
function visitCommands(
	asset: SvgAsset,
	shape: SvgShape,
	visitor: SvgCommandVisitor,
): void {
	forEachCommand(assetData(asset), shape, visitor);
}

export const unstable_internal = {
	createAsset,
	createAssetFromBase64,
	inspect,
	shapeAt,
	paintAt,
	visitCommands,
} as const;
