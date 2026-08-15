# `@rbxts/svg-transformer`

A roblox-ts transformer that makes this compile:

```ts
import Search from "./icons/search.svg";   // Search: SvgAsset
```

```bash
npm install --save-dev @rbxts/svg-transformer @rbxts/svg-compiler
```

## Setup

Register it in `tsconfig.json`:

```json
{
  "compilerOptions": {
    "rootDir": "src",
    "plugins": [{ "transform": "@rbxts/svg-transformer" }]
  }
}
```

Build the SVGs before compiling — `rbxtsc`'s first pass needs the generated
modules to exist:

```json
{
  "scripts": {
    "build": "rbxts-svg build && rbxtsc"
  }
}
```

It reads `rootDir` from the project's own `tsconfig.json`, so there is no second
configuration file to keep in sync.

## What it does, and what it deliberately does not

It rewrites one string:

```text
"./icons/search.svg"  →  "./svg-cache/icons/search.svg"
```

That is the whole job. It never reads an `.svg`, never loads the native
compiler, never watches, and never writes a file — all of that belongs to
[`@rbxts/svg-compiler`](https://www.npmjs.com/package/@rbxts/svg-compiler),
which also owns freshness. Both sides share one path mapping
(`@rbxts/svg-compiler/paths`) so they cannot disagree.

Static imports and re-exports are rewritten. A missing `.svg`, an unbuilt cache,
a non-relative specifier and a dynamic import each produce a diagnostic naming
the file and what to do about it, rather than a mysterious failure later in the
build.

## See also

- [Direct `.svg` imports](https://github.com/astra-void/rbxts-svg/blob/main/docs/SVG-IMPORTS.md) — the full contract, including the intentionally unsupported syntaxes
- [Repository](https://github.com/astra-void/rbxts-svg)

## Licence

MIT.
