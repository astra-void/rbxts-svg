# Studio verification — Vide

The companion to [`STUDIO-VERIFY.md`](STUDIO-VERIFY.md), for the second UI
binding. Everything in that document about `EditableImage` — the security
toggle a published experience needs, the 1024×1024 limit, why no asset
permissions are involved — applies unchanged, because it is a property of the
core renderer and not of any framework. This file covers only what is different
under Vide: that a reactive scope owns the raster's lifetime correctly, and that
reactivity does not quietly cost rasters.

## Status

Performed on 2026-08-09 against a local Studio session, driven through the
Roblox Studio MCP tools. `examples/vide` was built with the real `rbxts-svg
build` + `rbxtsc`, synced into a place, and run. Results at the bottom.

**Re-run on 2026-08-10** after two changes: the placeholder raster was removed,
and the example was rewritten to draw `@rbxts/lucide-vide` components alongside
one direct `.svg` import. Results in *[The 2026-08-10 run](#the-2026-08-10-run)*.
The 2026-08-09 numbers are kept below it for comparison.

Still unverified, exactly as for React: a **published** experience, which is the
one case the `EditableImage` security toggle applies to.

### Lucide checklist, in addition to the items below

- **L1. Named imports draw.** The Lucide icons render identically to the
  `.svg`-imported ones they replaced. Nothing about the pipeline changed —
  `@rbxts/lucide-vide` is `<Svg>` with the asset already bound.
- **L2. No runtime XML.** Confirm from the synced tree that
  `node_modules/@rbxts/lucide-vide/out/generated/icons/search` contains a base64
  IR blob and no `<svg` anywhere. There is no XML parser in the runtime at all,
  so a failure here would be a generation bug, not a rendering one.
- **L3. Reactive colour is still free.** Item 6 below, but on a generated Lucide
  component: pressing `colour` must not move `misses`. This is the one thing a
  wrapper could plausibly break, by introducing a reactive read of the colour
  that `<Svg>` deliberately avoids for a tintable asset.
- **L4. No placeholder.** Read the cache *immediately* after mount, before
  layout settles, and again after. The scale-driven `Settings` must contribute
  zero entries on the first read and exactly one on the second — never a 1×1.
- **L5. Cross-framework sharing.** Build and run `examples/cross-framework`,
  which mounts a React `Search` and a Vide `Search` from the two separate
  packages. Its client prints one line; it must report `entries=1` with
  `references=3`. Two packages, two copies of the bytes, one `EditableImage`.
- **L6. The barrel is eager.** `examples/vide` imports from the package root, so
  requiring it initializes every icon module. Time it against a single subpath
  import in the same session: the standalone-`luau` figure (~275 ms to decode
  1,767 assets) does not include ModuleScript `require` overhead, which is the
  part only Studio can measure. It is the evidence behind the advice in
  [`LUCIDE.md`](LUCIDE.md) to prefer per-icon subpath imports.

## The 2026-08-10 run

Driven through the Roblox Studio MCP tools against a place built with
`rojo build examples/vide/verify.project.json`, which maps each package's `out`
explicitly rather than syncing the workspace directories that contain them —
see *Two copies of the core* below, and note that this run is the reason that
section now matters more than it did.

### What it found first: a second core

Adding `@rbxts/lucide-vide` to the example gave pnpm a *new* route to
`@rbxts/svg`, and TypeScript took it. The emitted Luau read

```lua
local _svg = TS.import(script, …, "@rbxts", "lucide-vide", "node_modules", "@rbxts", "svg", "out")
```

so the application would have loaded one core and the binding another: two
caches, two renderer registries, two of every raster, and no error. The whole
test suite passed — the assertion that should have caught it named
`@rbxts/svg-vide` specifically, and the nesting had moved to `lucide-vide`.

Fixed by pinning `@rbxts/svg` and the binding in both examples'
`compilerOptions.paths`, and the test now rejects *any* require that walks into
a second `node_modules` rather than one particular package's.

### Rendering

Ten `ImageLabel`s, all drawn, with **no `AssetService` or `WritePixelsBuffer`
errors** and nothing from the application in the output window: three `Search`
at 24 px in white, red and green; a fourth and fifth `Search` whose colour and
size the buttons drive; `Settings` in blue at 32 px with a 1.5 stroke;
`ChevronDown` in grey at 32 px with `absoluteStrokeWidth`; an amber `Settings`
laid out with `UDim2.fromScale`; an amber `Bell`; and the fixed multi-colour
`logo.svg` at 32 px. Each crisp at its own resolution (L1).

### The placeholder is gone (L4)

The counter in the toolbar samples the cache at *component construction*, before
Roblox has laid anything out. The settled state is read a second later:

```text
at construction   rasters: 5
settled           entries=6 refs=10 hits=4 misses=6
scale-driven icon Size={0.1, 0}, {0.7, 0}   AbsoluteSize=44 × 39.2   drawn
```

Five rasters existed when the tree was built; the `UDim2`-driven `Settings`
contributed **none**. The sixth appeared only when its `AbsoluteSize` arrived.

`misses == entries` is the whole result. Every rasterization this session
performed is still in the cache, so nothing was created and thrown away — which
is exactly what the 2026-08-09 run's `misses=6` against `entries=5` recorded the
absence of.

### The buttons

One action per reading, with layout allowed to settle first:

```text
settled          entries=6 refs=10 hits=4 misses=6    10 labels, 10 drawn
colour ×1        entries=6 refs=10 hits=4 misses=6    ← unchanged; tints changed
colour ×2        entries=6 refs=10 hits=4 misses=6    ← unchanged
size 24→32       entries=7 refs=10 hits=4 misses=7    ← +1; 24×24 kept, still referenced
size 32→48       entries=7 refs=10 hits=4 misses=8    ← +1; 32×32 freed as 48×48 arrived
bell off         entries=6 refs=9  hits=4 misses=8    ← handle released, raster destroyed
bell on          entries=7 refs=10 hits=4 misses=9    ← +1
```

- **Item 6 / L3 holds, through a generated Lucide component.** Two colour
  changes across two labels moved `misses` by zero while the `ImageColor3`
  values on screen changed. The wrapper preserves the fast path: `<Svg>`'s
  raster effect never reads the colour for a tintable asset, and
  `createLucideIcon` adds nothing that would.
- **Item 7 holds.** One raster per new size, and `entries` grew only when the
  old raster was still referenced by another label.
- **Item 8 holds.** Hiding the bell took `refs` 10 → 9 and `entries` 7 → 6.

### Measured layout (item 9)

```text
scale 1   entries=6 refs=10 misses=6   AbsoluteSize 44 × 39.2    raster 44×39
scale 2   entries=6 refs=10 misses=7   AbsoluteSize 88 × 78.4    raster 88×78
scale 1   entries=6 refs=10 misses=8   AbsoluteSize 44 × 39.2
```

`entries` never grows: the previous raster is released as the new one is
acquired. `misses` rises by exactly one per change, not one per frame.

**And this is where the second bug turned up.** The fixed-`size` icons were laid
out at 48×48 under `Scale = 2` and kept their 24×24 raster — a 24-pixel image
displayed at twice its size, which is visibly blurry and was the whole point of
having a resolution-dependent rasterizer to avoid. The 2026-08-09 run recorded
the same thing and called it correct on the grounds that `size={24}` means
"rasterize at 24".

It does not. `size` is a *layout* size; the raster follows what the engine
actually laid out, in every case. Both bindings now observe `AbsoluteSize`
whether or not a `UDim2` decided the layout, using the declared size only as the
resolution to draw at until the first measurement arrives — so nothing waits a
frame, and an icon laid out at exactly its declared size measures the resolution
it is already drawing at and never re-rasterizes. See `resolveSvgSizing`'s
`initialPixels`.

### What the barrel costs (L6)

Measured with the example's own client disabled, so nothing raced the probe to
require the package first:

```text
subpath  @rbxts/lucide-vide/icons/search     8.7–9.9 ms    685 KiB
barrel   @rbxts/lucide-vide                161.5–168.9 ms   ~11.0 MiB
                                          2022 exports from 2027 ModuleScripts
```

The subpath figure is the *whole* cold chain — `@rbxts/svg`, `@rbxts/svg-vide`,
Vide and one icon. The barrel's ~162 ms and ~11 MiB are what the other 2,026
modules add on top, and they are far larger than the standalone-`luau` decode
figure (~275 ms, 2.2 MiB) because a ModuleScript costs more than a `buffer`.
This is the measurement behind [`LUCIDE.md`](LUCIDE.md)'s recommendation.

### No runtime XML (L2)

Sampled every seventh icon module in the shipped package — 290 modules, canonical
and alias:

```text
containing "<svg" / "viewBox=" / "stroke-linecap"        0
containing createAssetFromBase64 (compiled IR)         248
alias re-exports of a canonical module                  42
```

There is no XML parser in the runtime at all; this confirms there is nothing for
one to parse either.

### Tintability of the shipped assets

Six of the ten labels carried a non-white `ImageColor3`, and the binding writes
a colour there *only* for a tintable asset — every other kind is pinned to
white. Together with three differently coloured 24 px `Search` labels resolving
to one cache entry, that is the shipped package's tintability demonstrated
behaviourally rather than read off the generator's report.

## What the automated suites already cover

Do not re-check these by hand. `tests/luau/vide.luau` runs the *compiled*
binding against the *real* `@rbxts/vide` with Roblox datatypes and instances
mocked, and asserts acquire/release counts, cleanup ordering, strict-mode
double evaluation, the three colour paths, pixel snapping and cross-framework
cache sharing. What Studio adds is the engine: real `AssetService`, real layout,
real `Content.fromObject`.

## Checklist

1. Build the example.

   ```bash
   pnpm --filter rbxts-svg-example-vide run build
   ```

2. Sync it into Studio with Rojo (`rojo serve` from `examples/vide`, then
   connect from the Rojo plugin) and open a place. See *Running it without
   Rojo* below.
3. Run the client (F5, or Play). `main.client.tsx` mounts `<Toolbar />` through
   Vide's `mount`.
4. Confirm nine icons appear: three `Search` at 24 px in white, red and green;
   a fourth `Search` whose colour the `colour` button changes; a fifth whose
   size the `size` button changes; `Settings` in blue at 32 px with a 1.5
   stroke; `ChevronDown` in grey at 32 px with `absoluteStrokeWidth`; an amber
   `Settings` laid out with `UDim2.fromScale`; and an amber `Bell`.
5. Confirm each is crisp. Every size is rasterized at its own resolution, so
   nothing should look resampled.
6. **Colour reactivity.** Press `colour` twice. The two reactive `Search` icons
   must change colour with no flicker and no disappearance, and the `rasters:`
   counter — which reads the shared cache's miss count — **must not move**. A
   tintable asset's colour is an `ImageColor3` write, not a new raster.
7. **Size reactivity.** Press `size`. The fifth `Search` steps 24 → 32 → 48.
   The counter must rise by exactly one per new size, and the entry count must
   not grow: the previous raster is released as the new one is acquired.
8. **Conditional scope destruction.** Press `bell`. The `Bell` disappears, its
   handle is released and its raster destroyed. Press again: it returns, and one
   new raster is made. This is the case that only a real dynamic scope
   exercises — unmounting the whole app would not distinguish a correct cleanup
   from a leaked one.
9. **Measured layout.** Set `PlayerGui.SvgExampleVide.ExampleScale.Scale` to 2
   and back to 1, waiting a few frames each time. The `UDim2`-driven icon's
   raster must follow its `AbsoluteSize` and then return to its original size,
   and the entry count must not grow. The fixed-`size` icons keep their declared
   raster — that is correct, not a bug.
10. **One core.** Read the cache from a script inside the running game:

    ```lua
    local include = game:GetService("ReplicatedStorage").rbxts_include
    local TS = require(include.RuntimeLib)
    local svg = TS.import(script, include.node_modules["@rbxts"].svg.out)
    print(svg.getSvgRenderCache():stats())
    ```

    The reference count must equal the number of live `<Svg>` labels. If it is
    zero while icons are on screen, the binding and the application have loaded
    two copies of `@rbxts/svg` — see *Two copies of the core* below.
11. Confirm the output window shows no `AssetService` or `WritePixelsBuffer`
    errors.

## The 2026-08-09 run

The original session, kept for comparison. It predates both the placeholder fix
and the Lucide rewrite of the example, so its counts describe a nine-icon
toolbar drawn entirely from direct `.svg` imports.

### What was observed

All nine `<Svg>` instances drew, with **no `AssetService` or
`WritePixelsBuffer` errors** and nothing from the application in the output
window (item 11). Visually (items 4–5): search in white, red and green at 24 px,
a red reactive search, a red 48 px search, settings in blue at 32 px, chevron
down in grey, an amber scale-driven settings and an amber bell — each crisp at
its own resolution.

The cache was read through a probe script running *inside* the game (item 10),
which is also what answers the one-core question:

```text
initial          entries=5 handles=9 hits=4 misses=6
                 24x24->24,24 (×5)  32x32->32,32 (×2)  48.0x39.2->48,39  24x24->24,24
```

Nine labels, five rasters, and `handles=9` read from the *application's* import
of `@rbxts/svg` while every one of those handles was taken by the *binding's*
import. One core, one cache. `misses=6` against `entries=5` is the scale-driven
icon's transient 1×1 raster, taken before layout ran and released once its real
`AbsoluteSize` arrived — **the one cache miss this run was not supposed to
have.** It is what prompted the fix; the 2026-08-10 run above reads
`misses == entries`, with nothing created and discarded.

Then, pressing each button and re-reading:

```text
colour ×2        entries=5 handles=9 hits=4 misses=6    ← unchanged
size (24→32)     entries=6 handles=9 hits=4 misses=7
size (32→48)     entries=6 handles=9 hits=4 misses=8    ← 32×32 freed as 48×48 arrived
bell off         entries=5 handles=8 hits=4 misses=8
bell on          entries=6 handles=9 hits=4 misses=9
scale 1→2        entries=6 handles=9 hits=4 misses=10   ← 48.0x39.2 → 96.0x78.4 -> 96,78
scale 2→1        entries=6 handles=9 hits=4 misses=11   ← back to 48,39
```

- **Item 6 holds, and it is the headline result.** Two colour changes across two
  labels moved `misses` by zero. The `ImageColor3` values changed on screen
  while the rasters did not, so a Lucide icon's colour genuinely costs nothing
  under Vide — the raster effect never even wakes, because it does not read the
  colour for a tintable asset.
- **Item 7 holds.** 24 → 32 → 48 cost exactly one raster each, and `entries`
  stayed at 6 rather than climbing: the previous raster was released as the new
  one was acquired. The 24×24 entry survived the first step because four other
  labels still referenced it, which is the reference counting doing its job.
- **Item 8 holds.** Hiding the bell took `handles` 9 → 8 and `entries` 6 → 5, so
  the scope's destruction released the handle and the last reference destroyed
  the image. Showing it again cost one raster and restored both counts.
- **Item 9 holds.** The scale-driven icon re-rasterized to match its laid-out
  size (48×39 → 96×78 → 48×39) with `misses` rising by exactly one per change —
  not one per frame. `AbsoluteSize` was `96.0×78.4` and the raster `96×78`, so
  the snapping the binding relies on is doing the rounding.
- **Fixed-`size` icons kept their declared raster** as the scale grew: at
  `Scale = 2` the 24 px icons were laid out at 48×48 and still drew their 24×24
  raster. This was recorded as correct, on the grounds that `size={24}` means
  "rasterize at 24". **That was wrong, and it is fixed** — see the
  [2026-08-10 run](#the-2026-08-10-run). A 24×24 image displayed at 48×48 is an
  upscale, and `size` is a layout size rather than a raster size.

### Running it without Rojo

The session above was populated by walking `examples/vide/out`, `include/` and
the resolved `node_modules` into an instance tree over HTTP, which is the same
thing Rojo does. Vide is a pure Luau package with no sibling-implementation
folder, so the `@rbxts-js` complication described in
[`STUDIO-VERIFY.md`](STUDIO-VERIFY.md) does not arise here. Two details are
still worth knowing:

- **`out/client` is mapped into `StarterPlayerScripts`** by
  `examples/vide/default.project.json`. A `LocalScript` synced under
  `ReplicatedStorage` never runs.
- **Studio's command bar has its own module cache.** Requiring `@rbxts/svg`
  from it loads a *second* copy with its own render cache, which reports zero
  references while the game is drawing perfectly. Probe from a script inside the
  game, as item 10 does.

### Two copies of the core

Worth knowing about, and no longer hypothetical — the 2026-08-10 run hit exactly
this, by a route nobody had thought about. `TS.getModule` resolves
a package by walking up from the calling script to the nearest
`node_modules/@rbxts`, so if a synced tree contains `@rbxts/svg-vide` *with its
own nested `node_modules`* — which a pnpm workspace link exposes and a published
install does not — the binding will find that nested `@rbxts/svg` while the
application finds the top-level one. Two caches, two renderer registries, two of
every raster, and no error to tell you.

Adding `@rbxts/lucide-vide` created a *second* such route — `@rbxts/svg` is now
reachable through the Lucide package's nested `node_modules` as well as the
binding's — and the compiler took it, silently, while every test passed.

The symptom is item 10 reporting `handles=0` with icons on screen, or the
emitted Luau naming two `node_modules` in one require path. There are two fixes
and both are wanted: sync the packages' `out` folders rather than the workspace
directories that contain them, *and* pin the shared packages in
`compilerOptions.paths` so the compiler cannot pick the nested route in the
first place. `examples/vide/tsconfig.json` pins the compile-time half of the
same problem with a `paths` mapping; see the note there.
