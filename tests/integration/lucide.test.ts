/**
 * The Lucide packages, and the generator that produces them.
 *
 * Two thousand icons cannot be reviewed by reading them, so the properties that
 * would otherwise be checked by eye are checked here instead:
 *
 * - every upstream icon is accounted for, exactly once, in both packages;
 * - the two packages' icon data is identical, byte for byte;
 * - regenerating changes nothing, and an icon removed upstream is removed here;
 * - every icon is a tintable alpha mask, which is the assumption the whole
 *   colour fast path rests on;
 * - a real roblox-ts consumer compiles, and the emitted Luau says which import
 *   style loads what.
 *
 * The generator is driven through its module API rather than its CLI, so a
 * failure points at a function instead of at a parsed stdout line.
 */

import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";

import {
	OWNED_DIR,
	assignExportNames,
	generateLucide,
	lucideTargets,
	manifestPath,
	toExportName,
	type LucideManifest,
	type LucideTarget,
	type Upstream,
} from "../../tools/lucide/dist/index.js";
import { renderSvgIr } from "@rbxts/svg-compiler";

import { REPO_ROOT } from "./fixture.js";

const REACT = join(REPO_ROOT, "packages/lucide-react");
const VIDE = join(REPO_ROOT, "packages/lucide-vide");

/** The committed manifest — what `pnpm generate:lucide` last produced. */
function committedManifest(): LucideManifest {
	return JSON.parse(readFileSync(manifestPath(REPO_ROOT), "utf8")) as LucideManifest;
}

/** Pulls the base64 IR back out of a generated icon module. */
const EMBEDDED_IR = /"([A-Za-z0-9+/=]{40,})"/;

function generatedIconSource(packageDir: string, sourceName: string): string {
	return readFileSync(join(packageDir, OWNED_DIR, "icons", `${sourceName}.tsx`), "utf8");
}

function listGeneratedIcons(packageDir: string): string[] {
	return readdirSync(join(packageDir, OWNED_DIR, "icons"))
		.filter((name) => name.endsWith(".tsx"))
		.map((name) => name.slice(0, -".tsx".length))
		.sort();
}

describe("the upstream icon set", () => {
	it("is pinned to an exact version", () => {
		// A caret would let an upstream release change the published packages
		// without anyone deciding to. An icon set bump should be a commit.
		const generator = JSON.parse(
			readFileSync(join(REPO_ROOT, "tools/lucide/package.json"), "utf8"),
		) as { dependencies: Record<string, string> };
		expect(generator.dependencies["lucide-static"]).toMatch(/^\d+\.\d+\.\d+$/);

		const manifest = committedManifest();
		expect(manifest.version).toBe(generator.dependencies["lucide-static"]);
	});

	it("is fully accounted for: one manifest entry and one module per SVG", () => {
		const manifest = committedManifest();
		const upstreamIcons = readdirSync(
			join(REPO_ROOT, "tools/lucide/node_modules/lucide-static/icons"),
		)
			.filter((name) => name.endsWith(".svg"))
			.map((name) => name.slice(0, -".svg".length))
			.sort();

		const manifestNames = manifest.icons.map((icon) => icon.sourceName);
		expect(manifestNames).toEqual(upstreamIcons);
		expect(listGeneratedIcons(REACT)).toEqual(upstreamIcons);
		expect(listGeneratedIcons(VIDE)).toEqual(upstreamIcons);

		// No file maps twice, and the counts agree from every direction.
		expect(new Set(manifestNames).size).toBe(manifestNames.length);
		expect(manifest.canonicalCount + manifest.aliasCount).toBe(manifest.icons.length);
	});

	it("compiles every canonical icon, with nothing skipped", () => {
		const manifest = committedManifest();
		const canonical = manifest.icons.filter((icon) => icon.aliasOf === undefined);
		expect(canonical.length).toBe(manifest.canonicalCount);
		for (const icon of canonical) {
			expect(icon.hash).toMatch(/^[0-9a-f]{64}$/);
			expect(icon.byteLength).toBeGreaterThan(0);
		}

		// `allowUnsupported` downgrades an unsupported construct to a warning
		// and produces an asset that is quietly wrong. A generated package is a
		// compatibility claim, so the generator must never reach for it.
		const generatorSources = readdirSync(join(REPO_ROOT, "tools/lucide/src"))
			.map((name) => readFileSync(join(REPO_ROOT, "tools/lucide/src", name), "utf8"))
			.join("\n");
		expect(generatorSources).not.toMatch(/allowUnsupported\s*:/);
	});
});

