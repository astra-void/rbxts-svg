/**
 * Coverage estimation: vertical supersampling with exact horizontal spans.
 * Port of the `ScanlineSupersampler` half of `svg-raster/src/edges.rs`.
 *
 * Coverage is exact along x — every span contributes its true fractional width
 * to the pixels it partly covers — and sampled along y with
 * {@link SUB_SCANLINES} sub-scanlines per pixel row. The known limitation is
 * inherited knowingly from the reference: a feature that is both nearly
 * horizontal and thinner than a pixel can be off by about 8 of 255 alpha
 * levels. This module is the isolated seam where analytical coverage would go,
 * in both implementations at once.
 *
 * This is the hottest code in the renderer, so the inner loops work on bare
 * numbers in reused scratch arrays: no per-sample tables, no per-pixel
 * closures, and one insertion sort per sub-scanline over a handful of nearly
 * sorted crossings.
 */

import type { EdgeSet } from "./edges";
import { isFiniteNumber } from "./geom";

/**
 * Sub-scanlines sampled per pixel row. A power of two makes the weight `1/16`
 * exact in binary, so a fully covered pixel accumulates exactly 1 — one less
 * source of drift between the two implementations. Identical to Rust.
 */
export const SUB_SCANLINES = 16;

/**
 * Receives each row's coverage: the row index, a reused row buffer of
 * per-pixel coverage in `0..=1`, and the half-open pixel range that is
 * actually non-zero. Rows arrive in ascending order, each at most once.
 */
export type CoverageEmitter = (y: number, row: number[], startX: number, endX: number) => void;

export class ScanlineSupersampler {
	/** Indices into the edge set, for edges the scan has reached but not passed. */
	private readonly active: number[] = [];
	/** Scratch for one sub-scanline's crossings, as parallel arrays. */
	private readonly crossingX: number[] = [];
	private readonly crossingWinding: number[] = [];
	/** One row of accumulated coverage. Reused; cleared over the written range. */
	private readonly row: number[] = [];

	/**
	 * Computes coverage row by row. `evenOdd` selects the fill rule; `false`
	 * is non-zero.
	 */
	rasterize(edges: EdgeSet, evenOdd: boolean, width: number, height: number, emit: CoverageEmitter): void {
		const range = edges.rowRange(height);
		const firstRow = range.first;
		const lastRow = range.last;
		if (width === 0 || firstRow >= lastRow) {
			return;
		}

		const all = edges.edges;
		const edgeCount = all.size();
		const active = this.active;
		const crossingX = this.crossingX;
		const crossingWinding = this.crossingWinding;
		const row = this.row;

		active.clear();
		for (let x = 0; x < width; x++) {
			row[x] = 0;
		}

		// Edges are sorted by `yTop`, so a single cursor advancing with the
		// scan admits each edge exactly once.
		let nextEdge = 0;
		while (nextEdge < edgeCount && all[nextEdge].yTop < firstRow) {
			if (all[nextEdge].yBottom > firstRow) {
				active.push(nextEdge);
			}
			nextEdge += 1;
		}

		const weight = 1 / SUB_SCANLINES;

		for (let y = firstRow; y < lastRow; y++) {
			const rowTop = y;
			const rowBottom = rowTop + 1;

			while (nextEdge < edgeCount && all[nextEdge].yTop < rowBottom) {
				active.push(nextEdge);
				nextEdge += 1;
			}
			// In-place retain of edges still spanning this row.
			let keep = 0;
			const activeCount = active.size();
			for (let index = 0; index < activeCount; index++) {
				const edgeIndex = active[index];
				if (all[edgeIndex].yBottom > rowTop) {
					active[keep] = edgeIndex;
					keep += 1;
				}
			}
			for (let index = active.size() - 1; index >= keep; index--) {
				active.pop();
			}
			if (keep === 0) {
				continue;
			}

			let dirtyStart = width;
			let dirtyEnd = 0;

			for (let sample = 0; sample < SUB_SCANLINES; sample++) {
				const sampleY = rowTop + (sample + 0.5) * weight;

				// Collect crossings for this sub-scanline.
				let crossings = 0;
				for (let index = 0; index < keep; index++) {
					const edge = all[active[index]];
					// Half-open in y, so a vertex shared by two edges is
					// counted once: the edge ending there does not fire, the
					// one starting there does.
					if (sampleY < edge.yTop || sampleY >= edge.yBottom) {
						continue;
					}
					const x = edge.xTop + (sampleY - edge.yTop) * edge.dxDy;
					if (isFiniteNumber(x)) {
						crossingX[crossings] = x;
						crossingWinding[crossings] = edge.winding;
						crossings += 1;
					}
				}
				if (crossings < 2) {
					continue;
				}

				// Insertion sort by x. Stable, allocation-free, and the
				// crossings arrive nearly sorted because the active edges keep
				// their order between adjacent sub-scanlines.
				for (let index = 1; index < crossings; index++) {
					const x = crossingX[index];
					const w = crossingWinding[index];
					let slot = index - 1;
					while (slot >= 0 && crossingX[slot] > x) {
						crossingX[slot + 1] = crossingX[slot];
						crossingWinding[slot + 1] = crossingWinding[slot];
						slot -= 1;
					}
					crossingX[slot + 1] = x;
					crossingWinding[slot + 1] = w;
				}

				// Walk the crossings, accumulating the intervals the fill rule
				// says are inside.
				let winding = 0;
				let spanStart = 0;
				let inside = false;

				for (let index = 0; index < crossings; index++) {
					const x = crossingX[index];
					const wasInside = inside;
					winding += crossingWinding[index];
					if (evenOdd) {
						// Inside on every odd crossing count, whatever the
						// directions were. Luau's `%` is a floored modulo, so
						// this matches Rust's `winding & 1` for negatives too.
						inside = winding % 2 !== 0;
					} else {
						// Non-zero: inside wherever the accumulated winding is
						// not zero.
						inside = winding !== 0;
					}

					if (!wasInside && inside) {
						spanStart = x;
					} else if (wasInside && !inside) {
						// Accumulate the span [spanStart, x): exact in x.
						const clampedStart = math.min(math.max(spanStart, 0), width);
						const clampedEnd = math.min(math.max(x, 0), width);
						if (clampedEnd > clampedStart) {
							const first = math.floor(clampedStart);
							const last = math.floor(clampedEnd);
							const firstIndex = first;
							// `clampedEnd` can land exactly on the right edge,
							// whose floor is one past the last pixel.
							const lastIndex = math.min(last, width);

							if (firstIndex < dirtyStart) {
								dirtyStart = firstIndex;
							}
							const dirtyCandidate = math.min(lastIndex + 1, width);
							if (dirtyCandidate > dirtyEnd) {
								dirtyEnd = dirtyCandidate;
							}

							if (firstIndex === lastIndex) {
								if (firstIndex < width) {
									row[firstIndex] += (clampedEnd - clampedStart) * weight;
								}
							} else {
								if (firstIndex < width) {
									row[firstIndex] += (first + 1 - clampedStart) * weight;
								}
								const interiorEnd = math.min(lastIndex, width);
								for (let px = firstIndex + 1; px < interiorEnd; px++) {
									row[px] += weight;
								}
								if (lastIndex < width) {
									row[lastIndex] += (clampedEnd - last) * weight;
								}
							}
						}
					}
				}
			}

			if (dirtyStart < dirtyEnd) {
				emit(y, row, dirtyStart, dirtyEnd);
				for (let x = dirtyStart; x < dirtyEnd; x++) {
					row[x] = 0;
				}
			}
		}
	}
}
