/**
 * The example's client entry point.
 *
 * Mounts `<Toolbar />` so the project is something you can actually press Play
 * on. The `UIScale` is not decoration: it is the handle for checking that a
 * `Size`-driven `<Svg>` re-rasterizes when its laid-out size changes, which is
 * the one part of the renderer that only a running engine can confirm. See
 * `docs/STUDIO-VERIFY-VIDE.md`.
 */

import Vide, { mount } from "@rbxts/vide";

import { Toolbar } from "../Toolbar";

// `@rbxts/services` is not a dependency here; one GetService call is cheaper
// than another package in an example about SVG.
const players = game.GetService("Players");
const playerGui = players.LocalPlayer.WaitForChild("PlayerGui");

const screen = new Instance("ScreenGui");
screen.Name = "SvgExampleVide";
screen.ResetOnSpawn = false;
screen.IgnoreGuiInset = true;
screen.Parent = playerGui;

// Named so a smoke test can find it and change the scale without a rebuild.
const scale = new Instance("UIScale");
scale.Name = "ExampleScale";
scale.Scale = 1;
scale.Parent = screen;

// `mount` creates the stable scope every `<Svg>`'s render handle is owned by:
// destroying it releases them all, which is the same lifetime a React root's
// unmount gives the React binding.
mount(() => <Toolbar />, screen);
