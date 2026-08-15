/** Compiling SVG source and files. */

import { readFileSync } from "node:fs";
import { relative } from "node:path";

import {
	loadNative,
	type NativeDiagnostic,
	type NativeRasterImage,
	type NativeRasterOptions,
} from "./native.js";
import type {
	CompileOptions,
	CompiledSvg,
	SvgDiagnostic,
	SvgDiagnosticSeverity,
} from "./types.js";
import { SvgCompileError } from "./types.js";

function toDiagnostic(raw: NativeDiagnostic): SvgDiagnostic {
	// Rebuild rather than spread: napi hands back plain objects with `undefined`
	// for absent optionals, and `exactOptionalPropertyTypes` rejects those.
	const diagnostic: {
		-readonly [K in keyof SvgDiagnostic]: SvgDiagnostic[K];
	} = {
		severity: raw.severity as SvgDiagnosticSeverity,
		code: raw.code,
		message: raw.message,
		rendered: raw.rendered,
	};
	if (raw.tag !== undefined) diagnostic.tag = raw.tag;
	if (raw.id !== undefined) diagnostic.id = raw.id;
	if (raw.path !== undefined) diagnostic.path = raw.path;
	if (raw.line !== undefined) diagnostic.line = raw.line;
	if (raw.column !== undefined) diagnostic.column = raw.column;
	return diagnostic;
}

/**
 * Compiles SVG source into the serialized IR.
 *
 * @throws {SvgCompileError} if the document is malformed, has no usable
 * coordinate system, or uses rendering features `@rbxts/svg` does not support
 * (unless `allowUnsupported` is set).
 */
export function compileSvg(
	source: string | Buffer,
	options: CompileOptions = {},
): CompiledSvg {
	const native = loadNative();

	let result;
	try {
		result = native.compileSvg(source, {
			...(options.dpi !== undefined ? { dpi: options.dpi } : {}),
			...(options.allowUnsupported !== undefined
				? { allowUnsupported: options.allowUnsupported }
				: {}),
			...(options.sourceName !== undefined
				? { sourceName: options.sourceName }
				: {}),
		});
	} catch (cause) {
		// The native layer already rendered the diagnostics; passing the message
		// through verbatim keeps the formatting the compiler intended.
		const message = cause instanceof Error ? cause.message : String(cause);
		throw new SvgCompileError(message, options.sourceName);
	}

	return {
		data: result.data,
		viewBox: {
			x: result.viewBoxX,
			y: result.viewBoxY,
			width: result.width,
			height: result.height,
		},
		width: result.width,
		height: result.height,
		preserveAspectRatio: result.preserveAspectRatio,
		flags: result.flags,
		hash: result.hash,
		irVersion: result.irVersion,
		shapeCount: result.shapeCount,
		diagnostics: result.diagnostics.map(toDiagnostic),
	};
}

/**
 * Compiles an SVG file.
 *
 * `sourceName` defaults to the path relative to `cwd`, so diagnostics read as
 * `src/icons/search.svg:3:5` rather than as an absolute path — stable across
 * machines, which matters for reproducible build logs.
 */
export function compileSvgFile(
	path: string,
	options: CompileOptions = {},
): CompiledSvg {
	const source = readFileSync(path);
	const sourceName = options.sourceName ?? relative(process.cwd(), path);
	return compileSvg(source, { ...options, sourceName });
}

/** Decodes serialized IR back into an inspectable structure, for tooling. */
export function decodeSvgIr(data: Buffer) {
	return loadNative().decodeSvgIr(data);
}

/**
 * Renders serialized IR through the **reference** rasterizer.
 *
 * This is `svg-raster` — the executable specification of what the Roblox
 * renderer must produce — and exists for golden-fixture generation and
 * tooling. It is not, and must never become, how anything renders at runtime.
 */
export function renderSvgIr(
	data: Buffer,
	width: number,
	height: number,
	options?: NativeRasterOptions,
): NativeRasterImage {
	return loadNative().renderSvgIr(data, width, height, options);
}

/** The IR format version this compiler produces. */
export function irVersion(): number {
	return loadNative().irVersion();
}
