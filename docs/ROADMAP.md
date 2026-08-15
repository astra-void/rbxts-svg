# Roadmap

What exists, what does not, and the order the rest should be built in.

## Implemented today

- **Compiler.** SVG → semantic model via `usvg`, with view box preservation,
  transform baking, `currentColor` detection, primitive lowering and structured
  diagnostics.
- **Serialized IR.** Versioned, compact, with encoder, decoder and a byte-layout
  specification ([`IR-FORMAT.md`](IR-FORMAT.md)).
- **Native bindings.** `compileSvg`, `decodeSvgIr`, `irVersion` over napi-rs.
- **`@rbxts/svg-compiler`.** Typed wrapper, generated-module emission,
  compilation cache, the shared source→generated path mapping
  (`@rbxts/svg-compiler/paths`, free of any native dependency), and the
  `rbxts-svg` CLI with `build` and `watch`.
- **`@rbxts/svg-transformer` and direct `.svg` imports.**
  `import Search from "./icons/search.svg"` works through real `rbxtsc`. The
  transformer rewrites static import and re-export specifiers onto the generated
  modules, shares one path mapping with the generator, and emits actionable
  diagnostics for a missing `.svg`, an unbuilt cache, a non-relative specifier
  or an unsupported dynamic import. `rbxts-svg build` also emits the ambient
  `*.svg` declaration that types the import as `SvgAsset` under plain `tsc`.
  See [`SVG-IMPORTS.md`](SVG-IMPORTS.md).
- **`@rbxts/svg`.** The opaque `SvgAsset`, a Luau IR decoder, and the
  reference-counted render cache.
- **`@rbxts/svg-react`.** `<Svg>` and `useSvg`, managing raster lifetime and
  driving raster resolution from the instance's `AbsoluteSize` when layout is a
  `UDim2`.
- **`@rbxts/svg-vide`.** The same `<Svg>` under Vide, with the raster's lifetime
  owned by a reactive scope rather than a component. Every transition hands over
  explicitly — acquire the new raster, publish it, *then* release the old — and
  only scope destruction releases from `cleanup`; Vide flushes a scope's
  cleanups before rerunning it, so releasing there would drop a shared entry's
  last reference immediately before the rerun asked for it again.
  `AbsoluteSize` is observed through Vide's own `changed()` action. The sizing policy both bindings use moved into
  `@rbxts/svg` (`resolveSvgSizing`, `snapSvgPixelSize`, `measureSvgPixelSize`)
  so they cannot disagree, and React's original `snapToPixels`/`svgSizing` names
  still resolve. Neither binding rasterizes before its first real measurement:
  an unlaid-out `AbsoluteSize` is `undefined`, not 1×1.
- **`svg-raster`.** The Rust reference rasterizer: adaptive cubic flattening,
  scanline fill with both fill rules, stroke expansion with every cap, join and
  the miter limit, coverage anti-aliasing, source-over compositing and a
  tintable alpha-mask path. It defines what the Luau renderer must reproduce.
- **The production Luau `EditableImage` renderer.** A stage-by-stage port of
  `svg-raster` under `packages/svg/src/raster/` (`geom`, `flatten`, `stroke`,
  `edges`, `coverage`, `image`, `render`), plus the Roblox adapter
  (`editableImageRenderer`) that allocates through
  `AssetService:CreateEditableImage`, writes one `WritePixelsBuffer` per
  raster, enforces the platform's 1024×1024 limit, and reports allocation
  failure actionably. Installed with `installEditableImageRenderer()`.
  Validated byte-for-byte against reference rasters generated from the Rust
  renderer on every Luau test run.
- **Runtime `currentColor`.** `SvgRenderOptions.currentColor` resolves
  `currentColor` paints at render time. Cache keys include the colour only for
  assets that mix `currentColor` with fixed paints; tintable assets share one
  alpha mask across every `ImageColor3`, and fixed-colour assets ignore colour
  entirely.
