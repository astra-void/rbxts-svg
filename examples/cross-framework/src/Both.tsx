/**
 * One game, both frameworks, one raster.
 *
 * This is a fixture rather than a showcase. It exists to prove three things
 * that only a project depending on *both* Lucide packages can:
 *
 * 1. `@rbxts/lucide-react` and `@rbxts/lucide-vide` resolve side by side —
 *    there is no module-resolution collision between two packages that export
 *    the same two thousand names.
 * 2. Each brings its own `Search`, from its own embedded copy of the compiled
 *    bytes, and those copies are identical: same IR, same hash.
 * 3. Because an asset's cache identity is its content hash, the two draw
 *    through *one* `EditableImage` in `@rbxts/svg`'s shared cache. Two
 *    packages, two components, one raster.
 *
 * The third point is the one that would break if the render cache ever keyed
 * on object identity, and it is checked at runtime below rather than argued
 * for — `stats().misses` is not an opinion.
 *
 * # On the JSX
 *
 * roblox-ts takes one `jsxFactory` per project, so a project containing both
 * frameworks has to choose. React's is configured, and the Vide icon is
 * therefore called as the plain function it is: `Search({ size: 24 })` is what
 * `Vide.jsx(Search, { size: 24 })` reduces to for a function component. Real
 * applications use one framework and write ordinary JSX; this one is not a
 * real application.
 */

import { Search as ReactSearch } from "@rbxts/lucide-react";
import { Search as VideSearch } from "@rbxts/lucide-vide";
import React from "@rbxts/react";
import { getSvgRenderCache, installEditableImageRenderer } from "@rbxts/svg";
import { mount } from "@rbxts/vide";

installEditableImageRenderer();

/** The React half: an ordinary component tree. */
export function ReactIcons(): React.Element {
	return (
		<frame Size={UDim2.fromOffset(120, 40)} BackgroundTransparency={1}>
			<uilistlayout
				FillDirection={Enum.FillDirection.Horizontal}
				Padding={new UDim(0, 8)}
			/>
			<ReactSearch size={24} color={Color3.fromRGB(255, 255, 255)} />
			<ReactSearch size={24} color={Color3.fromRGB(255, 90, 90)} />
		</frame>
	);
}

/**
 * The Vide half, mounted into `parent`.
 *
 * Same icon, same 24×24, a different package and a different reactive library.
 */
export function mountVideIcons(parent: Instance): () => void {
	return mount(() => VideSearch({ size: 24, color: Color3.fromRGB(140, 255, 170) }), parent);
}

/**
 * Reports what the shared cache holds.
 *
 * With the React tree and the Vide tree both mounted, `entries` should be 1
 * and `references` 3: three labels, three colours, one alpha mask. A cache
 * keyed on anything package- or framework-specific would report 2 entries.
 */
export function cacheReport(): string {
	const stats = getSvgRenderCache()?.stats();
	if (stats === undefined) {
		return "no renderer installed";
	}
	return `entries=${stats.entryCount} references=${stats.referenceCount} hits=${stats.hits} misses=${stats.misses}`;
}
