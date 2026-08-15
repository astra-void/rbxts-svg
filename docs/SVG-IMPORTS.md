# Direct `.svg` imports

The user-facing API:

```ts
import Search from "./icons/search.svg";   // Search: SvgAsset
```

This document records how that works, and why it is built the way it is.

## The pipeline

```text
src/icons/search.svg
        │  rbxts-svg build            (Rust compiler → serialized IR)
        ▼
src/svg-cache/icons/search.svg.ts    (generated TypeScript module)
        │  rbxtsc + @rbxts/svg-transformer
        │    "./icons/search.svg"  →  "./svg-cache/icons/search.svg"
        ▼
out/svg-cache/icons/search.svg.luau  (Luau SvgAsset module)
```

Two tools, two responsibilities. The generator compiles SVGs and owns
freshness. The transformer rewrites one string and owns nothing else — it never
reads an `.svg`, never loads the native compiler, never watches, and never
writes a file.

## Setup

Install the transformer alongside the compiler:

```bash
npm install --save-dev @rbxts/svg-compiler @rbxts/svg-transformer
```

Register it in `tsconfig.json`:

```json
{
  "compilerOptions": {
    "rootDir": "src",
    "plugins": [{ "transform": "@rbxts/svg-transformer" }]
  }
}
```

Build the SVGs before compiling:

```json
{
  "scripts": {
    "build": "rbxts-svg build && rbxtsc",
    "watch": "node scripts/watch.mjs"
  }
}
```

Ordering matters in exactly one place: `rbxtsc`'s first pass needs the generated
modules to exist, so `rbxts-svg build` runs first. After that the two watchers
are independent. `examples/react/scripts/watch.mjs` is a ~40-line cross-platform
script that seeds once and then runs both.

Add `svg-cache/` to `.gitignore`. The generated modules are derived, deterministic
output.

## Configuration

The transformer needs two paths, and reads both from the project's own
`tsconfig.json` so there is nothing extra to keep in sync:

| | default | override |
|---|---|---|
| source root | `compilerOptions.rootDir` | `rootDir` on the plugin entry |
| generated output | `<rootDir>/svg-cache` | `outDir` on the plugin entry |

`rbxts-svg` reads the same `tsconfig.json` for its own defaults, so
`rbxts-svg build` with no flags and the transformer with no config always agree.
Use `--root` / `--out` (and the matching plugin fields) only together:

```json
{ "transform": "@rbxts/svg-transformer", "outDir": "src/generated" }
```

```bash
rbxts-svg build --out src/generated
```

Pointing only one side at a custom directory is a build error naming the module
it could not find, not a broken `require` — `tests/integration/rbxtsc.test.ts`
pins that.

There is no `rbxts-svg.config.json`. It would have added a third place for the
same two values to drift.

roblox-ts requires `rootDir` (or `rootDirs`) to be set, so the default is always
available. A project with only `rootDirs` gets an actionable error rather than a
guess: a merged virtual directory has no single answer to "where does the source
tree start."

## Typing

`rbxts-svg build` emits one declaration beside the generated modules:

```ts
// src/svg-cache/svg-modules.d.ts
declare module "*.svg" {
	const asset: import("@rbxts/svg").SvgAsset;
	export default asset;
}
```

That is what makes plain `tsc --noEmit` accept the source form, before any
transformer runs. Nothing is hand-written by a consumer, and nothing needs
wiring: the file lives inside the project's existing `include` globs.

It could not live in `@rbxts/svg`'s own `index.d.ts` — TypeScript only honours
`declare module "*.svg"` in a file that is *not* itself a module, and inside a
module it is read as an augmentation and rejected. Generating it is also what
keeps the type plumbing type-only: a `.d.ts` under `rootDir` is neither compiled
nor copied by roblox-ts, so no ModuleScript exists at runtime just to carry a
type.

`allowArbitraryExtensions` is **not** required. roblox-ts pins
`moduleResolution: "Node10"`, where `./svg-cache/icons/search.svg` already
resolves to `search.svg.ts` by extension substitution. That is also why the
rewritten specifier keeps its `.svg` and does not gain a `.ts`.

## What is rewritten

