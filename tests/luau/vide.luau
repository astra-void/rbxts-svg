--!nocheck
--[[
	Lifecycle and reactivity tests for `@rbxts/svg-vide`.

	These run the *compiled* binding — the Luau `rbxtsc` emitted — against the
	*real* `@rbxts/vide`, with only Roblox datatypes and instances mocked. That
	combination is the point. A stand-in reactive library would agree with
	whatever the binding happened to do; the real one has opinions the binding
	has to be correct against:

	- a scope's cleanups are flushed *before* it reruns, not after;
	- strict mode evaluates every reactive scope twice on purpose;
	- a source only notifies when its new value differs from the old one.

	What is deliberately not tested here is anything about pixels. Flattening,
	stroke expansion, coverage and compositing belong to the core and are
	covered by `spec.luau` and `raster.luau` against the Rust reference. A
	framework binding's job is which requests reach the renderer and when the
	results are let go of, so that is all this file asserts.

	Run with `pnpm test:luau`, which regenerates the bundle first.
]]

local harness = require("./harness")
local sources = require("./sources")
local fixtures = require("./fixtures")
local roblox = require("./roblox")
local loadVide = require("./vide-loader")

local Vector2 = roblox.Vector2
local Color3 = roblox.Color3
local UDim2 = roblox.UDim2
local Content = roblox.Content

local vide = loadVide(roblox.globals())

-- Filled in as each package loads. `@rbxts/svg` is registered before the
-- binding is loaded, so the binding's `TS.getModule(script, "@rbxts", "svg")`
-- resolves to *this* core instance: one core, one cache, which is the
-- invariant the cross-framework test below depends on.
local packages = { ["@rbxts/vide"] = vide }
local loader = harness.new(sources, roblox.globals(), packages)

local svg = loader.loadPackage("packages/svg/out")
packages["@rbxts/svg"] = svg

local svgVide = loader.loadPackage("packages/svg-vide/out")
local Svg = svgVide.Svg
local internal = svg.unstable_internal

local passed, failed = 0, 0
local failures = {}

local function check(name, ok, detail)
	if ok then
		passed += 1
	else
		failed += 1
		table.insert(failures, `{name}: {detail or "assertion failed"}`)
	end
end

local function equal(name, actual, expected)
	check(name, actual == expected, `expected {expected}, got {actual}`)
end

local function asset(fixtureName)
	local fixture = fixtures.assets[fixtureName]
	return internal.createAssetFromBase64(fixture.base64, fixture.hash)
end

-- `search` is the Lucide shape of asset: monochrome currentColor, so tintable.
-- `mixedCurrentColor` mixes currentColor with fixed paints, so its colour is
-- geometry. `solidFill` has no currentColor at all. Those three classes are
-- what every colour assertion below is really about.
local SEARCH = asset("lucideSearch")
local SETTINGS = asset("lucideSettings")
local MIXED = asset("mixedCurrentColor")
local FIXED = asset("solidFill")

check("search is tintable", svg.isTintable(SEARCH), "fixture assumption")
check("mixed uses currentColor", svg.usesCurrentColor(MIXED), "fixture assumption")
check("mixed is not tintable", not svg.isTintable(MIXED), "fixture assumption")
check("fixed has no currentColor", not svg.usesCurrentColor(FIXED), "fixture assumption")

--[[
	A renderer that records what it was asked for, in order.

	The order is as important as the count: the binding is required to acquire a
	new raster *before* releasing the old one, and the only way to see that from
	outside is that no image is destroyed between the two.
]]
local function newRenderer()
	local renderer = { version = 1, images = {}, destroyed = {}, events = {} }

	-- Colon-called: roblox-ts emits `self.renderer:render(...)` for an interface
	-- method, so each implementation receives the renderer itself first.
	renderer.render = function(_, _asset, options)
		local image = {
			index = #renderer.images + 1,
			width = options.pixelWidth,
			height = options.pixelHeight,
			strokeWidth = options.strokeWidth,
			currentColor = options.currentColor,
		}
		table.insert(renderer.images, image)
		table.insert(renderer.events, `render {image.width}x{image.height}#{image.index}`)
		return image
	end

	renderer.destroy = function(_, image)
		table.insert(renderer.destroyed, image)
		table.insert(renderer.events, `destroy {image.width}x{image.height}#{image.index}`)
	end

	return renderer
