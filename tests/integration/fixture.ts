/**
 * Scaffolding for tests that run the *real* compilers.
 *
 * A transformer that produces a plausible AST proves nothing on its own: the
 * output that matters is the Luau `require` roblox-ts finally emits, and the
 * only thing that can produce that is roblox-ts. So these fixtures are genuine
 * roblox-ts projects — a `tsconfig.json`, a Rojo project file, real
 * `node_modules` — driven by the actual `rbxtsc` and the actual `rbxts-svg`.
 *
 * `node_modules` is borrowed from `examples/react` by symlink rather than
 * installed per fixture. Installing would make each test a network operation,
 * and the example's tree already holds exactly the packages a consumer needs:
 * `roblox-ts`, `typescript`, `@rbxts/svg`, and the transformer itself, wired
 * through pnpm's workspace links. Borrowing it also means these tests exercise
 * the same pnpm symlink layout real users have, which is where
 * `preserveSymlinks` earns its keep.
 */

import { execFileSync, spawn } from "node:child_process";
import {
	existsSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	symlinkSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

/** Repository root, found from this file rather than from cwd. */
export const REPO_ROOT = resolve(__dirname, "../..");

/** The example's installed tree, which a fixture borrows unless told otherwise. */
const NODE_MODULES = join(REPO_ROOT, "examples/react/node_modules");

/**
 * The Vide example's tree, for fixtures that compile Vide sources.
 *
 * A separate tree rather than one with everything in it, because "the React
 * example's node_modules contains no Vide, and the Vide example's contains no
 * React" is itself worth being true: a test that emits no React require is only
 * meaningful if React was resolvable in the first place.
 */
export const VIDE_NODE_MODULES = join(REPO_ROOT, "examples/vide/node_modules");

/** The real CLIs, invoked as a user would invoke them. */
export const RBXTS_SVG_CLI = join(REPO_ROOT, "packages/compiler/dist/cli.js");
export const RBXTSC_CLI = join(NODE_MODULES, "roblox-ts/out/CLI/cli.js");

export interface FixtureOptions {
	/** Extra `compilerOptions` merged over the roblox-ts defaults. */
	readonly compilerOptions?: Record<string, unknown>;
	/** Plugin entry fields merged over `{ transform: "@rbxts/svg-transformer" }`. */
	readonly pluginConfig?: Record<string, unknown>;
	/** Omits the plugin entirely, for testing the un-transformed baseline. */
	readonly withoutTransformer?: boolean;
	/** The installed tree to borrow. Defaults to the React example's. */
	readonly nodeModules?: string;
}

/** A throwaway roblox-ts project on disk. */
export class Fixture {
	readonly dir: string;

	constructor(options: FixtureOptions = {}) {
		this.dir = mkdtempSync(join(tmpdir(), "rbxts-svg-fixture-"));
		symlinkSync(options.nodeModules ?? NODE_MODULES, join(this.dir, "node_modules"), "dir");

		this.write(
			"package.json",
			JSON.stringify({ name: "fixture", version: "0.0.0", private: true }, undefined, 2),
		);
		this.write(
			"default.project.json",
			JSON.stringify(
				{
					name: "fixture",
					tree: {
						$className: "DataModel",
						ReplicatedStorage: {
							rbxts_include: {
								$path: "include",
								node_modules: {
									$className: "Folder",
									"@rbxts": { $path: "node_modules/@rbxts" },
								},
							},
							TS: { $path: "out" },
						},
					},
				},
				undefined,
				2,
			),
		);

		const plugins = options.withoutTransformer
			? undefined
			: [{ transform: "@rbxts/svg-transformer", ...options.pluginConfig }];
		this.write(
			"tsconfig.json",
			JSON.stringify(
				{
					compilerOptions: {
						allowSyntheticDefaultImports: true,
						downlevelIteration: true,
						jsx: "react",
						jsxFactory: "React.createElement",
						module: "commonjs",
						moduleResolution: "Node",
						moduleDetection: "force",
						noLib: true,
						resolveJsonModule: true,
						forceConsistentCasingInFileNames: true,
						skipLibCheck: true,
						strict: true,
						target: "ESNext",
						typeRoots: ["node_modules/@rbxts"],
						rootDir: "src",
						outDir: "out",
						declaration: false,
						preserveSymlinks: true,
						...options.compilerOptions,
						...(plugins === undefined ? {} : { plugins }),
					},
					include: ["src"],
				},
				undefined,
				2,
			),
		);
	}

	/** Writes a file, creating parent directories. Path is fixture-relative. */
	write(relativePath: string, contents: string): string {
		const path = join(this.dir, relativePath);
		mkdirSync(dirname(path), { recursive: true });
		writeFileSync(path, contents, "utf8");
		return path;
	}

	path(relativePath: string): string {
		return join(this.dir, relativePath);
	}

	/** Runs `rbxts-svg build`. Extra arguments are passed straight through. */
	buildSvgs(...args: string[]): CommandResult {
		return this.run(RBXTS_SVG_CLI, ["build", ...args]);
	}

	/** Runs `rbxtsc`. */
	compile(...args: string[]): CommandResult {
		return this.run(RBXTSC_CLI, args);
	}

	private run(script: string, args: readonly string[]): CommandResult {
		try {
			const stdout = execFileSync(process.execPath, [script, ...args], {
				cwd: this.dir,
				encoding: "utf8",
				stdio: ["ignore", "pipe", "pipe"],
			});
			return { ok: true, output: stdout };
		} catch (error) {
			const failure = error as { stdout?: string; stderr?: string; message: string };
			return {
				ok: false,
				output: `${failure.stdout ?? ""}${failure.stderr ?? ""}` || failure.message,
			};
		}
	}

	/**
	 * Starts a long-running process in the fixture, e.g. `rbxts-svg watch`.
	 *
	 * Output is buffered so a failing test can show what the watcher said,
	 * which is the difference between "timed out" and a usable bug report.
	 */
	spawnWatcher(script: string, args: readonly string[]): Watcher {
		const child = spawn(process.execPath, [script, ...args], {
			cwd: this.dir,
			stdio: ["ignore", "pipe", "pipe"],
		});
		let output = "";
		child.stdout.on("data", (chunk: Buffer) => (output += chunk.toString()));
		child.stderr.on("data", (chunk: Buffer) => (output += chunk.toString()));
		return {
			get output() {
				return stripAnsi(output);
			},
			stop: () => {
				child.kill("SIGTERM");
			},
		};
	}

	dispose(): void {
		rmSync(this.dir, { recursive: true, force: true });
	}
}

/** A running background process owned by a fixture. */
export interface Watcher {
	readonly output: string;
	stop(): void;
}

/**
 * Waits for a filesystem condition, or fails with what it was waiting for.
 *
 * Polling a condition rather than sleeping a fixed interval is what keeps the
 * watch tests honest: a slow machine takes longer, not a different result, and
 * a genuinely broken chain fails with a message naming the condition instead of
 * an assertion on stale bytes.
 */
export async function waitFor(
	description: string,
	condition: () => boolean,
	options: { timeoutMs?: number; describeFailure?: () => string } = {},
): Promise<void> {
	const timeoutMs = options.timeoutMs ?? 60_000;
	const deadline = Date.now() + timeoutMs;
	while (Date.now() < deadline) {
		if (condition()) {
			return;
		}
		await new Promise((done) => setTimeout(done, 50));
	}
	const detail = options.describeFailure?.() ?? "";
	throw new Error(
		`timed out after ${timeoutMs}ms waiting for ${description}${detail === "" ? "" : `\n\n${detail}`}`,
	);
}

/** Content of a file, or `undefined` if it does not exist yet. */
export function readIfExists(path: string): string | undefined {
	return existsSync(path) ? readFileSync(path, "utf8") : undefined;
}

export interface CommandResult {
	readonly ok: boolean;
	/** stdout and stderr together — how a user sees it. */
	readonly output: string;
}

/** Asserts a command succeeded, showing its output when it did not. */
export function expectOk(result: CommandResult, what: string): void {
	if (!result.ok) {
		throw new Error(`${what} failed:\n\n${stripAnsi(result.output)}`);
	}
}

/** Compiler output is colourized; tests match on the text underneath. */
export function stripAnsi(text: string): string {
	// eslint-disable-next-line no-control-regex
	return text.replace(/\u001b\[[0-9;]*m/g, "");
}

/** A real Lucide icon, so the fixtures compile something representative. */
export const SEARCH_SVG = join(REPO_ROOT, "tests/fixtures/lucide/search.svg");
export const BELL_SVG = join(REPO_ROOT, "tests/fixtures/lucide/bell.svg");
