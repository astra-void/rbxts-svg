# `@rbxts/svg`

The framework-neutral half of [rbxts-svg](https://github.com/astra-void/rbxts-svg):
the compiled SVG asset, its Luau decoder, the shared raster cache, and the
production `EditableImage` renderer.

```bash
npm install @rbxts/svg
```

No UI framework is a dependency of this package. React and Vide bindings live in
[`@rbxts/svg-react`](https://www.npmjs.com/package/@rbxts/svg-react) and
[`@rbxts/svg-vide`](https://www.npmjs.com/package/@rbxts/svg-vide), and both draw
through what is here.

## What it gives you

**`SvgAsset`** — an opaque handle to an SVG compiled at build time by
[`@rbxts/svg-compiler`](https://www.npmjs.com/package/@rbxts/svg-compiler).
Consumers cannot read its bytes, which is what lets the serialization change
without breaking anyone.

**A renderer.** Install it once at startup, before anything draws:

```ts
import { installEditableImageRenderer } from "@rbxts/svg";

installEditableImageRenderer();
```

It allocates through `AssetService:CreateEditableImage`, rasterizes in Luau, and
writes one `WritePixelsBuffer` per raster. Its output is validated byte-for-byte
against a Rust reference rasterizer on every test run.

**A shared raster cache.** An asset's cache identity is its content hash, so the
same icon at the same size is one `EditableImage` no matter how many components —
or how many frameworks — ask for it. An entry lives exactly as long as something
references it.

**Runtime `currentColor`.** The compiler records whether an asset is monochrome.
If it is, the runtime rasterizes one alpha mask and tints it with `ImageColor3`
for any colour, instead of rasterizing again per colour.

**Sizing policy** — `resolveSvgSizing`, `snapSvgPixelSize` and
`measureSvgPixelSize`, shared by both bindings so they cannot disagree about what
pixel size a given layout implies.

## Limits worth knowing

Roblox caps an `EditableImage` at 1024×1024; the renderer enforces that and
reports an allocation failure with something you can act on. Gradients, clipping,
masking and `stroke-dasharray` are not implemented yet — they are compile errors
with a file, line and element, not silent omissions.

## Documentation

- [Architecture](https://github.com/astra-void/rbxts-svg/blob/main/docs/ARCHITECTURE.md)
- [IR format](https://github.com/astra-void/rbxts-svg/blob/main/docs/IR-FORMAT.md)
- [Roadmap](https://github.com/astra-void/rbxts-svg/blob/main/docs/ROADMAP.md)

## Licence

MIT.
