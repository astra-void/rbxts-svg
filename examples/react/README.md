# React example

A roblox-ts project that draws SVGs both ways: named Lucide components, and its
own artwork imported straight from a `.svg`.

```bash
pnpm install
pnpm --filter rbxts-svg-example-react run build
```

```tsx
import { Search, Settings } from "@rbxts/lucide-react";

<Search size={24} color={Color3.fromRGB(255, 255, 255)} />
```

Those need no build step here at all — `@rbxts/lucide-react` ships them already
compiled, so a project using only named icons installs neither
`@rbxts/svg-transformer` nor the CLI. See
[`docs/LUCIDE.md`](../../docs/LUCIDE.md).

`logo.svg` is the other half, and it is why the transformer is still wired up:

```tsx
import Logo from "./icons/logo.svg";

<Svg source={Logo} size={32} />
```

Two things make *that* work, both configured here:

- `pnpm run svg` (`rbxts-svg build`) compiles every `src/**/*.svg` into a
  generated module under `src/svg-cache/`, plus the ambient declaration that
  types the import as `SvgAsset`.
- `@rbxts/svg-transformer`, registered in `tsconfig.json`'s
  `compilerOptions.plugins`, rewrites each specifier onto its generated module
  during `rbxtsc`.

See [`docs/SVG-IMPORTS.md`](../../docs/SVG-IMPORTS.md) for why the generated
modules exist rather than a transformer that compiles SVGs itself.

## Development

```bash
pnpm --filter rbxts-svg-example-react run watch
```

That is `scripts/watch.mjs`: it compiles the SVGs once, then runs `rbxts-svg
watch` and `rbxtsc -w` side by side and ties their lifetimes together. They stay
independent processes — editing an `.svg` regenerates its module, which `rbxtsc`
already watches, so the rebuild chain works without either tool knowing about
the other, and `Toolbar.tsx` never changes.

Run them separately if you prefer:

```bash
pnpm --filter rbxts-svg-example-react run svg:watch
```

```bash
pnpm --filter rbxts-svg-example-react exec rbxtsc -w
```

## Rendering

`Toolbar.tsx` calls `installEditableImageRenderer()` once at startup; after that
every icon draws through the software rasterizer into a cached `EditableImage`.
That call is the same one whether the asset came from a package or from your own
`.svg`. The two same-size `Search` icons in the toolbar share one raster — only
their `ImageColor3` differs — while `logo.svg`, being fixed multi-colour fill
artwork rather than `currentColor`, is not a shared alpha mask at all.

`src/client/main.client.tsx` mounts `<Toolbar />` into a `ScreenGui`, so this is
a project you can actually press Play on. Sync with Rojo and run the client. The
manual checklist, and what a live Studio run has already confirmed, are in
[`docs/STUDIO-VERIFY.md`](../../docs/STUDIO-VERIFY.md).

## Notes

- The package is named `rbxts-svg-example-react`, not `@rbxts/…`. roblox-ts
  treats a scoped name as a *package* and emits `_G[script]` instead of a
  RuntimeLib require, which is wrong for a game.
- `preserveSymlinks` is set because pnpm symlinks workspace packages; see the
  pnpm section of [`docs/SVG-IMPORTS.md`](../../docs/SVG-IMPORTS.md).
- `out/client` is mapped into `StarterPlayer.StarterPlayerScripts`. A
  `LocalScript` synced under `ReplicatedStorage` would never run.
- `@rbxts/react` loads its implementation from a sibling `@rbxts-js` folder,
  which pnpm's default isolated linker keeps inside `node_modules/.pnpm`. If
  Rojo syncs a tree where `@rbxts-js` is missing, set `node-linker=hoisted` in
  an `.npmrc`.
