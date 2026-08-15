# Architecture

Why the pieces are shaped the way they are. For the byte layout see
[`IR-FORMAT.md`](IR-FORMAT.md); for what is not built yet, [`ROADMAP.md`](ROADMAP.md).

## The central idea

Everything hangs off one abstraction: a **framework-neutral compiled asset**.

```text
SVG source
    ↓  usvg
normalized SVG
    ↓
semantic vector IR
    ↓
optimization / lowering
    ↓
compact serialized IR
    ↓
SvgAsset
```

Everything downstream of it is a consumer, and every consumer is equal:

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
```

The branch happens at the *binding* layer and nowhere earlier. A binding owns
the lifetime of a shared raster and the mapping from props to instance
properties; it does not own an asset format, a rasterizer or a cache, and it
cannot — those live one level in, where there is only one of each. That is why
a React tree and a Vide tree in the same game share an `EditableImage`, and why
adding the third arrow above required no change to the compiler, the IR, the
rasterizer or the `.svg` import system.

Convenience sits *on top of* a binding, never beneath it:

```text
                existing SVG infrastructure

                      @rbxts/svg
                     /          \
                    /            \
          @rbxts/svg-react   @rbxts/svg-vide
                 ^                 ^
                 |                 |
     @rbxts/lucide-react  @rbxts/lucide-vide
                 ^                 ^
                  \               /
                   shared generator
                         ^
                         |
                    Lucide SVGs
```

Note what is *not* in that picture: an `@rbxts/lucide` between the generator and
the two packages. The framework-neutral thing already exists and is called
`SvgAsset`; a published package holding icon bytes would add a dependency edge
and a version to keep in step without adding an abstraction. The sharing that
matters happens above npm, in the generator — one compile pass renders one tree
that is written into both packages — and below it, in the render cache, where an
asset's identity is its content hash rather than its object identity. Two
packages, two copies of the bytes, one raster. See [`LUCIDE.md`](LUCIDE.md).

`SvgAsset` is opaque to consumers. That is not fussiness — the serialization is
*expected* to change (fixed-point coordinates, delta-encoded command streams,
compression) and none of those changes should be visible to anyone holding an
asset. Everything in the public TypeScript API is therefore expressed in terms
of an opaque brand plus accessor functions.

## Crate boundaries

### `svg-core` — the semantic model

The vocabulary the rest of the project is written in. One dependency
(`bitflags`), and that is meant to stay true. No Node, napi-rs, Roblox, React,
or serialization.

Crucially it models **vector graphics, not SVG documents**. By the time
something is expressed in these types, XML, CSS, groups, `use` references,
inherited presentation attributes and primitive shapes are gone. What remains:

```rust
pub struct SvgDocument {
    pub view_box: ViewBox,
    pub shapes: Vec<Shape>,       // painter's order
    pub features: FeatureFlags,
}

pub struct Shape {
    pub geometry: Path,           // view box space, transforms baked in
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
    pub paint_order: PaintOrder,
}
```

Geometry and paint are separate fields so a future renderer can consume outlines
without caring about colour (stroke expansion, bounds, hit testing) and vice
versa.

> **Deviation from the original spec.** The spec sketched `paths: Vec<Path>`.
> A path with no paint cannot be drawn, so shapes carry both, with geometry
> still held separately *within* the shape. This satisfies "geometry separate
> from paint" while keeping the association a renderer needs.

### `svg-compiler` — source to model

```text
SVG bytes ──▶ roxmltree ──┬──▶ source scan  (diagnostics, currentColor)
                          │
                          └──▶ usvg  ──▶ normalize ──▶ optimize ──▶ SvgDocument
