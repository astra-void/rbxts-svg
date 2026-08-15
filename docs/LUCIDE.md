# Lucide

`@rbxts/lucide-react` and `@rbxts/lucide-vide` are the Lucide icon set,
precompiled through this repository's ordinary SVG pipeline.

```text
                       upstream Lucide SVGs
                                |
                         shared generator          tools/lucide
                                |
                     @rbxts/svg-compiler           (unchanged, strict)
                                |
                       compiled SvgAsset IR
                         /             \
                        v               v
         @rbxts/lucide-react    @rbxts/lucide-vide
                    |                    |
                    v                    v
          @rbxts/svg-react        @rbxts/svg-vide
                     \                  /
                      v                v
                         @rbxts/svg
                             |
                      one render cache
                             |
                       EditableImage
```

There is deliberately **no `@rbxts/lucide`**, and no `-core` or `-data` package
either. Framework-neutral vector data already has a home — `@rbxts/svg` plus
direct `.svg` imports — so a third published package would exist only to be
depended on. The shared icon representation exists, but it lives inside the
generator, not on npm.

## Upstream

| | |
| --- | --- |
| Package | `lucide-static` |
| Version | `1.31.0`, pinned exactly |
| Licence | ISC — redistributed in each package as `LICENSE-lucide` |
| SVG files | 2,025 |
| Canonical icons | 1,767 |
| Alias names | 258 |
| Exported names | 2,021 |

The version is pinned without a caret. An icon-set bump changes what the
published packages contain, so it should be a commit somebody made on purpose,
not something a fresh `pnpm install` does.

`lucide-static` ships `icons/*.svg` for canonical icons *and* alias names
together, with `icon-nodes.json` as its statement of which names are canonical.
It does **not** ship a deprecation flag — the upstream monorepo records that in
per-icon metadata the published package omits. So a live alias and a deprecated
one are indistinguishable from the pinned source.

**Alias and deprecation policy: ship every name upstream ships.** An alias costs
no compiled bytes here, because its generated module re-exports the canonical
one rather than carrying a second copy of the artwork:

```ts
// generated/icons/alert-circle.tsx
export { CircleAlert as AlertCircle } from "./circle-alert";
export { CircleAlert as default } from "./circle-alert";
```

so following upstream's compatibility surface costs one tiny module per alias
and nothing at runtime unless the alias is actually imported. Four aliases —
`arrow-down-01`, `arrow-down-10`, `arrow-up-01`, `arrow-up-10` — spell the same
identifier as the canonical icons they alias (`ArrowDown01` and friends). Each
still gets its own module, so the subpath import resolves under either spelling,
but the barrel exports the name once. That is why 2,025 files produce 2,021
exports.

## The compatibility result

Every canonical icon, compiled in strict mode with `allowUnsupported` never set:

| | |
| --- | --- |
| Compiled | 1,767 / 1,767 |
| Compile failures | 0 |
| Warnings | 0 |
| Tintable | 1,767 / 1,767 (100%) |
| View boxes | one: `0 0 24 24` |
| `preserveAspectRatio` | one: `xMidYMid meet` |
| IR version | 2 throughout |
| Unique hashes | 1,766 |
| Duplicate hashes | 1 group: `clock` = `clock-4` |
| Max shape count | 15 (`brain-cog`) |
| Serialized IR | 725,577 bytes total; mean 410.6, median 395, min 86, max 1,323 |
| Largest | `brain-circuit` 1,323 · `grip` 1,250 · `brain-cog` 1,212 · `grape` 1,167 · `hop` 1,167 |

Feature flags observed, and only these two combinations:

```text
21 × 1758   UsesCurrentColor | HasStroke | Monochrome
23 ×    9   UsesCurrentColor | HasFill | HasStroke | Monochrome
```

Full tintability is the load-bearing one. It means every Lucide icon rasterizes
to a single alpha mask that any `ImageColor3` tints correctly, so colour is not
in the render cache's key and recolouring is free. It is *measured* on every
generation run rather than assumed — if upstream ever ships a full-colour icon,
the generator says so loudly instead of quietly shipping it.

`clock` and `clock-4` are the same drawing under two canonical names. Both get
modules; both compile to one hash, so at runtime they are one cache entry.

## Import styles, and what they cost

roblox-ts has no tree shaking. This is measured from emitted Luau, not assumed.

**Root barrel — eager.** `packages/lucide-react/out/init.luau` re-exports the
generated barrel, which requires every icon module:

