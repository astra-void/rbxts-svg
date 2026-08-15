/**
 * Unsupported features must fail loudly, and supported ones must not.
 *
 * The value of a build-time pipeline is precisely that it can refuse: an SVG
 * that would render wrongly inside Roblox should never reach Roblox. These
 * tests pin both halves of that — what is rejected, and what is deliberately
 * not.
 */

import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { SvgCompileError, compileSvgFile } from "@rbxts/svg-compiler";

const FIXTURES = join(__dirname, "../fixtures");

describe("unsupported features", () => {
	it.each([
		["gradients", "unsupported/gradient-fill.svg"],
		["filters", "unsupported/filter.svg"],
		["text", "unsupported/text.svg"],
		["clip paths", "unsupported/clip-path.svg"],
		["stroke-dasharray", "unsupported/stroke-dasharray.svg"],
	])("%s are rejected rather than silently dropped", (_name, fixture) => {
		expect(() => compileSvgFile(join(FIXTURES, fixture))).toThrow(SvgCompileError);
	});

	it("names the file, the element and its path", () => {
		let message = "";
		try {
			compileSvgFile(join(FIXTURES, "unsupported/filter.svg"), {
				sourceName: "assets/logo.svg",
			});
		} catch (error) {
			message = (error as Error).message;
		}

		expect(message).toContain("Unsupported SVG feature in assets/logo.svg");
		expect(message).toContain("<filter> is not supported");
		expect(message).toContain('<filter id="shadow">');
		expect(message).toContain("svg > defs > filter#shadow");
		expect(message).toMatch(/assets\/logo\.svg:\d+:\d+/);
	});

	it("reports one problem once, not once per detector", () => {
		let message = "";
		try {
			compileSvgFile(join(FIXTURES, "unsupported/filter.svg"));
		} catch (error) {
			message = (error as Error).message;
		}
		const occurrences = message.split("error:").length - 1;
		expect(occurrences).toBe(1);
	});

	it("can be downgraded to warnings on request", () => {
		const result = compileSvgFile(join(FIXTURES, "unsupported/gradient-fill.svg"), {
			allowUnsupported: true,
		});
		expect(result.diagnostics.some((d) => d.severity === "warning")).toBe(true);
		expect(result.diagnostics.every((d) => d.severity !== "error")).toBe(true);
	});
});

describe("malformed input", () => {
	it("reports malformed XML without crashing", () => {
		expect(() => compileSvgFile(join(FIXTURES, "unsupported/malformed.svg"))).toThrow(
			SvgCompileError,
		);
	});

	it("explains a missing coordinate system", () => {
		expect(() => compileSvgFile(join(FIXTURES, "unsupported/no-viewbox.svg"))).toThrow(
			/viewBox/,
		);
	});
});

describe("things that must not be rejected", () => {
	it("accepts editor metadata and unreferenced definitions", () => {
		const result = compileSvgFile(join(FIXTURES, "basic/metadata-and-title.svg"));

		expect(result.shapeCount).toBe(1);
		expect(result.diagnostics.every((d) => d.severity !== "error")).toBe(true);

		const codes = result.diagnostics.map((d) => d.code);
		expect(codes).toContain("unreferenced-definition");
		expect(codes).toContain("ignored-metadata");
	});

	it("warns rather than fails when group opacity is approximated", () => {
		const result = compileSvgFile(join(FIXTURES, "basic/group-opacity.svg"));
		const warning = result.diagnostics.find(
			(d) => d.code === "approximated-group-opacity",
		);
		expect(warning?.severity).toBe("warning");
	});

	it("compiles every Lucide fixture cleanly", () => {
		for (const icon of [
			"search",
			"settings",
			"chevron-down",
			"circle-alert",
			"bell",
			"git-branch",
		]) {
			const result = compileSvgFile(join(FIXTURES, `lucide/${icon}.svg`));
			expect(result.diagnostics, icon).toEqual([]);
			expect(result.shapeCount, icon).toBeGreaterThan(0);
		}
	});
});

describe("diagnostic structure", () => {
	it("carries machine-readable fields alongside the rendered text", () => {
		const result = compileSvgFile(join(FIXTURES, "basic/metadata-and-title.svg"));
		const diagnostic = result.diagnostics.find(
			(d) => d.code === "unreferenced-definition",
		);

		expect(diagnostic).toBeDefined();
		expect(diagnostic!.tag).toBe("linearGradient");
		expect(diagnostic!.id).toBe("never-used");
		expect(diagnostic!.line).toBeGreaterThan(0);
		expect(diagnostic!.column).toBeGreaterThan(0);
		expect(diagnostic!.rendered).toContain(diagnostic!.message);
	});
});
