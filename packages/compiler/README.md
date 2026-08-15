# `@rbxts/svg-compiler`

The build-time half of [rbxts-svg](https://github.com/astra-void/rbxts-svg): a
Rust SVG compiler behind a stable TypeScript API, plus the `rbxts-svg` CLI.

```bash
npm install --save-dev @rbxts/svg-compiler
```

The native binary ships prebuilt for macOS, Linux and Windows on x64 and arm64.
Using this package needs no Rust toolchain.

## The CLI

```bash
rbxts-svg build     # compile every .svg under rootDir into generated modules
rbxts-svg watch     # the same, incrementally, as files change
```

`rbxts-svg build` turns `src/icons/search.svg` into
`src/svg-cache/icons/search.svg.ts`, and emits the ambient `*.svg` declaration
that types the import as `SvgAsset` under plain `tsc`. Paired with
[`@rbxts/svg-transformer`](https://www.npmjs.com/package/@rbxts/svg-transformer),
that makes this work:

```ts
import Search from "./icons/search.svg";
```

Run it before `rbxtsc` — the first `rbxtsc` pass needs the generated modules to
exist:

```json
{
  "scripts": {
    "build": "rbxts-svg build && rbxtsc"
  }
}
```

Add `svg-cache/` to `.gitignore`. The generated modules are deterministic output
derived from the SVGs.

## The API

```ts
import { compileSvgFile } from "@rbxts/svg-compiler";

const asset = compileSvgFile("src/icons/search.svg");
asset.width;   // 24  (view box units, not pixels)
asset.hash;    // "77b89d1d…"  content hash, deterministic
asset.data;    // Buffer of serialized IR
```

`@rbxts/svg-compiler/paths` is the source→generated path mapping on its own. It
has no native dependency, which is why the transformer can share it without
loading a binary.

## It refuses rather than approximates

A `<filter>` cannot be rendered faithfully, so compilation *fails* instead of
quietly dropping it and producing a picture that does not match the source:

```text
Unsupported SVG feature in assets/logo.svg:

error: <filter> is not supported by @rbxts/svg yet (filter effects).
  --> assets/logo.svg:3:5

Element:
  <filter id="shadow">

Path:
  svg > defs > filter#shadow
```

Gradients, clipping, masking and `stroke-dasharray` are in this category today.
Animation, CSS beyond what `usvg` resolves, filter effects, text layout,
`<foreignObject>` and embedded raster images are out of scope by design.

## Determinism

The same SVG compiles to the same bytes on every machine: no system font
enumeration, no ambient state. That is what makes the content hash usable as a
cache key all the way through to the runtime raster cache.

## See also

- [`@rbxts/svg`](https://www.npmjs.com/package/@rbxts/svg) — the runtime
- [Direct `.svg` imports](https://github.com/astra-void/rbxts-svg/blob/main/docs/SVG-IMPORTS.md)
- [IR format](https://github.com/astra-void/rbxts-svg/blob/main/docs/IR-FORMAT.md)

## Licence

MIT.