- **Tests.** Rust, Node and Luau suites, golden images, semantic snapshots,
  IR hashes, a differential comparison against `resvg`, a cross-language golden
  comparison of the Luau rasterizer against the Rust reference, and integration
  fixtures that run the real `rbxtsc` — including a watch-mode test that edits
  only an `.svg` and asserts the change reaches the emitted Luau while the
  importing source stays untouched.
- **`@rbxts/lucide-react` and `@rbxts/lucide-vide`.** The whole Lucide set
  (1,767 icons, 258 alias names) precompiled through the pipeline above by one
  shared generator in `tools/lucide`, from a pinned `lucide-static`. One compile
  pass feeds both packages, so their icon data is byte-identical and their asset
  hashes — and therefore their raster cache entries — are shared. No public
  `@rbxts/lucide`; see [`LUCIDE.md`](LUCIDE.md).
- **A real-engine smoke test.** The `EditableImage` path has been run in a live
  Studio session; see [`STUDIO-VERIFY.md`](STUDIO-VERIFY.md) for what was
  observed and what still needs checking in a published experience, and
  [`STUDIO-VERIFY-VIDE.md`](STUDIO-VERIFY-VIDE.md) for the Vide path.

## The build-out list

The original plan, in order, with what has since been built struck through.
Nothing still outstanding requires changing the asset model, the public API or
the compiler/runtime boundary.

### 1. ~~The Luau `EditableImage` renderer~~ — done

Built as a module-by-module port of `svg-raster`, sharing its constants
(`FLATNESS_TOLERANCE = 0.1`, depth limit 12, arc tolerance 0.1, 16
sub-scanlines) and its two documented approximations. The standalone Luau
suite compares its output against Rust-generated goldens and currently
requires byte-exact equality.

### 2. ~~`svg-raster`, a Rust reference renderer~~ — done

Built. See [`crates/svg-raster`](../crates/svg-raster). It consumes exactly the
same IR, and exists for:

1. a reference implementation of correct output,
2. golden image generation,
3. comparing the Luau renderer against it,
4. static rendering tooling, and
5. future Loom or browser use.

Rust cannot run inside Roblox, so the production renderer must be Luau. Keeping
both on the same input is what makes them comparable — and is why the IR is a
specification rather than an internal detail.

### 3. ~~`.svg` specifier rewriting~~ — done

Built as `@rbxts/svg-transformer`, and proven against actual emitted Luau rather
than against a transformed AST. `examples/react` imports its icons directly.
Implemented behaviour, the roblox-ts plugin contract it uses, and the
intentionally unsupported syntaxes are in [`SVG-IMPORTS.md`](SVG-IMPORTS.md).

### 4. ~~`@rbxts/svg-vide`~~ — done

A second UI binding, built to prove the architecture rather than to serve Vide
specifically: if `@rbxts/svg` is genuinely framework-neutral infrastructure,
adding a framework should touch no compiler, no IR version, no rasterizer, no
cache and no part of the `.svg` import system. It touched none of them.