describe("naming", () => {
	it("converts the real names in the set", () => {
		// Drawn from the pinned set rather than invented: the awkward cases are
		// the ones upstream actually ships.
		expect(toExportName("search")).toBe("Search");
		expect(toExportName("chevron-down")).toBe("ChevronDown");
		expect(toExportName("circle-alert")).toBe("CircleAlert");
		expect(toExportName("a-arrow-down")).toBe("AArrowDown");
		expect(toExportName("a-large-small")).toBe("ALargeSmall");
		expect(toExportName("bar-chart-2")).toBe("BarChart2");
		expect(toExportName("arrow-down-0-1")).toBe("ArrowDown01");
		expect(toExportName("axis-3-d")).toBe("Axis3D");
		expect(toExportName("axis-3d")).toBe("Axis3d");
		expect(toExportName("qr-code")).toBe("QrCode");
		expect(toExportName("package")).toBe("Package");
	});

	it("refuses a name it cannot convert honestly", () => {
		// `3d-view` would become `3dView`, which is not an identifier at all.
		expect(() => toExportName("3d-view")).toThrow(/lower-kebab-case/);
		expect(() => toExportName("Search")).toThrow(/lower-kebab-case/);
		expect(() => toExportName("arrow_down")).toThrow(/lower-kebab-case/);
	});

	it("fails on a collision between two different icons", () => {
		// The shape upstream could plausibly produce: a segment split moved, so
		// `bar-chart-2` and `bar-chart2` both spell `BarChart2` while being two
		// different drawings. Picking a winner would make one of them vanish.
		expect(() =>
			assignExportNames([
				{ sourceName: "bar-chart-2", aliasOf: undefined },
				{ sourceName: "bar-chart2", aliasOf: undefined },
			]),
		).toThrow(/name collision/);

		// An alias colliding with an icon that is *not* its target is equally
		// fatal — it is still two icons wanting one name.
		expect(() =>
			assignExportNames([
				{ sourceName: "bar-chart-2", aliasOf: undefined },
				{ sourceName: "chart-column", aliasOf: undefined },
				{ sourceName: "bar-chart2", aliasOf: "chart-column" },
			]),
		).toThrow(/name collision/);
	});

	it("subsumes an alias that collides with its own target", () => {
		// Upstream renamed `arrow-down-01` to `arrow-down-0-1`; both spell
		// `ArrowDown01` and both are the same icon, so the name is exported
		// once and the alias module still exists.
		const assigned = assignExportNames([
			{ sourceName: "arrow-down-0-1", aliasOf: undefined },
			{ sourceName: "arrow-down-01", aliasOf: "arrow-down-0-1" },
		]);
		expect(assigned.map((name) => name.subsumed)).toEqual([false, true]);
	});

	it("agrees with the committed manifest about every name", () => {
		for (const icon of committedManifest().icons) {
			expect(icon.exportName).toBe(toExportName(icon.sourceName));
		}
	});
});

