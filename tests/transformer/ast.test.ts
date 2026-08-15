/**
 * The AST rewrite, checked by printing.
 *
 * These run the real transformer factory through `ts.transform` and print the
 * result, because printed text is the same thing roblox-ts feeds back into its
 * language service — asserting on node identity would pass while the emitted
 * source said something else entirely.
 *
 * What matters most here is what is *not* touched: a transformer that rewrites
 * any string ending in `.svg` would quietly corrupt unrelated code.
 */

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import ts from "typescript";

import { DEFAULT_OUT_DIR, generatedModulePath } from "@rbxts/svg-compiler/paths";
import { createSvgTransformer } from "@rbxts/svg-transformer";

let projectDir: string;
let rootDir: string;
let outDir: string;

beforeEach(() => {
	projectDir = mkdtempSync(join(tmpdir(), "rbxts-svg-ast-"));
	rootDir = join(projectDir, "src");
	outDir = join(rootDir, DEFAULT_OUT_DIR);
	mkdirSync(rootDir, { recursive: true });
});

afterEach(() => {
	rmSync(projectDir, { recursive: true, force: true });
});

/** Creates a `.svg` and the generated module the transformer expects beside it. */
function addAsset(relativePath: string): void {
	const sourcePath = join(rootDir, relativePath);
	mkdirSync(dirname(sourcePath), { recursive: true });
	writeFileSync(sourcePath, '<svg viewBox="0 0 1 1"/>', "utf8");

	const modulePath = generatedModulePath(sourcePath, { rootDir, outDir });
	mkdirSync(dirname(modulePath), { recursive: true });
	writeFileSync(modulePath, "export default 0;\n", "utf8");
}

interface TransformOutcome {
	readonly text: string;
	readonly diagnostics: readonly string[];
}

/** Transforms one file's source and returns the printed result. */
function transform(fileName: string, source: string): TransformOutcome {
	const sourceFile = ts.createSourceFile(
		join(rootDir, fileName),
		source,
		ts.ScriptTarget.ESNext,
		/* setParentNodes */ true,
		fileName.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
	);

	const result = ts.transform(sourceFile, [
		createSvgTransformer({ tsApi: ts, config: { rootDir, outDir } }),
	]);
	const printed = ts.createPrinter().printFile(result.transformed[0]!);
	const diagnostics = (result.diagnostics ?? []).map((diagnostic) =>
		ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"),
	);
	result.dispose();
	return { text: printed, diagnostics };
}

describe("import rewriting", () => {
	it("rewrites a default import of a sibling .svg", () => {
		addAsset("icon.svg");
		const { text, diagnostics } = transform("main.ts", `import Icon from "./icon.svg";\n`);
		expect(diagnostics).toEqual([]);
		expect(text).toContain(`import Icon from "./svg-cache/icon.svg"`);
	});

	it("rewrites from a nested importer without hardcoding depth", () => {
		addAsset("assets/icons/search.svg");
		const { text, diagnostics } = transform(
			"screens/settings/Settings.tsx",
			`import Search from "../../assets/icons/search.svg";\n`,
		);
		expect(diagnostics).toEqual([]);
		expect(text).toContain(`"../../svg-cache/assets/icons/search.svg"`);
	});

	it("rewrites a re-export", () => {
		addAsset("icon.svg");
		const { text, diagnostics } = transform(
			"index.ts",
			`export { default as Icon } from "./icon.svg";\n`,
		);
		expect(diagnostics).toEqual([]);
		expect(text).toContain(`export { default as Icon } from "./svg-cache/icon.svg"`);
	});

	it("rewrites a bare default re-export", () => {
		addAsset("icon.svg");
		const { text } = transform("index.ts", `export { default } from "./icon.svg";\n`);
		expect(text).toContain(`export { default } from "./svg-cache/icon.svg"`);
	});

	it("preserves a type-only import's modifier", () => {
		addAsset("icon.svg");
		const { text } = transform("main.ts", `import type Icon from "./icon.svg";\n`);
		expect(text).toContain(`import type Icon from "./svg-cache/icon.svg"`);
	});

	it("preserves a namespace import clause", () => {
		addAsset("icon.svg");
		const { text } = transform("main.ts", `import * as Icon from "./icon.svg";\n`);
		expect(text).toContain(`import * as Icon from "./svg-cache/icon.svg"`);
	});

	it("rewrites a side-effect-only import", () => {
		addAsset("icon.svg");
		const { text } = transform("main.ts", `import "./icon.svg";\n`);
		expect(text).toContain(`import "./svg-cache/icon.svg"`);
	});
});

