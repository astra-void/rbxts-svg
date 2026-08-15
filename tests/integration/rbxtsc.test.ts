/**
 * Direct `.svg` imports, through the real compilers.
 *
 * The assertion that matters in every test here is on *emitted Luau*. A
 * transformed AST that TypeScript accepts still tells you nothing about what
 * roblox-ts will resolve it to — roblox-ts prints each transformed file back to
 * text, re-parses it through a language service, and only then computes a
 * `require` path. So these run `rbxts-svg build` and `rbxtsc` as a user would
 * and read the `.luau` that comes out.
 */

import { existsSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";

import { BELL_SVG, Fixture, SEARCH_SVG, expectOk, stripAnsi } from "./fixture.js";

const fixtures: Fixture[] = [];

afterEach(() => {
	while (fixtures.length > 0) {
		fixtures.pop()?.dispose();
	}
});

function newFixture(...args: ConstructorParameters<typeof Fixture>): Fixture {
	const fixture = new Fixture(...args);
	fixtures.push(fixture);
	return fixture;
}

/** Copies one of the repository's Lucide fixtures into a project. */
function addIcon(fixture: Fixture, relativePath: string, from = SEARCH_SVG): void {
	fixture.write(relativePath, readFileSync(from, "utf8"));
}

describe("a simple project", () => {
	it("compiles a direct .svg import to a require of the generated module", () => {
		const fixture = newFixture();
		addIcon(fixture, "src/icons/search.svg");
		fixture.write("src/main.ts", `import Search from "./icons/search.svg";\n\nexport = Search;\n`);

		expectOk(fixture.buildSvgs(), "rbxts-svg build");
		expectOk(fixture.compile(), "rbxtsc");

		const emitted = readFileSync(fixture.path("out/main.luau"), "utf8");
		// The require walks to the generated module, not to `icons/search.svg`.
		expect(emitted).toContain(`"TS", "svg-cache", "icons", "search.svg"`);
		expect(emitted).not.toContain(`"TS", "icons", "search.svg"`);

		// And the generated module carries the compiled IR, not the SVG source.
		const asset = readFileSync(fixture.path("out/svg-cache/icons/search.svg.luau"), "utf8");
		expect(asset).toContain("createAssetFromBase64");
		expect(asset).toContain("-- ir-version: 2");
		expect(asset).toMatch(/createAssetFromBase64\("UlNWR/);
	});

	it("still accepts an explicit generated-module import", () => {
		const fixture = newFixture();
		addIcon(fixture, "src/icons/search.svg");
		fixture.write(
			"src/main.ts",
			`import Search from "./svg-cache/icons/search.svg";\n\nexport = Search;\n`,
		);

		expectOk(fixture.buildSvgs(), "rbxts-svg build");
		expectOk(fixture.compile(), "rbxtsc");

		expect(readFileSync(fixture.path("out/main.luau"), "utf8")).toContain(
			`"svg-cache", "icons", "search.svg"`,
		);
	});

	it("leaves non-SVG imports untouched", () => {
		const fixture = newFixture();
		addIcon(fixture, "src/icons/search.svg");
		fixture.write("src/helper.ts", `export const value = 1;\n`);
		fixture.write(
			"src/main.ts",
			`import Search from "./icons/search.svg";\nimport { value } from "./helper";\n\nexport = [Search, value];\n`,
		);

		expectOk(fixture.buildSvgs(), "rbxts-svg build");
		expectOk(fixture.compile(), "rbxtsc");

		const emitted = readFileSync(fixture.path("out/main.luau"), "utf8");
		expect(emitted).toContain(`"TS", "helper"`);
		expect(emitted).toContain(`"svg-cache", "icons", "search.svg"`);
	});

	it("compiles a .svg re-export", () => {
		const fixture = newFixture();
		addIcon(fixture, "src/icons/search.svg");
		fixture.write("src/index.ts", `export { default as Search } from "./icons/search.svg";\n`);

		expectOk(fixture.buildSvgs(), "rbxts-svg build");
		expectOk(fixture.compile(), "rbxtsc");

		// roblox-ts emits `index.ts` as `init.luau`.
		expect(readFileSync(fixture.path("out/init.luau"), "utf8")).toContain(
			`"svg-cache", "icons", "search.svg"`,
		);
	});
});

describe("a nested project", () => {
	it("resolves relative depth without hardcoded assumptions", () => {
		const fixture = newFixture();
		addIcon(fixture, "src/assets/icons/search.svg");
		fixture.write(
			"src/screens/settings/Settings.ts",
			`import Search from "../../assets/icons/search.svg";\n\nexport = Search;\n`,
		);

		expectOk(fixture.buildSvgs(), "rbxts-svg build");
		expectOk(fixture.compile(), "rbxtsc");

		const emitted = readFileSync(fixture.path("out/screens/settings/Settings.luau"), "utf8");
		expect(emitted).toContain(`"TS", "svg-cache", "assets", "icons", "search.svg"`);
		expect(emitted).not.toContain(`"TS", "assets", "icons", "search.svg"`);
	});
});

describe("a custom generated directory", () => {
	it("works when both sides are pointed at it", () => {
		const fixture = newFixture({ pluginConfig: { outDir: "src/generated" } });
		addIcon(fixture, "src/icons/search.svg");
		fixture.write("src/main.ts", `import Search from "./icons/search.svg";\n\nexport = Search;\n`);

		expectOk(fixture.buildSvgs("--out", "src/generated"), "rbxts-svg build --out");
		expectOk(fixture.compile(), "rbxtsc");

		expect(readFileSync(fixture.path("out/main.luau"), "utf8")).toContain(
			`"generated", "icons", "search.svg"`,
		);
	});

	it("fails loudly when only one side is pointed at it", () => {
		// The generator writes to the default cache; the transformer is told to
		// look somewhere else. This is exactly the drift the shared path module
		// exists to prevent, and it must be an error, not a broken require.
		const fixture = newFixture({ pluginConfig: { outDir: "src/generated" } });
		addIcon(fixture, "src/icons/search.svg");
		fixture.write("src/main.ts", `import Search from "./icons/search.svg";\n\nexport = Search;\n`);

		expectOk(fixture.buildSvgs(), "rbxts-svg build");
		const result = fixture.compile();

		expect(result.ok).toBe(false);
		expect(stripAnsi(result.output)).toContain("generated asset module is missing");
		expect(existsSync(fixture.path("out/main.luau"))).toBe(false);
	});
});

describe("diagnostics", () => {
	it("names the source and the expected module when the cache is unbuilt", () => {
		const fixture = newFixture();
		addIcon(fixture, "src/icons/search.svg");
		fixture.write("src/main.ts", `import Search from "./icons/search.svg";\n\nexport = Search;\n`);

		const result = fixture.compile();
		const output = stripAnsi(result.output);

		expect(result.ok).toBe(false);
		expect(output).toContain("generated asset module is missing");
		expect(output).toContain("src/icons/search.svg");
		expect(output).toContain("src/svg-cache/icons/search.svg.ts");
		expect(output).toContain("rbxts-svg build");
	});

	it("names the importing file when the .svg does not exist", () => {
		const fixture = newFixture();
		addIcon(fixture, "src/icons/search.svg");
		fixture.write(
			"src/ui/Toolbar.ts",
			`import Missing from "../icons/does-not-exist.svg";\n\nexport = Missing;\n`,
		);

		expectOk(fixture.buildSvgs(), "rbxts-svg build");
		const output = stripAnsi(fixture.compile().output);

		expect(output).toContain("cannot resolve SVG import");
		expect(output).toContain("../icons/does-not-exist.svg");
		expect(output).toContain("src/ui/Toolbar.ts");
	});

	it("rejects a non-relative .svg specifier instead of guessing", () => {
		const fixture = newFixture();
		addIcon(fixture, "src/icons/search.svg");
		fixture.write("src/main.ts", `import Search from "@/icons/search.svg";\n\nexport = Search;\n`);

		expectOk(fixture.buildSvgs(), "rbxts-svg build");
		const output = stripAnsi(fixture.compile().output);

		expect(output).toContain("only relative .svg imports are supported");
	});
});

describe("typing", () => {
	it("typechecks a direct .svg import as SvgAsset with no hand-written shim", () => {
		const fixture = newFixture();
		addIcon(fixture, "src/icons/search.svg");
		// `satisfies` is the assertion: if the ambient declaration typed this as
		// `any`, this would pass for the wrong reason, so the negative case
		// below pins it down.
		fixture.write(
			"src/main.ts",
			`import type { SvgAsset } from "@rbxts/svg";\n` +
				`import Search from "./icons/search.svg";\n\n` +
				`const asset: SvgAsset = Search;\n` +
				`export = asset;\n`,
		);

		expectOk(fixture.buildSvgs(), "rbxts-svg build");
		expectOk(fixture.compile(), "rbxtsc");
	});

	it("is not `any` — an SvgAsset does not satisfy an unrelated type", () => {
		const fixture = newFixture();
		addIcon(fixture, "src/icons/search.svg");
		fixture.write(
			"src/main.ts",
			`import Search from "./icons/search.svg";\n\n` +
				`const wrong: string = Search;\n` +
				`export = wrong;\n`,
		);

		expectOk(fixture.buildSvgs(), "rbxts-svg build");
		const result = fixture.compile();

		expect(result.ok).toBe(false);
		expect(stripAnsi(result.output)).toContain("not assignable to type 'string'");
	});

	it("emits no runtime module for the ambient declaration", () => {
		const fixture = newFixture();
		addIcon(fixture, "src/icons/search.svg");
		fixture.write("src/main.ts", `import Search from "./icons/search.svg";\n\nexport = Search;\n`);

		expectOk(fixture.buildSvgs(), "rbxts-svg build");
		expectOk(fixture.compile(), "rbxtsc");

		expect(existsSync(fixture.path("src/svg-cache/svg-modules.d.ts"))).toBe(true);
		expect(existsSync(fixture.path("out/svg-cache/svg-modules.d.ts"))).toBe(false);
		expect(existsSync(fixture.path("out/svg-cache/svg-modules.luau"))).toBe(false);
	});
});

describe("editing only the .svg", () => {
	// This is the whole architecture in one test: the source TypeScript names a
	// stable path, so changing the picture changes the generated module and the
	// emitted asset while the importing file — and its emitted require — stay
	// exactly as they were.
	it("changes the generated module and the emitted asset, and nothing else", () => {
		const fixture = newFixture();
		addIcon(fixture, "src/icons/search.svg");
		const importer = fixture.write(
			"src/main.ts",
			`import Search from "./icons/search.svg";\n\nexport = Search;\n`,
		);

		expectOk(fixture.buildSvgs(), "rbxts-svg build");
		expectOk(fixture.compile(), "rbxtsc");

		const before = {
			importer: readFileSync(importer, "utf8"),
			importerMtime: statSync(importer).mtimeMs,
			emittedImporter: readFileSync(fixture.path("out/main.luau"), "utf8"),
			generated: readFileSync(fixture.path("src/svg-cache/icons/search.svg.ts"), "utf8"),
			asset: readFileSync(fixture.path("out/svg-cache/icons/search.svg.luau"), "utf8"),
		};

		// Only the picture changes.
		writeFileSync(fixture.path("src/icons/search.svg"), readFileSync(BELL_SVG, "utf8"), "utf8");
		expectOk(fixture.buildSvgs(), "rbxts-svg build (again)");
		expectOk(fixture.compile(), "rbxtsc (again)");

		expect(readFileSync(importer, "utf8")).toBe(before.importer);
		expect(statSync(importer).mtimeMs).toBe(before.importerMtime);
		expect(readFileSync(fixture.path("out/main.luau"), "utf8")).toBe(before.emittedImporter);

		const generatedAfter = readFileSync(fixture.path("src/svg-cache/icons/search.svg.ts"), "utf8");
		expect(generatedAfter).not.toBe(before.generated);
		expect(readFileSync(fixture.path("out/svg-cache/icons/search.svg.luau"), "utf8")).not.toBe(
			before.asset,
		);
	});

	it("does not rewrite a generated module whose content is unchanged", () => {
		const fixture = newFixture();
		addIcon(fixture, "src/icons/search.svg");
		expectOk(fixture.buildSvgs(), "rbxts-svg build");

		const generated = fixture.path("src/svg-cache/icons/search.svg.ts");
		const mtime = statSync(generated).mtimeMs;

		expectOk(fixture.buildSvgs(), "rbxts-svg build (again)");
		expect(statSync(generated).mtimeMs).toBe(mtime);
	});
});

describe("raw .svg files in the output tree", () => {
	it("are copied by roblox-ts, but nothing requires them", () => {
		// roblox-ts copies every non-compilable file under rootDir into `out`.
		// There is no supported way to exclude one, and Rojo ignores unknown
		// extensions, so the copy is inert — but it should be a documented fact
		// rather than a surprise, and the require must not point at it.
		const fixture = newFixture();
		addIcon(fixture, "src/icons/search.svg");
        fixture.write("src/main.ts", `import Search from "./icons/search.svg";\n\nexport = Search;\n`);

		expectOk(fixture.buildSvgs(), "rbxts-svg build");
		expectOk(fixture.compile(), "rbxtsc");

		expect(existsSync(fixture.path("out/icons/search.svg"))).toBe(true);
		expect(existsSync(fixture.path("out/icons/search.svg.luau"))).toBe(false);
		expect(readFileSync(fixture.path("out/main.luau"), "utf8")).toContain("svg-cache");
	});
});
