--!nocheck
--[[
	What Lucide costs. Driven by `tests/luau/lucide-bench.mjs`.

	Two separate questions, and they are not the same question:

	1. **Loading.** roblox-ts has no tree shaking, so importing anything from a
	   Lucide package's root barrel initializes every icon module. This measures
	   what that initialization *is*: decoding every canonical icon's compiled
	   IR out of base64 and building its lookup tables. It is a floor, not a
	   total — in Roblox each module is also a ModuleScript to replicate and
	   `require`, which this cannot see.

	2. **Rendering.** Whether a hundred components, a hundred colours and a
	   hundred distinct icons cost what the cache design says they cost.

	Times come from the standalone `luau` CLI on the host machine and use a fake
	image factory, exactly as the other Luau suites do. They are indicative of
	algorithmic cost, not of Roblox device performance.
]]

local harness = require("./harness")
local sources = require("./sources")
local roblox = require("./roblox")
local loadVide = require("./vide-loader")
local blobs = require("./lucide-fixtures")

local Vector2 = roblox.Vector2
local Color3 = roblox.Color3

local vide = loadVide(roblox.globals())
local packages = { ["@rbxts/vide"] = vide }
local loader = harness.new(sources, roblox.globals(), packages)
local svg = loader.loadPackage("packages/svg/out")
packages["@rbxts/svg"] = svg
local svgVide = loader.loadPackage("packages/svg-vide/out")
local Svg = svgVide.Svg
local internal = svg.unstable_internal

-- A renderer that allocates nothing. The rasterizer's own cost is measured by
-- `bench.luau` against the Rust reference; what matters here is how many times
-- a workload reaches it.
local function newRenderer()
	local renderer = { version = 1, count = 0 }
	renderer.render = function(_, _asset, options)
		renderer.count += 1
		return { width = options.pixelWidth, height = options.pixelHeight }
	end
	renderer.destroy = function() end
	return renderer
end

local function fresh()
	local renderer = newRenderer()
	svg.setSvgRenderer(renderer)
	return renderer, svg.getSvgRenderCache()
end

-- ---------------------------------------------------------------------------
-- 1. What the eager root barrel costs
-- ---------------------------------------------------------------------------

collectgarbage()
local baseline = collectgarbage("count")
local start = os.clock()
local assets = table.create(#blobs)
local byName = {}
for index, row in blobs do
	local asset = internal.createAssetFromBase64(row[1], row[2])
	assets[index] = asset
	byName[row[3]] = asset
end
local elapsed = os.clock() - start
collectgarbage()
local retained = collectgarbage("count") - baseline

print(string.format(
	"barrel load        %d assets in %.1f ms (%.3f ms each), %.2f MiB retained",
	#assets, elapsed * 1000, elapsed * 1000 / #assets, retained / 1024
))

-- ---------------------------------------------------------------------------
-- 2. Cold rasterization
-- ---------------------------------------------------------------------------

local white = Color3.fromRGB(255, 255, 255)

local function coldOne(name)
	local renderer = fresh()
	local began = os.clock()
	local handle = svg.renderSvg(byName[name], { size = Vector2.new(24, 24) })
	local took = os.clock() - began
	print(string.format("cold %-12s  %.2f ms, %d rasterization(s)", name .. " 24", took * 1000, renderer.count))
	handle:release()
end

coldOne("search")
coldOne("settings")

-- ---------------------------------------------------------------------------
-- 3. A hundred of the same icon, a hundred distinct icons
-- ---------------------------------------------------------------------------

do
	local renderer, cache = fresh()
	local handles = table.create(100)
	local began = os.clock()
	for index = 1, 100 do
		handles[index] = svg.renderSvg(byName.search, { size = Vector2.new(24, 24) })
	end
	local took = os.clock() - began
	print(string.format(
		"100 × Search 24    %.2f ms, %d rasterization(s), %d cache entr(ies), %d reference(s)",
		took * 1000, renderer.count, cache:stats().entryCount, cache:stats().referenceCount
	))
	for _, handle in handles do
		handle:release()
	end
end

do
	-- Same icon, a hundred colours. A tintable asset's colour is not in the
	-- cache key, so this is one raster and a hundred `ImageColor3` values.
	local renderer, cache = fresh()
	local handles = table.create(100)
	local began = os.clock()
	for index = 1, 100 do
		handles[index] = svg.renderSvg(byName.search, {
			size = Vector2.new(24, 24),
			currentColor = Color3.fromRGB(index * 2 % 256, 255 - index % 256, index % 256),
		})
	end
	local took = os.clock() - began
	print(string.format(
		"100 colours of one %.2f ms, %d rasterization(s), %d cache entr(ies)",
		took * 1000, renderer.count, cache:stats().entryCount
	))
	for _, handle in handles do
		handle:release()
	end
end

do
	local renderer, cache = fresh()
	local handles = table.create(100)
	local began = os.clock()
	for index = 1, 100 do
		handles[index] = svg.renderSvg(assets[index], { size = Vector2.new(24, 24) })
	end
	local took = os.clock() - began
	print(string.format(
		"100 distinct icons %.2f ms, %d rasterization(s), %d cache entr(ies)",
		took * 1000, renderer.count, cache:stats().entryCount
	))
	for _, handle in handles do
		handle:release()
	end
end

-- ---------------------------------------------------------------------------
-- 4. A hundred reactive colour updates, through the real Vide binding
-- ---------------------------------------------------------------------------

do
	vide.strict = false
	local renderer, cache = fresh()
	local colour = vide.source(white)

	local destroy = vide.root(function()
		return Svg({ source = byName.search, size = 24, color = colour })
	end)

	local afterMount = renderer.count
	local began = os.clock()
	for step = 1, 100 do
		colour(Color3.fromRGB(step * 2 % 256, 255 - step % 256, step % 256))
	end
	local took = os.clock() - began
	print(string.format(
		"100 Vide recolours %.2f ms, %d rasterization(s) after mount (mount cost %d), %d entr(ies)",
		took * 1000, renderer.count - afterMount, afterMount, cache:stats().entryCount
	))
	destroy()
end
