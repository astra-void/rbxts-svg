/**
 * The specifier mapping, on its own.
 *
 * Everything here is a pure function, so these tests are about the properties
 * the rest of the pipeline relies on: that the emitted specifier is POSIX and
 * relative on every host, that the acceptance rules match the generator's, and
 * that the transformer never rewrites something that is not an SVG import.
 *
 * The Windows cases run on POSIX machines and vice versa: `node:path` exports
 * `win32` and `posix` implementations, and the mapping takes whichever it is
 * handed. Cross-platform behaviour is not something to find out about from a CI
 * runner that happens to be Linux.
 */

import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

import {
	DEFAULT_OUT_DIR,
	SvgOutsideRootError,
	generatedModulePath,
	generatedModuleSpecifier,
	isInside,
	isRelativeSpecifier,
	isSvgSpecifier,
	posixPaths,
	resolveOutDir,
	toModuleSpecifier,
	windowsPaths,
} from "@rbxts/svg-compiler/paths";
import { mapSpecifier, shouldTransformFile } from "@rbxts/svg-transformer";
import type { FileSystemHost } from "@rbxts/svg-transformer";

const ROOT = resolve("/project/src");
const OUT = join(ROOT, DEFAULT_OUT_DIR);
const CONFIG = { rootDir: ROOT, outDir: OUT };

/** A filesystem where a fixed set of paths exists and nothing else does. */
function fakeHost(...existing: string[]): FileSystemHost {
	const set = new Set(existing.map((path) => resolve(path)));
	return { fileExists: (path) => set.has(resolve(path)) };
}

/** The two files a successful mapping needs to find. */
function bothExist(source: string): FileSystemHost {
	return fakeHost(source, generatedModulePath(source, CONFIG));
}

describe("generatedModulePath", () => {
	it("mirrors the source tree under the generated directory", () => {
		expect(generatedModulePath(join(ROOT, "icons/search.svg"), CONFIG)).toBe(
			join(OUT, "icons/search.svg.ts"),
		);
	});

	it("is stable regardless of how deep the source sits", () => {
		expect(generatedModulePath(join(ROOT, "a/b/c/d/icon.svg"), CONFIG)).toBe(
			join(OUT, "a/b/c/d/icon.svg.ts"),
		);
	});

	it("honours a custom generated directory outside the source root", () => {
		const custom = { rootDir: ROOT, outDir: resolve("/project/generated") };
		expect(generatedModulePath(join(ROOT, "icons/search.svg"), custom)).toBe(
			resolve("/project/generated/icons/search.svg.ts"),
		);
	});

	it("refuses a source outside the root, exactly as the generator does", () => {
		expect(() => generatedModulePath(resolve("/elsewhere/icon.svg"), CONFIG)).toThrow(
			SvgOutsideRootError,
		);
	});

	it("defaults the generated directory to <rootDir>/svg-cache", () => {
		expect(resolveOutDir({ rootDir: ROOT })).toBe(OUT);
	});
});

describe("generatedModuleSpecifier", () => {
	const cases: ReadonlyArray<readonly [string, string, string, string]> = [
		["same directory", "", "icon.svg", "./svg-cache/icon.svg"],
		["a sibling directory", "", "icons/search.svg", "./svg-cache/icons/search.svg"],
		["one level down", "ui", "icons/search.svg", "../svg-cache/icons/search.svg"],
		[
			"a deeply nested importer",
			"screens/settings/panels",
			"assets/icons/search.svg",
			"../../../svg-cache/assets/icons/search.svg",
		],
		[
			"an importer already inside the mirrored tree",
			"assets/icons",
			"assets/icons/search.svg",
			"../../svg-cache/assets/icons/search.svg",
		],
	];

	for (const [name, importerDir, source, expected] of cases) {
		it(`maps ${name}`, () => {
			expect(
				generatedModuleSpecifier(join(ROOT, importerDir), join(ROOT, source), CONFIG),
			).toBe(expected);
		});
	}

	it("drops the generated .ts suffix", () => {
		// Node10 resolution — which roblox-ts pins — finds `search.svg.ts` from
		// `./…/search.svg` by adding the extension itself.
		expect(generatedModuleSpecifier(ROOT, join(ROOT, "a.svg"), CONFIG)).not.toContain(".ts");
	});

	it("produces the same POSIX specifier from Windows paths", () => {
		const config = { rootDir: "C:\\project\\src", outDir: "C:\\project\\src\\svg-cache" };
		expect(
			generatedModuleSpecifier(
				"C:\\project\\src\\screens\\settings",
				"C:\\project\\src\\assets\\icons\\search.svg",
				config,
				windowsPaths,
			),
		).toBe("../../svg-cache/assets/icons/search.svg");
	});

	it("produces the same POSIX specifier from POSIX paths", () => {
		const config = { rootDir: "/project/src", outDir: "/project/src/svg-cache" };
		expect(
			generatedModuleSpecifier(
				"/project/src/screens/settings",
				"/project/src/assets/icons/search.svg",
				config,
				posixPaths,
			),
		).toBe("../../svg-cache/assets/icons/search.svg");
	});

	it("rejects a Windows source on another drive", () => {
		expect(() =>
			generatedModulePath(
				"D:\\elsewhere\\icon.svg",
				{ rootDir: "C:\\project\\src" },
				windowsPaths,
			),
		).toThrow(SvgOutsideRootError);
	});
});