describe("the two packages", () => {
	it("have identical icon data for every icon", () => {
		// The hard requirement: the framework wrapper may differ, the vector
		// data must not. Checked over the whole set, not a sample.
		const manifest = committedManifest();
		const mismatched: string[] = [];
		for (const icon of manifest.icons) {
			const react = generatedIconSource(REACT, icon.sourceName);
			const vide = generatedIconSource(VIDE, icon.sourceName);
			if (react !== vide) {
				mismatched.push(icon.sourceName);
			}
		}
		expect(mismatched).toEqual([]);
	});

	it("embed the manifest's hash, which is the runtime cache identity", () => {
		const manifest = committedManifest();
		for (const icon of manifest.icons) {
			if (icon.aliasOf !== undefined) {
				continue;
			}
			const source = generatedIconSource(REACT, icon.sourceName);
			// The hash is passed to `createAssetFromBase64`, not merely
			// recorded: it is what makes two independently generated copies of
			// one icon resolve to one cached raster.
			expect(source).toContain(`"${icon.hash}"`);
			expect(source).toContain("unstable_internal.createAssetFromBase64(");
		}
	});

	it("ship compiled IR, never SVG XML", () => {
		const manifest = committedManifest();
		for (const packageDir of [REACT, VIDE]) {
			for (const icon of manifest.icons.slice(0, 200)) {
				const source = generatedIconSource(packageDir, icon.sourceName);
				expect(source).not.toContain("<svg");
				expect(source).not.toContain("viewBox=");
				expect(source).not.toContain("stroke-linecap");
			}
		}
	});

	it("give an alias the canonical module rather than a second copy", () => {
		const manifest = committedManifest();
		const alias = manifest.icons.find((icon) => icon.sourceName === "alert-circle");
		expect(alias?.aliasOf).toBe("circle-alert");

		const source = generatedIconSource(REACT, "alert-circle");
		expect(source).toContain('from "./circle-alert"');
		// No embedded artwork at all: an alias is a name, not a copy.
		expect(EMBEDDED_IR.test(source)).toBe(false);

		// And it still carries the canonical icon's identity in the manifest,
		// so "how many bytes is this set" stays answerable.
		const canonical = manifest.icons.find((icon) => icon.sourceName === "circle-alert");
		expect(alias?.hash).toBe(canonical?.hash);
	});

	it("do not depend on each other, or on the other framework", () => {
		const react = JSON.parse(readFileSync(join(REACT, "package.json"), "utf8")) as Manifest;
		const vide = JSON.parse(readFileSync(join(VIDE, "package.json"), "utf8")) as Manifest;

		expect(dependencyNames(react)).toEqual(
			expect.arrayContaining(["@rbxts/svg", "@rbxts/svg-react", "@rbxts/react"]),
		);
		for (const forbidden of ["@rbxts/vide", "@rbxts/svg-vide", "@rbxts/lucide-vide"]) {
			expect(dependencyNames(react)).not.toContain(forbidden);
		}

		expect(dependencyNames(vide)).toEqual(
			expect.arrayContaining(["@rbxts/svg", "@rbxts/svg-vide", "@rbxts/vide"]),
		);
		for (const forbidden of ["@rbxts/react", "@rbxts/svg-react", "@rbxts/lucide-react"]) {
			expect(dependencyNames(vide)).not.toContain(forbidden);
		}

		// And the arrow never points back into the core.
		const core = JSON.parse(
			readFileSync(join(REPO_ROOT, "packages/svg/package.json"), "utf8"),
		) as Manifest;
		expect(dependencyNames(core).filter((name) => name.includes("lucide"))).toEqual([]);
	});

	it("need neither Rust nor upstream Lucide nor the SVG transformer at runtime", () => {
		for (const packageDir of [REACT, VIDE]) {
			const manifest = JSON.parse(
				readFileSync(join(packageDir, "package.json"), "utf8"),
			) as Manifest;
			// A consumer installs a package of precompiled assets. Nothing in
			// it should drag in a native compiler, the upstream icon set, or a
			// transformer whose whole job is rewriting `.svg` imports the
			// consumer does not have.
			for (const forbidden of [
				"lucide-static",
				"@rbxts/svg-compiler",
				"@rbxts/svg-native",
				"@rbxts/svg-transformer",
			]) {
				expect(Object.keys(manifest.dependencies ?? {})).not.toContain(forbidden);
				expect(Object.keys(manifest.peerDependencies ?? {})).not.toContain(forbidden);
			}
			// No runtime dependencies at all, in fact: everything is a peer.
			expect(manifest.dependencies ?? {}).toEqual({});
		}
	});

	it("redistribute upstream's licence", () => {
		const upstreamLicense = readFileSync(
			join(REPO_ROOT, "tools/lucide/node_modules/lucide-static/LICENSE"),
			"utf8",
		);
		for (const packageDir of [REACT, VIDE]) {
			expect(readFileSync(join(packageDir, "LICENSE-lucide"), "utf8")).toBe(upstreamLicense);
			const manifest = JSON.parse(
				readFileSync(join(packageDir, "package.json"), "utf8"),
			) as Manifest & { license: string; files: string[] };
			// The wrapper is MIT; the artwork is upstream's ISC. Both travel.
			expect(manifest.license).toBe("MIT AND ISC");
			expect(manifest.files).toContain("LICENSE");
			expect(manifest.files).toContain("LICENSE-lucide");
		}
	});
});

