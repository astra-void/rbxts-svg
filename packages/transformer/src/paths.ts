/**
 * Deciding what a `.svg` module specifier should become.
 *
 * This is the whole of the transformer's logic, kept apart from the AST so it
 * can be tested as what it is: a function from
 *
 * ```text
 * (importing file, specifier) → rewritten specifier | leave alone | error
 * ```
 *
 * The path arithmetic itself is not implemented here. It comes from
 * `@rbxts/svg-compiler/paths`, the same module `rbxts-svg build` uses to decide
 * where to *write* each generated module — one definition, so the two can never
 * disagree. That entry point is pure path handling and pulls in no native
 * binary, which is why the transformer can depend on it without dragging a Rust
 * compiler into every `rbxtsc` run.
 *
 * The only thing this module touches beyond arithmetic is `fileExists`, and
 * only to tell the user *which* of two mistakes they made — a missing `.svg` or
 * an unbuilt cache. It never reads, hashes, or compiles an SVG: whether a
 * generated module is up to date is the generator's business, not ours.
 */

import { dirname, relative, resolve } from "node:path";

import {
	generatedModulePath,
	generatedModuleSpecifier,
	isInside,
	isRelativeSpecifier,
	isSvgSpecifier,
} from "@rbxts/svg-compiler/paths";

import type { ResolvedConfig } from "./config.js";

/** What should happen to one module specifier. */
export type SpecifierMapping =
	| {
			readonly kind: "rewrite";
			/** The POSIX specifier to substitute. */
			readonly specifier: string;
			/** Absolute path of the source `.svg`. */
			readonly sourcePath: string;
			/** Absolute path of its generated module. */
			readonly modulePath: string;
	  }
	| { readonly kind: "skip" }
	| {
			readonly kind: "error";
			/** Ready to print, already multi-line and already actionable. */
			readonly message: string;
	  };

/** The one filesystem question this module asks. */
export interface FileSystemHost {
	fileExists(path: string): boolean;
}

/**
 * Whether a file's own imports should be examined at all.
 *
 * Generated modules live under `outDir` and import `@rbxts/svg`, never a
 * `.svg`. Skipping them outright means a file named `search.svg.ts` can never
 * be mistaken for an SVG source and mapped a second time.
 */
export function shouldTransformFile(fileName: string, config: ResolvedConfig): boolean {
	return !isInside(fileName, config.outDir);
}

/**
 * Maps one specifier from one importing file.
 *
 * `importerFileName` is the absolute path of the file the specifier appears in;
 * everything is resolved relative to its directory, exactly as TypeScript would.
 */
export function mapSpecifier(
	importerFileName: string,
	specifier: string,
	config: ResolvedConfig,
	host: FileSystemHost,
): SpecifierMapping {
	if (!isSvgSpecifier(specifier)) {
		return { kind: "skip" };
	}

	if (!isRelativeSpecifier(specifier)) {
		return {
			kind: "error",
			message:
				`@rbxts/svg-transformer: only relative .svg imports are supported\n\n` +
				`  ${specifier}\n\n` +
				`imported from:\n\n` +
				`  ${display(importerFileName)}\n\n` +
				`Path aliases ("baseUrl", "paths") are not resolved for .svg imports, ` +
				`because roblox-ts does not resolve them for anything else either. ` +
				`Use a relative specifier:\n\n` +
				`  import Icon from "./icons/search.svg";\n`,
		};
	}

	const importerDir = dirname(resolve(importerFileName));
	const sourcePath = resolve(importerDir, specifier);

	// Already pointing at a generated module. `svg-cache/icons/search.svg`
	// resolves to `search.svg.ts` on its own, so this is a valid — if verbose —
	// thing to write, and rewriting it again would look for a `.svg` source
	// inside the cache that was never there.
	if (isInside(sourcePath, config.outDir)) {
		return { kind: "skip" };
	}

	if (!isInside(sourcePath, config.rootDir)) {
		return {
			kind: "error",
			message:
				`@rbxts/svg-transformer: .svg import escapes the source root\n\n` +
				`  ${specifier}\n\n` +
				`imported from:\n\n` +
				`  ${display(importerFileName)}\n\n` +
				`resolves to:\n\n` +
				`  ${display(sourcePath)}\n\n` +
				`which is outside the source root:\n\n` +
				`  ${display(config.rootDir)}\n\n` +
				`\`rbxts-svg build\` only scans inside that root, so no generated module ` +
				`would ever exist for this file. Move the .svg inside the root.\n`,
		};
	}

	if (!host.fileExists(sourcePath)) {
		return {
			kind: "error",
			message:
				`@rbxts/svg-transformer: cannot resolve SVG import\n\n` +
				`  ${specifier}\n\n` +
				`imported from:\n\n` +
				`  ${display(importerFileName)}\n\n` +
				`No such file:\n\n` +
				`  ${display(sourcePath)}\n`,
		};
	}

	const modulePath = generatedModulePath(sourcePath, config);
	if (!host.fileExists(modulePath)) {
		return {
			kind: "error",
			message:
				`@rbxts/svg-transformer: generated asset module is missing for\n\n` +
				`  ${display(sourcePath)}\n\n` +
				`imported from:\n\n` +
				`  ${display(importerFileName)}\n\n` +
				`Expected:\n\n` +
				`  ${display(modulePath)}\n\n` +
				`Compile the project's SVGs first:\n\n` +
				`  rbxts-svg build\n\n` +
				`or leave a watcher running beside rbxtsc:\n\n` +
				`  rbxts-svg watch\n`,
		};
	}

	return {
		kind: "rewrite",
		specifier: generatedModuleSpecifier(importerDir, sourcePath, config),
		sourcePath,
		modulePath,
	};
}

/**
 * Paths in messages are shown relative to cwd when that is shorter.
 *
 * Only ever cosmetic — it never reaches the emitted specifier, which is
 * computed from absolute paths and is identical on every machine.
 */
function display(path: string): string {
	const rel = relative(process.cwd(), path);
	const chosen = rel !== "" && !rel.startsWith("..") ? rel : path;
	return chosen.split("\\").join("/");
}
