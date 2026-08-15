/**
 * The generator, end to end.
 *
 * ```text
 *      lucide-static (pinned)
 *              │
 *        classify names          upstream.ts   naming.ts
 *              │
 *      compile ONCE, strict      compile.ts
 *              │
 *          manifest              manifest.ts
 *              │
 *        render tree             emit.ts
 *         ╱          ╲
 *  lucide-react    lucide-vide   output.ts
 * ```
 *
 * The single compile pass in the middle is the load-bearing part. Both packages
 * are rendered from the same `CompiledLucideIcon` values, so their vector data
 * cannot differ — not because two outputs are compared afterwards, but because
 * there is only one.
 */

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { compileIconSet, describeFailures, type CompiledLucideIcon } from "./compile.js";
import { renderGeneratedTree, type GeneratedFile } from "./emit.js";
import { assignExportNames } from "./naming.js";
import { buildManifest, renderManifest, type LucideManifest } from "./manifest.js";
import {
	checkGeneratedTree,
	isClean,
	writeFileIfChanged,
	writeGeneratedTree,
	type CheckReport,
	type WriteReport,
} from "./output.js";
import { collectStats, describeStats, type IconSetStats } from "./stats.js";
import { classifyIcons, readUpstream, type Upstream } from "./upstream.js";

/** A package this generator writes into. */
export interface LucideTarget {
	/** Published npm name, for messages. */
	readonly packageName: string;
	/** Absolute path of the package directory. */
	readonly packageDir: string;
}

/**
 * The directory each package's generated tree owns completely.
 *
 * Everything under it is written, compared and deleted by this generator.
 * Everything outside it — `createLucideIcon.tsx`, `index.ts`, `package.json`,
 * the README, the licences — is hand-written and never touched.
 */
export const OWNED_DIR = "src/generated";

/** Where upstream's licence is redistributed inside each package. */
export const UPSTREAM_LICENSE_FILE = "LICENSE-lucide";

export interface GenerateOptions {
	readonly repoRoot: string;
	/** Compare against what is on disk instead of writing. */
	readonly check?: boolean;
	/**
	 * The upstream package to read. Defaults to the pinned `lucide-static`.
	 *
	 * Overridable so the test suite can drive the whole pipeline over a
	 * synthetic two-icon set — which is the only practical way to test that
	 * removing an icon upstream removes it from both packages, without
	 * downgrading the real dependency.
	 */
	readonly upstream?: Upstream;
	/** The packages to write into. Defaults to the two real ones. */
	readonly targets?: readonly LucideTarget[];
	/** Where the manifest is written. Defaults to `tools/lucide/manifest.json`. */
	readonly manifestFile?: string;
}

export interface GenerateResult {
	readonly upstream: Upstream;
	readonly manifest: LucideManifest;
	readonly compiled: readonly CompiledLucideIcon[];
	readonly stats: IconSetStats;
	readonly files: readonly GeneratedFile[];
	readonly targets: readonly LucideTarget[];
	/** Per-target write reports. Empty in `--check` mode. */
	readonly writes: readonly (readonly [LucideTarget, WriteReport])[];
	/** Per-target check reports. Empty in write mode. */
	readonly checks: readonly (readonly [LucideTarget, CheckReport])[];
	/** True in `--check` mode when everything on disk is current. */
	readonly clean: boolean;
	/** Human-readable summary of what a check pass found. */
	readonly staleSummary: string;
}

/** The two packages, in a fixed order. */
export function lucideTargets(repoRoot: string): LucideTarget[] {
	return [
		{
			packageName: "@rbxts/lucide-react",
			packageDir: join(repoRoot, "packages/lucide-react"),
		},
		{
			packageName: "@rbxts/lucide-vide",
			packageDir: join(repoRoot, "packages/lucide-vide"),
		},
	];
}

/** Where the committed manifest lives. Not published — see `manifest.ts`. */
export function manifestPath(repoRoot: string): string {
	return join(repoRoot, "tools/lucide/manifest.json");
}

