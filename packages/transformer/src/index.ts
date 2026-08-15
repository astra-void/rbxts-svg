/**
 * `@rbxts/svg-transformer` — makes `import Search from "./icons/search.svg"`
 * work under roblox-ts.
 *
 * # Setup
 *
 * ```json
 * {
 *   "compilerOptions": {
 *     "plugins": [{ "transform": "@rbxts/svg-transformer" }]
 *   }
 * }
 * ```
 *
 * and build the SVGs before compiling:
 *
 * ```bash
 * rbxts-svg build && rbxtsc
 * ```
 *
 * # What it does, and what it deliberately does not
 *
 * ```text
 * search.svg ──[ rbxts-svg build ]──> svg-cache/icons/search.svg.ts
 *                                              │
 * "./icons/search.svg" ──[ this ]──> "./svg-cache/icons/search.svg"
 *                                              │
 *                                        [ rbxtsc ] ──> search.svg.luau
 * ```
 *
 * The compiling is already done, by the time this runs, by a separate tool.
 * That split is the point: a generated `.ts` file is an ordinary TypeScript
 * input, so `rbxtsc -w` watches it, and editing an `.svg` rebuilds through the
 * compiler instead of around it. A transformer that read and compiled `.svg`
 * files itself would be shorter and would break watch mode, because TypeScript
 * would have no idea the `.svg` was an input at all.
 *
 * So this package holds no SVG knowledge whatsoever. It never loads
 * `@rbxts/svg-native`, never spawns a compiler, never watches anything, and
 * never writes a file. It rewrites a string.
 *
 * # roblox-ts plugin contract
 *
 * roblox-ts 3.x reads `compilerOptions.plugins` from `tsconfig.json`, resolves
 * each `transform` from the project directory, `require`s it, and calls the
 * default export. With no explicit `"type"`, the entry is the `"program"` form:
 *
 * ```ts
 * (program: ts.Program, config: PluginConfig, extras: { ts: typeof ts })
 *   => ts.TransformerFactory<ts.SourceFile>
 * ```
 *
 * See `roblox-ts/out/Project/transformers/createTransformerList.js`.
 */

import { type SvgTransformerConfig, resolveConfig } from "./config.js";
import { createSvgTransformer } from "./transform.js";

import type * as ts from "typescript";

/** The third argument roblox-ts passes to a `"program"`-type plugin. */
export interface TransformerExtras {
	readonly ts: typeof ts;
}

/**
 * The plugin entry point.
 *
 * `extras.ts` is roblox-ts's own TypeScript instance, and is used in preference
 * to one we would resolve ourselves — a second copy of the compiler in the same
 * process is a reliable source of subtle, hard-to-see bugs.
 */
export default function svgTransformer(
	program: ts.Program,
	config: SvgTransformerConfig = {},
	extras?: TransformerExtras,
): ts.TransformerFactory<ts.SourceFile> {
	const tsApi = extras?.ts ?? (require("typescript") as typeof ts);
	return createSvgTransformer({ tsApi, config: resolveConfig(program, config) });
}

export { resolveConfig, SvgTransformerConfigError } from "./config.js";
export type { ResolvedConfig, SvgTransformerConfig } from "./config.js";
export { mapSpecifier, shouldTransformFile } from "./paths.js";
export type { FileSystemHost, SpecifierMapping } from "./paths.js";
export { createSvgTransformer } from "./transform.js";
export type { TransformOptions } from "./transform.js";
export type { DiagnosticSink } from "./diagnostics.js";