```lua
-- out/init.luau
for _k, _v in TS.import(script, script, "generated") or {} do
    exports[_k] = _v
end

-- out/generated/init.luau
exports.AArrowDown = TS.import(script, script, "icons", "a-arrow-down").AArrowDown
exports.AArrowUp   = TS.import(script, script, "icons", "a-arrow-up").AArrowUp
-- … 2,021 lines
```

so

```tsx
import { Search } from "@rbxts/lucide-react";
```

emits one `TS.import(…, "lucide-react", "out")` and initializes the whole set.

Measured in a live Studio session against the shipped Vide package, with nothing
else racing to require it first:

```text
subpath  @rbxts/lucide-vide/icons/search     8.7–9.9 ms    685 KiB
barrel   @rbxts/lucide-vide                161.5–168.9 ms   ~11.0 MiB
                                          2022 exports from 2027 ModuleScripts
```

The subpath figure is the *whole* cold chain — `@rbxts/svg`, the binding, Vide
and one icon. The barrel's ~162 ms and ~11 MiB are what the other 2,026 modules
add on top. Both are far larger than the standalone-`luau` decode figure
(~275 ms and 2.2 MiB for 1,767 assets, from `node tests/luau/lucide-bench.mjs`),
because a ModuleScript costs a great deal more than the `buffer` inside it —
which is precisely why this number had to come from Studio.

**Per-icon subpath — exact.**

```tsx
import Search from "@rbxts/lucide-react/icons/search";
```

emits

```lua
local Search = TS.import(script, …, "lucide-react", "out", "generated", "icons", "search").default
```

— one module, which itself requires only `@rbxts/svg` and the shared factory.
An alias module requires only its canonical icon. Resolution works through
`typesVersions` in each package's manifest, and is covered by real `rbxtsc`
fixtures in `tests/integration/lucide.test.ts`.

**Recommendation.** Use the subpath form for a known set of icons; that is what
the package documentation leads with. On the measurements above, an application
using a handful of icons pays about 9 ms and 0.7 MiB instead of about 170 ms and
11.7 MiB. The root barrel is kept because it is a genuinely nicer API and
because dynamic icon selection needs it, but it is documented as what it is.
Neither package claims to be tree-shakable.

A transformer that rewrote root named imports into subpath imports would give
the nice API at the exact cost — but it would be a fifth published package and a
second thing that has to stay correct, so it is a follow-up rather than part of
this work.

## What is generated, and what is not

```text
packages/lucide-react/
├── src/
│   ├── createLucideIcon.tsx      hand-written — the only framework code
│   ├── index.ts                  hand-written
│   └── generated/                owned entirely by the generator
│       ├── icons/*.tsx           2,025 modules
│       └── index.ts              the barrel
├── package.json                  hand-written
├── LICENSE                       MIT — the wrapper
└── LICENSE-lucide                ISC — upstream's, copied by the generator
```

A generated icon module is four meaningful lines:

```tsx
// Generated by tools/lucide from lucide-static 1.31.0 (icons/search.svg). Do not edit.
import { unstable_internal } from "@rbxts/svg";

import { createLucideIcon } from "../../createLucideIcon";

export const Search = createLucideIcon(
	unstable_internal.createAssetFromBase64("UlNWRwIA…", "5116d3eb…"),
);

export default Search;
```

The asset expression comes from `@rbxts/svg-compiler`'s
`generateAssetExpression` — the same function the `.svg`-import modules use, so
there is one definition of "serialized IR plus hash becomes an `SvgAsset`" and
not two.

The factory is the whole of the framework layer:

```tsx
export function createLucideIcon(asset: SvgAsset): LucideIcon {
	function LucideIcon(props: LucideIconProps): React.Element {
		return <Svg source={asset} {...props} />;
	}
	return LucideIcon;
}
```

with `LucideIconProps = Without<SvgProps, "source">`. There is no `size`
default, no colour default and no stroke handling here: `<Svg>` already falls
back to the view box (24×24 for every Lucide icon), `currentColor` already
defaults in the core, and `strokeWidth`/`absoluteStrokeWidth` are already
geometry the rasterizer and cache understand.

**Component names.** Every icon's component function is called `LucideIcon`.
That is a platform limit, not a choice: React identifies a function component by
a `displayName` property, a Luau function is not a table, and roblox-ts rejects
named function *expressions*, so a name cannot be applied at runtime or threaded
through the factory. Giving 2,000 icons 2,000 function names would mean
generating the factory 2,000 times to buy a name nothing reads. The module path
identifies an icon exactly.

