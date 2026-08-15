/**
 * The Lucide generator's programmatic surface.
 *
 * Exported so the test suite can drive the pipeline directly — over a synthetic
 * icon directory for the naming and stale-file cases, and over the real pinned
 * set for the integrity and parity ones — rather than shelling out to the CLI
 * and parsing its output.
 *
 * Nothing here is published. This package produces `@rbxts/lucide-react` and
 * `@rbxts/lucide-vide`; it is not one of them, and no consumer ever installs it.
 */

export { compileIconSet, describeFailures } from "./compile.js";
export type { CompileFailure, CompileReport, CompiledLucideIcon } from "./compile.js";
export {
	GENERATED_MARKER,
	renderAliasModule,
	renderBarrel,
	renderGeneratedTree,
	renderIconModule,
} from "./emit.js";
export type { GeneratedFile } from "./emit.js";
export {
	OWNED_DIR,
	UPSTREAM_LICENSE_FILE,
	describeStats,
	generateLucide,
	lucideTargets,
	manifestPath,
} from "./generate.js";
export type { GenerateOptions, GenerateResult, LucideTarget } from "./generate.js";
export { buildManifest, renderManifest } from "./manifest.js";
export type { LucideManifest, LucideManifestEntry } from "./manifest.js";
export { assertConvertibleName, assignExportNames, toExportName } from "./naming.js";
export type { NameAssignment, NamedIcon } from "./naming.js";
export {
	checkGeneratedTree,
	isClean,
	writeFileIfChanged,
	writeGeneratedTree,
} from "./output.js";
export type { CheckReport, WriteReport } from "./output.js";
export { collectStats, describeFlags } from "./stats.js";
export type { IconSetStats } from "./stats.js";
export { classifyIcons, readUpstream, resolveUpstreamRoot } from "./upstream.js";
export type { IconInventory, Upstream, UpstreamIcon } from "./upstream.js";