describe("toModuleSpecifier", () => {
	it("normalizes Windows separators", () => {
		expect(toModuleSpecifier("..\\svg-cache\\icons\\search.svg")).toBe(
			"../svg-cache/icons/search.svg",
		);
	});

	it("adds ./ so a sibling is not read as a package name", () => {
		expect(toModuleSpecifier("svg-cache/foo.svg")).toBe("./svg-cache/foo.svg");
	});

	it("leaves an already-relative specifier alone", () => {
		expect(toModuleSpecifier("../a/b.svg")).toBe("../a/b.svg");
	});
});

describe("predicates", () => {
	it("recognizes .svg specifiers case-insensitively", () => {
		expect(isSvgSpecifier("./a.svg")).toBe(true);
		expect(isSvgSpecifier("./a.SVG")).toBe(true);
		expect(isSvgSpecifier("./a.svgz")).toBe(false);
		expect(isSvgSpecifier("@rbxts/svg")).toBe(false);
	});

	it("treats only ./ and ../ as relative", () => {
		expect(isRelativeSpecifier("./a.svg")).toBe(true);
		expect(isRelativeSpecifier("../a.svg")).toBe(true);
		expect(isRelativeSpecifier("@/a.svg")).toBe(false);
		expect(isRelativeSpecifier("assets/a.svg")).toBe(false);
	});

	it("does not mistake a sibling directory for a nested one", () => {
		expect(isInside(resolve("/a/bcd"), resolve("/a/bc"))).toBe(false);
		expect(isInside(resolve("/a/bc/d"), resolve("/a/bc"))).toBe(true);
	});
});

describe("mapSpecifier", () => {
	const importer = join(ROOT, "ui/Toolbar.tsx");
	const source = join(ROOT, "icons/search.svg");

	it("rewrites a relative .svg import", () => {
		const result = mapSpecifier(importer, "../icons/search.svg", CONFIG, bothExist(source));
		expect(result).toEqual({
			kind: "rewrite",
			specifier: "../svg-cache/icons/search.svg",
			sourcePath: source,
			modulePath: join(OUT, "icons/search.svg.ts"),
		});
	});

	it("leaves non-SVG specifiers alone without touching the filesystem", () => {
		const explode: FileSystemHost = {
			fileExists: () => {
				throw new Error("should not be consulted");
			},
		};
		expect(mapSpecifier(importer, "./normal-module", CONFIG, explode)).toEqual({ kind: "skip" });
		expect(mapSpecifier(importer, "@rbxts/svg", CONFIG, explode)).toEqual({ kind: "skip" });
	});

	it("leaves an explicit generated-module specifier alone", () => {
		// Still valid to write by hand, and rewriting it would look for a `.svg`
		// source inside the cache that never existed.
		const result = mapSpecifier(importer, "../svg-cache/icons/search.svg", CONFIG, bothExist(source));
		expect(result).toEqual({ kind: "skip" });
	});

	it("reports a non-relative specifier rather than guessing an alias", () => {
		const result = mapSpecifier(importer, "@/icons/search.svg", CONFIG, bothExist(source));
		expect(result.kind).toBe("error");
		expect(result.kind === "error" && result.message).toContain("only relative .svg imports");
	});

	it("reports a source that escapes the root", () => {
		const result = mapSpecifier(importer, "../../../outside.svg", CONFIG, fakeHost());
		expect(result.kind).toBe("error");
		expect(result.kind === "error" && result.message).toContain("outside the source root");
	});

	it("reports a missing .svg, naming the specifier and the importer", () => {
		const result = mapSpecifier(importer, "../icons/nope.svg", CONFIG, fakeHost());
		expect(result.kind).toBe("error");
		if (result.kind !== "error") return;
		expect(result.message).toContain("cannot resolve SVG import");
		expect(result.message).toContain("../icons/nope.svg");
		expect(result.message).toContain("Toolbar.tsx");
	});

	it("reports a missing generated module, naming the build command", () => {
		const result = mapSpecifier(importer, "../icons/search.svg", CONFIG, fakeHost(source));
		expect(result.kind).toBe("error");
		if (result.kind !== "error") return;
		expect(result.message).toContain("generated asset module is missing");
		expect(result.message).toContain("search.svg.ts");
		expect(result.message).toContain("rbxts-svg build");
	});

	it("follows a custom generated directory", () => {
		const custom = { rootDir: ROOT, outDir: resolve("/project/src/generated") };
		const host = fakeHost(source, generatedModulePath(source, custom));
		const result = mapSpecifier(importer, "../icons/search.svg", custom, host);
		expect(result.kind === "rewrite" && result.specifier).toBe(
			"../generated/icons/search.svg",
		);
	});
});

describe("shouldTransformFile", () => {
	it("skips generated modules so they are never remapped again", () => {
		expect(shouldTransformFile(join(OUT, "icons/search.svg.ts"), CONFIG)).toBe(false);
	});

	it("transforms ordinary source files", () => {
		expect(shouldTransformFile(join(ROOT, "ui/Toolbar.tsx"), CONFIG)).toBe(true);
	});
});
