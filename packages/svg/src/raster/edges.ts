/**
 * Directed edges: the one geometry representation everything is rasterized
 * from. Port of the `EdgeSet` half of `svg-raster/src/edges.rs`.
 *
 * Each non-horizontal segment becomes an edge normalised to point downwards,
 * remembering with `winding` whether it originally did. Horizontal segments
 * are dropped: a scanline never crosses one. Edges are sorted by their top y,
 * which is what lets the scan admit each edge exactly once.
 */

import type { Vec2 } from "./geom";
import { isFiniteNumber, isFiniteVec } from "./geom";

/** A directed, non-horizontal segment, normalised to run downwards. */
export interface Edge {
	/** Smaller y. The edge is live for scanlines in `[yTop, yBottom)`. */
	yTop: number;
	yBottom: number;
	/** x where the edge meets `yTop`. */
	xTop: number;
	/** dx/dy, finite because `yBottom > yTop` by construction. */
	dxDy: number;
	/** `+1` if the segment originally ran downwards, `-1` if upwards. */
	winding: number;
	/**
	 * Insertion order, the sort's tie-break. Rust's `sort_by` is stable and
	 * `table.sort` is not, so determinism has to be carried explicitly.
	 */
	sequence: number;
}

/**
 * A set of edges ready to be scan-converted. Reused across shapes:
 * {@link EdgeSet.clear} keeps the allocation.
 */
export class EdgeSet {
	readonly edges: Edge[] = [];
	private yMin = math.huge;
	private yMax = -math.huge;

	clear(): void {
		this.edges.clear();
		this.yMin = math.huge;
		this.yMax = -math.huge;
	}

	isEmpty(): boolean {
		return this.edges.size() === 0;
	}

	/**
	 * Adds a polygon, closing it implicitly. Implicit closure is part of the
	 * specification: SVG fills a subpath as if it were closed whether or not
	 * the author wrote `Z`, and stroke outlines are closed by construction.
	 */
	addPolygon(points: Vec2[]): void {
		const count = points.size();
		if (count < 2) {
			return;
		}
		for (let index = 0; index < count; index++) {
			this.addSegment(points[index], points[(index + 1) % count]);
		}
	}

	private addSegment(a: Vec2, b: Vec2): void {
		// Non-finite input cannot reach here through `flatten`, which rejects
		// it, but the stroker also produces points and a guard here is cheaper
		// than an invariant spread across two modules.
		if (!(isFiniteVec(a) && isFiniteVec(b)) || a.y === b.y) {
			return;
		}

		let top = a;
		let bottom = b;
		let winding = 1;
		if (a.y >= b.y) {
			top = b;
			bottom = a;
			winding = -1;
		}
		const dy = bottom.y - top.y;
		const dxDy = (bottom.x - top.x) / dy;
		if (!isFiniteNumber(dxDy)) {
			return;
		}

		this.yMin = math.min(this.yMin, top.y);
		this.yMax = math.max(this.yMax, bottom.y);
		this.edges.push({
			yTop: top.y,
			yBottom: bottom.y,
			xTop: top.x,
			dxDy,
			winding,
			sequence: this.edges.size(),
		});
	}

	/** Sorts edges by their top y, which the active edge table relies on. */
	finish(): void {
		this.edges.sort((a, b) => {
			if (a.yTop !== b.yTop) {
				return a.yTop < b.yTop;
			}
			return a.sequence < b.sequence;
		});
	}

	/**
	 * The rows this edge set can possibly touch, clipped to `height`, as a
	 * half-open `[first, last)` pair. Everything outside contributes nothing,
	 * and skipping it is what keeps a small icon in a large raster cheap.
	 */
	rowRange(height: number): { first: number; last: number } {
		if (this.edges.size() === 0) {
			return { first: 0, last: 0 };
		}
		const first = math.floor(math.max(this.yMin, 0));
		const last = math.ceil(math.min(this.yMax, height));
		if (!(isFiniteNumber(first) && isFiniteNumber(last)) || last <= first) {
			return { first: 0, last: 0 };
		}
		return { first, last: math.min(last, height) };
	}
}
