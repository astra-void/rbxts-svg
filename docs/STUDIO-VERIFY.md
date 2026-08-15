# Studio verification

The automated suites cover a lot of this pipeline — the Luau rasterizer is
compared byte-for-byte against the Rust reference on every test run, and the
`EditableImage` adapter is exercised through a fake factory — but neither runs
the Roblox engine. This is the checklist for the part only Studio can answer:
that `AssetService:CreateEditableImage`, `WritePixelsBuffer` and
`Content.fromObject` behave as the adapter assumes, and that icons actually
appear.

## Status

Performed on 2026-08-09 against a local Studio session, driven through the
Roblox Studio MCP tools. The whole example was run — `main.client.tsx` mounts
`<Toolbar />` through React, so every icon on screen came from a `<Svg>`
component fed by a direct `.svg` import. Results at the bottom.

The Vide binding has been through the same engine path; the parts that differ —
reactive scope lifetime, colour reactivity, conditional scope destruction — are
in [`STUDIO-VERIFY-VIDE.md`](STUDIO-VERIFY-VIDE.md). Everything here about
`EditableImage` itself applies to both, because it belongs to the core renderer
rather than to a framework.

Still unverified: a **published** experience, which is the one case the security
toggle below actually applies to.

> **Not yet re-run since the example changed.** `examples/react/src/Toolbar.tsx`
> now draws `Bell`, `ChevronDown`, `Search` and `Settings` from
> `@rbxts/lucide-react`, keeping one direct `.svg` import (`logo.svg`) beside
> them, so the icon inventory in the checklist and the counts at the bottom
> describe the previous shape. Everything they establish about the
> `EditableImage` path is unaffected — that path did not change — but a fresh
> session should re-measure rather than have numbers edited into it.
>
> The Lucide-specific items to add are listed in
> [`STUDIO-VERIFY-VIDE.md`](STUDIO-VERIFY-VIDE.md#lucide-checklist-in-addition-to-the-items-below)
> (L1–L6); the React-relevant ones are L1, L2, L5 and L6, plus the one that is
> React's own: three colours of one Lucide icon must produce **one** raster,
> **one** `EditableImage` and three `ImageColor3` values.
>
> L2 (no runtime XML), L4 (no placeholder raster) and L6 (what the eager barrel
> costs) are properties of the core and of the generated packages rather than of
> a framework, and the Vide session on 2026-08-10 measured all three. What a
> React session would add is the React lifecycle over the same assets.

## Requirements

`EditableImage` is not available unconditionally.

- **In Studio**, it works for the logged-in user with no extra setup.
- **In a published experience**, it "fails by default for security purposes." To
  enable it you must be **13+ age verified and ID verified**, and then toggle on
  **Enable Mesh / Image APIs** (Creator Dashboard → the experience's settings;
  the same toggle is surfaced in Studio under Experience Settings → Security).

  Verified against
  [`creator-docs/.../EditableImage.yaml`](https://github.com/Roblox/creator-docs/blob/main/content/en-us/reference/engine/classes/EditableImage.yaml)
  on 2026-08-09.

- **Maximum size is 1024×1024.** `@rbxts/svg` enforces the same limit in
  `EDITABLE_IMAGE_MAX_DIMENSION` and fails with an actionable message rather
  than letting the engine reject the allocation.

The asset-ownership rules you may have read about — that an image must be owned
by or shared with the experience owner, the Studio user, or the player — apply to
`AssetService:CreateEditableImageAsync`, which *loads* an existing image asset.
`@rbxts/svg` never calls it. It allocates a blank image with
`AssetService:CreateEditableImage` and writes pixels into it, so no asset
permissions are involved.

## Checklist

1. Build the example.

   ```bash
   pnpm --filter rbxts-svg-example-react run build
   ```

2. Sync it into Studio with Rojo (`rojo serve` from `examples/react`, then
   connect from the Rojo plugin) and open a place. See *Running it without
   Rojo* below for the two layout details that catch people out.
3. Run the client (F5, or Play). `main.client.tsx` mounts `<Toolbar />`.
4. Confirm the two `Search` icons appear, in white and red — same glyph, two
   colours.
5. Confirm `Settings` (blue, 32 px), `Bell` (amber, 24 px) and `ChevronDown`
   (grey, 32 px) appear.
6. Confirm each is crisp. Every size is rasterized at its own resolution, so
   nothing should look resampled.
7. Confirm the sixth icon — the green `Search` laid out with
   `UDim2.fromScale` — appears and fills its share of the toolbar.
8. Confirm no duplicate raster is created for a tint-only colour change:

   ```lua
   local stats = svg.getSvgRenderCache():stats()
   print(stats.entryCount, stats.referenceCount, stats.hits, stats.misses)
   ```

   Six components must report five entries: the two same-size `Search` labels
   share one alpha mask, so `referenceCount` exceeds `entryCount`.
9. Set `PlayerGui.SvgExample.ExampleScale.Scale` to 2 and back to 1, waiting a
   few frames each time. **Every** icon's raster must follow its `AbsoluteSize`
   and then return to its original size, and `entryCount` must not grow. That
   includes the fixed-`size` ones: `size={24}` sets the layout size, and an
   icon laid out at 48×48 must be rasterized at 48×48 rather than upscaled from
   24. Nothing on screen should look soft at `Scale = 2`.
10. Confirm the output window shows no `AssetService` or `WritePixelsBuffer`
    errors.

## What was observed

`Toolbar` mounted and drew all six `<Svg>` instances with **no `AssetService` or
`WritePixelsBuffer` errors** and nothing in the output window (item 10).
Visually (items 4–7): search in white and red at 24 px, bell in amber, settings
in blue at 32 px, chevron-down in grey at 32 px, and a green scale-driven
search — each crisp at its own resolution.

The probe then read the cache while changing a `UIScale`, which is what moves
`AbsoluteSize` and so exercises the measured-layout path:

```text
scale 1   entries=5 handles=6 hits=1 misses=5  24x24->24x24 24x24->24x24 24x24->24x24 32x32->32x32 32x32->32x32 *39.0x38.4->39x38
scale 2   entries=5 handles=6 hits=1 misses=6  48x48->24x24 48x48->24x24 48x48->24x24 64x64->32x32 64x64->32x32 *78.0x76.8->78x77
scale 1   entries=5 handles=6 hits=1 misses=7  24x24->24x24 24x24->24x24 24x24->24x24 32x32->32x32 32x32->32x32 *39.0x38.4->39x38
```

`*` marks the `UDim2`-driven icon; each pair is `AbsoluteSize -> raster`.

- **Item 8 holds.** Six components, five rasters: the two 24 px `Search` labels
  in white and red share one alpha mask (`hits=1`), and their colours are
  applied by `ImageColor3`.
- **Item 9 holds.** The scale-driven icon re-rasterizes to match its laid-out
  size (39×38 → 78×77 → 39×38) and lands back on exactly its original raster.
  `entries` stays at 5 throughout, so the previous raster is released as the new
  one is acquired — no accumulation. `misses` rises by exactly one per size
  change, not one per frame: `snapToPixels` rounds to whole pixels and the
  effect bails out when the snapped size is unchanged.
- Fixed-`size` icons kept their declared raster (24, 32) as `AbsoluteSize`
  grew, which this run recorded as correct on the grounds that `size={24}`
  means "rasterize at 24". **That was wrong and is fixed**: `size` is a layout
  size, the raster follows what was laid out, and an icon under a `UIScale` of
  2 is now rasterized at twice the resolution instead of being upscaled. See
  [`STUDIO-VERIFY-VIDE.md`](STUDIO-VERIFY-VIDE.md#the-2026-08-10-run).
- An earlier run of the same path without React confirmed the pixels are real
  rather than blank buffers: `ReadPixelsBuffer` showed coverage from 5.6 %
  (`chevron-down`) to 23.8 % (`settings`), with the tinted `Search` labels
  byte-identical to each other.

### Running it without Rojo

The session above was populated by walking `examples/react/out`, `include/` and
the resolved `node_modules` into an instance tree over HTTP, which is the same
thing Rojo does. Two layout details are worth knowing if you sync by hand:

- **pnpm hides `@rbxts-js`.** `@rbxts/react` does
  `require(script.Parent.Parent:WaitForChild("@rbxts-js").React)`, and under
  pnpm's default isolated linker those packages live in `node_modules/.pnpm/…`
  rather than beside `@rbxts`. `node-linker=hoisted` in `.npmrc` is the
  documented fix; the repository does not set it, because it would relink the
  whole workspace.
- **`out/client` is mapped into `StarterPlayerScripts`** by
  `examples/react/default.project.json`. A `LocalScript` under
  `ReplicatedStorage` never runs.