```

The XML is parsed **once** and shared: the scan needs source positions usvg
discards, and usvg needs a tree the scan already built. `usvg` re-exports both
`roxmltree` and `tiny_skia_path`, so the compiler always speaks to usvg in
exactly the types it was compiled against.

Three problems needed solving here.

#### usvg discards the view box

usvg resolves a document to a pixel `Size` and folds the view-box-to-viewport
mapping into the root group's transform. Every path's `abs_transform()` then
includes it, so geometry arrives in *pixel* space.

That is exactly wrong for a resolution-independent asset. So the compiler parses
`viewBox` itself, reconstructs the same transform usvg applied, inverts it, and
composes that with each path's absolute transform. Geometry lands in view box
space.

The reconstruction is `svg_core::view_box_transform` — the *same* function the
reference rasterizer uses to map an asset onto a target rectangle. usvg's own
`ViewBox::to_transform` is `pub(crate)`, so it has to be reproduced somewhere;
reproducing it once, in the crate every renderer already depends on, is what
stops the compiler's idea of viewport fitting and a renderer's from drifting
apart.

The source `width`/`height` attributes are then dropped entirely: they describe
a default presentation size, and consumers always supply their own at render
time.

`preserveAspectRatio`, however, is **kept**. Undoing usvg's mapping is only half
the job: a renderer handed a target rectangle of a different shape has to know
whether the author meant "stretch" or "letterbox", and the view box alone cannot
say. A 24x12 asset drawn into a 100x100 square is a completely different picture
under `xMidYMid meet` and under `none`. So the authored policy travels with the
geometry, through the semantic model and the serialized IR to every runtime.
(Version 1 of the IR dropped it, which made the two indistinguishable.)

#### usvg resolves `currentColor` away

usvg resolves `currentColor` against the inherited `color` property and returns
a plain `Paint::Color`, with no record of where it came from. Losing that would
cost the entire tinting fast path.

There is no hook, so the compiler uses a sentinel: before parsing, it injects
the stylesheet `svg{color:#7B2DF1}` via `usvg::Options::style_sheet`; afterwards,
any paint equal to that colour must have been a `currentColor`.

Guard rails, so this cannot mislead:

- The stylesheet is injected **only** if the source mentions `currentColor` at
  all. A document that cannot benefit also cannot be harmed.
- It is **not** injected if the root `<svg>` sets `color` itself, since our rule
  would otherwise override a deliberate author decision (CSS beats presentation
  attributes).
- A document that genuinely paints with `#7B2DF1` *and* uses `currentColor`
  would misreport that paint as tintable. This is the one accepted residual
  risk, and it is documented at the constant.

#### usvg drops unsupported content silently

usvg is a renderer's front end: it skips what it cannot use. We need the
opposite. So a source-level scan walks the `roxmltree` document looking for
elements we cannot draw, with real line and column numbers, and the tree walk
adds a backstop for content that reaches the tree indirectly (via `<use>`).

The two overlap on purpose. Diagnostics carry a stable feature key so one
problem reported by both is collapsed to one message — keeping whichever report
has a source location.

Content inside `<defs>` is only an error if something actually references it. An
unused gradient definition is reported as *ignored*, not as a failure, because
rejecting a perfectly renderable file over dead markup would be wrong.

### `svg-ir` — the wire format

The semantic model is shaped for the compiler's convenience. This format is
shaped for the **decoder's**, and the decoder that matters is written in Luau
and runs inside Roblox.

That means little-endian scalars (Luau's `buffer.readf32` is little-endian),
fixed-stride tables so element *i* is at `base + i * STRIDE`, and counts up
front so bounds are validated once rather than per element.

Keeping the two representations separate is what lets the compiler's internals
change freely while the runtime format stays a stable, versioned contract.

The format is explicitly versioned, and the decoder **rejects** a version it
does not implement rather than guessing.

### `svg-node` — the native boundary

A boundary, not a layer with opinions: `compileSvg`, `decodeSvgIr`,
`irVersion`. Nothing else.

The semantic Rust model is deliberately not projected into JavaScript. Doing so
would make every internal refactor a breaking change for npm consumers, and JS
tooling has no use for it — what tooling needs is the compact blob plus enough
metadata to cache and route it.

### `svg-raster` — the reference renderer

Consumes `svg-core`'s model and produces RGBA. Its only dependency is that
model: no image codecs, no SVG parser, no platform graphics library, because a
specification with dependencies is a specification nobody can reproduce.

It is a *reference*, not the production renderer — see
[The reference rasterizer](#the-reference-rasterizer) below for what that
distinction buys.

## Canonical geometry

The runtime understands four commands:

```text
MoveTo   LineTo   CubicTo   Close
```

The compiler lowers the whole SVG path grammar into them:

```text
H, V     →  LineTo
Q, T     →  CubicTo   (degree elevation — exact)
S        →  CubicTo   (explicit control points)
A        →  CubicTo   (one or more segments)
```

`usvg` already does most of this and already lowers `rect`/`circle`/`ellipse`/
`line`/`polyline`/`polygon` into paths, so there is no reason to reimplement it.
What remains is elevating quadratics, which `tiny-skia-path` still emits.

**Curves stay curves.** How finely a cubic must be subdivided depends on the
pixel size it will be drawn at, which the compiler does not know and must not
guess. Flattening belongs to the rasterizer — see
[`svg-raster`'s `flatten`](../crates/svg-raster/src/flatten.rs), where the
tolerance is expressed in output pixels and so means the same thing at every
size.

Subpath structure is preserved exactly. Fill rules depend on it, so subpaths are
never merged or reordered.

## Feature flags and the tinting fast path

Compiled assets carry a `u32` bitset: `USES_CURRENT_COLOR`, `HAS_FILL`,
`HAS_STROKE`, `HAS_EVEN_ODD_FILL`, `MONOCHROME`, `HAS_TRANSPARENCY`,
`HAS_STROKE_FIRST`, plus reserved bits for gradients, clips, masks and dashes.

`MONOCHROME` means **every visible paint in the asset is the same paint value**.
That is the precise condition under which the asset can be rasterized once as a
coverage mask and recoloured afterwards: with a single paint, colour scales the
whole image uniformly, so `ImageColor3` reproduces any tint exactly. Opacity may
still vary between shapes — that lives in the mask's alpha, not the tint.

`MONOCHROME + USES_CURRENT_COLOR` is *tintable*, which every Lucide icon is.

## The render cache

Caching is a design requirement, not an optimization added later.

```text
search.svg, 24×24, strokeWidth 2
             │
             ▼
  one cached alpha raster
             │
     ┌───────┼───────┐
     ▼       ▼       ▼
   white    red    blue     (ImageColor3, no re-rasterization)
```

The cache key is: **asset identity, pixel size, geometry-affecting overrides,
renderer version**. Colour is deliberately absent — including it would rasterize
the same icon once per colour, exactly the cost this design exists to avoid.
(For a non-tintable asset colours are already baked into the paints, so they are
covered by the asset identity.)

Asset identity is the content hash when one is known, which is what lets two
imports of the same icon share a single raster.

Ownership is explicit. Every entry is reference counted; `acquire` hands out a
handle, `release` decrements, and at zero the image is destroyed immediately. An
`EditableImage` holds real memory, so its lifetime should be deterministic
rather than left to the collector. Handles are idempotent: releasing twice
cannot free an image someone else is using.

Sizes are snapped to whole pixels before keying, or an animated size would miss
the cache every frame.

Nothing in the key names a framework, which is what makes cross-framework
sharing fall out rather than need building: `<Svg source={Search} size={24} />`
in React and the same in Vide produce the same key and therefore the same
image.

### What a binding owns

```text
      props                                    instance
        │                                          ▲
        ▼                                          │
  which reads may reach the renderer          ImageContent
        │                                     ImageColor3
        ▼                                          │
   renderSvg(asset, options) ──▶ handle ───────────┘
        │
        ▼
   release, exactly once, when the lifetime ends
```

The two bindings differ only in what "lifetime" means — a React component, a
Vide reactive scope — and in how they observe `AbsoluteSize`. Everything that
decides *what* is drawn is shared: `resolveSvgSizing` in
`packages/svg/src/render/sizing.ts` answers `size` versus `Size` for both, and
`resolveRenderOptions` converts `absoluteStrokeWidth` for both. Neither binding
contains arithmetic of its own, which is the cheapest way to guarantee they
cannot drift.

The subtle part is which reactive reads are *allowed* to reach the renderer.
Under React that is a dependency array; under Vide it is which sources a scope
touches. Both arrive at the same rule — the `currentColor` is a raster
dependency only for an asset that mixes `currentColor` with fixed paints, never
for a tintable one — because for a tintable asset the release/acquire pair
around a recolour would momentarily drop the last reference to the very image
the fast path exists to keep.

The ordering of that pair is the second thing a binding owns, and under Vide it
is not the obvious one:

```text
effect rerun:        acquire new ─▶ publish new ─▶ release old
scope destruction:   cleanup ─▶ release current
```

Not "acquire in the effect, release in its cleanup". Vide flushes a scope's
cleanups *before* rerunning it, so a cleanup-based release would drop a shared
entry's last reference immediately before the rerun asked for the same entry
again — destroying and re-rasterizing an image for nothing, on every change and
twice per mount under strict mode. Only the *end* of a lifetime releases from
`cleanup`. React reaches the same place by a different route: its effect
cleanup runs after the new render has already committed.

The third is what to do before a size is known. A `UDim2` layout cannot be
resolved to pixels until the instance has taken part in layout, and until then
the answer is `undefined` rather than a number: both bindings acquire nothing,
show `Content.none`, and rasterize once, at the first real `AbsoluteSize`.
`measureSvgPixelSize` is where that judgement lives, shared, because "zero is
not a very small size" is not a React or a Vide opinion. Vide needs it most —
its `changed()` action fires the callback at instance creation, before the
instance is parented, so the zero measurement is guaranteed rather than merely
likely.

## The reference rasterizer

`svg-raster` is not the renderer that runs inside Roblox. It is the definition
of what that renderer is supposed to produce.

The eventual Luau `EditableImage` renderer has to be written against something.
Against a prose specification it would be approximately right; against a working
implementation whose output can be diffed pixel by pixel, "approximately"
becomes measurable. Every decision in it — the flattening tolerance, the
coverage scheme, the alpha convention, the way strokes are built — is made to be
*reproducible in Luau* rather than to be as fast as a CPU rasterizer could be.

```text
view box + preserveAspectRatio + target size  ──▶  transform
                                                       │
path commands ────────────────────────────────────────┤
                                                       ▼
                                         adaptive cubic flattening
                                                       │
                             ┌─────────────────────────┴────────────┐
                             ▼                                      ▼
                   fill contours (closed)                  stroke expansion
                             │                                      │
                             └────────────────┬─────────────────────┘
                                              ▼
                                    directed edge set
                                              ▼
                             scanline coverage, nonzero / evenodd
                                              ▼
                                 source-over compositing
                                              ▼
                                      RGBA or alpha mask
```

Three choices are worth stating explicitly.

**A stroke is an area, not a line.** It is expanded into the region it covers
and then filled, through the same scan conversion as any fill. That is what
makes caps, joins, self-overlap and anti-aliasing behave identically for both
rather than being solved twice. The expansion emits a *set* of overlapping
pieces — one quadrilateral per segment, one wedge per join, one per cap, all
wound the same way — and lets the non-zero rule take their union, which
sidesteps every inner-corner special case an offset outline would need.

**Alpha is premultiplied internally and straight on the way out.**
Premultiplied makes source-over one expression per channel with no division;
straight RGBA8 is what `EditableImage.WritePixelsBuffer` wants, and it is what
makes an alpha mask tintable — a mask is only reusable if its RGB survives
independently of its alpha. Blending happens on sRGB-encoded values rather than
linearised ones, matching what every other SVG renderer does.

**Coverage is exact in x and sampled in y.** Sixteen sub-scanlines per pixel
row, with each span contributing its true fractional width to the pixels it
partly covers. The strategy sits behind an internal trait so it can be replaced
with analytical edge coverage later without disturbing flattening, stroking or
compositing.

Correctness is checked three ways: geometry unit tests that never look at a
pixel, golden images, and a differential comparison against `resvg`. Every case
with no partial coverage — axis-aligned fills, both fill rules, butt and square
caps, miter and bevel joins — matches resvg **exactly**. See
[`crates/svg-raster/tests/differential.rs`](../crates/svg-raster/tests/differential.rs)
for the measured numbers and the two known sources of the rest.

`resvg` appears only in that crate's dev-dependencies, as a judge. Rendering the
original SVG through it and calling that our output would exercise none of our
compilation, none of our IR, and none of the architecture the Luau backend
inherits.

## Determinism

The same source, compiler version and options produce byte-identical output on
every machine. Nothing reads the clock, the environment, the filesystem layout
or a random source; no ordering depends on hashing (the paint table is interned
through a `BTreeMap` and ordered by first use); and usvg's system-font
enumeration is disabled at the dependency level, which would otherwise make
output machine-dependent.

Content hashes are BLAKE3 over the serialized IR. Hashing the *output* rather
than the source means two SVGs differing only in whitespace share a hash, and a
compiler change that does not alter output does not invalidate caches.

`crates/svg-compiler/tests/golden/hashes.txt` pins the hash of every fixture.

## Testing strategy

Four layers, deliberately independent:

| Layer | Where | What it proves |
| --- | --- | --- |
| Rust unit | in-crate `#[cfg(test)]` | lowering, transforms, paint, flags, format constants, and every rasterizer stage in isolation |
| Rust fixtures | `crates/*/tests/` | real SVGs through the real pipeline |
| Semantic golden | `crates/svg-compiler/tests/golden/` | readable model snapshots + stable IR hashes |
| Raster golden | `crates/svg-raster/tests/golden/` | exact pixels, as the target for the Luau port |
| Differential | `crates/svg-raster/tests/differential.rs` | our output against `resvg`, on stated metrics |
| Node integration | `tests/integration/` | the published TypeScript API, via napi |
| Luau runtime | `tests/luau/` | the **compiled Luau** decoding **real compiler output** |
| Vide lifecycle | `tests/luau/vide.luau` | the **compiled binding** against the **real `@rbxts/vide`** |

Raster correctness deliberately does not rest on golden images. A golden says
*something changed*; the geometry tests in `crates/svg-raster/src/` say *what*,
without a pixel in sight — flattening tolerance, offset construction, miter
fallback, arc geometry, winding, parity, implicit closure, degenerate edges.
Those are what will make the Luau port debuggable.

The Vide layer is the same idea applied to a framework binding. Its assertions
are about which requests reach the renderer and when the results are released —
never about pixels, which belong to the core — and they run the real reactive
library rather than a stand-in, because a stand-in would agree with whatever the
binding happened to do. Vide's cleanup ordering (flushed *before* a rerun) and
its strict mode (every scope evaluated twice) are exactly the behaviours a
resource-owning binding has to be correct against.

The Luau layer is the other important one. `crates/svg-ir` writes the bytes and
`packages/svg` reads them back with an entirely separate implementation in a
different language. The Luau suite runs the actual `rbxtsc` output under the
`luau` CLI against IR produced by the actual Rust compiler, which is what makes
the format contract more than a comment.

## Deviations from the original specification

| Spec | Built | Why |
| --- | --- | --- |
| `SvgDocument { paths }` | `{ shapes }` | Paint has to travel with geometry; geometry is still a separate field. |
| `tests/{compiler,ir}/` | `crates/*/tests/` | Cargo requires integration tests inside their crate. Fixtures stay shared at `tests/fixtures/`. |
| `search.<hash>.svg.ts` | `search.svg.ts` | A hash in the path changes the import specifier on every edit. The hash lives in the header instead. See [`SVG-IMPORTS.md`](SVG-IMPORTS.md). |
| `.svg-cache/` | `svg-cache/` | TypeScript's `include` globs skip dot-directories, which would silently exclude every generated module. |
| `$internal` | `unstable_internal` | roblox-ts reserves `$`-prefixed identifiers. |
