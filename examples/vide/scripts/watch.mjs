#!/usr/bin/env node
/**
 * One command for the two watchers a `.svg`-importing project needs.
 *
 * ```text
 * rbxts-svg watch   .svg  →  src/svg-cache/**.svg.ts
 * rbxtsc -w         .ts   →  out/**.luau
 * ```
 *
 * They stay separate processes on purpose. Folding SVG compilation into
 * `rbxtsc` would put a Rust binary inside the TypeScript compiler and take the
 * `.svg` back out of TypeScript's dependency graph, which is the thing that
 * makes incremental rebuilds correct. This script only starts them in the right
 * order and ties their lifetimes together.
 *
 * Written in Node rather than as `a & b` because that shell syntax means
 * something different on Windows, and this example should work everywhere the
 * packages do.
 */

import { spawn, spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const projectDir = dirname(dirname(fileURLToPath(import.meta.url)));
const bin = (name) => join(projectDir, "node_modules", ".bin", name);

/** Compile once up front: rbxtsc's first pass needs the generated modules. */
const seed = spawnSync(bin("rbxts-svg"), ["build"], {
	cwd: projectDir,
	stdio: "inherit",
	shell: process.platform === "win32",
});
if (seed.status !== 0) {
	process.exit(seed.status ?? 1);
}

const children = [
	spawn(bin("rbxts-svg"), ["watch"], {
		cwd: projectDir,
		stdio: "inherit",
		shell: process.platform === "win32",
	}),
	spawn(bin("rbxtsc"), ["-w"], {
		cwd: projectDir,
		stdio: "inherit",
		shell: process.platform === "win32",
	}),
];

/** If either watcher dies, the other is no longer useful. Stop both. */
const shutdown = (code) => {
	for (const child of children) {
		if (child.exitCode === null && child.signalCode === null) {
			child.kill();
		}
	}
	process.exit(code);
};

for (const child of children) {
	child.on("exit", (code) => shutdown(code ?? 0));
	child.on("error", (error) => {
		console.error(error.message);
		shutdown(1);
	});
}

process.on("SIGINT", () => shutdown(0));
process.on("SIGTERM", () => shutdown(0));