Supported:

```ts
import Search from "./icons/search.svg";
import * as Search from "./icons/search.svg";
import type Search from "./icons/search.svg";
import "./icons/search.svg";
export { default as Search } from "./icons/search.svg";
export { default } from "./icons/search.svg";
```

Left alone:

- any specifier that is not an `.svg`
- string literals that merely end in `.svg` — only module specifiers are examined
- specifiers that already point inside the generated directory, so writing
  `./svg-cache/icons/search.svg` by hand still works and is never remapped twice
- every file inside the generated directory

Deliberately unsupported, each with a build error rather than silence:

- `import("./icon.svg")` and `import Icon = require("./icon.svg")`. Neither is
  rewritten, so both would compile to a `require` of the raw `.svg`, which is not
  a ModuleScript. Static imports are the whole surface for v0.1.
- non-relative specifiers such as `@/icons/search.svg` or `assets/search.svg`.
  Supporting them would mean reimplementing `baseUrl`/`paths`/`rootDirs`
  resolution, which roblox-ts does not support for anything else either. Use a
  relative specifier.

## Diagnostics

Every failure is a normal compiler error with the offending specifier
underlined, produced through `ts.TransformationContext.addDiagnostic` — roblox-ts
forwards those into its own diagnostic service and refuses to emit.

Unbuilt cache:

```text
src/Toolbar.tsx:17:18 - error TS0: @rbxts/svg-transformer: generated asset
module is missing for

  src/icons/bell.svg

imported from:

  src/Toolbar.tsx

Expected:

  src/svg-cache/icons/bell.svg.ts

Compile the project's SVGs first:

  rbxts-svg build
```

There are also errors for a `.svg` that does not exist, one that resolves outside
the source root (the generator would never produce a module for it, so the
transformer refuses to point at one — both sides share the acceptance rule), and
the unsupported syntaxes above.

## Why generated modules, not a transformer that reads files

The obvious approach is a transformer that resolves `./search.svg`, reads it from
disk, and injects a literal AST. It is simpler and it behaves badly.

**TypeScript would not know the `.svg` is an input.** The program's dependency
graph is built from module resolution. A file a transformer happens to
`readFileSync` during emit is invisible to it, so in watch mode editing the
`.svg` rebuilds nothing — the `.ts` that imported it has not changed. Every
workaround from there (watching separately and touching files, disabling
incremental builds) fights the compiler.

Generating a real `.ts` file puts the SVG back inside the graph:

```text
edit search.svg
  →  rbxts-svg watch rewrites svg-cache/icons/search.svg.ts
  →  rbxtsc -w sees a changed TypeScript input
  →  out/svg-cache/icons/search.svg.luau updates
```

The importing `.tsx` is never touched, and its emitted `require` never changes.
`tests/integration/watch.test.ts` runs both real watchers and asserts exactly
that.

It also means the compiled asset is visible and reviewable on disk, and that the
same generator serves other consumers later (a Lucide package generator, a Loom
build) without any of them depending on a roblox-ts transformer.

## Design properties

Tested in `tests/integration/generate.test.ts`, `tests/transformer/` and
`tests/integration/rbxtsc.test.ts`.

**One path mapping.** `generatedModulePath` lives in
`@rbxts/svg-compiler/paths`, a module with no dependency on the native binary.
Both the generator and the transformer import it. There is no second
implementation to drift.

**Stable paths.** `icons/search.svg` always maps to
`svg-cache/icons/search.svg.ts`, whatever the contents.

> The original specification suggested `search.<content-hash>.svg.ts`. That was
> rejected: a hash in the path changes the import specifier on every edit, so
> every edit would also have to rewrite every importing file, and orphaned files
> would accumulate. The hash lives in the module header instead, where it does
> its job without churning the path.

**Deterministic.** Both the generated text and the rewritten specifier are pure
functions of their inputs. Specifiers are normalized to POSIX separators and
always carry a leading `./` or `../`, so Windows and POSIX machines emit
identical output.

**No spurious writes.** A generated file whose content is unchanged is not
rewritten, so its mtime does not move and downstream watchers stay quiet.

