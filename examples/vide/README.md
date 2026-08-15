# Vide example

A roblox-ts project that draws SVGs both ways under
[Vide](https://centau.github.io/vide/): named Lucide components, and its own
artwork imported straight from a `.svg`.

```bash
pnpm install
pnpm --filter rbxts-svg-example-vide run build
```

```tsx
import { Search, Settings } from "@rbxts/lucide-vide";

<Search size={size} color={colour} />
```

Every prop is a source if you want it to be — the generated wrapper is `<Svg>`
with the asset bound and adds no reactivity of its own, which is what the
`colour` button in the toolbar demonstrates. Those icons need no build step
here; `@rbxts/lucide-vide` ships them already compiled. See
[`docs/LUCIDE.md`](../../docs/LUCIDE.md).

`logo.svg` is the other half — the same import, the same generated module and
the same `SvgAsset` as [the React example](../react):

```tsx
import Logo from "./icons/logo.svg";

<Svg source={Logo} size={32} />
```

Only the binding differs:

```tsx
import { Svg } from "@rbxts/svg-vide";
```

Two things make that import work, both wired up here:

- `pnpm run svg` (`rbxts-svg build`) compiles every `src/**/*.svg` into a
  generated module under `src/svg-cache/`, plus the ambient declaration that
  types the import as `SvgAsset`.
- `@rbxts/svg-transformer`, registered in `tsconfig.json`'s
  `compilerOptions.plugins`, rewrites each specifier onto its generated module
  during `rbxtsc`.

Neither knows which UI library will consume the result. See
[`docs/SVG-IMPORTS.md`](../../docs/SVG-IMPORTS.md).

## JSX

Vide brings its own JSX factory, and it is not React's:

```json
"jsx": "react",
"jsxFactory": "Vide.jsx",
"jsxFragmentFactory": "Vide.Fragment"
```

`Vide.jsx` creates instances eagerly and invokes function components through
`untrack`, so React's factory would not merely be untidy here — it would not
run. Every `.tsx` file therefore needs `Vide` in scope.

## What it demonstrates

The toolbar is not just icons; the three buttons under it exercise the parts of
a reactive binding that can go wrong, and the `rasters:` counter beside them
reads the shared cache's miss count so you can see whether they did:

| Press | Expected |
| --- | --- |
| `colour` | The reactive `Search` changes colour. **The counter does not move** — a Lucide icon is a tintable alpha mask, so its colour is an `ImageColor3` write, not a new raster. |
| `size` | The second `Search` steps 24 → 32 → 48. The counter rises by one per new size, and the previous raster is released as the new one is acquired. |
| `bell` | The `Bell` disappears and comes back. Hiding it destroys the Vide scope it was created in, which releases its render handle. |

The toolbar also holds three static `Search` labels in different colours: one
raster between them.

## Development

```bash
pnpm --filter rbxts-svg-example-vide run watch
```

That is `scripts/watch.mjs`: it compiles the SVGs once, then runs `rbxts-svg
watch` and `rbxtsc -w` side by side and ties their lifetimes together. They stay
independent processes — editing an `.svg` regenerates its module, which `rbxtsc`
already watches, so the rebuild chain works without either tool knowing about
the other, and `Toolbar.tsx` never changes.

Run them separately if you prefer:

```bash
pnpm --filter rbxts-svg-example-vide run svg:watch
```

```bash
pnpm --filter rbxts-svg-example-vide exec rbxtsc -w
```

## Rendering

`Toolbar.tsx` calls `installEditableImageRenderer()` once at startup. That is
the *framework-neutral* renderer from `@rbxts/svg` — there is no
`installVideSvgRenderer`, and an application drawing SVGs from both React and
Vide still calls it exactly once. After that every `<Svg>` draws through the
software rasterizer into a cached `EditableImage`, and both bindings share the
cache.

`src/client/main.client.tsx` mounts `<Toolbar />` into a `ScreenGui`, so this is
a project you can press Play on. Sync with Rojo and run the client. The manual
checklist is in
[`docs/STUDIO-VERIFY-VIDE.md`](../../docs/STUDIO-VERIFY-VIDE.md).

## Notes

- The package is named `rbxts-svg-example-vide`, not `@rbxts/…`. roblox-ts
  treats a scoped name as a *package* and emits `_G[script]` instead of a
  RuntimeLib require, which is wrong for a game.
- `preserveSymlinks` is set because pnpm symlinks workspace packages; see the
  pnpm section of [`docs/SVG-IMPORTS.md`](../../docs/SVG-IMPORTS.md).
- `compilerOptions.paths` pins `@rbxts/vide` to this project's own copy. Under
  pnpm the same physical package is also reachable through
  `@rbxts/svg-vide/node_modules/`, and roblox-ts finds that route first — which
  emits a `require` path that exists in this repository and nowhere else. It is
  one copy either way; the mapping only makes the emitted path the one a
  published install produces.
- `out/client` is mapped into `StarterPlayer.StarterPlayerScripts`. A
  `LocalScript` synced under `ReplicatedStorage` would never run.
