/**
 * What the icon set turned out to be.
 *
 * Two thousand real icons are the largest compatibility workload this
 * repository has, and a run that only says "done" wastes it. These numbers are
 * the evidence behind every claim the packages make: that every icon compiles,
 * that every icon is a tintable alpha mask, that the IR is version 2 and stayed
 * that way, and roughly what all of it costs.
 *
 * Nothing here is asserted in advance. Tintability in particular is *measured*:
 * "Lucide is monochrome `currentColor`" is a statement about upstream's
 * artwork, and the day it stops being true is a day the packages should say so
 * loudly rather than quietly ship full-colour rasters.
 */

import { SvgFeatureFlags } from "@rbxts/svg-compiler";

import type { CompiledLucideIcon } from "./compile.js";
import type { LucideManifest } from "./manifest.js";

export interface IconSetStats {
	readonly discovered: number;
	readonly canonical: number;
	readonly aliases: number;
	readonly exports: number;
	readonly compiled: number;
	readonly failures: number;
	readonly tintable: number;
	readonly notTintable: readonly string[];
	readonly uniqueHashes: number;
	readonly duplicateHashGroups: readonly (readonly string[])[];
	readonly viewBoxes: readonly string[];
	readonly preserveAspectRatios: readonly string[];
	readonly irVersions: readonly number[];
	readonly flagCounts: readonly (readonly [number, number])[];
	readonly maxShapeCount: number;
	readonly maxShapeIcon: string;
	readonly totalIrBytes: number;
	readonly meanIrBytes: number;
	readonly medianIrBytes: number;
	readonly minIrBytes: number;
	readonly maxIrBytes: number;
	readonly largestByIr: readonly (readonly [string, number])[];
}

/**
 * The bits that make an asset a tintable alpha mask.
 *
 * Monochrome *and* `currentColor`: one colour everywhere, and that colour
 * decided at render time. Such an asset rasterizes once and is recoloured by
 * `ImageColor3` for free, which is the fast path the whole cache design exists
 * to serve — and the reason Lucide is a good fit for it.
 */
const TINTABLE = SvgFeatureFlags.Monochrome | SvgFeatureFlags.UsesCurrentColor;

const FLAG_NAMES: readonly (readonly [number, string])[] = [
	[SvgFeatureFlags.UsesCurrentColor, "UsesCurrentColor"],
	[SvgFeatureFlags.HasFill, "HasFill"],
	[SvgFeatureFlags.HasStroke, "HasStroke"],
	[SvgFeatureFlags.HasEvenOddFill, "HasEvenOddFill"],
	[SvgFeatureFlags.Monochrome, "Monochrome"],
	[SvgFeatureFlags.HasTransparency, "HasTransparency"],
	[SvgFeatureFlags.HasStrokeFirst, "HasStrokeFirst"],
];

/** Renders a feature bitset as the names it is made of. */
export function describeFlags(flags: number): string {
	const names = FLAG_NAMES.filter(([bit]) => (flags & bit) !== 0).map(([, name]) => name);
	return names.length === 0 ? "(none)" : names.join(" | ");
}

