/**
 * Generated-module emission.
 *
 * These test the properties the `.svg` import pipeline depends on: stable
 * paths, deterministic content, no spurious writes, deduplicated compilation,
 * and pruning that cannot eat hand-written files.
 */

import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
	GENERATED_HEADER,
	SvgCompilationCache,
	buildSvgAssets,
	findSvgFiles,
	generateModule,
	generateModuleSource,
	generatedModulePath,
	compileSvgFile,
} from "@rbxts/svg-compiler";

const FIXTURES = join(__dirname, "../fixtures");

const ICON =
	'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" ' +
	'stroke="currentColor" stroke-width="2"><path d="M4 12 L20 12"/></svg>';

let root: string;
let outDir: string;

beforeEach(() => {
	root = mkdtempSync(join(tmpdir(), "rbxts-svg-"));
	outDir = join(root, ".svg-cache");
});

afterEach(() => {
	rmSync(root, { recursive: true, force: true });
});

function writeIcon(relativePath: string, contents = ICON): string {
	const path = join(root, relativePath);
	mkdirSync(join(path, ".."), { recursive: true });
	writeFileSync(path, contents, "utf8");
	return path;
}

describe("generatedModulePath", () => {
	it("mirrors the source tree and is a pure function of the paths", () => {
		const path = generatedModulePath(join(root, "icons/search.svg"), {
			rootDir: root,
			outDir,
		});
		expect(path).toBe(join(outDir, "icons/search.svg.ts"));
	});

	it("is stable across edits, so a rewritten import specifier stays valid", () => {
		const source = writeIcon("icons/search.svg");
		const before = generateModule(source, { rootDir: root, outDir });

		writeIcon("icons/search.svg", ICON.replace("L20 12", "L20 13"));
		const after = generateModule(source, { rootDir: root, outDir });

		expect(after.modulePath).toBe(before.modulePath);
		expect(after.compiled.hash).not.toBe(before.compiled.hash);
	});

	it("refuses sources outside the project root", () => {
		expect(() =>
			generatedModulePath("/elsewhere/icon.svg", { rootDir: root, outDir }),
		).toThrow(/outside rootDir/);
	});
});

describe("generateModuleSource", () => {
	it("is deterministic", () => {
		const compiled = compileSvgFile(join(FIXTURES, "lucide/search.svg"));
		const a = generateModuleSource(compiled, "icons/search.svg");
		const b = generateModuleSource(compiled, "icons/search.svg");
		expect(a).toBe(b);
	});

	it("emits a module that imports the runtime and carries its provenance", () => {
		const compiled = compileSvgFile(join(FIXTURES, "lucide/search.svg"));
		const text = generateModuleSource(compiled, "icons/search.svg");

		expect(text.startsWith(GENERATED_HEADER)).toBe(true);
		expect(text).toContain("// source: icons/search.svg");
		expect(text).toContain(`// hash: ${compiled.hash}`);
		expect(text).toContain(`// ir-version: ${compiled.irVersion}`);
		expect(text).toContain(
			`// preserve-aspect-ratio: ${compiled.preserveAspectRatio}`,
		);
		expect(text).toContain('import { unstable_internal } from "@rbxts/svg";');
		expect(text).toContain("export default unstable_internal.createAssetFromBase64(");
		// The hash is passed through so identical icons share a cached raster.
		expect(text).toContain(`"${compiled.hash}",`);
	});

	it("round-trips the payload through base64", () => {
		const compiled = compileSvgFile(join(FIXTURES, "lucide/search.svg"));
		const text = generateModuleSource(compiled, "icons/search.svg");
		const payload = /createAssetFromBase64\(\s*"([^"]+)"/.exec(text)?.[1];

		expect(payload).toBeDefined();
		expect(Buffer.from(payload!, "base64").equals(compiled.data)).toBe(true);
	});

	it("normalizes path separators so output matches on every platform", () => {
		const compiled = compileSvgFile(join(FIXTURES, "lucide/search.svg"));
		const text = generateModuleSource(compiled, `icons${require("node:path").sep}a.svg`);
		expect(text).toContain("// source: icons/a.svg");
	});
});