describe("compatibility across the whole set", () => {
	it("is 100% tintable", () => {
		// The claim the colour fast path rests on: every Lucide icon is a
		// monochrome `currentColor` asset, so it rasterizes once and is
		// recoloured by `ImageColor3` for free. Measured, not assumed — if
		// upstream ever ships a full-colour icon this is where it surfaces.
		const result = generateLucide({ repoRoot: REPO_ROOT, check: true });
		expect(result.stats.notTintable).toEqual([]);
		expect(result.stats.tintable).toBe(result.stats.compiled);
		expect(result.stats.failures).toBe(0);
	});

	it("uses IR version 2 and one view box throughout", () => {
		const result = generateLucide({ repoRoot: REPO_ROOT, check: true });
		// A Lucide bump that forced an IR change would mean a generic SVG
		// problem, not a Lucide one. Nothing here should move it.
		expect(result.stats.irVersions).toEqual([2]);
		expect(result.stats.viewBoxes).toEqual(["0 0 24 24"]);
		expect(result.stats.preserveAspectRatios).toEqual(["xMidYMid meet"]);
	});

	it("rasterizes every icon at 24×24 to a non-empty alpha mask", () => {
		// The full-set smoke test: decode the bytes the package actually ships
		// and draw them. Cheap enough (under a second for the whole set) that
		// there is no reason to sample.
		const manifest = committedManifest();
		const blank: string[] = [];
		for (const icon of manifest.icons) {
			if (icon.aliasOf !== undefined) {
				continue;
			}
			const embedded = EMBEDDED_IR.exec(generatedIconSource(REACT, icon.sourceName));
			expect(embedded).not.toBeNull();
			const image = renderSvgIr(Buffer.from(embedded![1]!, "base64"), 24, 24, {
				alphaMask: true,
			});
			let coverage = 0;
			for (let index = 3; index < image.pixels.length; index += 4) {
				coverage += image.pixels[index]!;
			}
			if (coverage === 0) {
				blank.push(icon.sourceName);
			}
		}
		expect(blank).toEqual([]);
	});

	it("reports the duplicates it finds rather than hiding them", () => {
		// `clock` and `clock-4` are the same drawing under two canonical names.
		// Both get modules; both compile to one hash, so at runtime they are
		// one cache entry. Worth knowing, not worth "fixing".
		const result = generateLucide({ repoRoot: REPO_ROOT, check: true });
		expect(result.stats.uniqueHashes).toBe(result.stats.compiled - 1);
		expect(result.stats.duplicateHashGroups).toEqual([["clock", "clock-4"]]);
	});
});