export function collectStats(
	manifest: LucideManifest,
	compiled: readonly CompiledLucideIcon[],
	failures: number,
): IconSetStats {
	const bytes = compiled.map((icon) => icon.byteLength).sort((a, b) => a - b);
	const total = bytes.reduce((sum, value) => sum + value, 0);

	const byHash = new Map<string, string[]>();
	for (const icon of compiled) {
		const group = byHash.get(icon.hash);
		if (group === undefined) {
			byHash.set(icon.hash, [icon.sourceName]);
		} else {
			group.push(icon.sourceName);
		}
	}

	const flagCounts = new Map<number, number>();
	for (const icon of compiled) {
		flagCounts.set(icon.flags, (flagCounts.get(icon.flags) ?? 0) + 1);
	}

	const bySize = [...compiled].sort((a, b) => b.byteLength - a.byteLength);
	const byShapes = [...compiled].sort((a, b) => b.shapeCount - a.shapeCount);

	return {
		discovered: manifest.icons.length,
		canonical: manifest.canonicalCount,
		aliases: manifest.aliasCount,
		exports: manifest.exportCount,
		compiled: compiled.length,
		failures,
		tintable: compiled.filter((icon) => (icon.flags & TINTABLE) === TINTABLE).length,
		notTintable: compiled
			.filter((icon) => (icon.flags & TINTABLE) !== TINTABLE)
			.map((icon) => icon.sourceName),
		uniqueHashes: byHash.size,
		duplicateHashGroups: [...byHash.values()]
			.filter((group) => group.length > 1)
			.map((group) => [...group].sort())
			.sort((a, b) => (a[0] ?? "").localeCompare(b[0] ?? "")),
		viewBoxes: [
			...new Set(
				compiled.map(
					(icon) =>
						`${icon.viewBox.x} ${icon.viewBox.y} ${icon.viewBox.width} ${icon.viewBox.height}`,
				),
			),
		].sort(),
		preserveAspectRatios: [...new Set(compiled.map((icon) => icon.preserveAspectRatio))].sort(),
		irVersions: [...new Set(compiled.map((icon) => icon.irVersion))].sort((a, b) => a - b),
		flagCounts: [...flagCounts].sort((a, b) => b[1] - a[1]),
		maxShapeCount: byShapes[0]?.shapeCount ?? 0,
		maxShapeIcon: byShapes[0]?.sourceName ?? "",
		totalIrBytes: total,
		meanIrBytes: bytes.length === 0 ? 0 : total / bytes.length,
		medianIrBytes: bytes[Math.floor(bytes.length / 2)] ?? 0,
		minIrBytes: bytes[0] ?? 0,
		maxIrBytes: bytes[bytes.length - 1] ?? 0,
		largestByIr: bySize.slice(0, 10).map((icon) => [icon.sourceName, icon.byteLength] as const),
	};
}

/** The report a generation run prints. */
export function describeStats(manifest: LucideManifest, stats: IconSetStats): string {
	const lines = [
		`upstream                 ${manifest.upstream} ${manifest.version} (${manifest.license})`,
		`svg files discovered     ${stats.discovered}`,
		`  canonical icons        ${stats.canonical}`,
		`  alias names            ${stats.aliases}`,
		`barrel exports           ${stats.exports}`,
		`compiled                 ${stats.compiled}`,
		`compile failures         ${stats.failures}`,
		`tintable                 ${stats.tintable} / ${stats.compiled}`,
		`non-tintable             ${stats.notTintable.length}`,
	];
	if (stats.notTintable.length > 0) {
		lines.push(
			"",
			"!! Non-tintable Lucide icons — this is not expected. Either upstream's artwork",
			"!! changed, the compiler lost `currentColor`, or the tintability detector is",
			"!! wrong. Do not ship full-colour Lucide assets without understanding why:",
			...stats.notTintable.map((name) => `!!   ${name}`),
			"",
		);
	}
	lines.push(
		`unique compiled hashes   ${stats.uniqueHashes}`,
		`duplicate hash groups    ${stats.duplicateHashGroups.length}${
			stats.duplicateHashGroups.length === 0
				? ""
				: ` (${stats.duplicateHashGroups.map((group) => group.join("=")).join(", ")})`
		}`,
		`unique view boxes        ${stats.viewBoxes.join(" | ")}`,
		`preserveAspectRatio      ${stats.preserveAspectRatios.join(" | ")}`,
		`ir versions              ${stats.irVersions.join(", ")}`,
		"feature flags observed",
		...stats.flagCounts.map(
			([flags, count]) => `  ${String(flags).padStart(4)} × ${String(count).padEnd(5)} ${describeFlags(flags)}`,
		),
		`max shape count          ${stats.maxShapeCount} (${stats.maxShapeIcon})`,
		`serialized ir bytes      total ${stats.totalIrBytes}  mean ${stats.meanIrBytes.toFixed(1)}  median ${stats.medianIrBytes}  min ${stats.minIrBytes}  max ${stats.maxIrBytes}`,
		"largest icons by ir",
		...stats.largestByIr.map(([name, size]) => `  ${String(size).padStart(5)}  ${name}`),
	);
	return lines.join("\n");
}
