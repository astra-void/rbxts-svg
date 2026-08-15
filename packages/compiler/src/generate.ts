/**
 * Generated-module emission — the build-time half of `import Search from
 * "./search.svg"`.
 *
 * # Why generate modules at all
 *
 * roblox-ts turns TypeScript modules into Luau modules. An `.svg` file is not a
 * TypeScript module, so something has to produce one. The alternative — a
 * transformer that reads the `.svg` from disk and injects a literal AST — looks
 * simpler and behaves badly: TypeScript's dependency graph would not know the
 * `.svg` is an input, so editing it would rebuild nothing in watch mode.
 *
 * Generating a real `.ts` file puts the SVG back inside the graph. The
 * generated module is an ordinary TypeScript input that `rbxtsc` already
 * watches, so the chain "edit `.svg` → regenerate `.ts` → rbxtsc rebuilds"
 * works with the compiler rather than around it.
 *
 * # Guarantees
 *
 * - **Deterministic.** The emitted text is a pure function of the compiled IR.
 *   No timestamps, no absolute paths, no machine-specific anything.
 * - **Stable paths.** `icons/search.svg` always maps to
 *   `<outDir>/icons/search.svg.ts`. The path is a pure function of the source
 *   path, so a specifier rewrite needs no lookup table, and watch mode is not
 *   chasing a filename that moves on every edit. (The content hash lives in the
 *   file header instead, where it does its job without churning the path.)
 * - **No spurious writes.** A file whose content is unchanged is not rewritten,
 *   so its mtime does not move and downstream watchers stay quiet.
 */

import { createHash } from "node:crypto";
import {
	existsSync,
	mkdirSync,
	readFileSync,
	readdirSync,
	rmSync,
	statSync,
	writeFileSync,
} from "node:fs";
import { dirname, join, posix, relative, resolve, sep } from "node:path";

import { compileSvg } from "./compile.js";
import {
	GENERATED_HEADER,
	GENERATED_SUFFIX,
	ambientModulePath,
	generatedModulePath,
	resolveOutDir,
	type SvgPathOptions,
} from "./paths.js";
import type { CompileOptions, CompiledSvg } from "./types.js";

export {
	AMBIENT_MODULE_FILE,
	DEFAULT_OUT_DIR,
	GENERATED_HEADER,
	GENERATED_SUFFIX,
	ambientModulePath,
	generatedModulePath,
} from "./paths.js";

export interface GenerateOptions extends CompileOptions, SvgPathOptions {
	/** Project root; generated paths mirror the source tree relative to this. */
	readonly rootDir: string;
	/** Where generated modules are written. Defaults to `<rootDir>/svg-cache`. */
	readonly outDir?: string | undefined;
}

export interface GeneratedModule {
	/** Absolute path of the source `.svg`. */
	readonly sourcePath: string;
	/** Absolute path of the generated `.ts`. */
	readonly modulePath: string;
	readonly compiled: CompiledSvg;
	/** False when the file already had exactly this content. */
	readonly written: boolean;
}

/**
 * Renders the generated ambient declaration that types `.svg` imports.
 *
 * This is what makes
 *
 * ```ts
 * import Search from "./icons/search.svg";
 * ```
 *
 * typecheck as an `SvgAsset` under plain `tsc --noEmit`, before any transformer
 * has run. TypeScript only honours `declare module "*.svg"` in a file that is
 * not itself a module, so this cannot live inside `@rbxts/svg`'s own
 * `index.d.ts` — there it would be read as a module augmentation and rejected.
 * Emitting it beside the generated modules puts it inside the project's
 * existing `include` globs, which is why a consumer never has to write or wire
 * up a shim of their own.
 *
 * It is a declaration file, so `rbxtsc` neither compiles nor copies it: no
 * ModuleScript exists at runtime just to carry a type.
 */
export function generateAmbientModuleSource(): string {
	return [
		GENERATED_HEADER,
		"//",
		"// Types `import Icon from \"./icon.svg\"` as an SvgAsset. The import is",
		"// rewritten to the generated module beside this file by",
		"// @rbxts/svg-transformer; this declaration is what lets the *source*",
		"// typecheck.",
		"",
		'declare module "*.svg" {',
		'\tconst asset: import("@rbxts/svg").SvgAsset;',
		"\texport default asset;",
		"}",
		"",
	].join("\n");
}

