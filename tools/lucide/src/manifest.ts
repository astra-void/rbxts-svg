/**
 * The manifest: one deterministic description of the icon set, shared by
 * everything downstream.
 *
 * ```text
 *                     manifest
 *                        │
 *      ┌──────────┬──────┴──────┬────────────────┐
 *      ▼          ▼             ▼                ▼
 *  React modules  Vide modules  stale-file       tests
 *                               detection
 * ```
 *
 * Both framework outputs are generated from *this*, not from two independent
 * walks of the icons directory. That is what makes "the React package and the
 * Vide package contain the same icons under the same names" true by
 * construction rather than by a test that happens to pass.
 *
 * It is committed, so a CI check can compare it against a fresh run and notice
 * that upstream moved. It is not published: consumers have components, and a
 * manifest of two thousand hashes in a published tarball would be dead weight.
 */

import type { CompiledLucideIcon } from "./compile.js";
import type { NameAssignment } from "./naming.js";

/** One icon in the manifest — canonical or alias. */
export interface LucideManifestEntry {
	/** Upstream file base name. */
	readonly sourceName: string;
	/** The exported component identifier. */
	readonly exportName: string;
	/** Upstream file, relative to the `lucide-static` package root. */
	readonly sourceFile: string;
	/** Content hash of the compiled IR — the canonical icon's, for an alias. */
	readonly hash: string;
	/** Serialized IR length in bytes — the canonical icon's, for an alias. */
	readonly byteLength: number;
	/** For an alias, the canonical source name it re-exports. */
	readonly aliasOf?: string;
	/**
	 * Set when this alias's export name is the same as its canonical target's,
	 * so the root barrel exports the name once. The module still exists.
	 */
	readonly subsumed?: boolean;
}

export interface LucideManifest {
	/** The pinned upstream package, e.g. `"lucide-static"`. */
	readonly upstream: string;
	/** Its exact version. Changing this is what an upstream bump *is*. */
	readonly version: string;
	/** Upstream's SPDX license id. */
	readonly license: string;
	/** Icons with their own artwork. */
	readonly canonicalCount: number;
	/** Icons that are another icon's name. */
	readonly aliasCount: number;
	/** Distinct names the root barrel exports. */
	readonly exportCount: number;
	/** Every icon, canonical and alias, sorted by `sourceName`. */
	readonly icons: readonly LucideManifestEntry[];
}

export interface BuildManifestInput {
	readonly upstreamName: string;
	readonly version: string;
	readonly license: string;
	readonly names: readonly NameAssignment[];
	readonly compiled: readonly CompiledLucideIcon[];
}

/**
 * Assembles the manifest from the name assignments and the one compile pass.
 *
 * Sorted by `sourceName` throughout, never by filesystem order, so two machines
 * — and two runs a year apart — produce byte-identical output.
 */
export function buildManifest(input: BuildManifestInput): LucideManifest {
	const compiledByName = new Map(input.compiled.map((icon) => [icon.sourceName, icon]));

	const icons = [...input.names]
		.sort((a, b) => (a.sourceName < b.sourceName ? -1 : a.sourceName > b.sourceName ? 1 : 0))
		.map((name): LucideManifestEntry => {
			// An alias carries its target's hash and size because it *is* its
			// target: the same asset, reached by a second name. Recording the
			// alias as having no bytes of its own would misreport the set;
			// recording it as having a copy would imply a duplication that the
			// generated module deliberately does not create.
			const artworkName = name.aliasOf ?? name.sourceName;
			const compiled = compiledByName.get(artworkName);
			if (compiled === undefined) {
				throw new Error(
					`Lucide manifest: "${name.sourceName}" resolves to artwork "${artworkName}", which was never compiled.\n` +
						"This means the alias classification and the compile pass disagree about the icon set.",
				);
			}
			return {
				sourceName: name.sourceName,
				exportName: name.exportName,
				sourceFile: `icons/${name.sourceName}.svg`,
				hash: compiled.hash,
				byteLength: compiled.byteLength,
				...(name.aliasOf === undefined ? {} : { aliasOf: name.aliasOf }),
				...(name.subsumed ? { subsumed: true } : {}),
			};
		});

	return {
		upstream: input.upstreamName,
		version: input.version,
		license: input.license,
		canonicalCount: icons.filter((icon) => icon.aliasOf === undefined).length,
		aliasCount: icons.filter((icon) => icon.aliasOf !== undefined).length,
		exportCount: icons.filter((icon) => icon.subsumed !== true).length,
		icons,
	};
}

/** The manifest as committed: pretty-printed JSON with a trailing newline. */
export function renderManifest(manifest: LucideManifest): string {
	return `${JSON.stringify(manifest, undefined, "\t")}\n`;
}
