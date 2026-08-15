# `@rbxts/lucide-react`

The [Lucide](https://lucide.dev) icon set as React components for roblox-ts,
precompiled through [`@rbxts/svg`](../svg).

```bash
npm install @rbxts/lucide-react @rbxts/svg @rbxts/svg-react
```

```tsx
import Search from "@rbxts/lucide-react/icons/search";

<Search size={24} color={Color3.fromRGB(255, 255, 255)} />
```

Install the renderer once at startup, as for any `@rbxts/svg` drawing:

```ts
import { installEditableImageRenderer } from "@rbxts/svg";

installEditableImageRenderer();
```

## What an icon is

An ordinary compiled `SvgAsset` behind `<Svg>`. There is **no uploaded image
asset, no XML parsed in Roblox and no `Frame`-per-segment approximation** — each
icon went through the same Rust compiler into the same versioned IR, and is
drawn by the same rasterizer into the same shared raster cache as any `.svg` you
compile yourself.

You need **no Rust, no upstream Lucide package, and no `@rbxts/svg-transformer`**
to use this. The icons are compiled before publication; the transformer only
matters if your own project also imports its own `.svg` files.

## Props

Everything [`<Svg>`](../svg-react) takes, except `source` — the icon already is
one.

```tsx
<Search
	size={24}
	color={Color3.fromRGB(255, 255, 255)}
	strokeWidth={1.5}
	absoluteStrokeWidth
	Position={UDim2.fromScale(0.5, 0.5)}
	AnchorPoint={new Vector2(0.5, 0.5)}
/>
```

Under React these are ordinary values.

- **`size`** — square pixel size. Omit it and the icon draws at its view box,
  which is 24×24 for every Lucide icon; there is no second defaulting mechanism
  layered on top.
- **`Size`** — a Roblox `UDim2`. Wins over `size`, and the raster resolution
  then follows the laid-out `AbsoluteSize`. Nothing is rasterized until that
  first measurement arrives.
- **`color`** — the SVG `currentColor`, not a blanket tint. Every Lucide icon is
  a monochrome `currentColor` asset, so this is free: one shared alpha mask
  serves every colour through `ImageColor3`. A hundred colours of one icon is
  one `EditableImage`.
- **`strokeWidth`** — in view box units, so it scales with the icon.
- **`absoluteStrokeWidth`** — pins the stroke in output pixels instead, keeping
  its apparent weight at any size. Lucide's own semantics, handled by the core.

## Which import to use

```tsx
import Search from "@rbxts/lucide-react/icons/search";   // one module
import { Search } from "@rbxts/lucide-react";            // all of them
```

Both work. They do not cost the same, and this package does not claim to be
tree-shakable, because roblox-ts has no tree shaking: the root barrel compiles
to a module that requires every icon module in the package — 2,026 of them —
whatever you name in the braces. The per-icon subpath requires exactly one.

Prefer the subpath for a known set of icons: measured in Studio, that is about
9 ms and 0.7 MiB against about 170 ms and 11.7 MiB for the barrel. The barrel is
kept for convenience and for dynamic icon selection, and the measurements behind
that advice are in [`docs/LUCIDE.md`](../../docs/LUCIDE.md).

## Icons

1,767 canonical icons plus 258 upstream alias names, from `lucide-static`
1.31.0. An alias is a re-export of its canonical module, not a second copy of
the artwork, so `AlertCircle` and `CircleAlert` are the same asset and the same
raster.

Names are the upstream file names in PascalCase: `search` → `Search`,
`chevron-down` → `ChevronDown`, `a-arrow-down` → `AArrowDown`.

## Licence

The wrapper code is MIT. The icons are Lucide's, under the ISC licence, included
as [`LICENSE-lucide`](LICENSE-lucide). Generated from `lucide-static` 1.31.0.
