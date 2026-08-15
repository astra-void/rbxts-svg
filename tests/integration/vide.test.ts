/**
 * The Vide binding, through the real compilers and against the real package
 * graph.
 *
 * Two different things are being defended here.
 *
 * The first is that a Vide project's `.svg` imports work exactly as a React
 * project's do — same generated modules, same transformer, no branch anywhere
 * that asks which framework is consuming the asset. As with the React tests,
 * the assertion is on *emitted Luau*, because a transformed AST that
 * TypeScript accepts tells you nothing about the `require` path roblox-ts will
 * finally compute.
 *
 * The second is the shape of the dependency graph. `svg-react → svg` and
 * `svg-vide → svg`, never the reverse and never sideways, is the property that
 * makes `@rbxts/svg` framework-neutral rather than merely framework-flavoured,
 * and it is the kind of thing a single convenient import quietly undoes.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";

import {
	Fixture,
	REPO_ROOT,
	SEARCH_SVG,
	VIDE_NODE_MODULES,
	expectOk,
} from "./fixture.js";

const fixtures: Fixture[] = [];

afterEach(() => {
	while (fixtures.length > 0) {
		fixtures.pop()?.dispose();
	}
});

/** A project configured the way `examples/vide` is: Vide's JSX factory. */
function newVideFixture(): Fixture {
	const fixture = new Fixture({
		nodeModules: VIDE_NODE_MODULES,
		compilerOptions: {
			jsxFactory: "Vide.jsx",
			jsxFragmentFactory: "Vide.Fragment",
			baseUrl: ".",
			// See `examples/vide/tsconfig.json`: pnpm exposes one physical
			// `@rbxts/vide` by two routes, and roblox-ts names the nested one.
			paths: { "@rbxts/vide": ["node_modules/@rbxts/vide"] },
		},
	});
	fixtures.push(fixture);
	fixture.write("src/icons/search.svg", readFileSync(SEARCH_SVG, "utf8"));
	return fixture;
}