end

--[[
	Installs a fresh renderer and returns it plus the cache behind it.

	`setSvgRenderer` clears the cache, so each scenario starts with no entries
	and no outstanding references regardless of what the previous one left.
]]
local function newScenario()
	local renderer = newRenderer()
	svg.setSvgRenderer(renderer)
	return renderer, svg.getSvgRenderCache()
end

local function stats(cache)
	return cache:stats()
end

-- Strict mode is Vide's development aid: it runs every reactive scope twice to
-- surface scopes that are not idempotent. It is off for the counting tests
-- because doubled evaluations would make every count ambiguous, and gets its
-- own scenario at the end where the doubling is the thing under test.
vide.strict = false

-- ---------------------------------------------------------------------------
-- No renderer installed
--
-- First, because there is no way back to "no renderer" once one is installed —
-- and because it is the state a real application is in until it calls
-- `installEditableImageRenderer()`.
-- ---------------------------------------------------------------------------

do
	-- The same failure React gets, for the same reason: a missing renderer is a
	-- configuration error, and an icon that silently renders as nothing is far
	-- harder to diagnose than one that fails at startup.
	check("no renderer is installed yet", svg.getSvgRenderer() == nil, "one was installed early")

	local ok, message = pcall(function()
		vide.root(function()
			return Svg({ source = SEARCH, size = 24 })
		end)
	end)

	check("mounting without a renderer fails", not ok, "expected an error")
	check(
		"and the error is the core's actionable one",
		not ok and string.find(tostring(message), "installEditableImageRenderer", 1, true) ~= nil,
		`message was {message}`
	)
end

-- ---------------------------------------------------------------------------
-- A static mount
-- ---------------------------------------------------------------------------

