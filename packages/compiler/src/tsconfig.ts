/**
 * Reading the source root out of the project's own `tsconfig.json`.
 *
 * # Why the CLI looks at tsconfig at all
 *
 * The generator and the transformer must agree on `rootDir` and `outDir`, or
 * the generator writes `src/svg-cache/icons/search.svg.ts` while the
 * transformer points imports at somewhere else entirely. Rather than introduce
 * an `rbxts-svg.config.json` for two values, both sides read the one file the
 * project already has: the transformer gets `rootDir` from the `ts.Program` it
 * is handed, and this module gets it from the same `tsconfig.json` on the CLI
 * side. roblox-ts *requires* `rootDir` (or `rootDirs`) to be set, so it is
 * always there to be read.
 *
 * `typescript` is loaded lazily and optionally. It is present in every
 * roblox-ts project, but `@rbxts/svg-compiler` is usable without one — a script
 * that just calls `buildSvgAssets` should not need a compiler installed — so a
 * missing `typescript` is a clear message on the path that needs it, not a hard
 * dependency for everyone.
 */

import { existsSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

/** The subset of `typescript` this module uses. */
interface TypeScriptModule {
	sys: {
		readFile(path: string, encoding?: string): string | undefined;
		fileExists(path: string): boolean;
		readDirectory(
			path: string,
			extensions?: readonly string[],
			exclude?: readonly string[],
			include?: readonly string[],
			depth?: number,
		): readonly string[];
		useCaseSensitiveFileNames: boolean;
		getCurrentDirectory(): string;
	};
	getParsedCommandLineOfConfigFile(
		configFileName: string,
		optionsToExtend: object | undefined,
		host: unknown,
	): { options: { rootDir?: string; rootDirs?: string[] } } | undefined;
}

function loadTypeScript(): TypeScriptModule | undefined {
	try {
		// eslint-disable-next-line @typescript-eslint/no-require-imports
		return require("typescript") as TypeScriptModule;
	} catch {
		return undefined;
	}
}

/**
 * Resolves `--project` into an actual `tsconfig.json` path.
 *
 * Accepts either the file or the directory containing it, matching what every
 * other TypeScript tool accepts.
 */
export function resolveTsConfigPath(projectPath: string): string | undefined {
	const absolute = resolve(projectPath);
	if (!existsSync(absolute)) {
		return undefined;
	}
	const candidate = statSync(absolute).isDirectory()
		? join(absolute, "tsconfig.json")
		: absolute;
	return existsSync(candidate) ? candidate : undefined;
}

/** Why a source root could not be derived, phrased for a terminal. */
export class TsConfigReadError extends Error {
	override readonly name = "TsConfigReadError";
}

/**
 * The absolute `rootDir` a `tsconfig.json` declares, or `undefined` if it
 * declares none.
 *
 * Only `rootDir` is honoured. `rootDirs` describes a *merged virtual*
 * directory, which has no single answer to "where does the source tree start",
 * and guessing one would be exactly the silent drift this module exists to
 * prevent.
 */
export function readTsConfigRootDir(tsConfigPath: string): string | undefined {
	const ts = loadTypeScript();
	if (ts === undefined) {
		throw new TsConfigReadError(
			`rbxts-svg needs "typescript" installed to read ${tsConfigPath}.\n` +
				`Install it, or pass the source root explicitly:\n\n` +
				`  rbxts-svg build --root src\n`,
		);
	}

	const host = {
		...ts.sys,
		onUnRecoverableConfigFileDiagnostic: (diagnostic: { messageText: unknown }) => {
			throw new TsConfigReadError(
				`rbxts-svg could not read ${tsConfigPath}: ${String(diagnostic.messageText)}`,
			);
		},
	};

	const parsed = ts.getParsedCommandLineOfConfigFile(tsConfigPath, {}, host);
	const rootDir = parsed?.options.rootDir;
	return rootDir === undefined ? undefined : resolve(dirname(tsConfigPath), rootDir);
}