describe("a Vide project", () => {
	it("compiles a direct .svg import to a require of the generated module", () => {
		const fixture = newVideFixture();
		fixture.write(
			"src/Icon.tsx",
			`import { Svg } from "@rbxts/svg-vide";\n` +
				`import Vide from "@rbxts/vide";\n\n` +
				`import Search from "./icons/search.svg";\n\n` +
				`export function Icon(): Vide.Node {\n` +
				`\treturn <Svg source={Search} size={24} color={Color3.fromRGB(255, 255, 255)} />;\n` +
				`}\n`,
		);

		expectOk(fixture.buildSvgs(), "rbxts-svg build");
		expectOk(fixture.compile(), "rbxtsc");

		const emitted = readFileSync(fixture.path("out/Icon.luau"), "utf8");

		// The same rewrite the React example gets: the specifier names the
		// `.svg`, the require names the generated module.
		expect(emitted).toContain(`"TS", "svg-cache", "icons", "search.svg"`);
		expect(emitted).not.toContain(`"TS", "icons", "search.svg"`);

		// And the generated module is the framework-neutral asset rather than a
		// component: it reaches for the core's decoder and nothing else.
		const asset = readFileSync(fixture.path("out/svg-cache/icons/search.svg.luau"), "utf8");
		expect(asset).toContain("createAssetFromBase64");
		expect(asset).toContain("unstable_internal");
		expect(asset).not.toContain("Svg(");
	});

	it("generates the same module a React project would", () => {
		// The strongest statement of framework-neutrality available at this
		// layer: the generator is handed a Vide project and a React project and
		// produces the same bytes, because it was never told which it had.
		const videProject = newVideFixture();
		videProject.write("src/main.ts", `import Search from "./icons/search.svg";\n\nexport = Search;\n`);
		expectOk(videProject.buildSvgs(), "rbxts-svg build (vide)");

		const reactProject = new Fixture();
		fixtures.push(reactProject);
		reactProject.write("src/icons/search.svg", readFileSync(SEARCH_SVG, "utf8"));
		reactProject.write("src/main.ts", `import Search from "./icons/search.svg";\n\nexport = Search;\n`);
		expectOk(reactProject.buildSvgs(), "rbxts-svg build (react)");

		const generated = "src/svg-cache/icons/search.svg.ts";
		expect(readFileSync(videProject.path(generated), "utf8")).toBe(
			readFileSync(reactProject.path(generated), "utf8"),
		);
	});

	it("emits Vide's JSX factory and no React", () => {
		const fixture = newVideFixture();
		fixture.write(
			"src/Icon.tsx",
			`import { Svg } from "@rbxts/svg-vide";\n` +
				`import Vide from "@rbxts/vide";\n\n` +
				`import Search from "./icons/search.svg";\n\n` +
				`export function Icon(): Vide.Node {\n` +
				`\treturn <Svg source={Search} size={24} />;\n` +
				`}\n`,
		);

		expectOk(fixture.buildSvgs(), "rbxts-svg build");
		expectOk(fixture.compile(), "rbxtsc");

		const emitted = readFileSync(fixture.path("out/Icon.luau"), "utf8");
		expect(emitted).toContain("Vide.jsx");
		expect(emitted).toContain(`"@rbxts", "svg-vide", "out"`);
		expect(emitted).toContain(`"@rbxts", "vide", "src"`);
		// Nothing pulls React in, which is the point of a separate binding
		// package rather than one with both.
		expect(emitted).not.toMatch(/"@rbxts", "react/);
	});

});

describe("the built Vide example", () => {
	// Asserted on the example rather than on a fixture because the thing being
	// checked *is* a project configuration: which `node_modules` route
	// roblox-ts names. Requires `pnpm build`, which builds the examples.
	function emitted(relativePath: string): string {
		const path = join(REPO_ROOT, "examples/vide/out", relativePath);
		try {
			return readFileSync(path, "utf8");
		} catch {
			throw new Error(
				`${path} is missing. Run \`pnpm --filter rbxts-svg-example-vide run build\` first.`,
			);
		}
	}

	/**
	 * Requires that reach a package through a *nested* `node_modules`.
	 *
	 * roblox-ts emits the route TypeScript resolved, so a workspace's pnpm
	 * nesting can leak into the output as a second copy of a shared package.
	 * The published shape has exactly one `node_modules` in any require path.
	 */
	function nestedModuleRequires(source: string): string[] {
		return (source.match(/TS\.import\([^\n]*/g) ?? []).filter(
			(line) => (line.match(/"node_modules"/g) ?? []).length > 1,
		);
	}

	it("requires one core, by the path a published install produces", () => {
		// Two copies of `@rbxts/svg` would mean two caches, two renderer
		// registries and two of every raster — invisible until you counted
		// them. Under pnpm the core is reachable directly *and* through a
		// nested `node_modules` inside every workspace package that also
		// depends on it, and only the direct route exists for someone
		// installing from npm.
		//
		// This used to name `@rbxts/svg-vide` specifically, and adding
		// `@rbxts/lucide-vide` to the example walked straight past it: the
		// emitted requires went through `lucide-vide/node_modules/@rbxts/svg`
		// and every suite still passed. A live Studio session found it. So the
		// assertion is now about the shape rather than about a package name —
		// no require may walk into a *second* `node_modules` at all.
		for (const file of ["Toolbar.luau", "svg-cache/icons/logo.svg.luau"]) {
			const source = emitted(file);
			expect(source).toContain(`"@rbxts", "svg", "out"`);
			expect(nestedModuleRequires(source)).toEqual([]);
		}
	});

	it("draws from both a direct .svg import and named Lucide components", () => {
		// The example uses each route on purpose, and they must stay
		// distinguishable: `logo.svg` goes through the generated module the
		// transformer rewrote onto, and the Lucide icons come precompiled from
		// a package with no transformer involved at all.
		const toolbar = emitted("Toolbar.luau");
		expect(toolbar).toContain(`"TS", "svg-cache", "icons", "logo.svg"`);
		expect(toolbar).toContain(`"@rbxts", "svg-vide", "out"`);
		expect(toolbar).toContain(`"@rbxts", "lucide-vide", "out"`);
		for (const icon of ["Bell", "ChevronDown", "Search", "Settings"]) {
			expect(toolbar).toContain(`local ${icon} = _lucide_vide.${icon}`);
		}
	});

	it("pulls in no React", () => {
		for (const file of ["Toolbar.luau", "client/main.client.luau"]) {
			expect(emitted(file)).not.toMatch(/TS\.import\([^\n]*"@rbxts", "react/);
		}
	});
});

describe("the package graph", () => {
	interface Manifest {
		readonly name: string;
		readonly dependencies?: Record<string, string>;
		readonly peerDependencies?: Record<string, string>;
		readonly devDependencies?: Record<string, string>;
	}

	function manifest(packageDir: string): Manifest {
		return JSON.parse(
			readFileSync(join(REPO_ROOT, "packages", packageDir, "package.json"), "utf8"),
		) as Manifest;
	}

	/** Every package this one depends on, in any capacity. */
	function allDependencies(pkg: Manifest): string[] {
		return [
			...Object.keys(pkg.dependencies ?? {}),
			...Object.keys(pkg.peerDependencies ?? {}),
			...Object.keys(pkg.devDependencies ?? {}),
		];
	}

	const core = manifest("svg");
	const react = manifest("svg-react");
	const vide = manifest("svg-vide");

	it("keeps the core free of every UI framework", () => {
		// The arrows point inwards, always. A core that reached back out to a
		// framework would make "framework-neutral" a documentation claim rather
		// than a structural fact.
		for (const name of allDependencies(core)) {
			expect(name).not.toMatch(/^@rbxts\/(react|react-roblox|vide)$/);
			expect(name).not.toMatch(/^@rbxts\/svg-(react|vide)$/);
		}
	});

	it("keeps the bindings apart", () => {
		expect(allDependencies(react)).not.toContain("@rbxts/svg-vide");
		expect(allDependencies(react)).not.toContain("@rbxts/vide");

		expect(allDependencies(vide)).not.toContain("@rbxts/svg-react");
		expect(allDependencies(vide)).not.toContain("@rbxts/react");
		expect(allDependencies(vide)).not.toContain("@rbxts/react-roblox");
	});

	it("points each binding at the core and its own framework", () => {
		expect(vide.name).toBe("@rbxts/svg-vide");
		expect(Object.keys(vide.peerDependencies ?? {}).sort()).toEqual([
			"@rbxts/svg",
			"@rbxts/vide",
		]);
		expect(Object.keys(react.peerDependencies ?? {}).sort()).toEqual([
			"@rbxts/react",
			"@rbxts/svg",
		]);
	});
});

describe("the sizing policy", () => {
	function source(path: string): string {
		return readFileSync(join(REPO_ROOT, path), "utf8");
	}

	it("has exactly one implementation, in the core", () => {
		// The two bindings differ in lifecycle, not in rendering semantics. The
		// cheapest way to keep that true is for neither of them to be able to
		// disagree: the arithmetic exists once, in `@rbxts/svg`.
		const core = source("packages/svg/src/render/sizing.ts");
		expect(core).toContain("export function snapSvgPixelSize");
		expect(core).toContain("export function measureSvgPixelSize");
		expect(core).toContain("export function resolveSvgSizing");

		for (const path of ["packages/svg-react/src/Svg.tsx", "packages/svg-vide/src/Svg.tsx"]) {
			const binding = source(path);
			expect(binding).toContain("resolveSvgSizing");
			// Both bindings turn an observed `AbsoluteSize` into a resolution
			// through the same function, which is what makes "not laid out yet
			// means acquire nothing" one decision rather than two.
			expect(binding).toContain("measureSvgPixelSize");
			expect(binding).toMatch(/from "@rbxts\/svg"/);
			// No local reimplementation: the rounding, the 1×1 floor and the
			// unmeasured case live in one place, so "48.2 becomes 48" and
			// "zero is not a size" cannot each mean two things.
			expect(binding).not.toContain("math.round");
		}
	});

	it("keeps the React package's original names working", () => {
		const compat = source("packages/svg-react/src/sizing.ts");
		expect(compat).toContain("snapSvgPixelSize as snapToPixels");
		expect(compat).toContain("resolveSvgSizing as svgSizing");

		const index = source("packages/svg-react/src/index.ts");
		expect(index).toContain("snapToPixels");
		expect(index).toContain("svgSizing");
		expect(index).toContain("SvgSizing");
	});
});