describe("the generator", () => {
	it("compiles each SVG exactly once for both packages", () => {
		const result = generateLucide({ repoRoot: REPO_ROOT, check: true });
		// One compile pass, whose results are rendered into both trees. The
		// count is the canonical set, not the canonical set twice and not the
		// full file list — aliases are names, not artwork.
		expect(result.compiled.length).toBe(result.manifest.canonicalCount);
		expect(result.targets.length).toBe(2);

		// And the rendered tree is one object, written twice: identical file
		// lists, identical contents. Byte parity by construction rather than by
		// comparison.
		expect(result.files.length).toBe(result.manifest.icons.length + 1);
	});

	it("is deterministic: the committed output is what it produces", () => {
		// Run twice, in memory, and compare against disk. No timestamps, no
		// absolute paths, no filesystem ordering — if any of those leaked in,
		// this is where it shows.
		const first = generateLucide({ repoRoot: REPO_ROOT, check: true });
		const second = generateLucide({ repoRoot: REPO_ROOT, check: true });
		expect(first.clean).toBe(true);
		expect(second.clean).toBe(true);
		expect(second.files).toEqual(first.files);
		expect(second.manifest).toEqual(first.manifest);
	});

	it("removes an icon that upstream no longer ships", () => {
		// The whole pipeline over a synthetic two-icon set, then over the same
		// set with one icon gone. Nothing else can prove that an upstream
		// removal reaches both packages — and a package that accumulates icons
		// forever would keep exporting names upstream has dropped.
		const workspace = scratch();
		const upstream = syntheticUpstream(workspace, ["alpha", "beta"]);
		const targets = syntheticTargets(workspace);
		const manifestFile = join(workspace, "manifest.json");

		generateLucide({ repoRoot: REPO_ROOT, upstream, targets, manifestFile });
		for (const target of targets) {
			expect(listGeneratedIcons(target.packageDir)).toEqual(["alpha", "beta"]);
		}
		const alphaBefore = generatedIconSource(targets[0]!.packageDir, "alpha");

		rmSync(join(upstream.iconsDir, "beta.svg"));
		writeFileSync(
			join(upstream.root, "icon-nodes.json"),
			JSON.stringify({ alpha: [] }),
			"utf8",
		);

		const after = generateLucide({ repoRoot: REPO_ROOT, upstream, targets, manifestFile });
		for (const [target, report] of after.writes) {
			expect(listGeneratedIcons(target.packageDir), target.packageName).toEqual(["alpha"]);
			expect(report.removed).toContain("src/generated/icons/beta.tsx");
		}
		// The surviving icon is untouched, not rewritten.
		expect(generatedIconSource(targets[0]!.packageDir, "alpha")).toBe(alphaBefore);
		const manifest = JSON.parse(readFileSync(manifestFile, "utf8")) as LucideManifest;
		expect(manifest.icons.map((icon) => icon.sourceName)).toEqual(["alpha"]);
		expect(readFileSync(join(targets[0]!.packageDir, OWNED_DIR, "index.ts"), "utf8")).not.toContain(
			"Beta",
		);
	});

	it("never deletes a hand-written file", () => {
		// Pruning is what makes upstream removals work, and it is also the one
		// operation that could destroy source. It only ever removes a file
		// carrying the generated marker.
		const workspace = scratch();
		const upstream = syntheticUpstream(workspace, ["alpha"]);
		const targets = syntheticTargets(workspace);
		const manifestFile = join(workspace, "manifest.json");
		generateLucide({ repoRoot: REPO_ROOT, upstream, targets, manifestFile });

		const handWritten = join(targets[0]!.packageDir, OWNED_DIR, "notes.ts");
		writeFileSync(handWritten, "// mine, actually\nexport const kept = true;\n", "utf8");

		const again = generateLucide({ repoRoot: REPO_ROOT, upstream, targets, manifestFile });
		expect(again.writes[0]![1].removed).toEqual([]);
		expect(readFileSync(handWritten, "utf8")).toContain("mine, actually");
	});

	it("reports stale committed output instead of silently fixing it", () => {
		const workspace = scratch();
		const upstream = syntheticUpstream(workspace, ["alpha", "beta"]);
		const targets = syntheticTargets(workspace);
		const manifestFile = join(workspace, "manifest.json");
		generateLucide({ repoRoot: REPO_ROOT, upstream, targets, manifestFile });

		writeFileSync(
			join(targets[1]!.packageDir, OWNED_DIR, "icons", "beta.tsx"),
			"// Generated by tools/lucide — tampered with\n",
			"utf8",
		);
		const check = generateLucide({
			repoRoot: REPO_ROOT,
			upstream,
			targets,
			manifestFile,
			check: true,
		});
		expect(check.clean).toBe(false);
		expect(check.staleSummary).toContain("src/generated/icons/beta.tsx");
	});
});

// ---------------------------------------------------------------------------
// A real consumer, through the real compiler
// ---------------------------------------------------------------------------

const scratchDirs: string[] = [];

afterEach(() => {
	while (scratchDirs.length > 0) {
		rmSync(scratchDirs.pop()!, { recursive: true, force: true });
	}
});

function scratch(): string {
	const dir = mkdtempSync(join(tmpdir(), "rbxts-lucide-"));
	scratchDirs.push(dir);
	return dir;
}

/** A believable `lucide-static` containing exactly the named icons. */
function syntheticUpstream(workspace: string, names: readonly string[]): Upstream {
	const root = join(workspace, "upstream");
	const iconsDir = join(root, "icons");
	mkdirSync(iconsDir, { recursive: true });
	for (const [index, name] of names.entries()) {
		// Distinct geometry per icon, so two icons never collapse into one
		// alias group by accident.
		writeFileSync(
			join(iconsDir, `${name}.svg`),
			`<svg class="lucide lucide-${name}" xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M${3 + index} ${3 + index}h12v12" /></svg>\n`,
			"utf8",
		);
	}
	writeFileSync(
		join(root, "icon-nodes.json"),
		JSON.stringify(Object.fromEntries(names.map((name) => [name, []]))),
		"utf8",
	);
	writeFileSync(join(root, "LICENSE"), "ISC License\n\nsynthetic\n", "utf8");
	return {
		root,
		version: "0.0.0-test",
		license: "ISC",
		licenseText: "ISC License\n\nsynthetic\n",
		iconsDir,
	};
}

