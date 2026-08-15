/**
 * `search.svg` → `Search`, and the rules that make that safe two thousand times.
 *
 * The conversion itself is three lines. What this module is really for is
 * refusing to guess: an icon set this large will eventually contain two names
 * that want the same identifier, and the failure mode of resolving that
 * silently is an icon that quietly stops existing when upstream adds a name
 * next door to it.
 */

/** One source name and the identifier it wants. */
export interface NamedIcon {
	readonly sourceName: string;
	/** `undefined` for a canonical icon; the canonical source name for an alias. */
	readonly aliasOf: string | undefined;
}

/**
 * Converts an upstream file name to an exported component name.
 *
 * ```text
 * search.svg          → Search
 * chevron-down.svg    → ChevronDown
 * circle-alert.svg    → CircleAlert
 * a-arrow-down.svg    → AArrowDown
 * arrow-down-0-1.svg  → ArrowDown01
 * axis-3-d.svg        → Axis3D
 * bar-chart-2.svg     → BarChart2
 * ```
 *
 * Only the first character of each hyphen-separated segment is touched. Nothing
 * is lowercased, no acronym is special-cased, and no digit is moved: upstream's
 * segmentation *is* the word boundary information, and inventing more of it is
 * how `axis-3-d` and `axis-3d` would stop being distinguishable.
 *
 * The pinned set needs nothing else. Every one of its 2025 names matches
 * `[a-z][a-z0-9]*(-[a-z0-9]+)*`, none begins with a digit, and no PascalCase
 * result is a reserved word — the shape is verified by
 * {@link assertConvertibleName} rather than assumed, because the next upstream
 * release is not bound by the last one.
 */
export function toExportName(sourceName: string): string {
	assertConvertibleName(sourceName);
	return sourceName
		.split("-")
		.map((segment) => segment.charAt(0).toUpperCase() + segment.slice(1))
		.join("");
}

/** The shape every upstream icon name has had, and must keep having. */
const KEBAB_CASE = /^[a-z][a-z0-9]*(-[a-z0-9]+)*$/;

/**
 * Rejects a name the conversion above cannot honestly handle.
 *
 * A leading digit is the interesting case: `3d-view` would become `3dView`,
 * which is not an identifier at all. Rather than silently prefixing something,
 * generation stops and says so — the fix is a decision for a human, once.
 */
export function assertConvertibleName(sourceName: string): void {
	if (!KEBAB_CASE.test(sourceName)) {
		throw new Error(
			`Lucide icon name "${sourceName}" is not lower-kebab-case.\n` +
				"tools/lucide/src/naming.ts converts names by capitalizing each hyphen-separated " +
				"segment, which is only meaningful for that shape. Decide what this name should " +
				"export as before regenerating.",
		);
	}
}

/** A source name paired with the identifier it will be exported as. */
export interface NameAssignment {
	readonly sourceName: string;
	readonly exportName: string;
	readonly aliasOf: string | undefined;
	/**
	 * True when this alias's export name is already taken by the very icon it
	 * aliases, so the barrel exports the name once rather than twice.
	 *
	 * Upstream does this deliberately: `arrow-down-01` is the old spelling of
	 * `arrow-down-0-1`, and both spell `ArrowDown01`. The alias still gets its
	 * own module, so `@rbxts/lucide-react/icons/arrow-down-01` resolves, but
	 * the root barrel names `ArrowDown01` exactly once.
	 */
	readonly subsumed: boolean;
}

/**
 * Assigns every icon its export name, failing on any genuine collision.
 *
 * The one collision that is *not* genuine is an alias colliding with its own
 * target: two spellings of one file name that converge on one identifier and
 * refer to one icon. Nothing is lost by exporting it once, and it is upstream's
 * intent — the alias exists because the name was rewritten.
 *
 * Everything else is fatal, and the error names both sides. Two different icons
 * cannot share an identifier, and picking a winner automatically would mean one
 * of them silently disappearing from the package on some future upstream bump.
 */
export function assignExportNames(icons: readonly NamedIcon[]): NameAssignment[] {
	const byExportName = new Map<string, NamedIcon[]>();
	for (const icon of icons) {
		const exportName = toExportName(icon.sourceName);
		const group = byExportName.get(exportName);
		if (group === undefined) {
			byExportName.set(exportName, [icon]);
		} else {
			group.push(icon);
		}
	}

	const subsumed = new Set<string>();
	const conflicts: string[] = [];
	for (const [exportName, group] of byExportName) {
		if (group.length === 1) {
			continue;
		}
		const canonical = group.filter((icon) => icon.aliasOf === undefined);
		const aliases = group.filter((icon) => icon.aliasOf !== undefined);
		const benign =
			canonical.length === 1 &&
			aliases.every((alias) => alias.aliasOf === canonical[0]?.sourceName);
		if (benign) {
			for (const alias of aliases) {
				subsumed.add(alias.sourceName);
			}
			continue;
		}
		conflicts.push(
			`  ${exportName} ← ${group
				.map((icon) =>
					icon.aliasOf === undefined
						? `${icon.sourceName} (canonical)`
						: `${icon.sourceName} (alias of ${icon.aliasOf})`,
				)
				.join(", ")}`,
		);
	}

	if (conflicts.length > 0) {
		throw new Error(
			`${conflicts.length} Lucide name collision(s) — two different icons want one exported name:\n\n` +
				`${conflicts.sort().join("\n")}\n\n` +
				"Generation will not pick a winner: whichever lost would vanish from the package " +
				"without anything failing. Decide the mapping in tools/lucide/src/naming.ts.",
		);
	}

	return icons
		.map((icon) => ({
			sourceName: icon.sourceName,
			exportName: toExportName(icon.sourceName),
			aliasOf: icon.aliasOf,
			subsumed: subsumed.has(icon.sourceName),
		}))
		.sort((a, b) => (a.sourceName < b.sourceName ? -1 : a.sourceName > b.sourceName ? 1 : 0));
}
