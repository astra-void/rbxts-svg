#!/usr/bin/env node
/**
 * Release preflight.
 *
 * Every failure mode below has published a broken package for somebody at some
 * point, and none of them is caught by `npm publish`:
 *
 * - A roblox-ts package whose `files` is `["out"]` publishes an *empty tarball*
 *   if `rbxtsc` never ran. npm reports success.
 * - `@rbxts/svg-native` declares its platform binaries as optionalDependencies
 *   at exactly its own version. A version skew between the workspace packages
 *   means an installable main package that can never resolve a binary.
 * - A platform directory with no `.node` in it publishes an empty binary
 *   package, which fails at *install* time on that platform only.
 *
 * Run before publishing. Exits non-zero with a list of what is wrong.
 *
 * The platform-binary check only makes sense where all six binaries have been
 * collected, which in practice is the release job — one developer machine
 * builds one target. Pass `--native` (the release workflow does) to enforce it;
 * without it the check reports what it sees and moves on.
 */

import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)));

/** Every package this repository publishes, and the file that proves it built. */
const PUBLISHED = [
	{ dir: "crates/svg-node", entry: "index.js" },
	{ dir: "packages/compiler", entry: "dist/index.js" },
	{ dir: "packages/transformer", entry: "dist/index.js" },
	{ dir: "packages/svg", entry: "out/init.luau" },
	{ dir: "packages/svg-react", entry: "out/init.luau" },
	{ dir: "packages/svg-vide", entry: "out/init.luau" },
	{ dir: "packages/lucide-react", entry: "out/init.luau" },
	{ dir: "packages/lucide-vide", entry: "out/init.luau" },
];

const problems = [];
const note = (message) => problems.push(message);

const readJson = (path) => JSON.parse(readFileSync(path, "utf8"));

/* ---- 1. Every published package built, and carries its licence. ---------- */

const versions = new Map();

for (const { dir, entry } of PUBLISHED) {
	const packageDir = resolve(repoRoot, dir);
	const manifestPath = join(packageDir, "package.json");

	if (!existsSync(manifestPath)) {
		note(`${dir}: no package.json`);
		continue;
	}

	const manifest = readJson(manifestPath);
	versions.set(manifest.name, manifest.version);

	if (!existsSync(join(packageDir, entry))) {
		note(`${manifest.name}: ${entry} is missing — run \`pnpm build\` first`);
	}

	if (!existsSync(join(packageDir, "LICENSE"))) {
		note(`${manifest.name}: no LICENSE file`);
	}

	if (!existsSync(join(packageDir, "README.md"))) {
		note(`${manifest.name}: no README.md — npm would render a blank page`);
	}

	if (manifest.private) {
		note(`${manifest.name}: marked private but listed as published`);
	}

	if (manifest.publishConfig?.access !== "public") {
		note(`${manifest.name}: scoped package without publishConfig.access "public"`);
	}
}

/* ---- 2. One version across the workspace. -------------------------------- */

const distinct = new Set(versions.values());
if (distinct.size > 1) {
	const listed = [...versions].map(([name, version]) => `${name}@${version}`).join(", ");
	note(`version skew across published packages: ${listed}`);
}

const [releaseVersion] = distinct;

/* ---- 3. Rust and npm agree on the version. ------------------------------- */

const cargoToml = readFileSync(resolve(repoRoot, "Cargo.toml"), "utf8");
const cargoVersion = /^version\s*=\s*"([^"]+)"/m.exec(cargoToml)?.[1];
if (cargoVersion !== releaseVersion) {
	note(`Cargo.toml workspace version ${cargoVersion} != npm version ${releaseVersion}`);
}

/* ---- 4. Every declared napi target has a binary to publish. -------------- */

const enforceNative = process.argv.includes("--native");

const nativeDir = resolve(repoRoot, "crates/svg-node");
const npmDir = join(nativeDir, "npm");
const targets = readJson(join(nativeDir, "package.json")).napi.targets;

const nativeProblem = (message) => (enforceNative ? note(message) : console.log(`  (skipped) ${message}`));

if (!existsSync(npmDir)) {
	nativeProblem("crates/svg-node/npm is missing — run `napi create-npm-dirs` and `napi artifacts`");
} else {
	const built = readdirSync(npmDir);

	if (built.length !== targets.length) {
		nativeProblem(`crates/svg-node/npm has ${built.length} platform dirs, expected ${targets.length}`);
	}

	for (const platform of built) {
		const contents = readdirSync(join(npmDir, platform));
		if (!contents.some((file) => file.endsWith(".node"))) {
			nativeProblem(`crates/svg-node/npm/${platform}: no .node binary — the artifact never arrived`);
		}
	}
}

if (!enforceNative) {
	console.log("Platform binaries not checked; pass --native to require all "
		+ `${targets.length} targets.`);
}

/* ---- Report. ------------------------------------------------------------- */

if (problems.length > 0) {
	console.error(`Release preflight failed (${problems.length}):\n`);
	for (const problem of problems) {
		console.error(`  - ${problem}`);
	}
	process.exit(1);
}

console.log(`Release preflight passed — ${PUBLISHED.length} packages ready at ${releaseVersion}.`);
