/**
 * What consuming compiled SVG assets looks like — both ways of getting one.
 *
 * ```text
 * import { Search } from "@rbxts/lucide-react";   precompiled, named
 * import Logo from "./icons/logo.svg";            your own artwork
 *                        │
 *                        ▼
 *          the same compiler, IR, rasterizer and cache
 * ```
 *
 * The Lucide icons need no build step here at all: `@rbxts/lucide-react` ships
 * them already compiled, so a project that only uses named icons never installs
 * `@rbxts/svg-transformer` and never runs `rbxts-svg build`.
 *
 * `logo.svg` is the other half, and it is why the transformer is still wired up
 * in this example: `pnpm run svg` compiles every `src/**` /*.svg into a
 * generated module under `src/svg-cache/`, and the transformer rewrites the
 * specifier onto it during `rbxtsc`. Nothing in this file has to know that. See
 * `docs/SVG-IMPORTS.md`.
 */

import React from "@rbxts/react";
import { Bell, ChevronDown, Search, Settings } from "@rbxts/lucide-react";
import { installEditableImageRenderer } from "@rbxts/svg";
import { Svg } from "@rbxts/svg-react";

import Logo from "./icons/logo.svg";

// The production renderer is installed once, explicitly, at startup — nothing
// installs it as an import side effect. In a real project this call belongs in
// your client entry script, before the first icon mounts. It is the same call
// whether the assets came from a package or from your own `.svg` files.
installEditableImageRenderer();

export function Toolbar(): React.Element {
	return (
		<frame Size={UDim2.fromOffset(340, 48)} BackgroundColor3={Color3.fromRGB(30, 30, 34)}>
			<uilistlayout
				FillDirection={Enum.FillDirection.Horizontal}
				Padding={new UDim(0, 8)}
				VerticalAlignment={Enum.VerticalAlignment.Center}
			/>

			{/* Lucide icons are monochrome currentColor assets, so the two
			    Searches below share ONE rasterized alpha mask and differ only
			    in ImageColor3. Adding a third colour would cost nothing. */}
			<Search size={24} color={Color3.fromRGB(255, 255, 255)} />
			<Search size={24} color={Color3.fromRGB(255, 90, 90)} />
			<Bell size={24} color={Color3.fromRGB(255, 200, 120)} />

			{/* A thinner stroke is geometry, so it is a separate raster. */}
			<Settings size={32} color={Color3.fromRGB(120, 170, 255)} strokeWidth={1.5} />

			{/* An absolute stroke keeps its 2px apparent weight at any size. */}
			<ChevronDown
				size={32}
				color={Color3.fromRGB(200, 200, 200)}
				strokeWidth={2}
				absoluteStrokeWidth
			/>

			{/* A UDim2 layout: the raster resolution follows the laid-out
			    AbsoluteSize, snapped to integers, so this icon stays crisp at
			    whatever size the parent gives it. Nothing is rasterized until
			    that first measurement arrives. */}
			<Search Size={UDim2.fromScale(0.08, 0.6)} color={Color3.fromRGB(140, 255, 170)} />

			{/* Arbitrary artwork, imported straight from the `.svg`. Fixed
			    multi-colour fills rather than currentColor, so `color` would do
			    nothing to it and it is not a shared alpha mask — the same
			    pipeline, a different kind of picture. */}
			<Svg source={Logo} size={32} />
		</frame>
	);
}