describe("generateModule", () => {
	it("does not rewrite an unchanged file", () => {
		const source = writeIcon("icons/search.svg");
		const options = { rootDir: root, outDir };

		const first = generateModule(source, options);
		expect(first.written).toBe(true);
		const mtime = statSync(first.modulePath).mtimeMs;

		const second = generateModule(source, options);
		expect(second.written).toBe(false);
		expect(statSync(second.modulePath).mtimeMs).toBe(mtime);
	});

	it("rewrites when the source changes", () => {
		const source = writeIcon("icons/search.svg");
		const options = { rootDir: root, outDir };
		generateModule(source, options);

		writeIcon("icons/search.svg", ICON.replace("L20 12", "L20 13"));
		expect(generateModule(source, options).written).toBe(true);
	});

	it("attributes diagnostics to the path relative to the project root", () => {
		writeIcon("icons/bad.svg", '<svg viewBox="0 0 24 24"><text x="0" y="0">hi</text></svg>');
		expect(() =>
			generateModule(join(root, "icons/bad.svg"), { rootDir: root, outDir }),
		).toThrow(/icons\/bad\.svg/);
	});
});

describe("buildSvgAssets", () => {
	it("compiles every SVG under the root", () => {
		writeIcon("a.svg");
		writeIcon("nested/b.svg");
		writeIcon("nested/deeper/c.svg");

		const { modules } = buildSvgAssets({ rootDir: root, outDir });
		expect(modules).toHaveLength(3);
		for (const module of modules) {
			expect(readFileSync(module.modulePath, "utf8")).toContain(GENERATED_HEADER);
		}
	});

	it("prunes generated modules whose source is gone", () => {
		writeIcon("a.svg");
		const stale = writeIcon("b.svg");
		const first = buildSvgAssets({ rootDir: root, outDir });
		expect(first.modules).toHaveLength(2);

		rmSync(stale);
		const second = buildSvgAssets({ rootDir: root, outDir });
		expect(second.modules).toHaveLength(1);
		expect(second.pruned).toHaveLength(1);
		expect(second.pruned[0]).toContain("b.svg.ts");
	});

	it("never deletes a file it did not generate", () => {
		writeIcon("a.svg");
		mkdirSync(outDir, { recursive: true });
		const handWritten = join(outDir, "notes.ts");
		writeFileSync(handWritten, "export const mine = 1;\n", "utf8");

		const { pruned } = buildSvgAssets({ rootDir: root, outDir });
		expect(pruned).toHaveLength(0);
		expect(readFileSync(handWritten, "utf8")).toBe("export const mine = 1;\n");
	});

	it("does not treat its own output as source", () => {
		writeIcon("a.svg");
		buildSvgAssets({ rootDir: root, outDir });
		const found = findSvgFiles(root, outDir);
		expect(found).toHaveLength(1);
	});

	it("returns sources in a stable order", () => {
		writeIcon("z.svg");
		writeIcon("a.svg");
		writeIcon("m.svg");
		const names = findSvgFiles(root, outDir).map((p) => p.split("/").pop());
		expect(names).toEqual(["a.svg", "m.svg", "z.svg"]);
	});
});

describe("SvgCompilationCache", () => {
	it("compiles duplicate sources once", () => {
		const cache = new SvgCompilationCache();
		const source = Buffer.from(ICON, "utf8");

		const a = cache.compile(source);
		const b = cache.compile(Buffer.from(ICON, "utf8"));

		expect(cache.size).toBe(1);
		expect(b).toBe(a);
	});

	it("keys on the source rather than on the file name", () => {
		const cache = new SvgCompilationCache();
		cache.compile(Buffer.from(ICON, "utf8"), { sourceName: "one.svg" });
		cache.compile(Buffer.from(ICON, "utf8"), { sourceName: "two.svg" });
		expect(cache.size).toBe(1);
	});

	it("separates entries whose options change the output", () => {
		const cache = new SvgCompilationCache();
		const source = Buffer.from(ICON, "utf8");
		cache.compile(source, { allowUnsupported: false });
		cache.compile(source, { allowUnsupported: true });
		expect(cache.size).toBe(2);
	});
});
