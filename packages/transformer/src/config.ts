/**
 * Where the transformer's two settings come from.
 *
 * It needs exactly two things — the source root and the generated output
 * directory — and both already exist somewhere in the project. Asking for them
 * a second time in a config file of our own would be the surest way to end up
 * with a generator writing to one place and a transformer pointing at another.
 * So the defaults are read from the project's `tsconfig.json`, which is the
 * same file `rbxts-svg` reads on the CLI side.
 */

import { dirname, isAbsolute, resolve } from "node:path";

import { DEFAULT_OUT_DIR, resolveOutDir } from "@rbxts/svg-compiler/paths";

import type * as ts from "typescript";

/**
 * The plugin entry as authored in `tsconfig.json`.
 *
 * roblox-ts passes the whole entry through, minus the keys it consumes itself
 * (`transform`, `import`, `type`, `after`, `afterDeclarations`), so any other
 * key is ours.
 *
 * ```json
 * {
 *   "compilerOptions": {
 *     "plugins": [{ "transform": "@rbxts/svg-transformer" }]
 *   }
 * }
 * ```
 */
export interface SvgTransformerConfig {
	/**
	 * Source root. Defaults to the project's `compilerOptions.rootDir`.
	 *
	 * Relative values resolve against the directory holding `tsconfig.json`.
	 */
	readonly rootDir?: string;
	/**
	 * Generated module directory. Defaults to `<rootDir>/svg-cache`.
	 *
	 * Must match whatever `rbxts-svg build --out` was given, if anything.
	 */
	readonly outDir?: string;
}

/** The settings, made absolute. */
export interface ResolvedConfig {
	readonly rootDir: string;
	readonly outDir: string;
}

/** Thrown for a configuration the transformer cannot act on. */
export class SvgTransformerConfigError extends Error {
	override readonly name = "SvgTransformerConfigError";
}

/**
 * Resolves the plugin entry against the program.
 *
 * Order of preference, for both values:
 *
 * 1. the plugin entry
 * 2. the project's own `tsconfig.json`
 * 3. an actionable error
 *
 * There is deliberately no "guess the project root" step. roblox-ts already
 * *requires* `rootDir` (or `rootDirs`) to be set, so a project reaching step 3
 * is one where nothing sensible could be inferred anyway, and a guess would
 * silently point every `.svg` import at the wrong place.
 */
export function resolveConfig(
	program: ts.Program,
	config: SvgTransformerConfig,
): ResolvedConfig {
	const compilerOptions = program.getCompilerOptions();
	const baseDir = configBaseDir(program);

	const rootDir = resolveRootDir(config, compilerOptions, baseDir);
	const outDir =
		config.outDir === undefined
			? resolveOutDir({ rootDir })
			: absolutize(config.outDir, baseDir);

	return { rootDir, outDir };
}

function resolveRootDir(
	config: SvgTransformerConfig,
	compilerOptions: ts.CompilerOptions,
	baseDir: string,
): string {
	if (config.rootDir !== undefined) {
		return absolutize(config.rootDir, baseDir);
	}
	if (compilerOptions.rootDir !== undefined) {
		// Already absolute: TypeScript resolves it when it parses the config.
		return absolutize(compilerOptions.rootDir, baseDir);
	}
	throw new SvgTransformerConfigError(
		`@rbxts/svg-transformer cannot tell where this project's source root is.\n\n` +
			`Set "rootDir" in tsconfig.json (roblox-ts requires it anyway):\n\n` +
			`  { "compilerOptions": { "rootDir": "src" } }\n\n` +
			`or set it on the plugin entry:\n\n` +
			`  { "transform": "@rbxts/svg-transformer", "rootDir": "src" }\n\n` +
			`It must be the same root you pass to \`rbxts-svg build\`, and the ` +
			`generated modules must land in <rootDir>/${DEFAULT_OUT_DIR} unless ` +
			`"outDir" is set on both.`,
	);
}

/**
 * The directory relative plugin paths are measured from.
 *
 * `configFilePath` is set whenever the program came from a `tsconfig.json`,
 * which is always the case under `rbxtsc`. The fallback keeps the transformer
 * usable from a hand-built program in tests, where cwd is the only anchor
 * there is.
 */
function configBaseDir(program: ts.Program): string {
	const configFilePath = program.getCompilerOptions().configFilePath;
	return typeof configFilePath === "string"
		? dirname(configFilePath)
		: program.getCurrentDirectory();
}

function absolutize(path: string, baseDir: string): string {
	return isAbsolute(path) ? resolve(path) : resolve(baseDir, path);
}
