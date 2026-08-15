/**
 * Putting the generated tree on disk, and taking off what no longer belongs.
 *
 * Two behaviours matter more than the writing itself.
 *
 * **A file whose content is unchanged is not rewritten.** With 2025 files per
 * package, touching every mtime on every run would wake `rbxtsc --watch` and
 * every other watcher for nothing, and would make "did anything actually
 * change?" unanswerable from the filesystem.
 *
 * **Whatever is in the generated directory and not in the manifest is deleted.**
 * An upstream release that removes an icon must remove it here too; the
 * alternative is a package that accumulates icons forever and a barrel that
 * exports names upstream no longer has. Deletion is guarded: only a file
 * carrying the generated marker is ever removed, so a wrong output path cannot
 * destroy hand-written source.
 */

import {
	existsSync,
	mkdirSync,
	readFileSync,
	readdirSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { dirname, join, relative, sep, posix } from "node:path";

import { GENERATED_MARKER, type GeneratedFile } from "./emit.js";

/** What a write pass did, per package. */
export interface WriteReport {
	readonly written: readonly string[];
	readonly unchanged: readonly string[];
	readonly removed: readonly string[];
}

/** What a `--check` pass found, per package. */
export interface CheckReport {
	readonly missing: readonly string[];
	readonly changed: readonly string[];
	readonly extra: readonly string[];
}

/** True when a check pass found nothing to report. */
export function isClean(report: CheckReport): boolean {
	return (
		report.missing.length === 0 && report.changed.length === 0 && report.extra.length === 0
	);
}

/** Writes the tree under `packageDir`, pruning anything stale beneath `ownedDir`. */
export function writeGeneratedTree(
	packageDir: string,
	ownedDir: string,
	files: readonly GeneratedFile[],
): WriteReport {
	const written: string[] = [];
	const unchanged: string[] = [];

	for (const file of files) {
		const absolute = join(packageDir, file.path);
		if (existsSync(absolute) && readFileSync(absolute, "utf8") === file.contents) {
			unchanged.push(file.path);
			continue;
		}
		mkdirSync(dirname(absolute), { recursive: true });
		writeFileSync(absolute, file.contents, "utf8");
		written.push(file.path);
	}

	const expected = new Set(files.map((file) => file.path));
	const removed = pruneStale(packageDir, join(packageDir, ownedDir), expected);
	return { written: written.sort(), unchanged: unchanged.sort(), removed };
}

/** Compares the tree on disk with the tree that would be written. */
export function checkGeneratedTree(
	packageDir: string,
	ownedDir: string,
	files: readonly GeneratedFile[],
): CheckReport {
	const missing: string[] = [];
	const changed: string[] = [];

	for (const file of files) {
		const absolute = join(packageDir, file.path);
		if (!existsSync(absolute)) {
			missing.push(file.path);
		} else if (readFileSync(absolute, "utf8") !== file.contents) {
			changed.push(file.path);
		}
	}

	const expected = new Set(files.map((file) => file.path));
	const extra = listGenerated(packageDir, join(packageDir, ownedDir)).filter(
		(path) => !expected.has(path),
	);

	return { missing: missing.sort(), changed: changed.sort(), extra: extra.sort() };
}

/**
 * Every generated file beneath `ownedDir`, as package-relative paths.
 *
 * "Generated" is decided by the marker at the top of the file, not by location:
 * a directory is only owned as far as its contents say they were produced here.
 */
function listGenerated(packageDir: string, ownedDir: string): string[] {
	if (!existsSync(ownedDir)) {
		return [];
	}
	const found: string[] = [];
	const walk = (dir: string): void => {
		for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) =>
			a.name < b.name ? -1 : 1,
		)) {
			const absolute = join(dir, entry.name);
			if (entry.isDirectory()) {
				walk(absolute);
			} else if (
				entry.isFile() &&
				readFileSync(absolute, "utf8").startsWith(GENERATED_MARKER)
			) {
				found.push(relative(packageDir, absolute).split(sep).join(posix.sep));
			}
		}
	};
	walk(ownedDir);
	return found;
}

function pruneStale(
	packageDir: string,
	ownedDir: string,
	expected: ReadonlySet<string>,
): string[] {
	const removed = listGenerated(packageDir, ownedDir).filter((path) => !expected.has(path));
	for (const path of removed) {
		rmSync(join(packageDir, path));
	}
	return removed.sort();
}

/** Writes one file if its content differs. Used for the manifest and licenses. */
export function writeFileIfChanged(path: string, contents: string): boolean {
	if (existsSync(path) && readFileSync(path, "utf8") === contents) {
		return false;
	}
	mkdirSync(dirname(path), { recursive: true });
	writeFileSync(path, contents, "utf8");
	return true;
}