/**
 * The import a module needs before it can use {@link generateAssetExpression}.
 *
 * Exported alongside the expression so that a generator emitting asset code
 * never has to know *which* runtime entry point builds an asset — only that
 * this line goes at the top.
 */
export const ASSET_IMPORT_STATEMENT = 'import { unstable_internal } from "@rbxts/svg";';

/**
 * Renders the TypeScript expression that reconstructs a compiled asset.
 *
 * This is the single definition of "serialized IR plus hash becomes an
 * `SvgAsset`", and every generator in the repository goes through it: the
 * `.svg`-import modules below, and the Lucide packages' per-icon modules. A
 * second, slightly different template would be a second encoding contract with
 * `@rbxts/svg`, and the first time the internal entry point changed shape only
 * one of them would be updated.
 *
 * The hash is passed as the asset's runtime identity, not merely recorded in a
 * comment: it is what lets two independently generated copies of the same icon
 * — in `@rbxts/lucide-react` and in `@rbxts/lucide-vide`, say — resolve to one
 * cached raster.
 *
 * @param indent prefix for continuation lines, so the expression can be
 * embedded at any nesting depth without the caller reformatting it.
 */
export function generateAssetExpression(compiled: CompiledSvg, indent = ""): string {
	return [
		"unstable_internal.createAssetFromBase64(",
		`${indent}\t"${compiled.data.toString("base64")}",`,
		`${indent}\t"${compiled.hash}",`,
		`${indent})`,
	].join("\n");
}

/**
 * Renders the generated module's text.
 *
 * `sourceLabel` is written into a comment for traceability. It is normalized to
 * forward slashes so the output is identical on Windows and POSIX — otherwise
 * the same project would produce different bytes on different machines.
 */
export function generateModuleSource(
	compiled: CompiledSvg,
	sourceLabel: string,
): string {
	const label = sourceLabel.split(sep).join(posix.sep);
	return [
		GENERATED_HEADER,
		`// source: ${label}`,
		`// ir-version: ${compiled.irVersion}`,
		`// hash: ${compiled.hash}`,
		`// view-box: ${compiled.viewBox.x} ${compiled.viewBox.y} ${compiled.viewBox.width} ${compiled.viewBox.height}`,
		`// preserve-aspect-ratio: ${compiled.preserveAspectRatio}`,
		`// flags: ${compiled.flags}`,
		"",
		ASSET_IMPORT_STATEMENT,
		"",
		`export default ${generateAssetExpression(compiled)};`,
		"",
	].join("\n");
}

/** Compiles one SVG and writes its generated module. */
export function generateModule(
	sourcePath: string,
	options: GenerateOptions,
): GeneratedModule {
	const absoluteSource = resolve(sourcePath);
	const modulePath = generatedModulePath(absoluteSource, options);
	const sourceLabel = relative(resolve(options.rootDir), absoluteSource);

	const compiled = compileSvg(readFileSync(absoluteSource), {
		...options,
		sourceName: options.sourceName ?? sourceLabel,
	});

	const contents = generateModuleSource(compiled, sourceLabel);
	const written = writeIfChanged(modulePath, contents);
	return { sourcePath: absoluteSource, modulePath, compiled, written };
}

/**
 * Writes only when the content differs.
 *
 * Rewriting an identical file would bump its mtime and wake every downstream
 * watcher for nothing — the difference between a quiet incremental build and a
 * rebuild storm.
 */
function writeIfChanged(path: string, contents: string): boolean {
	if (existsSync(path) && readFileSync(path, "utf8") === contents) {
		return false;
	}
	mkdirSync(dirname(path), { recursive: true });
	writeFileSync(path, contents, "utf8");
	return true;
}

/**
 * Caches compilation by source content.
 *
 * Two modules importing the same `./search.svg`, or two identical SVGs at
 * different paths, compile once. The key is the *source bytes* plus the options
 * that affect output, so a cache hit is only ever a genuine one.
 */