**Correct invalidation.** Compilation is keyed on source bytes plus the options
that affect output. `sourceName` is excluded, since it only labels diagnostics.

**No duplicate compilation.** `SvgCompilationCache` compiles identical sources
once, whether they are one file imported twice or two copies of the same icon.

**Errors point at the SVG.** Compiler diagnostics carry the source path, line,
column, element and element path — of the `.svg`, never of the generated `.ts`.

**Freshness is the generator's job.** The transformer checks only that the
generated module *exists*. It never hashes an SVG, compares headers, or decides
whether generated content is stale; that would duplicate the generator's
responsibility in the one process that must stay fast.

## The roblox-ts plugin contract

Discovered from the installed compiler, not assumed:
`roblox-ts/out/Project/transformers/{getPluginConfigs,createTransformerList}.js`
and `out/Project/functions/compileFiles.js`.

roblox-ts reads `compilerOptions.plugins` from `tsconfig.json`, keeps entries
that have a string `transform`, resolves each from the project directory with
`resolve.sync`, `require`s it (so a plugin must be CommonJS), and calls the
default export. With no explicit `"type"`, the entry is the `"program"` form:

```ts
(program: ts.Program, config: PluginConfig, extras: { ts: typeof ts })
  => ts.TransformerFactory<ts.SourceFile>
```

`extras.ts` is roblox-ts's own TypeScript instance, and is what the transformer
uses — a second copy of the compiler in one process is a reliable source of
subtle bugs.

One consequence worth knowing: roblox-ts does not feed the transformed AST
straight into emit. It **prints each transformed file back to text**, feeds that
into a language service, and computes `require` paths from the re-resolved
program. So a rewritten specifier has to survive real module resolution, and a
transformer that produced a plausible AST could still emit a broken `require`.
That is why every integration test here asserts on emitted Luau.

## Output

A representative emit, from `examples/react`:

```lua
-- out/Toolbar.luau
local Search = TS.import(script, game:GetService("ReplicatedStorage"), "TS", "svg-cache", "icons", "search.svg").default
```

```lua
-- out/svg-cache/icons/search.svg.luau
-- Generated by @rbxts/svg — do not edit.
-- source: icons/search.svg
-- ir-version: 2
-- hash: 5116d3eb…
local unstable_internal = TS.import(script, game:GetService("ReplicatedStorage"), "rbxts_include", "node_modules", "@rbxts", "svg", "out").unstable_internal
local default = unstable_internal.createAssetFromBase64("UlNWRwIAJAAVAAAA…", "5116d3eb…")
```

### Raw `.svg` files in the output tree

roblox-ts copies every non-compilable file under `rootDir` into `outDir`, so
`src/icons/search.svg` also appears at `out/icons/search.svg`. This is inert:
Rojo syncs only the file types it recognizes, and nothing requires it — the
runtime `require` targets the generated module. roblox-ts offers no supported
way to exclude it, and pruning the output directory by hand would be a far worse
trade than an ignored file. `tests/integration/rbxtsc.test.ts` pins both facts.

## Known friction: pnpm and roblox-ts

pnpm symlinks workspace packages into `node_modules`. TypeScript resolves
symlinks to their real path by default, so `@rbxts/svg` resolves outside
`node_modules` and roblox-ts rejects it with:

```text
You cannot use modules directly under node_modules.
```

Set `preserveSymlinks` in the consuming project's `tsconfig.json`:

```json
{ "compilerOptions": { "preserveSymlinks": true } }
```

`examples/react` does this, and so does every fixture in
`tests/integration/`, which borrow the example's pnpm-linked `node_modules`.
The transformer neither causes nor changes this: it is resolved by roblox-ts
through Node's own resolver, not TypeScript's. Projects using npm or yarn, or
pnpm with `node-linker=hoisted`, are unaffected.

## Also worth knowing: project name and project type

roblox-ts infers a project as a **package** when `package.json`'s `name` is
scoped (`/^@[a-z0-9-]*\//`). A package emits `local TS = _G[script]` and expects
its importer to supply the runtime, which is wrong for a game and fails at run
time. Name a game project something unscoped — `examples/react` is
`rbxts-svg-example-react` for exactly this reason.
