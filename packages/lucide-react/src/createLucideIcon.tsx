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
 * Nothing about rendering, caching, colouring or sizing happens in this file.
 * `<Svg>` already knows all of it, and a Lucide icon is not a special kind of
 * SVG — it is an ordinary compiled asset that happens to have been compiled by
 * this repository rather than by the consumer's own `rbxts-svg build`.
 */

import React from "@rbxts/react";
import type { SvgAsset } from "@rbxts/svg";
import { Svg, type SvgProps } from "@rbxts/svg-react";

/**
 * `Omit`, but tolerant of keys the source type may not have.
 *
 * roblox-ts's lib constrains `Omit`'s key parameter to `keyof T`, which couples
 * the type below to the exact shape of `SvgProps`. That is a detail of another
 * package's typings, not of this one.
 */
type Without<T, K extends string> = Pick<T, Exclude<keyof T, K>>;

/**
 * What a Lucide icon component accepts: everything `<Svg>` accepts, except the
 * asset — which is the one thing the component already is.
 *
 * Derived from `SvgProps` rather than enumerated, so `size`, `color`,
 * `strokeWidth`, `absoluteStrokeWidth` and every ordinary layout property stay
 * in step with the binding automatically. If `<Svg>` grows a prop, so does
 * every icon, without this package being touched.
 */
export type LucideIconProps = Without<SvgProps, "source">;

/** The component type every export of this package has. */
export type LucideIcon = (props: LucideIconProps) => React.Element;

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
 * # On component names
 *
 * The returned closure is called `LucideIcon` rather than `Search`, and that is
 * a limitation of the platform rather than a choice. React identifies a
 * function component by a `displayName` property, and a Luau function is not a
 * table — `component.displayName = "Search"` is a runtime error, not a no-op.
 * roblox-ts closes the other route too: it rejects named function *expressions*
 * outright, so a name cannot be threaded in as a parameter and applied either.
 * The only remaining way to give two thousand icons two thousand distinct
 * function names would be to generate this file's body two thousand times, to
 * buy a name that nothing reads. The module path (`…/icons/search`) is what
 * identifies an icon in a stack trace, and it is exact.
 */
export function createLucideIcon(asset: SvgAsset): LucideIcon {
	// A declaration rather than a returned function expression, for two
	// reasons: roblox-ts rejects named function expressions outright, and a
	// declaration gives the emitted Luau closure a name that `debug.info`
	// reports in a stack trace.
	function LucideIcon(props: LucideIconProps): React.Element {
		return <Svg source={asset} {...props} />;
	}
	return LucideIcon;
}
