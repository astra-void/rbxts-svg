/**
 * The accumulation canvas and the RGBA8 output. Port of
 * `svg-raster/src/image.rs`.
 *
 * **Internally** the canvas holds *premultiplied* RGBA as plain numbers —
 * premultiplied is what makes source-over a single expression per channel,
 * and floating point because a document is composited shape by shape and
 * rounding to eight bits between shapes would accumulate visible error.
 *
 * **Externally** the output is straight (non-premultiplied) RGBA8 in sRGB,
 * row-major from the top-left, four bytes per pixel — exactly what
 * `EditableImage.WritePixelsBuffer` wants. The bytes are written directly
 * into a Luau `buffer`; no intermediate number array is built.
 *
 * Blending happens on the sRGB-encoded values, matching the reference (and
 * every other SVG renderer), not on linearised ones.
 */

/** Rounds a `0..=1` value to a byte, half away from zero. */
function toU8(value: number): number {
	return math.floor(math.clamp(value, 0, 1) * 255 + 0.5);
}

/** A premultiplied RGBA accumulation buffer. */
export class Canvas {
	/** Premultiplied `r, g, b, a` per pixel, each in `0..=1`. */
	private readonly pixels: number[];

	constructor(
		readonly width: number,
		readonly height: number,
	) {
		this.pixels = table.create(width * height * 4, 0);
	}

	/**
	 * Composites a run of one row with the colour (channels 0-255) at
	 * `alpha × coverage`. `coverage` holds the whole row; `startX..endX` is
	 * the half-open part worth touching.
	 */
	blendRow(
		y: number,
		coverage: number[],
		startX: number,
		endX: number,
		colourR: number,
		colourG: number,
		colourB: number,
		alpha: number,
	): void {
		if (y >= this.height || y < 0 || alpha <= 0) {
			return;
		}
		const red = colourR / 255;
		const green = colourG / 255;
		const blue = colourB / 255;

		const pixels = this.pixels;
		const rowBase = y * this.width * 4;
		const last = math.min(endX, this.width);
		for (let x = startX; x < last; x++) {
			// Coverage can exceed 1 only through floating-point slop in the
			// span accumulator; clamping keeps alpha a probability.
			const sourceAlpha = math.clamp(coverage[x] * alpha, 0, 1);
			if (sourceAlpha <= 0) {
				continue;
			}
			const at = rowBase + x * 4;
			const inverse = 1 - sourceAlpha;
			pixels[at] = red * sourceAlpha + pixels[at] * inverse;
			pixels[at + 1] = green * sourceAlpha + pixels[at + 1] * inverse;
			pixels[at + 2] = blue * sourceAlpha + pixels[at + 2] * inverse;
			pixels[at + 3] = sourceAlpha + pixels[at + 3] * inverse;
		}
	}

	/**
	 * Composites coverage into the alpha channel alone, leaving RGB untouched.
	 * The mask path: colour is not merely ignored — it must not be recorded at
	 * all, or the result would no longer be tintable.
	 */
	blendRowAlpha(y: number, coverage: number[], startX: number, endX: number, alpha: number): void {
		if (y >= this.height || y < 0 || alpha <= 0) {
			return;
		}
		const pixels = this.pixels;
		const rowBase = y * this.width * 4;
		const last = math.min(endX, this.width);
		for (let x = startX; x < last; x++) {
			const sourceAlpha = math.clamp(coverage[x] * alpha, 0, 1);
			if (sourceAlpha <= 0) {
				continue;
			}
			const at = rowBase + x * 4 + 3;
			pixels[at] = sourceAlpha + pixels[at] * (1 - sourceAlpha);
		}
	}

	/**
	 * Converts to straight RGBA8 in a fresh `buffer` of exactly
	 * `width * height * 4` bytes.
	 *
	 * `mask` replaces every colour with white, so that `ImageColor3` — which
	 * multiplies — reproduces any tint exactly.
	 */
	finish(mask: boolean): buffer {
		const pixels = this.pixels;
		const count = this.width * this.height;
		const out = buffer.create(count * 4);

		for (let index = 0; index < count; index++) {
			const at = index * 4;
			const alpha = math.clamp(pixels[at + 3], 0, 1);
			if (mask) {
				buffer.writeu8(out, at, 255);
				buffer.writeu8(out, at + 1, 255);
				buffer.writeu8(out, at + 2, 255);
				buffer.writeu8(out, at + 3, toU8(alpha));
			} else if (alpha <= 0) {
				// buffer.create zero-fills, so a fully transparent pixel needs
				// no writes at all.
			} else {
				// Undo the premultiplication. Values can exceed the alpha only
				// by rounding, so clamping is a guard rather than a correction.
				buffer.writeu8(out, at, toU8(pixels[at] / alpha));
				buffer.writeu8(out, at + 1, toU8(pixels[at + 1] / alpha));
				buffer.writeu8(out, at + 2, toU8(pixels[at + 2] / alpha));
				buffer.writeu8(out, at + 3, toU8(alpha));
			}
		}

		return out;
	}
}