What it did surface is the one thing shared code cannot settle for you — which
reactive reads may reach the renderer. See
[`ARCHITECTURE.md`](ARCHITECTURE.md#what-a-binding-owns).

### 5. ~~Lucide~~ — done, as two packages and no middle one

Built as [`@rbxts/lucide-react`](../packages/lucide-react) and
[`@rbxts/lucide-vide`](../packages/lucide-vide), generated from a pinned
`lucide-static@1.31.0` by one shared pipeline in [`tools/lucide`](../tools/lucide).
1,767 canonical icons and 258 alias names, every one compiling in strict mode
with zero warnings and every one a tintable alpha mask. See
[`LUCIDE.md`](LUCIDE.md).

The plan above wanted a framework-neutral `@rbxts/lucide` underneath the two
convenience packages. Building it showed that layer had no job: the
framework-neutral primitive is `SvgAsset`, which `@rbxts/svg` and direct `.svg`
imports already provide, so a published data package would exist only to be
depended on. What it was really protecting against — a generated artefact with a
framework baked into it — is instead prevented where it belongs, in the
generator: the icon modules are rendered **once** and written into both
packages, contain no framework vocabulary, and are byte-identical.

The cost of dropping it is that each package embeds its own copy of the compiled
bytes (0.52 MB packed, each). The property that actually matters at runtime
survives anyway, because an asset's cache identity is its content hash: a game
using both frameworks gets one `EditableImage` per icon and size, not two.

What the icon set did surface is a packaging fact rather than an architectural
one — roblox-ts has no tree shaking, so a root barrel over 2,000 modules is
eager. Both packages therefore document the per-icon subpath as the import to
reach for, with the emitted Luau and the numbers in [`LUCIDE.md`](LUCIDE.md).

### 6. Gradients

Reserved in the paint model (`Paint` is an enum), in the IR (paints are an
indexed table with a `kind` tag) and in the feature flags (`HAS_GRADIENT`).
Adding them is a version bump plus renderer work, not a redesign.

### 7. Clipping and masking

Reserved as `HAS_CLIP` and `HAS_MASK`. Both need the rasterizer first, since
both are compositing operations.

### 8. Bounded cache eviction

The cache today holds an entry exactly while something references it. A
size-bounded variant that retains unreferenced entries for reuse and evicts
under memory pressure layers on top of `SvgRenderCache` without changing its
interface.

## Deliberately out of scope

SVG animation, CSS stylesheets beyond what `usvg` resolves, filter effects, text
layout, embedded HTML, `<foreignObject>`, advanced compositing, embedded raster
images, and arbitrary external resource loading.

These are compile errors with actionable diagnostics, not silent omissions —
see the compatibility philosophy in the [README](../README.md).

## Suggested order

1. ~~**`svg-raster`** (Rust reference renderer)~~ — done. It was the fastest path
   to knowing what *correct* looks like, it is far easier to debug than Luau,
   and it produced the golden images everything after it is checked against.
2. ~~**The Luau `EditableImage` renderer**~~ — done, ported stage by stage and
   validated against Rust-generated goldens (currently byte-exact).
3. ~~**`.svg` specifier rewriting**~~ — done. `import Search from "./search.svg"`
   compiles through real `rbxtsc` in `examples/react` and `examples/vide`.
4. ~~**`@rbxts/svg-vide`**~~ — done. Deliberately ahead of Lucide: a second
   binding is what turns "framework-neutral" from an intention into a tested
   property, and it is much cheaper to find out that the core leaks a framework
   assumption now than after generating 1,700 icons against it. It paid off —
   the icon set needed no compiler, IR, rasterizer or cache change at all.
5. ~~**`@rbxts/lucide-react` and `@rbxts/lucide-vide`**~~ — done, from one shared
   generator and one compile pass. No framework-neutral middle package: see
   above.
6. **Gradients**, then clipping and masking. The first real feature gap left —
   nothing in the Lucide set needed either, so nothing is forcing the order.
7. **`stroke-dasharray`**, which is the next most commonly authored construct
   that currently fails to compile.
8. **Bounded cache eviction.** A 2,000-icon package makes the retention policy
   worth revisiting: an entry lives exactly as long as something references it,
   so a screen that swaps icon sets rasterizes the old ones again on the way
   back.

## Known limitations of the renderers

Both are shared, deliberately, by the Rust reference and the Luau port — that
is what keeps them byte-comparable. If either is ever improved, improve both
together and regenerate the goldens.

- **Vertical coverage is sampled, not analytical.** Coverage is exact along x
  and takes 16 sub-scanlines along y, so a feature that is both nearly
  horizontal and thinner than a pixel can be off by about 8 of 255 alpha levels.
  The coverage strategy sits behind an internal trait precisely so it can be
  replaced by analytical edge coverage without touching flattening, stroking or
  compositing.
- **Curves are flattened before being stroked.** A butt or square cap at the end
  of a *curve* is therefore perpendicular to the first flattened chord rather
  than to the true tangent, tilting it by a fraction of a pixel. Round caps —
  what every Lucide icon uses — are rotationally symmetric and unaffected.
  Stroking a polyline rather than offsetting a curve is what keeps the Luau port
  tractable, so this is a deliberate trade.
