/**
 * Compiling the icon set — once, in strict mode, through the ordinary compiler.
 *
 * There is nothing Lucide-specific below. `compileSvg` is the same entry point
 * `rbxts-svg build` uses for a consumer's own artwork, called with the same
 * options, and that is the point: two thousand real-world icons are the largest
 * compatibility test this repository has, and they are only a test if they take
 * the ordinary path. An icon that needed a special case here would be a generic
 * SVG feature gap wearing a disguise.
 *
 * `allowUnsupported` is never set. A package that shipped a silently degraded
 * icon would be making a compatibility claim it had not checked.
 */

import { readFileSync } from "node:fs";

import { compileSvg, type CompiledSvg } from "@rbxts/svg-compiler";

import type { UpstreamIcon } from "./upstream.js";

/**
 * One compiled icon, as the rest of the generator sees it.
 *
 * Purely internal: this is generator state, not a runtime type. Consumers of
 * the published packages see `SvgAsset` and nothing else, and nothing in here
 * is a wrapper around it.
 */
export interface CompiledLucideIcon {
	/** Upstream file base name, e.g. `"circle-alert"`. */
	readonly sourceName: string;
	/** `icons/circle-alert.svg`, relative and slash-normalized. */
	readonly sourceLabel: string;
	/** Content hash of the compiled IR — the asset's runtime identity. */
	readonly hash: string;
	/** The serialized IR, base64-encoded, ready to embed. */
	readonly base64: string;
	/** Length of the serialized IR in bytes, before encoding. */
	readonly byteLength: number;
	readonly viewBox: {
		readonly x: number;
		readonly y: number;
		readonly width: number;
		readonly height: number;
	};
	readonly preserveAspectRatio: string;
	readonly flags: number;
	readonly shapeCount: number;
	readonly irVersion: number;
	/** The compiled result itself, for the shared asset-expression emitter. */
	readonly compiled: CompiledSvg;
}

/** A single icon's compile failure, kept so every failure can be reported. */
export interface CompileFailure {
	readonly sourceName: string;
	readonly sourceLabel: string;
	readonly sourceFile: string;
	readonly message: string;
}

export interface CompileReport {
	readonly icons: readonly CompiledLucideIcon[];
	readonly failures: readonly CompileFailure[];
}

/**
 * Compiles every canonical icon exactly once.
 *
 * Once is the operative word. Both published packages embed the same bytes, and
 * the way to guarantee that is not to compare two compilations but to have one:
 * the React and Vide emitters are handed the same {@link CompiledLucideIcon}
 * values. Compiling per framework would also double the work for a set this
 * size, but the correctness argument is the one that matters — two compilations
 * are two chances to differ.
 *
 * Aliases are deliberately absent. They are names, not artwork, and their
 * generated modules re-export the canonical module rather than carrying a
 * second copy of the same IR.
 *
 * Failures are collected rather than thrown, so a broken upstream release is
 * reported as "3 of 1767 icons, here they are" instead of one icon at a time.
 */
export function compileIconSet(canonical: readonly UpstreamIcon[]): CompileReport {
	const icons: CompiledLucideIcon[] = [];
	const failures: CompileFailure[] = [];

	for (const icon of canonical) {
		try {
			const compiled = compileSvg(readFileSync(icon.sourceFile), {
				// Names the file in any diagnostic, and never affects the bytes.
				sourceName: icon.sourceLabel,
			});
			icons.push({
				sourceName: icon.sourceName,
				sourceLabel: icon.sourceLabel,
				hash: compiled.hash,
				base64: compiled.data.toString("base64"),
				byteLength: compiled.data.length,
				viewBox: compiled.viewBox,
				preserveAspectRatio: compiled.preserveAspectRatio,
				flags: compiled.flags,
				shapeCount: compiled.shapeCount,
				irVersion: compiled.irVersion,
				compiled,
			});
		} catch (error) {
			failures.push({
				sourceName: icon.sourceName,
				sourceLabel: icon.sourceLabel,
				sourceFile: icon.sourceFile,
				message: error instanceof Error ? error.message : String(error),
			});
		}
	}

	return { icons, failures };
}

/**
 * Renders a compile report's failures the way a maintainer needs to read them.
 *
 * Named icon, upstream file, compiler diagnostic — and no JavaScript stack,
 * which says nothing about why an SVG did not compile.
 */
export function describeFailures(
	failures: readonly CompileFailure[],
	discovered: number,
): string {
	const lines = [`Lucide generation failed: ${failures.length} of ${discovered} icons`, ""];
	for (const failure of failures) {
		lines.push(`${failure.sourceName}  (${failure.sourceLabel})`);
		lines.push(`  ${failure.sourceFile}`);
		for (const line of failure.message.split("\n")) {
			lines.push(`  ${line}`);
		}
		lines.push("");
	}
	lines.push(
		"No icon is skipped and `allowUnsupported` is never set: a generated package is a",
		"compatibility claim. If one of these is a real SVG feature gap, fix it in the",
		"compiler or the runtime and add a generic test — not a case for this icon.",
	);
	return lines.join("\n");
}