function syntheticTargets(workspace: string): LucideTarget[] {
	return lucideTargets(REPO_ROOT).map((target) => {
		const packageDir = join(workspace, target.packageName.replace("@rbxts/", ""));
		mkdirSync(join(packageDir, OWNED_DIR), { recursive: true });
		return { packageName: target.packageName, packageDir };
	});
}

interface Manifest {
	readonly dependencies?: Record<string, string>;
	readonly peerDependencies?: Record<string, string>;
	readonly devDependencies?: Record<string, string>;
}

function dependencyNames(manifest: Manifest): string[] {
	return [
		...Object.keys(manifest.dependencies ?? {}),
		...Object.keys(manifest.peerDependencies ?? {}),
		...Object.keys(manifest.devDependencies ?? {}),
	];
}

/** A throwaway roblox-ts project that borrows an example's installed tree. */
function consumer(
	nodeModules: string,
	compilerOptions: Record<string, unknown>,
	files: Record<string, string>,
): { dir: string; output: string; ok: boolean } {
	const dir = scratch();
	execFileSync("ln", ["-s", nodeModules, join(dir, "node_modules")]);
	const write = (relative: string, contents: string): void => {
		mkdirSync(dirname(join(dir, relative)), { recursive: true });
		writeFileSync(join(dir, relative), contents, "utf8");
	};
	write("package.json", JSON.stringify({ name: "consumer", version: "0.0.0", private: true }));
	write(
		"default.project.json",
		JSON.stringify({
			name: "consumer",
			tree: {
				$className: "DataModel",
				ReplicatedStorage: {
					rbxts_include: {
						$path: "include",
						node_modules: { $className: "Folder", "@rbxts": { $path: "node_modules/@rbxts" } },
					},
					TS: { $path: "out" },
				},
			},
		}),
	);
	write(
		"tsconfig.json",
		JSON.stringify({
			compilerOptions: {
				allowSyntheticDefaultImports: true,
				downlevelIteration: true,
				jsx: "react",
				jsxFactory: "React.createElement",
				module: "commonjs",
				moduleResolution: "Node",
				moduleDetection: "force",
				noLib: true,
				resolveJsonModule: true,
				forceConsistentCasingInFileNames: true,
				skipLibCheck: true,
				strict: true,
				target: "ESNext",
				typeRoots: ["node_modules/@rbxts"],
				rootDir: "src",
				outDir: "out",
				declaration: false,
				preserveSymlinks: true,
				...compilerOptions,
			},
			include: ["src"],
		}),
	);
	for (const [path, contents] of Object.entries(files)) {
		write(path, contents);
	}

	try {
		const output = execFileSync(
			process.execPath,
			[join(REPO_ROOT, "examples/react/node_modules/roblox-ts/out/CLI/cli.js")],
			{ cwd: dir, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
		);
		return { dir, output, ok: true };
	} catch (error) {
		const failure = error as { stdout?: string; stderr?: string };
		return { dir, output: `${failure.stdout ?? ""}${failure.stderr ?? ""}`, ok: false };
	}
}

const REACT_NODE_MODULES = join(REPO_ROOT, "examples/react/node_modules");
const VIDE_NODE_MODULES = join(REPO_ROOT, "examples/vide/node_modules");

describe("a React consumer", () => {
	it("compiles named icon imports and every documented prop", () => {
		const built = consumer(REACT_NODE_MODULES, {}, {
			"src/Icons.tsx":
				`import React from "@rbxts/react";\n` +
				`import { Search, Settings, ChevronDown } from "@rbxts/lucide-react";\n\n` +
				`export function Icons(): React.Element {\n` +
				`\treturn (\n` +
				`\t\t<frame>\n` +
				`\t\t\t<Search size={20} />\n` +
				`\t\t\t<Settings size={24} strokeWidth={1.5} />\n` +
				`\t\t\t<ChevronDown size={16} />\n` +
				`\t\t\t<Search\n` +
				`\t\t\t\tsize={24}\n` +
				`\t\t\t\tcolor={Color3.fromRGB(255, 255, 255)}\n` +
				`\t\t\t\tstrokeWidth={1.5}\n` +
				`\t\t\t\tabsoluteStrokeWidth\n` +
				`\t\t\t\tPosition={UDim2.fromScale(0.5, 0.5)}\n` +
				`\t\t\t\tAnchorPoint={new Vector2(0.5, 0.5)}\n` +
				`\t\t\t/>\n` +
				`\t\t</frame>\n` +
				`\t);\n` +
				`}\n`,
		});
		expect(built.ok, built.output).toBe(true);

		const emitted = readFileSync(join(built.dir, "out/Icons.luau"), "utf8");
		// The barrel: one require of the package root, which is where the cost
		// is. See the loading test below.
		expect(emitted).toContain(`"@rbxts", "lucide-react", "out"`);
		expect(emitted).toContain("React.createElement(Search");
	});

	it("compiles a per-icon subpath import", () => {
		const built = consumer(REACT_NODE_MODULES, {}, {
			"src/Icons.tsx":
				`import React from "@rbxts/react";\n` +
				`import Search from "@rbxts/lucide-react/icons/search";\n` +
				`import AlertCircle from "@rbxts/lucide-react/icons/alert-circle";\n\n` +
				`export function Icons(): React.Element {\n` +
				`\treturn <frame><Search size={24} /><AlertCircle size={24} /></frame>;\n` +
				`}\n`,
		});
		expect(built.ok, built.output).toBe(true);

		const emitted = readFileSync(join(built.dir, "out/Icons.luau"), "utf8");
		// One require per icon, naming the icon's own module — not the barrel.
		expect(emitted).toContain(`"lucide-react", "out", "generated", "icons", "search"`);
		expect(emitted).toContain(`"lucide-react", "out", "generated", "icons", "alert-circle"`);
		expect(emitted).not.toMatch(/"lucide-react", "out"\)/);
	});

	it("rejects a `source` prop, because an icon already is one", () => {
		const built = consumer(REACT_NODE_MODULES, {}, {
			"src/Icons.tsx":
				`import React from "@rbxts/react";\n` +
				`import { Search } from "@rbxts/lucide-react";\n\n` +
				`export function Icons(): React.Element {\n` +
				`\treturn <Search size={24} source={undefined!} />;\n` +
				`}\n`,
		});
		expect(built.ok).toBe(false);
		expect(built.output).toMatch(/source/);
	});
});