/**
 * Runs the whole pipeline.
 *
 * Throws — with every failing icon named — if any canonical icon does not
 * compile. Generation succeeding while an icon is missing would be worse than
 * failing: the package would simply not have it, and nothing would say so.
 */
export function generateLucide(options: GenerateOptions): GenerateResult {
	const upstream = options.upstream ?? readUpstream();
	const inventory = classifyIcons(upstream);
	const names = assignExportNames(inventory.icons);

	const { icons: compiled, failures } = compileIconSet(inventory.canonical);
	if (failures.length > 0) {
		throw new Error(describeFailures(failures, inventory.canonical.length));
	}
	if (compiled.length !== inventory.canonical.length) {
		throw new Error(
			`Lucide generation: compiled ${compiled.length} icons but discovered ${inventory.canonical.length}. ` +
				"Every discovered icon must be accounted for.",
		);
	}

	const manifest = buildManifest({
		upstreamName: "lucide-static",
		version: upstream.version,
		license: upstream.license,
		names,
		compiled,
	});
	const stats = collectStats(manifest, compiled, failures.length);

	// Rendered once and written twice. See `emit.ts` — this is what makes the
	// two packages' generated trees identical rather than merely equivalent.
	const files = renderGeneratedTree(manifest, compiled);
	const targets = options.targets ?? lucideTargets(options.repoRoot);
	const manifestFile = options.manifestFile ?? manifestPath(options.repoRoot);

	const writes: (readonly [LucideTarget, WriteReport])[] = [];
	const checks: (readonly [LucideTarget, CheckReport])[] = [];

	if (options.check === true) {
		for (const target of targets) {
			checks.push([target, checkGeneratedTree(target.packageDir, OWNED_DIR, files)]);
		}
	} else {
		for (const target of targets) {
			writes.push([target, writeGeneratedTree(target.packageDir, OWNED_DIR, files)]);
			// Upstream's licence travels with the icons it covers. Copied rather
			// than referenced, because a published tarball has no repository to
			// point at.
			writeFileIfChanged(
				join(target.packageDir, UPSTREAM_LICENSE_FILE),
				upstream.licenseText,
			);
		}
		writeFileIfChanged(manifestFile, renderManifest(manifest));
	}

	const manifestStale =
		options.check === true &&
		readIfExists(manifestFile) !== renderManifest(manifest);
	const licenseStale =
		options.check === true &&
		targets.some(
			(target) =>
				readIfExists(join(target.packageDir, UPSTREAM_LICENSE_FILE)) !==
				upstream.licenseText,
		);

	const clean =
		options.check !== true ||
		(checks.every(([, report]) => isClean(report)) && !manifestStale && !licenseStale);

	return {
		upstream,
		manifest,
		compiled,
		stats,
		files,
		targets,
		writes,
		checks,
		clean,
		staleSummary: describeStale(checks, manifestStale, licenseStale),
	};
}

function readIfExists(path: string): string | undefined {
	return existsSync(path) ? readFileSync(path, "utf8") : undefined;
}

function describeStale(
	checks: readonly (readonly [LucideTarget, CheckReport])[],
	manifestStale: boolean,
	licenseStale: boolean,
): string {
	const lines: string[] = [];
	for (const [target, report] of checks) {
		if (isClean(report)) {
			continue;
		}
		lines.push(`${target.packageName}:`);
		for (const path of report.missing) {
			lines.push(`  missing  ${path}`);
		}
		for (const path of report.changed) {
			lines.push(`  changed  ${path}`);
		}
		for (const path of report.extra) {
			lines.push(`  stale    ${path}`);
		}
	}
	if (manifestStale) {
		lines.push("tools/lucide/manifest.json is out of date");
	}
	if (licenseStale) {
		lines.push(`${UPSTREAM_LICENSE_FILE} is out of date in at least one package`);
	}
	return lines.join("\n");
}

export { describeStats };
