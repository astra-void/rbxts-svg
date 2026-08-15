# `@rbxts/svg-react`

React bindings for [`@rbxts/svg`](https://www.npmjs.com/package/@rbxts/svg).

```bash
npm install @rbxts/svg-react @rbxts/svg
```

```tsx
import Search from "./icons/search.svg";
import { Svg } from "@rbxts/svg-react";

<Svg source={Search} size={24} color={Color3.fromRGB(255, 255, 255)} />;
```

Install the renderer once at startup — it is framework-neutral, so an app
drawing from both React and Vide installs it once, not once per framework:

```ts
import { installEditableImageRenderer } from "@rbxts/svg";

installEditableImageRenderer();
```

## What this package is

A *lifetime adapter*, not a renderer. There is no React-specific compiler,
rasterizer or cache: this package ties a raster's lifetime to a component, and
everything else happens in `@rbxts/svg`. A React tree and a Vide tree in one game
share the same `EditableImage` per raster.

## API

**`<Svg>`** takes `source`, `size`, `color`, `strokeWidth` and
`absoluteStrokeWidth`, plus the usual Roblox layout props. Give it `Size` instead
of `size` and the raster follows the laid-out `AbsoluteSize`, so a scaled icon is
drawn at its real resolution rather than stretched:

```tsx
<Svg source={Search} Size={UDim2.fromScale(0.1, 0.1)} />
```

Nothing rasterizes before the first real measurement — an unlaid-out
`AbsoluteSize` is treated as unknown, not as 1×1.

**`useSvg`** is the same lifetime management without the component, for when you
need the raster itself.

**`color`** is the SVG `currentColor`, not a blanket tint. For a
monochrome-`currentColor` asset (every Lucide icon) it costs nothing: one shared
alpha mask serves every colour through `ImageColor3`.

**`strokeWidth`** is in view box units and scales with the icon. Add
`absoluteStrokeWidth` to pin it in output pixels — Lucide's semantics exactly.

## See also

- [`@rbxts/lucide-react`](https://www.npmjs.com/package/@rbxts/lucide-react) — the Lucide set, precompiled
- [`@rbxts/svg-compiler`](https://www.npmjs.com/package/@rbxts/svg-compiler) — the build-time compiler and `rbxts-svg` CLI
- [Repository and docs](https://github.com/astra-void/rbxts-svg)

## Licence

MIT.
