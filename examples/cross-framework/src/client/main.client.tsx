/**
 * Mounts both halves of the fixture and prints what the shared cache holds.
 *
 * The printed line is the whole point of the project: run it and the output
 * says whether a React `Search` and a Vide `Search`, from two separately
 * generated packages, are one raster or two.
 */

import React from "@rbxts/react";
import { createPortal, createRoot } from "@rbxts/react-roblox";

import { ReactIcons, cacheReport, mountVideIcons } from "../Both";

const players = game.GetService("Players");
const playerGui = players.LocalPlayer.WaitForChild("PlayerGui");

const screen = new Instance("ScreenGui");
screen.Name = "LucideCrossFramework";
screen.ResetOnSpawn = false;
screen.Parent = playerGui;

const videHost = new Instance("Frame");
videHost.Name = "VideHost";
videHost.Position = UDim2.fromOffset(0, 48);
videHost.Size = UDim2.fromOffset(120, 40);
videHost.BackgroundTransparency = 1;
videHost.Parent = screen;

createRoot(new Instance("Folder")).render(createPortal(<ReactIcons />, screen));
mountVideIcons(videHost);

print(`[lucide cross-framework] ${cacheReport()}`);
