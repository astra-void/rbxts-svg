/**
 * The one piece of framework code in this package.
 *
 * Every generated icon module is two lines: a compiled asset and a call to
 * {@link createLucideIcon}. That is deliberate and it is the whole design.
 * There are around two thousand of those modules, so anything that lives in
 * them lives two thousand times — in the source tree, in the emitted Luau, in
 * the published tarball and in every diff of an upstream bump. Anything that
 * can be shared is shared, here.
 *
 * ```text
 * generated/icons/search.tsx     ─┐
 * generated/icons/settings.tsx   ─┼─▶ createLucideIcon ─▶ <Svg> ─▶ @rbxts/svg
 * generated/icons/…              ─┘
 * ```
 *
 * There is no `effect` here, no handle to acquire and nothing reactive of this
 * package's own. All of that belongs to `@rbxts/svg-vide`, which already gets
 * the hard parts right — reading the colour only for assets whose pixels it
 * changes, handing a raster over before releasing the old one — and a wrapper
 * that reached for any of it would be a second chance to get them wrong.
 */

import Vide from "@rbxts/vide";
import type { SvgAsset } from "@rbxts/svg";
import { Svg, type SvgProps } from "@rbxts/svg-vide";

/**
 * `Omit`, but tolerant of keys the source type may not have.
 *
 * roblox-ts's lib constrains `Omit`'s key parameter to `keyof T`, which couples
 * the type below to the exact shape of `SvgProps` — itself composed from Vide's
 * generated instance attributes. That is a detail of two other packages'
 * typings, not of this one.
 */
type Without<T, K extends string> = Pick<T, Exclude<keyof T, K>>;

/**
 * What a Lucide icon component accepts: everything `<Svg>` accepts, except the
 * asset — which is the one thing the component already is.
 *
 * Derived from `SvgProps` rather than enumerated, so every prop stays reactive
 * exactly as it is on `<Svg>`: `size={iconSize}` and `color={theme}` are
 * sources here for the same reason and through the same code path.
 */
export type LucideIconProps = Without<SvgProps, "source">;

/** The component type every export of this package has. */
export type LucideIcon = (props: LucideIconProps) => Vide.Node;

/**
 * Wraps a compiled asset as a named icon component.
 *
 * ```tsx
 * export const Search = createLucideIcon(unstable_internal.createAssetFromBase64(…));
 * ```
 *
 * The asset is captured, not exposed: there is no `Search.asset`, because a
 * mutable field on a component would be a second way to change what an icon
 * draws and the wrong place to look for the first.
 *
 * The asset is also *not* wrapped in a source or read reactively. It never
 * changes for a given icon, and making it reactive would add a dependency to
 * `<Svg>`'s raster effect that could only ever fire spuriously.
 */
export function createLucideIcon(asset: SvgAsset): LucideIcon {
	// A declaration rather than a returned function expression, for two
	// reasons: roblox-ts rejects named function expressions outright, and a
	// declaration gives the emitted Luau closure a name that `debug.info`
	// reports in a stack trace.
	function LucideIcon(props: LucideIconProps): Vide.Node {
		return <Svg source={asset} {...props} />;
	}
	return LucideIcon;
}