describe("what it leaves alone", () => {
	it("does not touch ordinary imports", () => {
		const { text, diagnostics } = transform(
			"main.ts",
			`import { something } from "./normal-module";\n`,
		);
		expect(diagnostics).toEqual([]);
		expect(text).toContain(`from "./normal-module"`);
	});

	it("does not touch a string that merely ends in .svg", () => {
		const { text, diagnostics } = transform(
			"main.ts",
			`const value = "./icon.svg";\nconst other = { path: "../a/b.svg" };\n`,
		);
		expect(diagnostics).toEqual([]);
		expect(text).toContain(`"./icon.svg"`);
		expect(text).toContain(`"../a/b.svg"`);
		expect(text).not.toContain("svg-cache");
	});

	it("does not touch imports inside the generated directory", () => {
		addAsset("icon.svg");
		const outcome = transform(
			join(DEFAULT_OUT_DIR, "icon.svg.ts"),
			`import { unstable_internal } from "@rbxts/svg";\nimport X from "../icon.svg";\n`,
		);
		expect(outcome.diagnostics).toEqual([]);
		expect(outcome.text).toContain(`"../icon.svg"`);
		expect(outcome.text).not.toContain("svg-cache");
	});

	it("does not rewrite an already-generated specifier a second time", () => {
		addAsset("icons/search.svg");
		const { text, diagnostics } = transform(
			"main.ts",
			`import Search from "./svg-cache/icons/search.svg";\n`,
		);
		expect(diagnostics).toEqual([]);
		expect(text).toContain(`"./svg-cache/icons/search.svg"`);
		expect(text).not.toContain("svg-cache/svg-cache");
	});
});

describe("diagnostics", () => {
	it("reports a missing .svg and leaves the specifier in place", () => {
		const { text, diagnostics } = transform("main.ts", `import Icon from "./missing.svg";\n`);
		expect(diagnostics).toHaveLength(1);
		expect(diagnostics[0]).toContain("cannot resolve SVG import");
		expect(text).toContain(`"./missing.svg"`);
	});

	it("reports a missing generated module", () => {
		const sourcePath = join(rootDir, "icon.svg");
		writeFileSync(sourcePath, '<svg viewBox="0 0 1 1"/>', "utf8");
		const { diagnostics } = transform("main.ts", `import Icon from "./icon.svg";\n`);
		expect(diagnostics).toHaveLength(1);
		expect(diagnostics[0]).toContain("generated asset module is missing");
	});

	it("reports a non-relative .svg specifier", () => {
		const { diagnostics } = transform("main.ts", `import Icon from "@/icons/search.svg";\n`);
		expect(diagnostics).toHaveLength(1);
		expect(diagnostics[0]).toContain("only relative .svg imports are supported");
	});

	it("reports a dynamic import of a .svg", () => {
		addAsset("icon.svg");
		const { diagnostics } = transform(
			"main.ts",
			`export async function load() { return import("./icon.svg"); }\n`,
		);
		expect(diagnostics).toHaveLength(1);
		expect(diagnostics[0]).toContain("only static imports of .svg files are supported");
	});

	it("reports import = require() of a .svg", () => {
		addAsset("icon.svg");
		const { diagnostics } = transform("main.ts", `import Icon = require("./icon.svg");\n`);
		expect(diagnostics).toHaveLength(1);
		expect(diagnostics[0]).toContain("only static imports of .svg files are supported");
	});

	it("anchors the diagnostic on the specifier, not the whole statement", () => {
		const sourceFile = ts.createSourceFile(
			join(rootDir, "main.ts"),
			`import Icon from "./missing.svg";\n`,
			ts.ScriptTarget.ESNext,
			true,
		);
		const result = ts.transform(sourceFile, [
			createSvgTransformer({ tsApi: ts, config: { rootDir, outDir } }),
		]);
		const diagnostic = result.diagnostics?.[0];
		result.dispose();

		expect(diagnostic).toBeDefined();
		expect(sourceFile.text.slice(diagnostic!.start, diagnostic!.start + diagnostic!.length)).toBe(
			`"./missing.svg"`,
		);
	});

	it("leaves a dynamic import of a non-SVG module alone", () => {
		const { diagnostics } = transform(
			"main.ts",
			`export async function load() { return import("./other"); }\n`,
		);
		expect(diagnostics).toEqual([]);
	});
});