describe("a Vide consumer", () => {
	const VIDE_OPTIONS = {
		jsxFactory: "Vide.jsx",
		jsxFragmentFactory: "Vide.Fragment",
		baseUrl: ".",
		paths: { "@rbxts/vide": ["node_modules/@rbxts/vide"] },
	};

	it("compiles named icon imports with reactive props", () => {
		const built = consumer(VIDE_NODE_MODULES, VIDE_OPTIONS, {
			"src/Icons.tsx":
				`import Vide, { source } from "@rbxts/vide";\n` +
				`import { Search, Settings, ChevronDown } from "@rbxts/lucide-vide";\n\n` +
				`export function Icons(): Vide.Node {\n` +
				`\tconst size = source(24);\n` +
				`\tconst colour = source(Color3.fromRGB(255, 255, 255));\n` +
				`\treturn (\n` +
				`\t\t<frame>\n` +
				`\t\t\t<Search size={size} color={colour} />\n` +
				`\t\t\t<Settings size={24} strokeWidth={1.5} />\n` +
				`\t\t\t<ChevronDown size={16} />\n` +
				`\t\t</frame>\n` +
				`\t);\n` +
				`}\n`,
		});
		expect(built.ok, built.output).toBe(true);

		const emitted = readFileSync(join(built.dir, "out/Icons.luau"), "utf8");
		expect(emitted).toContain(`"@rbxts", "lucide-vide", "out"`);
		expect(emitted).toContain("Vide.jsx(Search");
		// The reactive sources are passed straight through, not read here: the
		// wrapper must not collapse a source into a value.
		expect(emitted).toContain("size = size");
		expect(emitted).toContain("color = colour");
		// And no React anywhere near it.
		expect(emitted).not.toContain("react");
	});

	it("compiles a per-icon subpath import", () => {
		const built = consumer(VIDE_NODE_MODULES, VIDE_OPTIONS, {
			"src/Icons.tsx":
				`import Vide from "@rbxts/vide";\n` +
				`import Search from "@rbxts/lucide-vide/icons/search";\n\n` +
				`export function Icons(): Vide.Node {\n` +
				`\treturn <Search Size={UDim2.fromScale(1, 1)} />;\n` +
				`}\n`,
		});
		expect(built.ok, built.output).toBe(true);

		const emitted = readFileSync(join(built.dir, "out/Icons.luau"), "utf8");
		expect(emitted).toContain(`"lucide-vide", "out", "generated", "icons", "search"`);
	});
});