## Why the two packages are byte-identical

`tools/lucide/src/emit.ts` renders the generated tree **once** and writes it
**twice**. Nothing it emits mentions React or Vide — both packages' icon modules
import `../../createLucideIcon`, and only that file differs between them.

So "the vector data must not differ between frameworks" is true by
construction rather than by comparison. The test still exists
(`tests/integration/lucide.test.ts` compares every icon in both trees, plus
every hash against the manifest), because a guarantee worth having is worth
checking.

The runtime consequence is the one that matters. Each package embeds its own
copy of every icon's bytes — that is the accepted cost of having no shared data
package — but an asset's cache identity is its *content hash*, so a game using
both frameworks gets one `EditableImage` per icon and size, not two.
`tests/luau/vide.luau` asserts it against a real cache with two separately
decoded assets, and `examples/cross-framework` demonstrates it in a real
project.

## Sizes

| | React | Vide |
| --- | --- | --- |
| Generated source | 1.76 MB | 1.76 MB |
| Published `out` (unpacked) | 2.51 MB | 2.51 MB |
| Published tarball | 0.52 MB | 0.52 MB |
| of which runtime `.luau` | 2.19 MB | 2.19 MB |
| of which `.d.ts` | 0.31 MB | 0.31 MB |
| Compiled IR across all icons | 725,577 bytes (≈968 KB base64) | same |

The duplication between the two packages is deliberate. Deduplicating it would
mean a third published package whose only job is to hold bytes, and the property
that actually matters at runtime — one raster for one icon — is already
guaranteed by content hashing.

## Generating

```bash
pnpm generate:lucide
```

One command, one compile pass, both packages. There is no
`generate:lucide-react` / `generate:lucide-vide` pair in the normal workflow:
generating separately would mean two compilations and two chances for the
outputs to diverge.

```bash
pnpm check:lucide
```

regenerates in memory, compares against what is committed, and fails if anything
is stale — an upstream bump nobody regenerated for, a hand-edited generated file,
or a manifest that no longer matches. That is what CI runs.

The manifest lives at `tools/lucide/manifest.json`. Both framework outputs are
generated from it, so "the two packages contain the same icons under the same
names" is structural. It is not published: consumers get components.

### If generation fails

Every failing icon is named, with its upstream file and the compiler's own
diagnostic:

```text
Lucide generation failed: 3 of 1767 icons

circle-x  (icons/circle-x.svg)
  …/lucide-static/icons/circle-x.svg
  error: <filter> is not supported by @rbxts/svg yet (filter effects).
  …
```

The fix is never an icon-specific case. `allowUnsupported` is never set, and no
name is special-cased anywhere in the generator: if a real Lucide icon exposes a
gap, the gap is generic and belongs in the compiler or the runtime, with a
generic test.

## Benchmarks

```bash
pnpm generate:lucide && node tests/luau/bundle.mjs && node tests/luau/lucide-bench.mjs
```

Standalone `luau` on the host, with a fake image factory — indicative of
algorithmic cost, not of Roblox device performance:

```text
barrel load        1767 assets in 274.6 ms (0.155 ms each), 2.22 MiB retained
cold search 24     0.01 ms, 1 rasterization(s)
cold settings 24   0.00 ms, 1 rasterization(s)
100 × Search 24    0.11 ms, 1 rasterization(s), 1 cache entry, 100 references
100 colours of one 0.12 ms, 1 rasterization(s), 1 cache entry
100 distinct icons 0.18 ms, 100 rasterization(s), 100 cache entries
100 Vide recolours 0.18 ms, 0 rasterization(s) after mount, 1 entry
```

A hundred instances of one icon is one raster. A hundred colours of one icon is
one raster. A hundred reactive recolours under Vide is *zero* further rasters —
the raster effect does not even wake, because it does not read the colour for a
tintable asset. A hundred distinct icons is a hundred rasters, which is the
honest floor: they are a hundred different pictures.

The same properties were confirmed in a live Studio session against the shipped
package, through a generated Lucide component rather than through `<Svg>`
directly: two colour changes across two labels moved the cache's miss count by
zero, and every size change cost exactly one raster with the previous one freed
as the new one arrived. See
[`STUDIO-VERIFY-VIDE.md`](STUDIO-VERIFY-VIDE.md#the-2026-08-10-run).