export class SvgCompilationCache {
	private readonly entries = new Map<string, CompiledSvg>();

	get size(): number {
		return this.entries.size;
	}

	/** Compiles `source`, reusing a previous result when one applies. */
	compile(source: Buffer, options: CompileOptions = {}): CompiledSvg {
		const key = cacheKey(source, options);
		const hit = this.entries.get(key);
		if (hit !== undefined) {
			return hit;
		}
		const compiled = compileSvg(source, options);
		this.entries.set(key, compiled);
		return compiled;
	}

	clear(): void {
		this.entries.clear();
	}
}

/**
 * Only options that change the compiled bytes belong in the key.
 *
 * `sourceName` is excluded on purpose: it only labels diagnostics, and
 * including it would defeat deduplication across paths.
 */
function cacheKey(source: Buffer, options: CompileOptions): string {
	const hash = createHash("sha256");
	hash.update(source);
	hash.update(` dpi=${options.dpi ?? 96}`);
	hash.update(` allowUnsupported=${options.allowUnsupported ?? false}`);
	return hash.digest("hex");
}

/** Recursively finds every `.svg` under `rootDir`, excluding `outDir`. */
export function findSvgFiles(rootDir: string, outDir: string): string[] {
	const root = resolve(rootDir);
	const excluded = resolve(outDir);
	const found: string[] = [];

	const walk = (dir: string): void => {
		for (const entry of readdirSync(dir, { withFileTypes: true })) {
			const path = join(dir, entry.name);
			if (entry.isDirectory()) {
				if (path === excluded || entry.name === "node_modules" || entry.name.startsWith(".")) {
					continue;
				}
				walk(path);
			} else if (entry.isFile() && entry.name.toLowerCase().endsWith(".svg")) {
				found.push(path);
			}
		}
	};

	if (existsSync(root) && statSync(root).isDirectory()) {
		walk(root);
	}
	// Sorted so a build's output order does not depend on the filesystem.
	return found.sort();
}

export interface BuildResult {
	readonly modules: readonly GeneratedModule[];
	/** Generated modules removed because their source no longer exists. */
	readonly pruned: readonly string[];
	/** Absolute path of the emitted `*.svg` ambient declaration. */
	readonly ambientModulePath: string;
}

/**
 * Compiles every SVG under `rootDir` and prunes orphaned generated modules.
 *
 * Pruning only ever deletes files carrying [`GENERATED_HEADER`], so a
 * misconfigured `outDir` cannot destroy hand-written source.
 */
export function buildSvgAssets(options: GenerateOptions): BuildResult {
	const rootDir = resolve(options.rootDir);
	const outDir = resolveOutDir(options);

	const sources = findSvgFiles(rootDir, outDir);
	const modules = sources.map((source) =>
		generateModule(source, { ...options, outDir }),
	);

	// Written unconditionally, including for a project with no SVGs yet: the
	// declaration is what makes the *first* `.svg` import typecheck, and having
	// it appear only once an SVG exists would be a confusing chicken-and-egg.
	const ambient = ambientModulePath({ rootDir, outDir });
	writeIfChanged(ambient, generateAmbientModuleSource());

	const expected = new Set(modules.map((m) => m.modulePath));
	expected.add(ambient);
	const pruned = pruneOrphans(outDir, expected);
	return { modules, pruned, ambientModulePath: ambient };
}

function pruneOrphans(outDir: string, expected: ReadonlySet<string>): string[] {
	if (!existsSync(outDir)) {
		return [];
	}
	const removed: string[] = [];

	const walk = (dir: string): void => {
		for (const entry of readdirSync(dir, { withFileTypes: true })) {
			const path = join(dir, entry.name);
			if (entry.isDirectory()) {
				walk(path);
			} else if (
				entry.isFile() &&
				path.endsWith(GENERATED_SUFFIX) &&
				!expected.has(path) &&
				readFileSync(path, "utf8").startsWith(GENERATED_HEADER)
			) {
				rmSync(path);
				removed.push(path);
			}
		}
	};

	walk(outDir);
	return removed.sort();
}
