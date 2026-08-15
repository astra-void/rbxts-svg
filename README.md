# `@rbxts/svg`

First-class SVG support for the [roblox-ts](https://roblox-ts.com) ecosystem.

SVG files are compiled **at build time** — parsed, normalized and lowered into a
compact vector IR by a Rust compiler — and shipped to Roblox as small, opaque
assets that a runtime renderer draws with `EditableImage`.

> **Status: end-to-end, and multi-framework.** The compiler, the asset format,
> the Rust *reference* rasterizer, the production Roblox `EditableImage`
> renderer, direct `.svg` imports and two UI bindings — **React** and **Vide** —
> are all implemented and tested. The Luau rasterizer's output is validated
> byte-for-byte against the Rust reference, and the full engine path — direct
> `.svg` import → `<Svg>` → `AssetService:CreateEditableImage` → software
> rasterizer → `WritePixelsBuffer` → `Content.fromObject` → `ImageLabel` — has
> been run in a live Studio session ([`docs/STUDIO-VERIFY.md`](docs/STUDIO-VERIFY.md));
> it has not yet been confirmed in a *published* experience, which needs an extra
> security toggle. On top of all that sit **`@rbxts/lucide-react`** and
> **`@rbxts/lucide-vide`**: the whole Lucide icon set, precompiled through this
> same pipeline — 1,767 icons and 258 alias names, every one of them compiling
> in strict mode and every one a tintable alpha mask. See
> [`docs/LUCIDE.md`](docs/LUCIDE.md) and [`docs/ROADMAP.md`](docs/ROADMAP.md).

---

## Why compile SVG at build time?

An SVG is a document format. Turning one into pixels means parsing XML,
resolving CSS and inherited properties, flattening a tree, and interpreting a
20-command path grammar — none of which a game should be doing at runtime, and
some of which is impractical in Luau at all.

Compiling ahead of time buys three things:

**It is cheaper.** All the parsing, transform resolution and primitive lowering
happens once, on a developer's machine. The runtime receives a flat command
stream with four opcodes. A Lucide icon is about 200 bytes.

**It can refuse.** A `<filter>` cannot be rendered faithfully, so the compiler
*fails* rather than quietly dropping it and producing a picture that does not
match the source. You find out at build time, with the file, line and element:

```text
Unsupported SVG feature in assets/logo.svg:

error: <filter> is not supported by @rbxts/svg yet (filter effects).
  --> assets/logo.svg:3:5

Element:
  <filter id="shadow">

Path:
  svg > defs > filter#shadow
```

**It knows things the runtime can exploit.** The compiler can tell that an icon
is monochrome `currentColor` and record it in the asset. The runtime then
rasterizes one alpha mask and tints it with `ImageColor3` for any colour,
instead of re-rasterizing per colour.

---

## Architecture

```text
SVG source
    ↓  usvg
normalized SVG
    ↓
semantic vector IR          (svg-core: framework-neutral, no I/O)
    ↓
optimization / lowering
    ↓
compact serialized IR       (svg-ir: versioned, Luau-friendly bytes)
    ↓
SvgAsset
```

One asset, many consumers:

```text
                              SvgAsset
                                 │
                       @rbxts/svg runtime
                    renderer + shared raster cache
                                 │
          ┌──────────────────────┼──────────────────────┐
          │                      │                      │
          ▼                      ▼                      ▼
        React                   Vide                   Loom
  @rbxts/svg-react        @rbxts/svg-vide         future backend
          ▲                      ▲
          │                      │
  @rbxts/lucide-react   @rbxts/lucide-vide
          ▲                      ▲
          └────────┬─────────────┘
             shared generator
                   ▲
              Lucide SVGs
```

The Lucide packages are convenience on top, not a layer underneath: an icon is
an ordinary `SvgAsset` that this repository compiled instead of you. There is
deliberately no `@rbxts/lucide` in the middle — anyone wanting framework-neutral
icon data already has `@rbxts/svg` and direct `.svg` imports, and a third
package would exist only to be depended on.

A binding is a *lifetime adapter*, not a renderer. There is no Vide-specific
compiler, rasterizer or cache, and none of them is a different kind of thing
from the others — a React tree and a Vide tree in one game consume the same
`SvgAsset` and share one `EditableImage` per raster.

`SvgAsset` is opaque. Consumers cannot read its bytes, so the serialization can
change — packed bytes, fixed-point, a denser command stream — without breaking
anyone. See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the full design
and [`docs/IR-FORMAT.md`](docs/IR-FORMAT.md) for the byte layout.

---

## Packages

| Package | Kind | What it is |
| --- | --- | --- |
| [`crates/svg-core`](crates/svg-core) | Rust | The semantic model. No Node, Roblox, React or serialization. |
| [`crates/svg-compiler`](crates/svg-compiler) | Rust | SVG → semantic model, via `usvg`. Diagnostics live here. |
| [`crates/svg-ir`](crates/svg-ir) | Rust | The versioned serialization format, plus its decoder. |
| [`crates/svg-raster`](crates/svg-raster) | Rust | The reference rasterizer. Defines correct output; not the Roblox renderer. |
| [`crates/svg-node`](crates/svg-node) | Rust | napi-rs bindings. Small on purpose. |
| [`packages/compiler`](packages/compiler) | TS | `@rbxts/svg-compiler` — build tooling over the native binary. |
| [`packages/svg`](packages/svg) | roblox-ts | `@rbxts/svg` — the asset, the decoder, the render cache, and the production `EditableImage` rasterizer. |
| [`packages/svg-react`](packages/svg-react) | roblox-ts | `@rbxts/svg-react` — the React lifecycle/UI adapter: `<Svg>` and `useSvg`. |
| [`packages/svg-vide`](packages/svg-vide) | roblox-ts | `@rbxts/svg-vide` — the Vide lifecycle/UI adapter: `<Svg>`, bound to Vide scopes. |
| [`packages/transformer`](packages/transformer) | TS | `@rbxts/svg-transformer` — rewrites `./icon.svg` imports onto their generated modules. |
| [`packages/lucide-react`](packages/lucide-react) | roblox-ts | `@rbxts/lucide-react` — the Lucide set as React components, precompiled. |
| [`packages/lucide-vide`](packages/lucide-vide) | roblox-ts | `@rbxts/lucide-vide` — the same set as Vide components. |
| [`tools/lucide`](tools/lucide) | TS | The generator behind both. Never published; the only thing here that knows what "Lucide" is. |

No UI framework is a dependency of `@rbxts/svg`. The arrows only ever point
inwards — `svg-react → svg` and `svg-vide → svg`, never the reverse and never
between the bindings — which is what makes framework-neutrality a structural
fact rather than a claim. `tests/integration/vide.test.ts` asserts it.

You never need `cargo` or `rustc` to use `@rbxts/svg`: the native compiler ships
prebuilt per platform.

---

## Usage

### Implemented today

Compile an SVG from Node:

```ts
import { compileSvgFile } from "@rbxts/svg-compiler";

const asset = compileSvgFile("src/icons/search.svg");
asset.width;      // 24  (view box units, not pixels)
asset.hash;       // "77b89d1d…"  content hash, deterministic
asset.data;       // Buffer of serialized IR
```

Import an SVG directly from roblox-ts:

```ts
import Search from "./icons/search.svg";
import { getViewBox, isTintable } from "@rbxts/svg";

getViewBox(Search);   // { x: 0, y: 0, width: 24, height: 24 }
isTintable(Search);   // true — one raster, any ImageColor3
```

That needs two things in the project: `rbxts-svg build` before `rbxtsc`, and the
transformer registered in `tsconfig.json`.

```json
{
  "compilerOptions": {
    "rootDir": "src",
    "plugins": [{ "transform": "@rbxts/svg-transformer" }]
  }
}
```

```bash
rbxts-svg build && rbxtsc
```

`rbxts-svg build` compiles each `src/icons/search.svg` into a generated module at
`src/svg-cache/icons/search.svg.ts`, and the transformer points the import at it.
You rarely need to name that path yourself — it exists because a real
TypeScript file is what keeps the `.svg` inside the compiler's dependency graph,
which is what makes incremental and watch builds correct.
[`docs/SVG-IMPORTS.md`](docs/SVG-IMPORTS.md) has the details.

Rasterize from Rust, which is how the reference output is produced:

```rust
use svg_raster::{RasterMode, RasterOptions, render};

// A tintable icon: rasterize the coverage once, colour it per instance.
let options = RasterOptions::square(24).with_mode(RasterMode::AlphaMask);
let image = render(&document, &options)?;   // straight RGBA8, 24 * 24 * 4 bytes
```

Render inside Roblox. Install the production renderer once at startup:

```ts
import { installEditableImageRenderer } from "@rbxts/svg";

installEditableImageRenderer();
```

That call is framework-neutral: an application drawing SVGs from both React and
Vide makes it once, not once per framework. There is no
`installVideSvgRenderer`.

Then draw. **React:**

```tsx
import Search from "./icons/search.svg";
import { Svg } from "@rbxts/svg-react";

<Svg source={Search} size={24} color={Color3.fromRGB(255, 255, 255)} />
```

**Vide:**

```tsx
import Search from "./icons/search.svg";
import { Svg } from "@rbxts/svg-vide";

<Svg source={Search} size={24} color={Color3.fromRGB(255, 255, 255)} />
```

The import is the same import — the transformer does not know or care which
framework consumes the asset — and `source`, `size`, `Size`, `color`,
`strokeWidth` and `absoluteStrokeWidth` mean the same thing in both. Only the
lifecycle differs: React ties a raster to a component, Vide ties it to a
reactive scope. Under Vide every prop may also be a source, so
`size={iconSize}` and `color={theme}` reactively update the one label.

Vide needs its own JSX factory in `tsconfig.json` — `Vide.jsx` is not React's
and the two are not interchangeable:

```json
{
  "compilerOptions": {
    "jsx": "react",
    "jsxFactory": "Vide.jsx",
    "jsxFragmentFactory": "Vide.Fragment"
  }
}
```

Either binding takes a Roblox-native layout too, where the raster follows the
laid-out `AbsoluteSize` rather than the view box — so a scaled icon is drawn at
its real resolution:

```tsx
<Svg source={Search} Size={UDim2.fromScale(0.1, 0.1)} />
```

**Colour.** `color` is the SVG `currentColor`, not a blanket tint. For a
monochrome-`currentColor` asset (every Lucide icon) it is free: one shared
alpha-mask raster serves every colour through `ImageColor3`, so
`<Search color={red} />` and `<Search color={blue} />` share one
`EditableImage`. An asset that mixes `currentColor` with fixed paints
rasterizes once per distinct colour, and an asset with no `currentColor`
ignores `color` entirely.

**Stroke width.** `strokeWidth` is in view box units and scales with the icon;
add `absoluteStrokeWidth` to pin it in output pixels so the line keeps its
apparent weight at any size — Lucide's semantics exactly.

**Platform requirements and limits.** Rendering uses
`AssetService:CreateEditableImage` + `WritePixelsBuffer` and displays through
`Content.fromObject`, so no uploaded image assets and no asset moderation are
involved. `EditableImage` memory is device-budgeted: if allocation fails, or a
raster larger than the platform's 1024×1024 `EditableImage` limit is requested,
`renderSvg` throws an actionable error rather than silently clamping or
rendering nothing.

**Caching.** Rasters are cached by asset hash, integer pixel size, stroke
override, renderer version — and colour only when it can actually change the
pixels. Entries are reference-counted and destroyed deterministically when the
last component using them unmounts. A cold rasterization is pure Luau scanline
work (no yielding); warm requests are a table lookup.

### Icons

The whole Lucide set, already compiled. **React:**

```tsx
import { Search, Settings, ChevronDown } from "@rbxts/lucide-react";

<Search size={20} />
<Settings size={24} strokeWidth={1.5} />
<ChevronDown size={16} color={Color3.fromRGB(255, 255, 255)} />
```

**Vide:**

```tsx
import { Search, Settings, ChevronDown } from "@rbxts/lucide-vide";

<Search size={20} />
<Settings size={24} strokeWidth={1.5} />
<ChevronDown size={16} color={themeColour} />
```

An icon component is `<Svg>` with the asset already bound, so `size`, `Size`,
`color`, `strokeWidth`, `absoluteStrokeWidth` and every ordinary layout property
mean exactly what they mean above — including, under Vide, being sources.

There is **no uploaded image asset, no XML parsed in Roblox, and no
`Frame`-per-segment approximation.** Each icon went through the same Rust
compiler into the same IR, and is drawn by the same rasterizer into the same
shared cache. Consumers need no Rust, no upstream Lucide package, and — unless
they also import their own `.svg` files — no `@rbxts/svg-transformer`.

**Which import to use.** roblox-ts has no tree shaking, so naming icons in
braces from the package root loads *every* icon module. Measured in a live
Studio session: **~9 ms and 0.7 MiB** for one icon through a subpath import
(including `@rbxts/svg`, the binding and Vide from cold), against **~170 ms and
~11.7 MiB** for the barrel. For a handful of icons, use the per-icon subpath:

```tsx
import Search from "@rbxts/lucide-react/icons/search";
import Search from "@rbxts/lucide-vide/icons/search";
```

which requires exactly one module. [`docs/LUCIDE.md`](docs/LUCIDE.md) has the
emitted Luau, the numbers and the reasoning.

### Your own artwork versus Lucide

They are the same pipeline reached two ways, and it is worth keeping the
distinction crisp:

```tsx
// arbitrary SVG — compiled by your build, via rbxts-svg + the transformer
import Logo from "./logo.svg";
import { Svg } from "@rbxts/svg-react";

<Svg source={Logo} />
```

```tsx
// Lucide — compiled by this repository, shipped precompiled
import { Search } from "@rbxts/lucide-react";

<Search />
```

Vide has the corresponding pair. Direct `.svg` imports are not replaced by the
icon packages; `examples/react` and `examples/vide` use both.

---

## Supported SVG subset

### Supported

- `viewBox`, and `width`/`height` as a fallback coordinate system
- `preserveAspectRatio`
- `<path>`, and the full path grammar: `M L H V C S Q T A Z`
- `<rect>` (including `rx`/`ry`), `<circle>`, `<ellipse>`, `<line>`,
  `<polyline>`, `<polygon>` — all lowered to path geometry
- `<g>`, `<use>`, `<symbol>`, `<svg>` nesting, `<switch>`
- `transform` on any element, including nesting
- `fill`, `stroke`, `fill-opacity`, `stroke-opacity`, `opacity`
- `fill-rule` — both `nonzero` and `evenodd`
- `stroke-width`, `stroke-linecap`, `stroke-linejoin`, `stroke-miterlimit`
- `paint-order`
- `currentColor`, detected and preserved rather than resolved away
- `<style>` and CSS presentation attributes (handled by `usvg`)
- `<title>`, `<desc>`, `<metadata>` and editor namespaces — ignored, as they do
  not affect rendering

### Not supported — these are compile errors, not silent omissions

Gradients, patterns, filters, masks, clipping paths, `stroke-dasharray`,
markers, text, embedded raster images, `<foreignObject>`, SVG animation, and
non-`normal` blend modes.

Each produces a diagnostic naming the file, line, element and element path. Pass
`allowUnsupported` to downgrade them to warnings and compile what remains.

### Approximated — these warn

- **Group opacity** is folded into each child's paint. Identical unless the
  children overlap, and warned about when a group has more than one child.
- **Stroke width under a non-uniform or skewed transform** uses an average scale
  factor, because no single width is correct for such a transform.

---

## Development

```bash
pnpm install
pnpm build:native   # cargo build + napi bindings (needs a Rust toolchain)
pnpm build          # native + all TypeScript packages
pnpm test           # Rust, Node and Luau suites
```

Individual suites:

```bash
cargo test --workspace                        # Rust suites + golden images
pnpm test:node                                # vitest integration tests
pnpm test:luau                                # real Luau: decoder, cache, rasterizer,
                                              # byte-exact goldens vs the Rust renderer,
                                              # and the Vide binding against real @rbxts/vide
UPDATE_GOLDEN=1 cargo test -p svg-compiler    # regenerate golden files
cargo clippy --workspace --all-targets
```

The Lucide packages are generated, and both come out of one compile pass:

```bash
pnpm generate:lucide    # regenerate @rbxts/lucide-react and @rbxts/lucide-vide
pnpm check:lucide       # fail if the committed output is stale — what CI wants
```

`generate:lucide` writes both packages' `src/generated/` trees, prunes any icon
upstream has dropped, and prints the compatibility statistics for the set.
`check:lucide` regenerates in memory and compares, so an upstream bump nobody
regenerated for is a build failure rather than a surprise. The same check runs
inside `pnpm test:node`. Bumping the icon set means changing the exact pinned
`lucide-static` version in `tools/lucide/package.json` and regenerating; see
[`docs/LUCIDE.md`](docs/LUCIDE.md).

```bash
node tests/luau/lucide-bench.mjs   # what the icon set costs to load and draw
```

Contributing to the compiler requires a Rust toolchain
([rustup](https://rustup.rs)); using `@rbxts/svg` does not.

## Licence

MIT.

The Lucide icons — in `@rbxts/lucide-react`, `@rbxts/lucide-vide` and the test
fixtures — are from [lucide-static](https://lucide.dev) and are ISC licensed.
Both published icon packages declare `MIT AND ISC` and ship upstream's licence
as `LICENSE-lucide` alongside this one.
