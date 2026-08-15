/**
 * The watch chain, with both real watchers running.
 *
 * ```text
 * edit search.svg
 *   ↓  rbxts-svg watch
 * rewrite svg-cache/icons/search.svg.ts
 *   ↓  rbxtsc -w notices a changed TypeScript input
 * out/svg-cache/icons/search.svg.luau
 * ```
 *
 * This is the reason the generated modules exist at all, and it is the one
 * property no unit test can stand in for: it depends on the `.svg` being
 * represented inside TypeScript's own dependency graph. A transformer that read
 * `.svg` files itself would pass every other test in this repository and fail
 * here, silently, by never rebuilding anything.
 *
 * Nothing here sleeps for a fixed interval. Each step waits on the filesystem
 * condition it actually cares about, with a bounded timeout, so a slow machine
 * is slow rather than flaky.
 */

import { readFileSync, statSync, writeFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";

import {
	BELL_SVG,
	Fixture,
	RBXTSC_CLI,
	RBXTS_SVG_CLI,
	SEARCH_SVG,
	type Watcher,
	readIfExists,
	waitFor,
} from "./fixture.js";

let fixture: Fixture | undefined;
const watchers: Watcher[] = [];

afterEach(() => {
	while (watchers.length > 0) {
		watchers.pop()?.stop();
	}
	fixture?.dispose();
	fixture = undefined;
});

describe("editing only the .svg", () => {
	it("flows through both watchers to the emitted Luau asset", async () => {
		const project = new Fixture();
		fixture = project;

		project.write("src/icons/search.svg", readFileSync(SEARCH_SVG, "utf8"));
		const importerPath = project.write(
			"src/main.ts",
			`import Search from "./icons/search.svg";\n\nexport = Search;\n`,
		);

		const generatedPath = project.path("src/svg-cache/icons/search.svg.ts");
		const emittedAssetPath = project.path("out/svg-cache/icons/search.svg.luau");
		const emittedImporterPath = project.path("out/main.luau");

		const svgWatcher = project.spawnWatcher(RBXTS_SVG_CLI, ["watch"]);
		watchers.push(svgWatcher);
		// rbxtsc must not start before the generated module exists, or its first
		// pass legitimately fails — which is the ordering a real `pnpm watch`
		// script has to respect too.
		await waitFor("the generated module to appear", () => readIfExists(generatedPath) !== undefined, {
			describeFailure: () => svgWatcher.output,
		});

		const tsWatcher = project.spawnWatcher(RBXTSC_CLI, ["-w"]);
		watchers.push(tsWatcher);
		await waitFor("the first rbxtsc pass to emit", () => readIfExists(emittedAssetPath) !== undefined, {
			describeFailure: () => tsWatcher.output,
		});
		await waitFor("the importer to be emitted", () => readIfExists(emittedImporterPath) !== undefined, {
			describeFailure: () => tsWatcher.output,
		});

		const before = {
			importer: readFileSync(importerPath, "utf8"),
			importerMtime: statSync(importerPath).mtimeMs,
			generated: readFileSync(generatedPath, "utf8"),
			emittedAsset: readFileSync(emittedAssetPath, "utf8"),
			emittedImporter: readFileSync(emittedImporterPath, "utf8"),
		};
		expect(before.emittedImporter).toContain(`"svg-cache", "icons", "search.svg"`);

		// The only edit in the entire test.
		writeFileSync(project.path("src/icons/search.svg"), readFileSync(BELL_SVG, "utf8"), "utf8");

		await waitFor(
			"the generated module to be regenerated",
			() => readIfExists(generatedPath) !== before.generated,
			{ describeFailure: () => `rbxts-svg watch said:\n${svgWatcher.output}` },
		);
		await waitFor(
			"rbxtsc to re-emit the asset",
			() => readIfExists(emittedAssetPath) !== before.emittedAsset,
			{ describeFailure: () => `rbxtsc -w said:\n${tsWatcher.output}` },
		);

		// The importing source never changed, was never rewritten, and still
		// points at the same module path — no specifier churn, no rebuild storm.
		expect(readFileSync(importerPath, "utf8")).toBe(before.importer);
		expect(statSync(importerPath).mtimeMs).toBe(before.importerMtime);
		expect(readFileSync(emittedImporterPath, "utf8")).toBe(before.emittedImporter);
		expect(readFileSync(emittedImporterPath, "utf8")).toContain(
			`"svg-cache", "icons", "search.svg"`,
		);

		// And the new picture really did reach the runtime asset.
		expect(readFileSync(generatedPath, "utf8")).toContain("// source: icons/search.svg");
		expect(readFileSync(emittedAssetPath, "utf8")).toContain("createAssetFromBase64");
	});
});
