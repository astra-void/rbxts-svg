#!/usr/bin/env node
/**
 * `rbxts-svg` — compiles a project's `.svg` files into generated TypeScript
 * modules.
 *
 * Run it before `rbxtsc`, or alongside it in watch mode:
 *
 * ```bash
 * rbxts-svg build --root src
 * rbxts-svg watch --root src   # in one terminal
 * rbxtsc -w                    # in another
 * ```
 *
 * See `docs/SVG-IMPORTS.md` for how this fits into a roblox-ts build.
 */

import { watch } from "node:fs";
import { join, relative, resolve } from "node:path";

import { buildSvgAssets, type GenerateOptions } from "./generate.js";
import { DEFAULT_OUT_DIR } from "./paths.js";
import { readTsConfigRootDir, resolveTsConfigPath } from "./tsconfig.js";
import { SvgCompileError } from "./types.js";

interface Args {
	command: string;
	rootDir: string;
	outDir: string;
	allowUnsupported: boolean;
	/** Where `rootDir` came from, for the one-line build summary. */
	rootDirSource: string;
}

const USAGE = `rbxts-svg — compile .svg files into roblox-ts modules

Usage:
  rbxts-svg build [options]
  rbxts-svg watch [options]

Options:
  --root <dir>          Project source root to scan
                        (default: "rootDir" from the project's tsconfig.json,
                        else src)
  --out <dir>           Where to write generated modules
                        (default: <root>/${DEFAULT_OUT_DIR})
  --project, -p <path>  tsconfig.json to read "rootDir" from, or the directory
                        containing it (default: ./tsconfig.json)
  --allow-unsupported   Downgrade unsupported SVG features to warnings
  -h, --help            Show this message

The source root defaults to the project's own "rootDir" so the generator and
@rbxts/svg-transformer cannot drift apart. Override it only if you also
override the transformer's, in its tsconfig plugin entry.
`;

function parseArgs(argv: readonly string[]): Args | undefined {
	if (argv.length === 0 || argv.includes("--help") || argv.includes("-h")) {
		return undefined;
	}

	const command = argv[0] ?? "";
	let rootDir: string | undefined;
	let outDir: string | undefined;
	let projectPath = ".";
	let allowUnsupported = false;

	for (let i = 1; i < argv.length; i += 1) {
		const flag = argv[i];
		if (flag === "--root") {
			rootDir = argv[++i] ?? rootDir;
		} else if (flag === "--out") {
			outDir = argv[++i];
		} else if (flag === "--project" || flag === "-p") {
			projectPath = argv[++i] ?? projectPath;
		} else if (flag === "--allow-unsupported") {
			allowUnsupported = true;
		} else {
			process.stderr.write(`Unknown option: ${flag}\n\n${USAGE}`);
			process.exit(2);
		}
	}

	const resolved = resolveRootDir(rootDir, projectPath);
	return {
		command,
		rootDir: resolved.rootDir,
		outDir: resolve(outDir ?? join(resolved.rootDir, DEFAULT_OUT_DIR)),
		allowUnsupported,
		rootDirSource: resolved.source,
	};
}

/**
 * Picks the source root, in the same order of preference the transformer uses:
 * an explicit value, then the project's `rootDir`, then `src`.
 */
function resolveRootDir(
	explicit: string | undefined,
	projectPath: string,
): { rootDir: string; source: string } {
	if (explicit !== undefined) {
		return { rootDir: resolve(explicit), source: "--root" };
	}

	const tsConfigPath = resolveTsConfigPath(projectPath);
	if (tsConfigPath !== undefined) {
		const fromConfig = readTsConfigRootDir(tsConfigPath);
		if (fromConfig !== undefined) {
			return {
				rootDir: fromConfig,
				source: `"rootDir" in ${relative(process.cwd(), tsConfigPath) || tsConfigPath}`,
			};
		}
	}
	return { rootDir: resolve("src"), source: "default" };
}

/** Returns true on success. Compile errors are reported, not thrown. */
function runBuild(options: GenerateOptions, rootDir: string): boolean {
	try {
		const { modules, pruned } = buildSvgAssets(options);

		let warnings = 0;
		for (const module of modules) {
			for (const diagnostic of module.compiled.diagnostics) {
				if (diagnostic.severity === "warning") {
					warnings += 1;
					process.stderr.write(`${diagnostic.rendered}\n\n`);
				}
			}
		}
		for (const path of pruned) {
			process.stdout.write(`removed ${relative(rootDir, path)}\n`);
		}

		const written = modules.filter((m) => m.written).length;
		process.stdout.write(
			`${modules.length} SVG(s), ${written} module(s) written, ` +
				`${warnings} warning(s)\n`,
		);
		return true;
	} catch (error) {
		if (error instanceof SvgCompileError) {
			process.stderr.write(`${error.message}\n`);
		} else {
			process.stderr.write(`${String(error)}\n`);
		}
		return false;
	}
}

function main(): void {
	let parsed: Args | undefined;
	try {
		parsed = parseArgs(process.argv.slice(2));
	} catch (error) {
		process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
		process.exit(2);
		return;
	}

	if (parsed === undefined) {
		process.stdout.write(USAGE);
		return;
	}
	const args = parsed;

	const options: GenerateOptions = {
		rootDir: args.rootDir,
		outDir: args.outDir,
		allowUnsupported: args.allowUnsupported,
	};

	process.stdout.write(
		`root ${relative(process.cwd(), args.rootDir) || "."} (${args.rootDirSource})\n`,
	);

	if (args.command === "build") {
		process.exit(runBuild(options, args.rootDir) ? 0 : 1);
	}

	if (args.command === "watch") {
		runBuild(options, args.rootDir);
		process.stdout.write(`watching ${args.rootDir} for .svg changes\n`);

		// Recursive watching is available on macOS and Windows on every
		// supported Node version, and on Linux since Node 20 — which is this
		// package's minimum. A rebuild is cheap (compiles are milliseconds and
		// unchanged modules are not rewritten), so a full pass per change is
		// simpler and more reliable than tracking per-file state.
		let pending: NodeJS.Timeout | undefined;
		watch(args.rootDir, { recursive: true }, (_event, filename) => {
			if (filename === null || !filename.toLowerCase().endsWith(".svg")) {
				return;
			}
			// Editors often write a file in several operations; coalesce them.
			clearTimeout(pending);
			pending = setTimeout(() => runBuild(options, args.rootDir), 25);
		});
		return;
	}

	process.stderr.write(`Unknown command: ${args.command}\n\n${USAGE}`);
	process.exit(2);
}

main();
