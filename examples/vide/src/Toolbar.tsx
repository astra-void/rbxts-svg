/**
 * What consuming compiled SVG assets looks like under Vide — both ways of
 * getting one.
 *
 * ```text
 * import { Search } from "@rbxts/lucide-vide";   precompiled, named
 * import Logo from "./icons/logo.svg";           your own artwork
 *                        │
 *                        ▼
 *          the same compiler, IR, rasterizer and cache
 * ```
 *
 * A generated Lucide component is `<Svg>` with the asset already bound, so
 * every prop below is as reactive here as it is there — the wrapper adds no
 * state, no effect and no cache behaviour of its own.
 *
 * The interactive controls exist to demonstrate the three things a reactive
 * binding has to get right, and the counter next to them is the evidence:
 *
 * ```text
 * recolour a Lucide icon   →  raster count unchanged
 * resize it                →  raster count increases, old raster freed
 * hide it                  →  handle released, raster freed
 * ```
 */

import { Bell, ChevronDown, Search, Settings } from "@rbxts/lucide-vide";
import { getSvgRenderCache, installEditableImageRenderer } from "@rbxts/svg";
import { Svg } from "@rbxts/svg-vide";
import Vide, { Show, source } from "@rbxts/vide";

import Logo from "./icons/logo.svg";

// The production renderer is installed once, explicitly, at startup — nothing
// installs it as an import side effect, and it is not a Vide concept. An
// application rendering SVGs from both React and Vide still calls this once.
installEditableImageRenderer();

const PALETTE = [
	Color3.fromRGB(255, 255, 255),
	Color3.fromRGB(120, 170, 255),
	Color3.fromRGB(255, 90, 90),
];

const SIZES = [24, 32, 48];

/** How many distinct rasters the shared cache has produced so far. */
function rasterCount(): number {
	return getSvgRenderCache()?.stats().misses ?? 0;
}

export function Toolbar(): Vide.Node {
	const colourIndex = source(0);
	const sizeIndex = source(0);
	const bellVisible = source(true);

	// Sampled after each interaction rather than derived, because the cache is
	// not reactive — it is the thing being observed, and pulling from it is how
	// the demonstration stays honest.
	const rasters = source(rasterCount());
	const sample = (): void => {
		rasters(rasterCount());
	};

	const colour = (): Color3 => PALETTE[colourIndex() % PALETTE.size()];
	const size = (): number => SIZES[sizeIndex() % SIZES.size()];

	const toolbar = (
		<frame
			Size={UDim2.fromOffset(460, 140)}
			Position={UDim2.fromScale(0.5, 0.5)}
			AnchorPoint={new Vector2(0.5, 0.5)}
			BackgroundColor3={Color3.fromRGB(30, 30, 34)}
		>
			<uilistlayout
				FillDirection={Enum.FillDirection.Vertical}
				Padding={new UDim(0, 8)}
				HorizontalAlignment={Enum.HorizontalAlignment.Center}
				VerticalAlignment={Enum.VerticalAlignment.Center}
			/>

			<frame Size={UDim2.fromOffset(440, 56)} BackgroundTransparency={1}>
				<uilistlayout
					FillDirection={Enum.FillDirection.Horizontal}
					Padding={new UDim(0, 10)}
					HorizontalAlignment={Enum.HorizontalAlignment.Center}
					VerticalAlignment={Enum.VerticalAlignment.Center}
				/>

				{/* Three static colours of one tintable icon: one rasterized
				    alpha mask, three ImageColor3 values. */}
				<Search size={24} color={Color3.fromRGB(255, 255, 255)} />
				<Search size={24} color={Color3.fromRGB(255, 90, 90)} />
				<Search size={24} color={Color3.fromRGB(140, 255, 170)} />

				{/* Reactive colour, straight through the generated wrapper.
				    Pressing "colour" changes what this draws and nothing else —
				    the raster counter does not move. */}
				<Search size={24} color={colour} />

				{/* Reactive size. Each distinct pixel size is a distinct raster,
				    so this one does move the counter — and frees the old. */}
				<Search size={size} color={colour} />

				{/* A thinner stroke is geometry, so it is its own raster. */}
				<Settings size={32} color={Color3.fromRGB(120, 170, 255)} strokeWidth={1.5} />

				{/* An absolute stroke keeps its 2px apparent weight at any size. */}
				<ChevronDown
					size={32}
					color={Color3.fromRGB(200, 200, 200)}
					strokeWidth={2}
					absoluteStrokeWidth
				/>

				{/* A UDim2 layout: the raster resolution follows the laid-out
				    AbsoluteSize, observed through Vide's own `changed` action.
				    Nothing is rasterized before that first measurement — the
				    label shows no image at all for a frame, rather than a
				    placeholder that is about to be thrown away. */}
				<Settings
					Size={UDim2.fromScale(0.1, 0.7)}
					color={Color3.fromRGB(255, 200, 120)}
				/>

				{/* A conditional scope. Hiding it destroys the scope the icon
				    was created in, which is what releases its render handle. */}
				<Show when={bellVisible}>
					{() => <Bell size={24} color={Color3.fromRGB(255, 200, 120)} />}
				</Show>

				{/* Arbitrary artwork, imported straight from the `.svg`. Fixed
				    multi-colour fills rather than currentColor, so it is not a
				    shared alpha mask — the same pipeline, a different kind of
				    picture. */}
				<Svg source={Logo} size={32} />
			</frame>

			<frame Size={UDim2.fromOffset(440, 32)} BackgroundTransparency={1}>
				<uilistlayout
					FillDirection={Enum.FillDirection.Horizontal}
					Padding={new UDim(0, 8)}
					HorizontalAlignment={Enum.HorizontalAlignment.Center}
				/>

				<Button
					text="colour"
					onClick={() => {
						colourIndex(colourIndex() + 1);
						sample();
					}}
				/>
				<Button
					text="size"
					onClick={() => {
						sizeIndex(sizeIndex() + 1);
						sample();
					}}
				/>
				<Button
					text="bell"
					onClick={() => {
						bellVisible(!bellVisible());
						sample();
					}}
				/>

				<textlabel
					Size={UDim2.fromOffset(120, 28)}
					BackgroundTransparency={1}
					TextColor3={Color3.fromRGB(200, 200, 200)}
					Font={Enum.Font.Code}
					TextSize={14}
					Text={() => `rasters: ${rasters()}`}
				/>
			</frame>
		</frame>
	);

	// Vide creates instances eagerly, so every icon above has already acquired
	// its raster by the time this line runs — every icon whose size is known,
	// at least. Sampling here rather than at the top of the component is the
	// difference between the counter starting at its real value and starting
	// at zero.
	sample();

	return toolbar;
}

function Button(props: { text: string; onClick: () => void }): Vide.Node {
	return (
		<textbutton
			Size={UDim2.fromOffset(80, 28)}
			BackgroundColor3={Color3.fromRGB(55, 55, 62)}
			TextColor3={Color3.fromRGB(240, 240, 240)}
			Font={Enum.Font.Code}
			TextSize={14}
			Text={props.text}
			Activated={props.onClick}
		/>
	);
}
