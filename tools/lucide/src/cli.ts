#!/usr/bin/env node
/**
 * `pnpm generate:lucide` — one command, both packages.
 *
 * There is deliberately no `generate:lucide-react` / `generate:lucide-vide`
 * pair in the normal workflow. Generating the two packages separately would
 * mean two compile passes and two chances for their icon data to diverge; the
 * canonical operation is the one that produces both from a single pass.
 *
 * `--check` regenerates in memory and compares. Nothing is written and the exit
 * status says whether the committed output is current, which is what CI needs
 * in order to notice an upstream bump that nobody regenerated for.
 */

import { resolve } from "node:path";

import { generateLucide, describeStats } from "./generate.js";

function main(argv: readonly string[]): number {
	const check = argv.includes("--check");
	const repoRoot = resolve(__dirname, "../../..");

	let result;
	try {
		result = generateLucide({ repoRoot, check });
	} catch (error) {
		// The message is the report — every failing icon, its upstream file and
		// the compiler's own diagnostic. A JavaScript stack would say where the
		// generator was, which is never the interesting question.
		process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
		return 1;
	}

	process.stdout.write(`${describeStats(result.manifest, result.stats)}\n\n`);

	if (check) {
		if (result.clean) {
			process.stdout.write(
				`generated output is up to date with ${result.manifest.upstream} ${result.manifest.version}\n`,
			);
			return 0;
		}
		process.stderr.write(
			"Generated Lucide output is stale. Run `pnpm generate:lucide` and commit the result.\n\n" +
				`${result.staleSummary}\n`,
		);
		return 1;
	}

	for (const [target, report] of result.writes) {
		process.stdout.write(
			`${target.packageName}: ${report.written.length} written, ` +
				`${report.unchanged.length} unchanged, ${report.removed.length} removed\n`,
		);
		for (const path of report.removed) {
			process.stdout.write(`  removed  ${path}\n`);
		}
	}
	return 0;
}

process.exitCode = main(process.argv.slice(2));
