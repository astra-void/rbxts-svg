/**
 * Determinism.
 *
 * The same SVG, compiler version and options must produce identical bytes on
 * every machine and in every process. Content hashes are the basis of build
 * caching and of the `.svg` import pipeline, so this is load-bearing rather
 * than merely tidy.
 */

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { compileSvg, compileSvgFile } from "@rbxts/svg-compiler";

const FIXTURES = join(__dirname, "../fixtures");

function compilableFixtures(): string[] {
	return ["basic", "lucide"].flatMap((dir) =>
		readdirSync(join(FIXTURES, dir))
			.filter((name) => name.endsWith(".svg"))
			.sort()
			.map((name) => `${dir}/${name}`),
	);
}

describe("determinism", () => {
	it("produces identical bytes across repeated compiles", () => {
		for (const fixture of compilableFixtures()) {
			const source = readFileSync(join(FIXTURES, fixture));
			const first = compileSvg(source);
			for (let i = 0; i < 5; i += 1) {
				const again = compileSvg(source);
				expect(again.data.equals(first.data), fixture).toBe(true);
				expect(again.hash, fixture).toBe(first.hash);
			}
		}
	});

	it("gives every fixture a distinct hash", () => {
		const seen = new Map<string, string>();
		for (const fixture of compilableFixtures()) {
			const { hash } = compileSvgFile(join(FIXTURES, fixture));
			const previous = seen.get(hash);
			expect(previous, `${fixture} collides with ${previous}`).toBeUndefined();
			seen.set(hash, fixture);
		}
	});

	it("ignores the source name, which only labels diagnostics", () => {
		const source = readFileSync(join(FIXTURES, "lucide/search.svg"));
		const a = compileSvg(source, { sourceName: "a.svg" });
		const b = compileSvg(source, { sourceName: "deeply/nested/b.svg" });
		expect(a.hash).toBe(b.hash);
	});

	it("ignores insignificant whitespace, so reformatting does not bust caches", () => {
		const compact =
			'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" ' +
			'stroke="currentColor" stroke-width="2"><path d="M4 12 L20 12"/></svg>';
		const spaced = `
<svg
  xmlns="http://www.w3.org/2000/svg"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
>
  <path d="M4 12 L20 12" />
</svg>
`;
		expect(compileSvg(compact).hash).toBe(compileSvg(spaced).hash);
	});

	it("changes the hash when the geometry changes", () => {
		const base =
			'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">' +
			'<path d="M4 12 L20 12" stroke="#000" stroke-width="2"/></svg>';
		const moved = base.replace("L20 12", "L20 13");
		expect(compileSvg(base).hash).not.toBe(compileSvg(moved).hash);
	});

	it("changes the hash when a colour changes", () => {
		const red =
			'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">' +
			'<rect width="10" height="10" fill="#ff0000"/></svg>';
		const blue = red.replace("#ff0000", "#0000ff");
		expect(compileSvg(red).hash).not.toBe(compileSvg(blue).hash);
	});

	it("keeps compiled assets small", () => {
		// A guard against an accidental change that bloats the format: a Lucide
		// icon should be a few hundred bytes, not kilobytes.
		for (const icon of ["search", "chevron-down", "git-branch"]) {
			const { data } = compileSvgFile(join(FIXTURES, `lucide/${icon}.svg`));
			expect(data.length, icon).toBeLessThan(1024);
		}
	});
});