do
	local renderer, cache = newScenario()
	local white = Color3.fromRGB(255, 255, 255)

	local destroy, label = vide.root(function()
		return Svg({ source = SEARCH, size = 24, color = white })
	end)

	equal("static mount rasterizes once", #renderer.images, 1)
	equal("static mount raster width", renderer.images[1].width, 24)
	equal("static mount raster height", renderer.images[1].height, 24)
	equal("static mount layout size", label.Size, UDim2.fromOffset(24, 24))
	equal("static mount shows the raster", label.ImageContent, Content.fromObject(renderer.images[1]))
	equal("static mount tints with the colour", label.ImageColor3, white)
	equal("static mount is transparent behind", label.BackgroundTransparency, 1)
	equal("static mount creates an ImageLabel", label.ClassName, "ImageLabel")

	-- The SVG-specific props are consumed, not forwarded: Vide assigns every
	-- string key of a props table onto the instance, so a leaked `source` would
	-- show up here as a property (and in the engine as an error).
	equal("source is not forwarded to the instance", label.source, nil)
	equal("size is not forwarded to the instance", label.size, nil)
	equal("color is not forwarded to the instance", label.color, nil)

	local before = stats(cache)
	equal("static mount holds one entry", before.entryCount, 1)
	equal("static mount holds one reference", before.referenceCount, 1)

	destroy()

	local after = stats(cache)
	equal("destroying the scope releases the entry", after.entryCount, 0)
	equal("destroying the scope releases the reference", after.referenceCount, 0)
	equal("destroying the scope destroys the image", #renderer.destroyed, 1)
end

-- ---------------------------------------------------------------------------
-- A reactive numeric size
-- ---------------------------------------------------------------------------

do
	local renderer, cache = newScenario()
	local size = vide.source(24)

	local destroy, label = vide.root(function()
		return Svg({ source = SEARCH, size = size })
	end)

	equal("reactive size starts at 24", renderer.images[1].width, 24)
	equal("reactive size lays out at 24", label.Size, UDim2.fromOffset(24, 24))

	size(48)

	equal("changing size rasterizes again", #renderer.images, 2)
	equal("the new raster is the new size", renderer.images[2].width, 48)
	equal("the layout follows too", label.Size, UDim2.fromOffset(48, 48))
	equal("the label shows the new raster", label.ImageContent, Content.fromObject(renderer.images[2]))

	-- The ordering rule: acquire, publish, *then* release. A release-first
	-- binding would emit destroy before render here, and would destroy an image
	-- another component might have been about to reuse.
	equal("the new raster is acquired first", renderer.events[2], "render 48x48#2")
	equal("the old raster is released after", renderer.events[3], "destroy 24x24#1")

	local live = stats(cache)
	equal("only the current size is cached", live.entryCount, 1)
	equal("only one reference is held", live.referenceCount, 1)

	destroy()
	equal("every raster is destroyed exactly once", #renderer.destroyed, 2)
	equal("nothing is left referenced", stats(cache).referenceCount, 0)
end

-- ---------------------------------------------------------------------------
-- Colour, by asset class
-- ---------------------------------------------------------------------------

do
	-- Tintable: the raster is a shared alpha mask, so colour is an ImageColor3
	-- write and nothing else. This is the Lucide fast path, and the single most
	-- important thing for the binding not to get wrong.
	local renderer, cache = newScenario()
	local colour = vide.source(Color3.fromRGB(255, 255, 255))

	local destroy, label = vide.root(function()
		return Svg({ source = SEARCH, size = 24, color = colour })
	end)

	equal("tintable mount rasterizes once", #renderer.images, 1)

	colour(Color3.fromRGB(0, 0, 255))
	equal("recolouring a tintable asset does not rasterize", #renderer.images, 1)
	equal("recolouring a tintable asset updates the tint", label.ImageColor3, Color3.fromRGB(0, 0, 255))

	colour(Color3.fromRGB(0, 255, 0))
	equal("recolouring again still does not rasterize", #renderer.images, 1)
	equal("recolouring again updates the tint", label.ImageColor3, Color3.fromRGB(0, 255, 0))
	equal("no raster was ever destroyed", #renderer.destroyed, 0)
	equal("the one entry is still the one entry", stats(cache).entryCount, 1)

	destroy()
end

do
	-- Mixed currentColor: the colour reaches the compositor, so each distinct
	-- colour genuinely is a different picture and a different cache entry.
	local renderer, cache = newScenario()
	local colour = vide.source(Color3.fromRGB(255, 0, 0))

	local destroy, label = vide.root(function()
		return Svg({ source = MIXED, size = 24, color = colour })
	end)

	equal("mixed mount rasterizes once", #renderer.images, 1)
	equal(
		"mixed mount passes the colour through",
		renderer.images[1].currentColor,
		Color3.fromRGB(255, 0, 0)
	)
	equal("mixed assets are not tinted by ImageColor3", label.ImageColor3, Color3.new(1, 1, 1))

	colour(Color3.fromRGB(0, 0, 255))
	equal("recolouring a mixed asset rasterizes again", #renderer.images, 2)
	equal(
		"the new raster has the new colour",
		renderer.images[2].currentColor,
		Color3.fromRGB(0, 0, 255)
	)
	equal("the previous raster is released", #renderer.destroyed, 1)
	equal("and it was the first one", renderer.destroyed[1].index, 1)
	equal("mixed assets are still not tinted", label.ImageColor3, Color3.new(1, 1, 1))
	equal("only the current colour is cached", stats(cache).entryCount, 1)

	destroy()
end

do
	-- No currentColor at all: `color` cannot change a single pixel, and must
	-- not be quietly promoted into a blanket image tint either.
	local renderer = newScenario()
	local colour = vide.source(Color3.fromRGB(255, 0, 0))

	local destroy, label = vide.root(function()
		return Svg({ source = FIXED, size = 24, color = colour })
	end)

	equal("fixed-colour mount rasterizes once", #renderer.images, 1)
	equal("a fixed-colour asset is not tinted", label.ImageColor3, Color3.new(1, 1, 1))

	colour(Color3.fromRGB(0, 0, 255))
	equal("recolouring a fixed-colour asset does not rasterize", #renderer.images, 1)
	equal("recolouring a fixed-colour asset does not tint", label.ImageColor3, Color3.new(1, 1, 1))

	destroy()
end

-- ---------------------------------------------------------------------------
-- Sharing: one raster, many labels — and many frameworks
-- ---------------------------------------------------------------------------

do
	local renderer, cache = newScenario()

	local destroy = vide.root(function()
		return {
			Svg({ source = SEARCH, size = 24, color = Color3.fromRGB(255, 0, 0) }),
			Svg({ source = SEARCH, size = 24, color = Color3.fromRGB(0, 0, 255) }),
		}
	end)

	equal("two colours of one tintable icon rasterize once", #renderer.images, 1)
	equal("they share one cache entry", stats(cache).entryCount, 1)
	equal("held by two references", stats(cache).referenceCount, 2)

	destroy()
	equal("and the image survives until the last one goes", #renderer.destroyed, 1)
end

do
	-- The cross-framework invariant, at the lowest layer it can be stated:
	-- `renderSvg` is exactly what `@rbxts/svg-react`'s `useSvg` calls, so a
	-- direct call here is a React tree's request. Nothing about the framework is
	-- in the cache key, so the two share an entry and an EditableImage.
	local renderer, cache = newScenario()

	local reactHandle = svg.renderSvg(SEARCH, {
		size = Vector2.new(24, 24),
		currentColor = Color3.fromRGB(255, 0, 0),
	})

	local destroy, label = vide.root(function()
		return Svg({ source = SEARCH, size = 24, color = Color3.fromRGB(0, 0, 255) })
	end)

	equal("a React request and a Vide component rasterize once", #renderer.images, 1)
	equal("they share one cache entry", stats(cache).entryCount, 1)
	equal("held by two references", stats(cache).referenceCount, 2)
	equal("and it is literally the same image", label.ImageContent, Content.fromObject(reactHandle.image))

	destroy()
	equal("destroying the Vide scope does not destroy React's image", #renderer.destroyed, 0)
	reactHandle:release()
	equal("the last release does", #renderer.destroyed, 1)
end

do
	--[[
		The same invariant, for two *separately created* assets.

		This is the Lucide case, and it is a stronger claim than the one above.
		`@rbxts/lucide-react` and `@rbxts/lucide-vide` each embed their own copy
		of every icon's compiled bytes — there is deliberately no shared data
		package — so a game using both has two distinct `SvgAsset` objects for
		one icon. If the cache keyed on object identity, that would be two
		rasters and two EditableImages for one drawing on screen.

		It does not: an asset's identity is its content hash, so identical bytes
		are one entry however many times they were decoded. Neither the package
		name nor the framework appears anywhere in the key.
	]]
	local renderer, cache = newScenario()

	local fromReactPackage = asset("lucideSearch")
	local fromVidePackage = asset("lucideSearch")
	check(
		"the two packages' copies are different objects",
		fromReactPackage ~= fromVidePackage,
		"expected distinct tables"
	)

	local reactHandle = svg.renderSvg(fromReactPackage, { size = Vector2.new(24, 24) })

	local destroy, label = vide.root(function()
		return Svg({ source = fromVidePackage, size = 24 })
	end)

	equal("two copies of one icon rasterize once", #renderer.images, 1)
	equal("sharing one cache entry", stats(cache).entryCount, 1)
	equal("held by two references", stats(cache).referenceCount, 2)
	equal("and one EditableImage", label.ImageContent, Content.fromObject(reactHandle.image))

	destroy()
	reactHandle:release()
	equal("freed exactly once, at the last release", #renderer.destroyed, 1)
end

-- ---------------------------------------------------------------------------
-- A declared `size` is a *layout* size
--
-- The raster follows what the engine actually laid out, in every case. A
-- `UIScale`, or any scale-based ancestor, makes "24" in the props and "24 on
-- screen" two different numbers, and rasterizing at the first would display an
-- image at a size it was not drawn for.
-- ---------------------------------------------------------------------------

do
	local renderer = newScenario()

	local destroy, label = vide.root(function()
		return Svg({ source = SEARCH, size = 24 })
	end)

	-- No waiting: a component that owns its layout size draws on the first
	-- frame, at that size, rather than after a layout round trip.
	equal("a declared size draws immediately", #renderer.images, 1)
	equal("at its declared resolution", renderer.images[1].width, 24)

	-- Now laid out at twice that — a UIScale of 2, say.
	label.AbsoluteSize = Vector2.new(48, 48)

	equal("being laid out larger re-rasterizes", #renderer.images, 2)
	equal("at the laid-out resolution", renderer.images[2].width, 48)
	equal("in both dimensions", renderer.images[2].height, 48)
	equal("and the raster it replaces is freed", #renderer.destroyed, 1)
	equal("the layout size is still the declared one", label.Size, UDim2.fromOffset(24, 24))

	label.AbsoluteSize = Vector2.new(24, 24)
	equal("and back down again", #renderer.images, 3)
	equal("to the smaller resolution", renderer.images[3].width, 24)

	destroy()
end

do
	-- The ordinary case, and the reason this costs nothing: an icon laid out at
	-- exactly the size it asked for measures the resolution it is already
	-- drawing at, so the source never changes and the effect never wakes.
	local renderer, cache = newScenario()

	local destroy, label = vide.root(function()
		return Svg({ source = SEARCH, size = 24 })
	end)

	label.AbsoluteSize = Vector2.new(24, 24)

	equal("a measurement that agrees costs no raster", #renderer.images, 1)
	equal("nothing is freed", #renderer.destroyed, 0)
	equal("and one entry is held once", stats(cache).referenceCount, 1)

	destroy()
end

do
	-- Same for the intrinsic case: no `size` and no `Size`, so the view box is
	-- the layout size — and the measurement still supersedes it.
	local renderer = newScenario()

	local destroy, label = vide.root(function()
		return Svg({ source = SEARCH })
	end)

	equal("the view box is the starting resolution", renderer.images[1].width, 24)

	label.AbsoluteSize = Vector2.new(72, 72)
	equal("a scaled-up intrinsic icon re-rasterizes", renderer.images[2].width, 72)

	destroy()
end

-- ---------------------------------------------------------------------------
-- A Roblox-native `Size`
-- ---------------------------------------------------------------------------

do
	local renderer, cache = newScenario()

	local destroy, label = vide.root(function()
		return Svg({ source = SEARCH, Size = UDim2.fromOffset(64, 32) })
	end)

	equal("the UDim2 is used verbatim for layout", label.Size, UDim2.fromOffset(64, 32))
	-- Layout has not run, so the resolution is unknown — and "unknown" is not a
	-- size. Rasterizing at 1x1, or at the view box size, would be a *wrong*
	-- answer that costs a real image and a real cache entry, and it would be
	-- discarded on the very next line. React acquires nothing here; so does this.
	equal("before layout nothing is rasterized", #renderer.images, 0)
	equal("so the cache holds nothing", stats(cache).entryCount, 0)
	equal("and nothing references anything", stats(cache).referenceCount, 0)
	equal("the label shows no image at all", label.ImageContent, Content.none)

	label.AbsoluteSize = Vector2.new(64, 32)

	equal("the first measurement makes the first raster", #renderer.images, 1)
	equal("at the laid-out width", renderer.images[1].width, 64)
	equal("at the laid-out height", renderer.images[1].height, 32)
	equal("with nothing destroyed on the way", #renderer.destroyed, 0)
	equal("one entry, one reference", stats(cache).entryCount, 1)
	equal("held once", stats(cache).referenceCount, 1)

	destroy()
end

do
	-- A scale-based UDim2 cannot be turned into pixels at all without layout,
	-- which is the whole reason the measurement exists.
	local renderer = newScenario()

	local destroy, label = vide.root(function()
		return Svg({ source = SEARCH, Size = UDim2.fromScale(0.5, 0.25) })
	end)

	equal("a scale Size rasterizes nothing before layout", #renderer.images, 0)

	label.AbsoluteSize = Vector2.new(100, 50)

	equal("and exactly one raster after it", #renderer.images, 1)
	equal("a scale Size rasters at the resolved size", renderer.images[#renderer.images].width, 100)
	equal("in both dimensions", renderer.images[#renderer.images].height, 50)

	destroy()
end

do
	-- Both props: `Size` wins, so the raster follows AbsoluteSize and not the 24
	-- that `size` asked for. This is the precedence React documents.
	local renderer = newScenario()

	local destroy, label = vide.root(function()
		return Svg({ source = SEARCH, size = 24, Size = UDim2.fromOffset(96, 96) })
	end)

	equal("Size defers even when size could have answered", #renderer.images, 0)

	label.AbsoluteSize = Vector2.new(96, 96)

	equal("Size wins over size", renderer.images[#renderer.images].width, 96)
	equal("and never rasterized the 24 that size asked for", #renderer.images, 1)

	destroy()
end

do
	-- Subpixel layout noise must not reach the renderer: two AbsoluteSizes that
	-- round to the same integers are the same raster. Without this, an animated
	-- layout rasterizes on every frame it moves at all.
	local renderer = newScenario()

	local destroy, label = vide.root(function()
		return Svg({ source = SEARCH, Size = UDim2.fromScale(1, 1) })
	end)

	label.AbsoluteSize = Vector2.new(47.8, 48.2)
	local afterFirst = #renderer.images
	equal("the first measurement is the only raster so far", afterFirst, 1)
	equal("a fractional size snaps to whole pixels", renderer.images[afterFirst].width, 48)
	equal("in both dimensions", renderer.images[afterFirst].height, 48)

	label.AbsoluteSize = Vector2.new(48.2, 47.8)
	equal("a size that snaps the same does not rasterize", #renderer.images, afterFirst)

	label.AbsoluteSize = Vector2.new(48, 48)
	equal("nor does landing exactly on it", #renderer.images, afterFirst)

	destroy()
end

-- ---------------------------------------------------------------------------
-- A reactive asset
-- ---------------------------------------------------------------------------

do
	local renderer, cache = newScenario()
	local icon = vide.source(SEARCH)

	local destroy, label = vide.root(function()
		return Svg({ source = icon, size = 24 })
	end)

	equal("a reactive source mounts", #renderer.images, 1)

	icon(SETTINGS)

	equal("swapping the asset rasterizes the new one", #renderer.images, 2)
	equal("the new raster is acquired first", renderer.events[2], "render 24x24#2")
	equal("the old one is released after", renderer.events[3], "destroy 24x24#1")
	equal("the label shows the new asset", label.ImageContent, Content.fromObject(renderer.images[2]))
	equal("only one entry is live", stats(cache).entryCount, 1)

	destroy()
end

-- ---------------------------------------------------------------------------
-- Stroke width
-- ---------------------------------------------------------------------------

do
	local renderer = newScenario()
	local width = vide.source(2)

	local destroy = vide.root(function()
		return Svg({ source = SEARCH, size = 24, strokeWidth = width })
	end)

	equal("the stroke override reaches the renderer", renderer.images[1].strokeWidth, 2)

	width(1)
	equal("changing it rasterizes again", #renderer.images, 2)
	equal("with the new width", renderer.images[2].strokeWidth, 1)

	destroy()
end

do
	-- `absoluteStrokeWidth` is a unit, and converting it is the core's job:
	-- 2 pixels at 48px on a 24-unit view box is 1 view box unit. The binding
	-- must pass the flag through rather than doing arithmetic of its own.
	local renderer = newScenario()

	local destroy = vide.root(function()
		return Svg({ source = SEARCH, size = 48, strokeWidth = 2, absoluteStrokeWidth = true })
	end)

	equal("an absolute stroke is converted to view box units", renderer.images[1].strokeWidth, 1)

	destroy()
end

-- ---------------------------------------------------------------------------
-- Conditional scopes
-- ---------------------------------------------------------------------------

do
	-- Destroying the whole root proves little on its own: the interesting case
	-- is a branch scope disappearing while the application keeps running, which
	-- is what `show` (and `<Show>`) creates.
	local renderer, cache = newScenario()
	local visible = vide.source(true)

	local destroy = vide.root(function()
		return vide.jsx(
			"frame",
			{},
			vide.show(visible, function()
				return Svg({ source = SEARCH, size = 24 })
			end)
		)
	end)

	equal("the branch mounts", stats(cache).referenceCount, 1)

	visible(false)
	equal("hiding the branch releases the handle", stats(cache).referenceCount, 0)
	equal("and destroys the raster", #renderer.destroyed, 1)

	visible(true)
	equal("showing it again acquires a new one", stats(cache).referenceCount, 1)
	equal("rasterizing again", #renderer.images, 2)

	destroy()
	equal("nothing is left over", stats(cache).referenceCount, 0)
	equal("every raster is destroyed exactly once", #renderer.destroyed, 2)
end

-- ---------------------------------------------------------------------------
-- Lifecycle counters
--
-- Not a raster benchmark — those belong to the core and are measured against
-- the Rust reference. These count the thing a framework binding can get wrong
-- at scale: how many times a run of updates reaches the rasterizer.
-- ---------------------------------------------------------------------------

do
	local renderer, cache = newScenario()
	local colour = vide.source(Color3.fromRGB(0, 0, 0))

	local destroy, label = vide.root(function()
		return Svg({ source = SEARCH, size = 24, color = colour })
	end)

	equal("mounting costs one raster", #renderer.images, 1)

	-- A theme animation, or a hundred frames of a colour tween.
	for step = 1, 100 do
		colour(Color3.fromRGB(step * 2 % 256, 255 - step % 256, step % 256))
	end

	equal("100 tint-only updates cost none", #renderer.images, 1)
	equal("and free nothing", #renderer.destroyed, 0)
	equal("and still hold one reference", stats(cache).referenceCount, 1)
	equal("with the last colour applied", label.ImageColor3, Color3.fromRGB(200, 155, 100))

	destroy()
end

do
	local renderer, cache = newScenario()
	local size = vide.source(16)

	local destroy = vide.root(function()
		return Svg({ source = SEARCH, size = size })
	end)

	-- 16 → 24 → 32 → 24. The last step is the interesting one: its raster was
	-- freed when nothing referenced it, so returning costs a rasterization
	-- rather than a hit. That is the current cache policy — an entry lives
	-- exactly as long as something holds it — and a future size-bounded cache
	-- that retains unreferenced entries would turn this into a hit without the
	-- binding changing at all.
	for _, target in { 24, 32, 24 } do
		size(target)
	end

	equal("a four-step size sequence costs four rasters", #renderer.images, 4)
	equal("three of which are freed on the way", #renderer.destroyed, 3)
	equal("leaving one entry", stats(cache).entryCount, 1)
	equal("and one reference", stats(cache).referenceCount, 1)

	destroy()
	equal("and nothing at the end", stats(cache).entryCount, 0)
	equal("with every raster destroyed exactly once", #renderer.destroyed, #renderer.images)
end

-- ---------------------------------------------------------------------------
-- Strict mode
-- ---------------------------------------------------------------------------

do
	-- Vide's strict mode evaluates every reactive scope twice. A binding that
	-- released its raster from `cleanup()` would, on each of those, drop the
	-- entry's last reference before re-acquiring it — destroying and
	-- re-rasterizing an image for nothing. Handing over explicitly is what makes
	-- this scenario cost one raster instead of several.
	vide.strict = true
	local renderer, cache = newScenario()
	local size = vide.source(24)

	local destroy = vide.root(function()
		return Svg({ source = SEARCH, size = size })
	end)

	equal("strict mode never destroys the live raster", #renderer.destroyed, 0)
	equal("and holds exactly one reference", stats(cache).referenceCount, 1)
	equal("having rasterized once", #renderer.images, 1)

	size(48)
	equal("after a change it still holds one reference", stats(cache).referenceCount, 1)
	equal("and one entry", stats(cache).entryCount, 1)
	equal("and rasterized the new size once", #renderer.images, 2)

	destroy()
	equal("strict mode leaves nothing referenced", stats(cache).referenceCount, 0)
	equal("and nothing cached", stats(cache).entryCount, 0)
	equal("every image it made was destroyed", #renderer.destroyed, #renderer.images)
	vide.strict = false
end

do
	-- The unmeasured state under strict mode. The doubled evaluation runs the
	-- raster effect twice before layout, and neither run may invent a
	-- resolution: two placeholder rasters would be twice as wrong as one.
	vide.strict = true
	local renderer, cache = newScenario()

	local destroy, label = vide.root(function()
		return Svg({ source = SEARCH, Size = UDim2.fromScale(1, 1) })
	end)

	equal("strict mode rasterizes nothing before layout either", #renderer.images, 0)
	equal("holding no references", stats(cache).referenceCount, 0)
	equal("and showing no image", label.ImageContent, Content.none)

	label.AbsoluteSize = Vector2.new(40, 40)

	equal("and exactly one raster once measured", #renderer.images, 1)
	equal("at the measured size", renderer.images[1].width, 40)
	equal("still holding exactly one reference", stats(cache).referenceCount, 1)
	equal("with nothing destroyed", #renderer.destroyed, 0)

	destroy()
	equal("teardown releases it", stats(cache).referenceCount, 0)
	equal("and destroys the one image", #renderer.destroyed, 1)
	vide.strict = false
end

-- ---------------------------------------------------------------------------

print(`vide: {passed} passed, {failed} failed`)
if failed > 0 then
	for _, failure in failures do
		print(`  FAIL {failure}`)
	end
	error(`{failed} Vide assertion(s) failed`, 0)
end