describe("both packages in one project", () => {
	it("resolve side by side onto one @rbxts/svg", () => {
		// The cross-framework fixture, compiled by the real `rbxtsc`. Two
		// packages exporting the same two thousand names have to coexist
		// without a module-resolution collision, and — the part that matters —
		// both have to reach the *same* core, or there would be two caches and
		// the shared-raster property would be vacuous.
		const built = execFileSync(
			process.execPath,
			[join(REPO_ROOT, "examples/react/node_modules/roblox-ts/out/CLI/cli.js")],
			{
				cwd: join(REPO_ROOT, "examples/cross-framework"),
				encoding: "utf8",
				stdio: ["ignore", "pipe", "pipe"],
			},
		);
		expect(built).not.toMatch(/error/i);

		const emitted = readFileSync(
			join(REPO_ROOT, "examples/cross-framework/out/Both.luau"),
			"utf8",
		);
		expect(emitted).toContain(`"@rbxts", "lucide-react", "out").Search`);
		expect(emitted).toContain(`"@rbxts", "lucide-vide", "out").Search`);
		// One core, named once, from neither package's nested copy.
		expect(emitted).toContain(`"@rbxts", "svg", "out"`);
		expect(emitted).not.toContain(`"lucide-react", "node_modules"`);
		expect(emitted).not.toContain(`"lucide-vide", "node_modules"`);
	});

	it("carry byte-identical bytes for the icon they share", () => {
		// The runtime consequence — one cache entry for a React `Search` and a
		// Vide `Search` — is asserted in `tests/luau/vide.luau`, where a real
		// cache is available. What can be checked here is its precondition:
		// the two packages' `Search` is the same asset.
		const react = generatedIconSource(REACT, "search");
		const vide = generatedIconSource(VIDE, "search");
		expect(react).toBe(vide);

		const hash = committedManifest().icons.find((icon) => icon.sourceName === "search")?.hash;
		expect(hash).toBeDefined();
		expect(react).toContain(`"${hash}"`);
		// Nothing package- or framework-specific is anywhere near the identity.
		expect(react).not.toContain("lucide-react");
		expect(react).not.toContain("react");
	});
});

describe("module loading", () => {
	it("makes the root barrel eager and the subpath import exact", () => {
		// Measured from the emitted Luau rather than assumed. roblox-ts has no
		// tree shaking: the package root's `init.luau` requires the generated
		// barrel, which requires every icon module in turn. Naming one icon in
		// the braces does not change that, and the packages document it rather
		// than calling themselves tree-shakable.
		const root = readFileSync(join(REACT, "out/init.luau"), "utf8");
		expect(root).toContain(`TS.import(script, script, "generated")`);

		const barrel = readFileSync(join(REACT, "out/generated/init.luau"), "utf8");
		const requires = barrel.match(/TS\.import\(script, script, "icons", "[^"]+"\)/g) ?? [];
		expect(requires.length).toBe(committedManifest().exportCount);

		// One icon module requires exactly two things: the core (for the asset)
		// and the shared factory. No barrel, no siblings.
		const icon = readFileSync(join(REACT, "out/generated/icons/search.luau"), "utf8");
		expect(icon).toContain(`TS.getModule(script, "@rbxts", "svg")`);
		expect(icon).toContain(`"createLucideIcon"`);
		expect(icon).not.toContain("generated");

		// An alias module requires only its canonical icon.
		const alias = readFileSync(join(REACT, "out/generated/icons/alert-circle.luau"), "utf8");
		expect(alias).toContain(`TS.import(script, script.Parent, "circle-alert")`);
		expect(alias.match(/TS\.import/g)?.length).toBe(2);
	});
});
