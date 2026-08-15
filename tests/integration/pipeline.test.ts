/**
 * The vertical slice, end to end from TypeScript.
 *
 * ```text
 * tests/fixtures/lucide/search.svg
 *           ↓  usvg
 *      semantic IR
 *           ↓
 *      compact SVG IR
 *           ↓  napi-rs
 *   TypeScript compiler API
 * ```
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import {
	SvgFeatureFlags,
	compileSvg,
	compileSvgFile,
	decodeSvgIr,
	irVersion,
} from "@rbxts/svg-compiler";

const FIXTURES = join(__dirname, "../fixtures");
const searchPath = join(FIXTURES, "lucide/search.svg");

describe("compileSvg", () => {
	it("compiles a real Lucide icon", () => {
		const result = compileSvg(readFileSync(searchPath));

		expect(result.width).toBe(24);
		expect(result.height).toBe(24);
		expect(result.data.length).toBeGreaterThan(0);
		expect(result.irVersion).toBe(irVersion());
		expect(result.shapeCount).toBe(2);
		expect(result.hash).toMatch(/^[0-9a-f]{64}$/);
	});

	it("accepts a string as well as a Buffer", () => {
		const fromBuffer = compileSvg(readFileSync(searchPath));
		const fromString = compileSvg(readFileSync(searchPath, "utf8"));
		expect(fromString.hash).toBe(fromBuffer.hash);
		expect(fromString.data.equals(fromBuffer.data)).toBe(true);
	});

	it("reports the view box rather than the width/height attributes", () => {
		// This file is width/height 96 with a "-12 -12 24 24" view box. The
		// compiled asset must describe the 24-unit coordinate system; the pixel
		// size is chosen at render time.
		const result = compileSvgFile(join(FIXTURES, "basic/offset-viewbox.svg"));
		expect(result.viewBox).toEqual({ x: -12, y: -12, width: 24, height: 24 });
	});

	it("detects currentColor and monochrome assets", () => {
		const result = compileSvgFile(searchPath);
		expect(result.flags & SvgFeatureFlags.UsesCurrentColor).toBeTruthy();
		expect(result.flags & SvgFeatureFlags.Monochrome).toBeTruthy();
		expect(result.flags & SvgFeatureFlags.HasStroke).toBeTruthy();
		expect(result.flags & SvgFeatureFlags.HasFill).toBeFalsy();
	});

	it("does not mistake a fixed colour for currentColor", () => {
		const result = compileSvgFile(join(FIXTURES, "basic/simple-path.svg"));
		expect(result.flags & SvgFeatureFlags.UsesCurrentColor).toBeFalsy();
	});
});

describe("preserveAspectRatio", () => {
	// The view box alone cannot say how an asset should fill a target rectangle
	// of a different shape, so the authored policy has to travel with it.
	it.each([
		["basic/aspect-meet.svg", "xMidYMid meet"],
		["basic/aspect-none.svg", "none"],
		["basic/aspect-slice.svg", "xMinYMin slice"],
	])("%s compiles to %s", (fixture, expected) => {
		expect(compileSvgFile(join(FIXTURES, fixture)).preserveAspectRatio).toBe(expected);
	});

	it("defaults to xMidYMid meet when the attribute is absent", () => {
		expect(compileSvgFile(searchPath).preserveAspectRatio).toBe("xMidYMid meet");
	});

	it("survives serialization", () => {
		for (const [fixture, expected] of [
			["basic/aspect-meet.svg", "xMidYMid meet"],
			["basic/aspect-none.svg", "none"],
			["basic/aspect-slice.svg", "xMinYMin slice"],
			["lucide/search.svg", "xMidYMid meet"],
		] as const) {
			const compiled = compileSvgFile(join(FIXTURES, fixture));
			expect(decodeSvgIr(compiled.data).preserveAspectRatio).toBe(expected);
		}
	});

	// Three fixtures with identical geometry and view boxes: if the policy were
	// being dropped they would compile to identical bytes.
	it("changes the compiled output", () => {
		const hashes = [
			"basic/aspect-meet.svg",
			"basic/aspect-none.svg",
			"basic/aspect-slice.svg",
		].map((fixture) => compileSvgFile(join(FIXTURES, fixture)).hash);
		expect(new Set(hashes).size).toBe(3);
	});
});

describe("decodeSvgIr", () => {
	it("decodes meaningful commands and styles, not just bytes", () => {
		const compiled = compileSvgFile(searchPath);
		const decoded = decodeSvgIr(compiled.data);

		expect(decoded.irVersion).toBe(compiled.irVersion);
		expect(decoded.width).toBe(24);
		expect(decoded.height).toBe(24);
		expect(decoded.shapes).toHaveLength(2);

		// `<path d="m21 21-4.34-4.34"/>`: a move and a line, stroked with
		// currentColor at width 2 and round caps.
		const [line, circle] = decoded.shapes;
		expect(line!.fill).toBeUndefined();
		expect(line!.stroke).toMatchObject({ kind: "currentColor", alpha: 1 });
		expect(line!.strokeWidth).toBe(2);
		expect(line!.lineCap).toBe("round");
		expect(line!.lineJoin).toBe("round");
		expect(line!.paintOrder).toBe("fillThenStroke");

		expect(line!.commands.map((c) => c.op)).toEqual(["moveTo", "lineTo"]);
		expect(line!.commands[0]!.points).toEqual([21, 21]);
		expect(line!.commands[1]!.points[0]).toBeCloseTo(16.66, 2);

		// `<circle r="8"/>` lowers to four cubic quadrants and a close.
		const ops = circle!.commands.map((c) => c.op);
		expect(ops.filter((op) => op === "cubicTo")).toHaveLength(4);
		expect(ops.at(-1)).toBe("close");
	});

	it("only ever emits the four canonical commands", () => {
		const canonical = new Set(["moveTo", "lineTo", "cubicTo", "close"]);
		for (const name of [
			"basic/quadratic-and-arc.svg",
			"basic/shorthand-commands.svg",
			"basic/rect.svg",
			"basic/polygon.svg",
			"lucide/settings.svg",
		]) {
			const decoded = decodeSvgIr(compileSvgFile(join(FIXTURES, name)).data);
			for (const shape of decoded.shapes) {
				for (const command of shape.commands) {
					expect(canonical.has(command.op), `${name}: ${command.op}`).toBe(true);
				}
			}
		}
	});

	it("preserves fill rules", () => {
		const decoded = decodeSvgIr(
			compileSvgFile(join(FIXTURES, "basic/evenodd.svg")).data,
		);
		expect(decoded.shapes[0]!.fillRule).toBe("evenodd");

		const nonZero = decodeSvgIr(
			compileSvgFile(join(FIXTURES, "basic/simple-path.svg")).data,
		);
		expect(nonZero.shapes[0]!.fillRule).toBe("nonzero");
	});

	it("preserves subpaths", () => {
		const decoded = decodeSvgIr(
			compileSvgFile(join(FIXTURES, "basic/multiple-subpaths.svg")).data,
		);
		const moves = decoded.shapes[0]!.commands.filter((c) => c.op === "moveTo");
		expect(moves).toHaveLength(2);
	});

	it("rejects corrupt input with a structured error", () => {
		const compiled = compileSvgFile(searchPath);

		const badMagic = Buffer.from(compiled.data);
		badMagic[0] = 0x58;
		expect(() => decodeSvgIr(badMagic)).toThrow(/not a compiled SVG IR blob/);

		const badVersion = Buffer.from(compiled.data);
		badVersion.writeUInt16LE(99, 4);
		expect(() => decodeSvgIr(badVersion)).toThrow(/version 99 is not supported/);

		expect(() => decodeSvgIr(Buffer.alloc(0))).toThrow();
		expect(() => decodeSvgIr(compiled.data.subarray(0, 20))).toThrow(/truncated/);
	});
});

describe("every primitive lowers to path geometry", () => {
	it.each([
		["rect", "basic/rect.svg"],
		["circle", "basic/circle.svg"],
		["ellipse", "basic/ellipse.svg"],
		["line", "basic/line.svg"],
		["polyline", "basic/polyline.svg"],
		["polygon", "basic/polygon.svg"],
	])("%s", (_name, fixture) => {
		const decoded = decodeSvgIr(compileSvgFile(join(FIXTURES, fixture)).data);
		expect(decoded.shapes).toHaveLength(1);
		expect(decoded.shapes[0]!.commands.length).toBeGreaterThan(1);
	});
});
