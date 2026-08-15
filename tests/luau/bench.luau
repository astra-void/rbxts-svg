--!nocheck
--[[
	Development benchmark for the Luau rasterizer. Not part of `pnpm test`.

	Run with:

		node tests/luau/bundle.mjs && luau tests/luau/bench.luau

	Measures the workloads from the renderer task: repeated identical requests
	(warm cache), distinct rasterizations (cold), the Lucide size ladder, and a
	mixed-currentColor asset. Times are from the standalone `luau` CLI on the
	host machine — indicative of algorithmic cost, not of Roblox device
	performance, and excluding `EditableImage` allocation (a fake factory
	stands in, exactly as in the tests).
]]

local harness = require("./harness")
local sources = require("./sources")
local fixtures = require("./fixtures")

local Vector2 = {}
Vector2.__index = Vector2
function Vector2.new(x, y)
	return setmetatable({ X = x, Y = y }, Vector2)
end
Vector2.zero = Vector2.new(0, 0)

local Color3 = {}
Color3.__index = Color3
function Color3.new(r, g, b)
	return setmetatable({ R = r or 0, G = g or 0, B = b or 0 }, Color3)
end
function Color3.fromRGB(r, g, b)
	return Color3.new((r or 0) / 255, (g or 0) / 255, (b or 0) / 255)
end

local loader = harness.new(sources, { Vector2 = Vector2, Color3 = Color3 })
local svg = loader.loadPackage("packages/svg/out")
local internal = svg.unstable_internal
local backend = loader.loadModule("packages/svg/out/raster/editableImageRenderer")
local renderModule = loader.loadModule("packages/svg/out/raster/render")

local function fakeImage()
	return {
		WritePixelsBuffer = function() end,
		Destroy = function() end,
	}
end

svg.setSvgRenderer(backend.createEditableImageRenderer({
	create = function()
		return fakeImage()
	end,
}))

local function assetOf(name)
	local fixture = fixtures.assets[name]
	return internal.createAssetFromBase64(fixture.base64, fixture.hash)
end

local function bench(label, iterations, fn)
	local started = os.clock()
	for index = 1, iterations do
		fn(index)
	end
	local elapsed = os.clock() - started
	print(string.format("%-46s %4d iter  total %8.2f ms  per %7.3f ms",
		label, iterations, elapsed * 1000, elapsed * 1000 / iterations))
end

local search = assetOf("lucideSearch")
local settings = assetOf("lucideSettings")
local mixed = assetOf("mixedCurrentColor")
local white = Color3.fromRGB(255, 255, 255)

-- Cold: every request a distinct raster (unique pixel size defeats the cache).
bench("cold: search 24px equivalents (distinct sizes)", 100, function(index)
	local handle = svg.renderSvg(search, { size = Vector2.new(100 + index, 100 + index), currentColor = white })
	handle.release()
end)

-- Warm: 100 identical requests after the first — pure cache/refcount work.
do
	local warmup = svg.renderSvg(search, { size = Vector2.new(24, 24), currentColor = white })
	bench("warm: 100x same search 24px", 100, function()
		local handle = svg.renderSvg(search, { size = Vector2.new(24, 24), currentColor = white })
		handle.release()
	end)
	warmup.release()
end

-- Distinct icons: every real Lucide fixture in the bundle, cold each round
-- via a fresh renderer install (which clears the cache).
local lucide = {}
for name in fixtures.assets do
	if string.sub(name, 1, 6) == "lucide" then
		table.insert(lucide, assetOf(name))
	end
end
bench(`cold: {#lucide} distinct Lucide icons at 24px`, 20, function()
	svg.setSvgRenderer(backend.createEditableImageRenderer({
		create = function()
			return fakeImage()
		end,
	}))
	for _, asset in lucide do
		svg.renderSvg(asset, { size = Vector2.new(24, 24), currentColor = white }).release()
	end
end)

-- The size ladder.
for _, size in { 16, 24, 32, 64 } do
	bench(`cold: search {size}px`, 50, function()
		renderModule.rasterize(internal.inspect(search), {
			pixelWidth = size,
			pixelHeight = size,
			alphaMask = true,
			currentColorR = 255,
			currentColorG = 255,
			currentColorB = 255,
		})
	end)
end

bench("cold: settings 24px", 50, function()
	renderModule.rasterize(internal.inspect(settings), {
		pixelWidth = 24,
		pixelHeight = 24,
		alphaMask = true,
		currentColorR = 255,
		currentColorG = 255,
		currentColorB = 255,
	})
end)

bench("cold: mixed currentColor 24px (full colour)", 50, function()
	renderModule.rasterize(internal.inspect(mixed), {
		pixelWidth = 24,
		pixelHeight = 24,
		alphaMask = false,
		currentColorR = 0,
		currentColorG = 128,
		currentColorB = 255,
	})
end)

local stats = renderModule.getSoftwareRasterStats()
print(string.format("rasterizations: %d  (last: %d points, %d edges)",
	stats.rasterCount, stats.lastPointCount, stats.lastEdgeCount))
