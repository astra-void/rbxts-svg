# `@rbxts/svg-vide`

Vide bindings for [`@rbxts/svg`](https://www.npmjs.com/package/@rbxts/svg).

```bash
npm install @rbxts/svg-vide @rbxts/svg
```

```tsx
import Search from "./icons/search.svg";
import { Svg } from "@rbxts/svg-vide";

<Svg source={Search} size={24} color={Color3.fromRGB(255, 255, 255)} />;
```

Install the renderer once at startup — it is framework-neutral, so there is
deliberately no `installVideSvgRenderer`:

```ts
import { installEditableImageRenderer } from "@rbxts/svg";

installEditableImageRenderer();
```

Vide needs its own JSX factory. `Vide.jsx` is not React's and the two are not
interchangeable:

```json
{
  "compilerOptions": {
    "jsx": "react",
    "jsxFactory": "Vide.jsx",
    "jsxFragmentFactory": "Vide.Fragment"
  }
}
```

## What this package is

A *lifetime adapter*, not a renderer. It owns exactly one thing React's binding
does differently: a raster's lifetime belongs to a reactive scope rather than to
a component. Every transition hands over explicitly — acquire the new raster,
publish it, *then* release the old — and only scope destruction releases from
`cleanup`, because Vide flushes a scope's cleanups before rerunning it.

Everything else is `@rbxts/svg`. A Vide tree and a React tree in one game share
the same `EditableImage` per raster.

## API

**`<Svg>`** takes `source`, `size`, `color`, `strokeWidth` and
`absoluteStrokeWidth` — the same names and meanings as the React binding. Under
Vide **every prop may also be a source**, so `size={iconSize}` and `color={theme}`
reactively update the one label:

```tsx
<Svg source={Search} size={iconSize} color={theme} />
```

Give it `Size` instead of `size` and the raster follows the laid-out
`AbsoluteSize`, observed through Vide's own `changed()` action:

```tsx
<Svg source={Search} Size={UDim2.fromScale(0.1, 0.1)} />
```

**`color`** is the SVG `currentColor`, not a blanket tint. For a
monochrome-`currentColor` asset (every Lucide icon) one shared alpha mask serves
every colour through `ImageColor3`.

**`strokeWidth`** is in view box units; add `absoluteStrokeWidth` to pin it in
output pixels.

## See also

- [`@rbxts/lucide-vide`](https://www.npmjs.com/package/@rbxts/lucide-vide) — the Lucide set, precompiled
- [`@rbxts/svg-compiler`](https://www.npmjs.com/package/@rbxts/svg-compiler) — the build-time compiler and `rbxts-svg` CLI
- [Repository and docs](https://github.com/astra-void/rbxts-svg)

## Licence

MIT.
