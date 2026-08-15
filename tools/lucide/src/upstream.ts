/**
 * Everything this repository knows about the *upstream* Lucide package.
 *
 * Confined to one module on purpose. Below this line, "Lucide" means a
 * directory of SVG files with names; above it, an icon set is a list of
 * `UpstreamIcon`s and nothing depends on how `lucide-static` chooses to lay
 * itself out. When upstream reorganizes — and it has, repeatedly — this is the
 * file that changes.
 *
 * # What `lucide-static@1.31.0` actually ships
 *
 * ```text
 * lucide-static/
 * ├── icons/            2025 .svg files — canonical icons *and* alias names
 * ├── icon-nodes.json   1767 keys — the canonical set, and only the canonical set
 * ├── tags.json         1767 keys — search keywords, same key set
 * ├── LICENSE           ISC
 * └── dist/, font/, sprite.svg   (not used here)
 * ```
 *
 * The `icons/` directory does not distinguish an alias from a canonical icon;
 * `icon-nodes.json` is what does. An alias file is a byte-for-byte copy of its
 * canonical icon apart from the `class="lucide lucide-<name>"` attribute, which
 * is what {@link classifyIcons} exploits to find each alias's target without
 * needing a mapping upstream does not ship.
 *
 * # What it does *not* ship
 *
 * A deprecation flag. The upstream monorepo marks some aliases deprecated in
 * per-icon `.json` metadata that `lucide-static` does not include, so from the
 * pinned package alone a live alias and a deprecated one are indistinguishable.
 * The policy that follows from that is in `docs/LUCIDE.md`: ship every name
 * upstream ships, because an alias costs no compiled bytes here — it is a
 * re-export of the canonical module.
 */

