/**
 * The reference-raster binding: `renderSvgIr` over `svg-raster`.
 *
 * This binding exists so the Luau test bundler can generate golden raster
 * fixtures (`tests/luau/goldens.luau`) from the executable specification. The
 * pixel-level correctness of the renderer itself is pinned by the Rust suites;
 * what these tests pin is the *binding*: options mapping, buffer shape, and
 * the properties golden generation depends on.
 */

import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { compileSvgFile, renderSvgIr } from "@rbxts/svg-compiler";

const FIXTURES = join(__dirname, "../fixtures");
const search = compileSvgFile(join(FIXTURES, "lucide/search.svg"));
const mixed = compileSvgFile(join(FIXTURES, "basic/mixed-current-color.svg"));

function alphaMass(pixels: Buffer): number {
	let mass = 0;
	for (let index = 3; index < pixels.length; index += 4) {
		mass += pixels[index];
	}
	return mass;
}

describe("renderSvgIr", () => {
	it("produces a buffer of exactly width * height * 4 bytes", () => {
		const image = renderSvgIr(search.data, 24, 24, { alphaMask: true });
		expect(image.width).toBe(24);
		expect(image.height).toBe(24);
		expect(image.pixels.length).toBe(24 * 24 * 4);
		expect(alphaMass(image.pixels)).toBeGreaterThan(0);
	});

	it("renders alpha masks with white RGB everywhere", () => {
		const image = renderSvgIr(search.data, 24, 24, { alphaMask: true });
		for (let index = 0; index < image.pixels.length; index += 4) {
			expect(image.pixels[index]).toBe(255);
			expect(image.pixels[index + 1]).toBe(255);
			expect(image.pixels[index + 2]).toBe(255);
		}
	});

	it("keeps the mask identical whatever the currentColor", () => {
		const red = renderSvgIr(search.data, 24, 24, { alphaMask: true, currentColor: [255, 0, 0] });
		const blue = renderSvgIr(search.data, 24, 24, { alphaMask: true, currentColor: [0, 0, 255] });
		expect(red.pixels.equals(blue.pixels)).toBe(true);
	});

	it("resolves currentColor in full-colour renders of a mixed asset", () => {
		const black = renderSvgIr(mixed.data, 24, 24, {});
		const blue = renderSvgIr(mixed.data, 24, 24, { currentColor: [0, 128, 255] });
		expect(black.pixels.equals(blue.pixels)).toBe(false);

		// The fixed red backdrop is identical in both.
		const at = (x: number, y: number) => (y * 24 + x) * 4;
		const backdrop = at(4, 4);
		expect(black.pixels[backdrop]).toBe(255);
		expect(blue.pixels[backdrop]).toBe(255);
		// The currentColor bar follows the request.
		const bar = at(12, 12);
		expect(black.pixels.subarray(bar, bar + 3)).toEqual(Buffer.from([0, 0, 0]));
		expect(blue.pixels.subarray(bar, bar + 3)).toEqual(Buffer.from([0, 128, 255]));
	});

	it("applies stroke overrides, relative and absolute", () => {
		const base = renderSvgIr(search.data, 48, 48, { alphaMask: true });
		const thin = renderSvgIr(search.data, 48, 48, { alphaMask: true, strokeWidth: 1 });
		const absolute = renderSvgIr(search.data, 48, 48, {
			alphaMask: true,
			strokeWidth: 2,
			absoluteStrokeWidth: true,
		});
		// Thinner stroke, less coverage.
		expect(alphaMass(thin.pixels)).toBeLessThan(alphaMass(base.pixels));
		// 2 absolute pixels at 48px is 1 view box unit for a 24-unit icon:
		// exactly the relative thin render. This is the unit contract the
		// runtime's resolveRenderOptions mirrors.
		expect(absolute.pixels.equals(thin.pixels)).toBe(true);
	});

	it("is deterministic", () => {
		const a = renderSvgIr(search.data, 32, 32, { alphaMask: true });
		const b = renderSvgIr(search.data, 32, 32, { alphaMask: true });
		expect(a.pixels.equals(b.pixels)).toBe(true);
	});

	it("rejects unusable dimensions and bad colours", () => {
		expect(() => renderSvgIr(search.data, 0, 24)).toThrow(/not usable/);
		expect(() => renderSvgIr(search.data, 24, 24, { currentColor: [1, 2] })).toThrow(/currentColor/);
		expect(() => renderSvgIr(search.data, 24, 24, { currentColor: [1, 2, 999] })).toThrow(/currentColor/);
	});

	it("rejects bytes that are not compiled IR", () => {
		expect(() => renderSvgIr(Buffer.from("not an asset"), 24, 24)).toThrow(/decode/);
	});
});