import { createHash } from "node:crypto";
import { readFileSync, readdirSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";

/** Where the pinned `lucide-static` is installed. */
export function resolveUpstreamRoot(): string {
	// Resolved through Node rather than assembled from a hardcoded path, so the
	// generator finds the copy pnpm actually linked for it.
	return dirname(require.resolve("lucide-static/package.json"));
}

export interface Upstream {
	/** Absolute path of the installed `lucide-static`. */
	readonly root: string;
	/** The exact pinned version, e.g. `"1.31.0"`. */
	readonly version: string;
	/** Upstream's SPDX license id, read from its manifest. */
	readonly license: string;
	/** Full text of upstream's `LICENSE`, for redistribution. */
	readonly licenseText: string;
	/** Absolute path of the `icons/` directory. */
	readonly iconsDir: string;
}

/** Reads the pinned upstream package's identity and license. */
export function readUpstream(): Upstream {
	const root = resolveUpstreamRoot();
	const manifest = JSON.parse(readFileSync(join(root, "package.json"), "utf8")) as {
		version: string;
		license: string;
	};
	const licensePath = join(root, "LICENSE");
	if (!existsSync(licensePath)) {
		throw new Error(
			`lucide-static@${manifest.version} ships no LICENSE file at ${licensePath}.\n` +
				"Redistributing the icons without it is not an option; investigate the upstream package before pinning it.",
		);
	}
	const iconsDir = join(root, "icons");
	if (!existsSync(iconsDir)) {
		throw new Error(
			`lucide-static@${manifest.version} has no icons/ directory at ${iconsDir}.\n` +
				"The upstream layout has changed; tools/lucide/src/upstream.ts is the file to update.",
		);
	}
	return {
		root,
		version: manifest.version,
		license: manifest.license,
		licenseText: readFileSync(licensePath, "utf8"),
		iconsDir,
	};
}

/** One `.svg` in upstream's `icons/`, classified. */
export interface UpstreamIcon {
	/** The upstream file's base name, e.g. `"circle-alert"`. */
	readonly sourceName: string;
	/** Absolute path of the `.svg`. */
	readonly sourceFile: string;
	/** Path as it should be *reported*: always `icons/<name>.svg`, never absolute. */
	readonly sourceLabel: string;
	/**
	 * The canonical icon this name refers to, or `undefined` when this *is* a
	 * canonical icon.
	 *
	 * An alias is not a second icon. It is a second name, and it compiles to
	 * the same bytes — so the generated module for one is a re-export of the
	 * other rather than a second copy of the artwork.
	 */
	readonly aliasOf: string | undefined;
}

export interface IconInventory {
	readonly icons: readonly UpstreamIcon[];
	/** Icons with their own artwork. Only these are ever compiled. */
	readonly canonical: readonly UpstreamIcon[];
	/** Icons that are another icon's name. */
	readonly aliases: readonly UpstreamIcon[];
}

/**
 * Lists upstream's icons and works out which are aliases of which.
 *
 * Canonical membership comes from `icon-nodes.json`, which is upstream's own
 * statement of what the icon set is. The alias *target* is then recovered by
 * content: every alias file is its canonical icon's file with a different
 * `class` attribute, so stripping that attribute and hashing groups each alias
 * with exactly one canonical name.
 *
 * That recovery is checked rather than assumed. An alias whose body matches no
 * canonical icon, or more than one, is a fatal error — the alternative is
 * silently emitting a duplicate copy of some icon under a name that upstream
 * meant to point somewhere else.
 */
export function classifyIcons(upstream: Upstream): IconInventory {
	const canonicalNames = new Set(
		Object.keys(
			JSON.parse(readFileSync(join(upstream.root, "icon-nodes.json"), "utf8")) as Record<
				string,
				unknown
			>,
		),
	);

	// Sorted, so nothing downstream inherits the filesystem's ordering.
	const files = readdirSync(upstream.iconsDir)
		.filter((name) => name.endsWith(".svg"))
		.map((name) => name.slice(0, -".svg".length))
		.sort();

	const bodyHash = (name: string): string =>
		createHash("sha256")
			.update(readFileSync(join(upstream.iconsDir, `${name}.svg`), "utf8").replace(CLASS_ATTRIBUTE, ""))
			.digest("hex");

	const canonicalByBody = new Map<string, string[]>();
	for (const name of files) {
		if (!canonicalNames.has(name)) {
			continue;
		}
		const key = bodyHash(name);
		const group = canonicalByBody.get(key);
		if (group === undefined) {
			canonicalByBody.set(key, [name]);
		} else {
			group.push(name);
		}
	}

	const icons: UpstreamIcon[] = [];
	const unresolved: string[] = [];
	for (const sourceName of files) {
		const isCanonical = canonicalNames.has(sourceName);
		let aliasOf: string | undefined;
		if (!isCanonical) {
			const group = canonicalByBody.get(bodyHash(sourceName)) ?? [];
			if (group.length !== 1) {
				unresolved.push(
					`  ${sourceName}.svg — matches ${group.length} canonical icon(s)${
						group.length === 0 ? "" : `: ${group.join(", ")}`
					}`,
				);
				continue;
			}
			aliasOf = group[0];
		}
		icons.push({
			sourceName,
			sourceFile: join(upstream.iconsDir, `${sourceName}.svg`),
			sourceLabel: `icons/${sourceName}.svg`,
			aliasOf,
		});
	}

	if (unresolved.length > 0) {
		throw new Error(
			`Could not resolve ${unresolved.length} Lucide alias name(s) to a canonical icon ` +
				`in lucide-static ${upstream.version}:\n\n${unresolved.join("\n")}\n\n` +
				"An alias is expected to be its canonical icon's file with a different `class`. " +
				"If upstream has changed that, tools/lucide/src/upstream.ts needs updating — " +
				"guessing here would publish an icon under the wrong name.",
		);
	}

	// Missing the other way round: a canonical name with no file at all.
	const present = new Set(icons.map((icon) => icon.sourceName));
	const missing = [...canonicalNames].filter((name) => !present.has(name)).sort();
	if (missing.length > 0) {
		throw new Error(
			`lucide-static ${upstream.version} lists ${missing.length} canonical icon(s) in ` +
				`icon-nodes.json with no icons/<name>.svg:\n\n  ${missing.join("\n  ")}`,
		);
	}

	return {
		icons,
		canonical: icons.filter((icon) => icon.aliasOf === undefined),
		aliases: icons.filter((icon) => icon.aliasOf !== undefined),
	};
}

/**
 * The one attribute that differs between an alias file and its canonical.
 *
 * `class="lucide lucide-alert-circle"` versus `class="lucide lucide-circle-alert"`.
 * It carries no rendering meaning for this pipeline — the compiler ignores it —
 * so removing it before hashing is what makes two spellings of one icon compare
 * equal.
 */
const CLASS_ATTRIBUTE = /\s*class="[^"]*"/;
