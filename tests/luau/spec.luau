--!nocheck
--[[
	Runtime tests for the Luau half of `@rbxts/svg`.

	These run the *compiled* package — the Luau `rbxtsc` emitted — against IR
	produced by the real Rust compiler. That makes them the test that actually
	proves the two halves of the format agree: `crates/svg-ir` writes the bytes,
	and this reads them back with an entirely separate implementation.

	Run with `pnpm test:luau`, which regenerates the bundle first.
]]

local harness = require("./harness")
local sources = require("./sources")
local fixtures = require("./fixtures")

-- The standalone luau CLI has no Roblox datatypes. Vector2 (sizes) and Color3
-- (currentColor) are the only two the package reaches for.
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

local function near(name, actual, expected, tolerance)
	local delta = math.abs(actual - expected)
	check(name, delta <= (tolerance or 1e-4), `expected ~{expected}, got {actual}`)
end

local function expectError(name, fn)
	local ok = pcall(fn)
	check(name, not ok, "expected an error, but the call succeeded")
end

-- ---------------------------------------------------------------------------
-- Base64 + header decoding
-- ---------------------------------------------------------------------------

for name, fixture in fixtures.assets do
	local asset = internal.createAssetFromBase64(fixture.base64, fixture.hash)
	local data = internal.inspect(asset)

	equal(`{name}: byte length`, buffer.len(data.data), fixture.byteLength)
	equal(`{name}: shape count`, data.shapeCount, fixture.shapeCount)
	equal(`{name}: feature flags`, data.features, fixture.flags)
	near(`{name}: viewBox.x`, data.viewBox.x, fixture.viewBox.x)
	near(`{name}: viewBox.y`, data.viewBox.y, fixture.viewBox.y)
	near(`{name}: viewBox.width`, data.viewBox.width, fixture.viewBox.width)
	near(`{name}: viewBox.height`, data.viewBox.height, fixture.viewBox.height)
	equal(`{name}: id is the content hash`, data.id, fixture.hash)

	-- Public accessors agree with the internal view.
	equal(`{name}: getShapeCount`, svg.getShapeCount(asset), fixture.shapeCount)
	equal(`{name}: getFeatures`, svg.getFeatures(asset), fixture.flags)
	near(`{name}: getViewBox width`, svg.getViewBox(asset).width, fixture.viewBox.width)
end

-- ---------------------------------------------------------------------------
-- Lucide: the properties the whole design depends on
-- ---------------------------------------------------------------------------

do
	local asset = internal.createAssetFromBase64(
		fixtures.assets.lucideSearch.base64,
		fixtures.assets.lucideSearch.hash
	)

	check("search is tintable", svg.isTintable(asset), "expected a tintable asset")
	equal("search shape count", svg.getShapeCount(asset), 2)

	local line = internal.shapeAt(asset, 0)
	check("search shape 0 has a stroke", line.hasStroke, "expected a stroke")
	check("search shape 0 has no fill", not line.hasFill, "Lucide sets fill=none")
	near("search stroke width", line.strokeWidth, 2)
	equal("search line cap is round", line.lineCap, 1)
	equal("search line join is round", line.lineJoin, 1)
	check("search fill rule is nonzero", not line.evenOdd, "expected nonzero")
	check("search paints fill first", not line.strokeFirst, "expected fill-then-stroke")

	local paint = internal.paintAt(asset, line.strokePaint)
	check("search stroke is currentColor", paint.isCurrentColor, "expected currentColor")
	near("search stroke alpha", paint.alpha, 1)

	-- The line is `m21 21-4.34-4.34`: a move followed by a single line.
	local ops = {}
	internal.visitCommands(asset, line, {
		-- roblox-ts emits colon-calls for interface methods, so each visitor
		-- receives the visitor table itself as its first argument.
		moveTo = function(_, x, y)
			table.insert(ops, { "moveTo", x, y })
		end,
		lineTo = function(_, x, y)
			table.insert(ops, { "lineTo", x, y })
		end,
		cubicTo = function(_, ...)
			table.insert(ops, { "cubicTo", ... })
		end,
		close = function()
			table.insert(ops, { "close" })
		end,
	})

	equal("search shape 0 command count", #ops, 2)
	equal("search shape 0 starts with a move", ops[1][1], "moveTo")
	near("search move x", ops[1][2], 21)
	near("search move y", ops[1][3], 21)
	equal("search shape 0 then lines", ops[2][1], "lineTo")
	near("search line x", ops[2][2], 16.66, 1e-2)
	near("search line y", ops[2][3], 16.66, 1e-2)

	-- The circle lowers to four cubic quadrants plus a close.
	local circle = internal.shapeAt(asset, 1)
	local cubics, closes = 0, 0
	internal.visitCommands(asset, circle, {
		moveTo = function() end,
		lineTo = function() end,
		cubicTo = function()
			cubics += 1
		end,
		close = function()
			closes += 1
		end,
	})
	equal("search circle cubic count", cubics, 4)
	equal("search circle close count", closes, 1)
end

-- ---------------------------------------------------------------------------
-- Paint, fill rule and coordinate space
-- ---------------------------------------------------------------------------

do
	local asset = internal.createAssetFromBase64(fixtures.assets.solidFill.base64)
	local shape = internal.shapeAt(asset, 0)
	local paint = internal.paintAt(asset, shape.fillPaint)

	check("solid fill is not currentColor", not paint.isCurrentColor, "expected a solid colour")
	equal("solid fill red", paint.r, 0x33)
	equal("solid fill green", paint.g, 0x66)
	equal("solid fill blue", paint.b, 0x99)
	check("solid fill has no stroke", not shape.hasStroke, "expected no stroke")
	check("anonymous assets still get an id", internal.inspect(asset).id ~= nil, "missing id")
end

do
	local asset = internal.createAssetFromBase64(fixtures.assets.evenOdd.base64)
	check("evenodd survives the round trip", internal.shapeAt(asset, 0).evenOdd, "expected evenodd")
end

do
	-- Two subpaths means two moveTo commands; a decoder that dropped subpath
	-- structure would break fill rules.
	local asset = internal.createAssetFromBase64(fixtures.assets.multipleSubpaths.base64)
	local moves = 0
	internal.visitCommands(asset, internal.shapeAt(asset, 0), {
		moveTo = function()
			moves += 1
		end,
		lineTo = function() end,
		cubicTo = function() end,
		close = function() end,
	})
	equal("subpaths are preserved", moves, 2)
end

do
	-- The source declares width/height 96 with viewBox "-12 -12 24 24". The
	-- runtime must see the 24-unit space, with geometry around the origin.
	local asset = internal.createAssetFromBase64(fixtures.assets.offsetViewBox.base64)
	local viewBox = svg.getViewBox(asset)
	near("offset viewBox x", viewBox.x, -12)
	near("offset viewBox width", viewBox.width, 24)

	local minX, maxX = math.huge, -math.huge
	local function track(x)
		minX = math.min(minX, x)
		maxX = math.max(maxX, x)
	end
	internal.visitCommands(asset, internal.shapeAt(asset, 0), {
		moveTo = function(_, x)
			track(x)
		end,
		lineTo = function(_, x)
			track(x)
		end,
		cubicTo = function(_, c1x, _c1y, c2x, _c2y, x)
			track(c1x)
			track(c2x)
			track(x)
		end,
		close = function() end,
	})
	near("offset geometry min x", minX, -10, 0.05)
	near("offset geometry max x", maxX, 10, 0.05)
end

-- ---------------------------------------------------------------------------
-- Malformed input must error, never misbehave silently
-- ---------------------------------------------------------------------------

expectError("empty payload is rejected", function()
	internal.createAssetFromBase64("")
end)

expectError("non-base64 payload is rejected", function()
	internal.createAssetFromBase64("!!!!")
end)

expectError("truncated payload is rejected", function()
	internal.createAssetFromBase64(string.sub(fixtures.assets.lucideSearch.base64, 1, 20))
end)

expectError("wrong magic is rejected", function()
	internal.createAssetFromBase64("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")
end)

expectError("shape index out of range is rejected", function()
	local asset = internal.createAssetFromBase64(fixtures.assets.lucideSearch.base64)
	internal.shapeAt(asset, 99)
end)

expectError("paint index out of range is rejected", function()
	local asset = internal.createAssetFromBase64(fixtures.assets.lucideSearch.base64)
	internal.paintAt(asset, 99)
end)

-- Corrupted variants of a real header, so each rejection path is exercised for
-- the reason it exists rather than incidentally.
expectError("a future IR version is rejected", function()
	internal.createAssetFromBase64(fixtures.corrupt.wrongVersion)
end)

expectError("bad magic is rejected", function()
	internal.createAssetFromBase64(fixtures.corrupt.wrongMagic)
end)

expectError("tables larger than the blob are rejected", function()
	internal.createAssetFromBase64(fixtures.corrupt.overlongTables)
end)

expectError("a truncated header is rejected", function()
	internal.createAssetFromBase64(fixtures.corrupt.headerOnly)
end)

do
	-- The version error must name both versions, so the fix is obvious.
	local ok, message = pcall(function()
		internal.createAssetFromBase64(fixtures.corrupt.wrongVersion)
	end)
	check("version error is actionable", not ok and string.find(tostring(message), "Recompile") ~= nil,
		`message was {message}`)
end

-- ---------------------------------------------------------------------------
-- preserveAspectRatio: the policy must survive compile -> IR -> Luau decode
-- ---------------------------------------------------------------------------

do
	local ALIGN = svg.SvgAspectAlign
	local SCALE = svg.SvgAspectScale

	local cases = {
		-- fixture, expected align, expected scale, expected compiler string
		{ "aspectMeet", ALIGN.XMidYMid, SCALE.Meet, "xMidYMid meet" },
		{ "aspectNone", ALIGN.None, SCALE.Meet, "none" },
		{ "aspectSlice", ALIGN.XMinYMin, SCALE.Slice, "xMinYMin slice" },
		-- No attribute at all: SVG's default, which is *not* align 0.
		{ "lucideSearch", ALIGN.XMidYMid, SCALE.Meet, "xMidYMid meet" },
	}

	for _, case in cases do
		local name, expectedAlign, expectedScale, expectedText = case[1], case[2], case[3], case[4]
		local fixture = fixtures.assets[name]
		local asset = internal.createAssetFromBase64(fixture.base64, fixture.hash)
		local aspect = svg.getPreserveAspectRatio(asset)

		equal(`{name}: aspect align`, aspect.align, expectedAlign)
		equal(`{name}: aspect scale`, aspect.scale, expectedScale)
		-- The compiler's own rendering of the same value, so the two halves of
		-- the pipeline are checked against each other and not just internally.
		equal(`{name}: compiler agrees`, fixture.preserveAspectRatio, expectedText)
	end
end

do
	-- The three aspect fixtures share their geometry and their view box, so the
	-- fitting transform is the only thing that can tell them apart. This is
	-- exactly the information IR version 1 discarded.
	local function fit(name, width, height)
		local fixture = fixtures.assets[name]
		local asset = internal.createAssetFromBase64(fixture.base64)
		return svg.viewBoxTransform(
			svg.getViewBox(asset),
			svg.getPreserveAspectRatio(asset),
			width,
			height
		)
	end

	local function mapPoint(t, x, y)
		return t.sx * x + t.kx * y + t.tx, t.ky * x + t.sy * y + t.ty
	end

	-- 24x12 letterboxed into 100x100 becomes 100x50, centred vertically.
	local meet = fit("aspectMeet", 100, 100)
	local x, y = mapPoint(meet, 0, 0)
	near("meet: origin x", x, 0)
	near("meet: origin y", y, 25)
	x, y = mapPoint(meet, 24, 12)
	near("meet: far corner x", x, 100)
	near("meet: far corner y", y, 75)

	-- `none` stretches: the view box corner lands on the target corner.
	local none = fit("aspectNone", 100, 100)
	x, y = mapPoint(none, 24, 12)
	near("none: far corner x", x, 100)
	near("none: far corner y", y, 100)
	near("none: scales differ", none.sx / none.sy, 100 / 24 / (100 / 12))

	-- `slice` from the top-left covers the target at 200x100 and crops.
	local slice = fit("aspectSlice", 100, 100)
	x, y = mapPoint(slice, 0, 0)
	near("slice: origin x", x, 0)
	near("slice: origin y", y, 0)
	x, y = mapPoint(slice, 24, 12)
	near("slice: far corner x", x, 200)
	near("slice: far corner y", y, 100)

	-- A non-zero view box origin must land on the aligned corner.
	local offset = fixtures.assets.offsetViewBox
	local offsetAsset = internal.createAssetFromBase64(offset.base64)
	local t = svg.viewBoxTransform(
		svg.getViewBox(offsetAsset),
		svg.getPreserveAspectRatio(offsetAsset),
		48,
		48
	)
	x, y = mapPoint(t, -12, -12)
	near("offset viewBox origin maps to the target origin x", x, 0)
	near("offset viewBox origin maps to the target origin y", y, 0)
end

-- ---------------------------------------------------------------------------
-- Sizing: layout size and raster resolution are different things
--
-- This policy lives in the core rather than in a binding because React and
-- Vide have to answer it identically; testing it here, with no component
-- mounted, is the point of it being separable.
-- ---------------------------------------------------------------------------

do
	local viewBox = { x = 0, y = 0, width = 24, height = 12 }

	-- Explicit numeric `size`: known up front, so nothing has to be measured.
	local numeric = svg.resolveSvgSizing(viewBox, 24, false)
	check("numeric size needs no measurement", not numeric.measureAbsoluteSize, "expected false")
	equal("numeric size raster width", numeric.declaredPixels.X, 24)
	equal("numeric size raster height", numeric.declaredPixels.Y, 24)

	-- `Size` alone: the raster resolution comes from AbsoluteSize.
	local robloxSize = svg.resolveSvgSizing(viewBox, nil, true)
	check("Size is measured", robloxSize.measureAbsoluteSize, "expected true")

	-- Both: `Size` wins, so the raster still follows AbsoluteSize. This is the
	-- bug that let `<Svg source={Search} Size={UDim2.fromOffset(96, 96)} />`
	-- rasterize at 24x24 and then stretch the result.
	local both = svg.resolveSvgSizing(viewBox, 24, true)
	check("Size wins over size", both.measureAbsoluteSize, "expected Size to take precedence")

	-- Neither: the view box's own dimensions.
	local intrinsic = svg.resolveSvgSizing(viewBox, nil, false)
	check("intrinsic needs no measurement", not intrinsic.measureAbsoluteSize, "expected false")
	equal("intrinsic width", intrinsic.declaredPixels.X, 24)
	equal("intrinsic height", intrinsic.declaredPixels.Y, 12)

	-- `initialPixels` is the resolution to draw at *until* the engine reports
	-- one. A component that owns its layout size has an answer straight away;
	-- a UDim2-driven one has none, and must draw nothing rather than guess.
	-- In both cases the measurement supersedes it once it arrives, which is
	-- what keeps a scaled-up icon from being an upscaled image.
	local declared = svg.resolveSvgSizing(viewBox, 24, false)
	equal("a declared size starts at itself", declared.initialPixels.X, 24)
	equal("in both dimensions", declared.initialPixels.Y, 24)
	equal("intrinsic starts at the view box", intrinsic.initialPixels.X, 24)
	equal("in both dimensions", intrinsic.initialPixels.Y, 12)
	check(
		"a UDim2 has no starting resolution",
		svg.resolveSvgSizing(viewBox, 24, true).initialPixels == nil,
		"expected nil"
	)
	equal(
		"but still lays out at the declared size",
		svg.resolveSvgSizing(viewBox, 24, true).declaredPixels.X,
		24
	)

	-- Snapping is what stops a resizing layout rasterizing every frame, and the
	-- 1x1 floor is what stops it producing an invalid EditableImage.
	equal("snap rounds up", svg.snapSvgPixelSize(Vector2.new(23.6, 23.6)).X, 24)
	equal("snap rounds down", svg.snapSvgPixelSize(Vector2.new(24.4, 24.4)).X, 24)
	equal("snap floors at one pixel", svg.snapSvgPixelSize(Vector2.new(0, 0)).X, 1)
	equal("snap floors negatives too", svg.snapSvgPixelSize(Vector2.new(-5, -5)).Y, 1)

	-- A measured AbsoluteSize is the one place that floor is wrong: an instance
	-- that has not been laid out reports zero, and "zero" is not "very small".
	-- Both bindings ask this question, so both get the same answer.
	equal("a measured size snaps like any other", svg.measureSvgPixelSize(Vector2.new(47.8, 48.2)).X, 48)
	equal("in both dimensions", svg.measureSvgPixelSize(Vector2.new(47.8, 48.2)).Y, 48)
	check("an unlaid-out instance has no size", svg.measureSvgPixelSize(Vector2.new(0, 0)) == nil, "expected nil")
	check("nor does one laid out in one axis only", svg.measureSvgPixelSize(Vector2.new(24, 0)) == nil, "expected nil")
	check("a size rounding to zero is not a size", svg.measureSvgPixelSize(Vector2.new(0.4, 0.4)) == nil, "expected nil")
	equal("but one rounding to a pixel is", svg.measureSvgPixelSize(Vector2.new(0.6, 0.6)).X, 1)
end

-- ---------------------------------------------------------------------------
-- No renderer installed: fail fast, before anything installs one
-- ---------------------------------------------------------------------------

do
	local asset = internal.createAssetFromBase64(fixtures.assets.lucideSearch.base64)
	check(
		"no renderer is installed yet",
		svg.getSvgRenderer() == nil,
		"a renderer was installed before this test"
	)

	local ok, message = pcall(function()
		svg.renderSvg(asset, { size = Vector2.new(24, 24) })
	end)
	check("renderSvg throws when no renderer is installed", not ok, "expected an error")
	check(
		"the no-renderer error names setSvgRenderer",
		not ok and string.find(tostring(message), "setSvgRenderer", 1, true) ~= nil,
		`message was {message}`
	)
end

-- ---------------------------------------------------------------------------
-- Render cache: keys, sharing and deterministic release
-- ---------------------------------------------------------------------------

do
	local asset = internal.createAssetFromBase64(
		fixtures.assets.lucideSearch.base64,
		fixtures.assets.lucideSearch.hash
	)

	local rendered, destroyed = 0, 0
	local fakeRenderer = {
		version = 1,
		render = function()
			rendered += 1
			return { id = rendered }
		end,
		destroy = function()
			destroyed += 1
		end,
	}

	svg.setSvgRenderer(fakeRenderer)

	local a = svg.renderSvg(asset, { size = Vector2.new(24, 24) })
	local b = svg.renderSvg(asset, { size = Vector2.new(24, 24) })
	equal("identical requests rasterize once", rendered, 1)
	equal("identical requests share an image", a.image, b.image)
	check("handle reports tintable", a.tintable, "search is tintable")

	local c = svg.renderSvg(asset, { size = Vector2.new(48, 48) })
	equal("a different size rasterizes again", rendered, 2)
	check("a different size is a different image", c.image ~= a.image, "images should differ")

	a.release()
	equal("image survives while referenced", destroyed, 0)
	a.release()
	equal("releasing twice does not double free", destroyed, 0)
	b.release()
	equal("last release destroys the image", destroyed, 1)

	c.release()
	equal("all images released", destroyed, 2)
	equal("cache is empty", svg.getSvgRenderCache():stats().entryCount, 0)

	-- Sub-pixel sizes must snap, or an animated size would miss every frame.
	local d = svg.renderSvg(asset, { size = Vector2.new(24.4, 23.6) })
	local e = svg.renderSvg(asset, { size = Vector2.new(24, 24) })
	equal("fractional sizes snap to the same entry", d.image, e.image)
	d.release()
	e.release()

	-- Colour is deliberately absent from the key for tintable assets, which is
	-- what the whole ImageColor3 fast path relies on. Nothing in the options
	-- carries a colour, so this is structural: assert the key's shape.
	local resolved = svg.resolveRenderOptions(asset, { size = Vector2.new(24, 24) })
	local key = svg.renderCacheKey(asset, resolved, 1)
	check(
		"cache key contains the asset hash",
		string.find(key, fixtures.assets.lucideSearch.hash, 1, true) ~= nil,
		`key was {key}`
	)
	check("cache key contains the pixel size", string.find(key, "24x24", 1, true) ~= nil, key)
	check("cache key contains the renderer version", string.find(key, "r1", 1, true) ~= nil, key)
end

do
	-- Absolute stroke width is specified in pixels and must be converted into
	-- view box units, so it produces a different key at a different size.
	local asset = internal.createAssetFromBase64(fixtures.assets.lucideSearch.base64)
	local atSmall = svg.resolveRenderOptions(
		asset,
		{ size = Vector2.new(24, 24), strokeWidth = 2, absoluteStrokeWidth = true }
	)
	local atLarge = svg.resolveRenderOptions(
		asset,
		{ size = Vector2.new(48, 48), strokeWidth = 2, absoluteStrokeWidth = true }
	)
	near("absolute stroke at 1x", atSmall.strokeWidth, 2)
	near("absolute stroke at 2x halves in view box units", atLarge.strokeWidth, 1)

	local relative = svg.resolveRenderOptions(
		asset,
		{ size = Vector2.new(48, 48), strokeWidth = 2 }
	)
	near("relative stroke is unchanged by size", relative.strokeWidth, 2)
end

-- ---------------------------------------------------------------------------
-- Colour paths: the three asset classes and their cache keys
-- ---------------------------------------------------------------------------

do
	local function assetOf(name)
		local fixture = fixtures.assets[name]
		return internal.createAssetFromBase64(fixture.base64, fixture.hash)
	end

	local fixed = assetOf("solidFill")
	local tintable = assetOf("lucideSearch")
	local mixed = assetOf("mixedCurrentColor")

	-- The feature helper that decides the paths.
	check("usesCurrentColor: fixed-colour asset", not svg.usesCurrentColor(fixed), "expected false")
	check("usesCurrentColor: tintable asset", svg.usesCurrentColor(tintable), "expected true")
	check("usesCurrentColor: mixed asset", svg.usesCurrentColor(mixed), "expected true")
	check("isTintable: mixed asset is NOT tintable", not svg.isTintable(mixed),
		"mixed currentColor must never take the ImageColor3 path")

	-- resolveRenderOptions carries a concrete colour, defaulting to black.
	local resolved = svg.resolveRenderOptions(tintable, { size = Vector2.new(24, 24) })
	check(
		"resolve: currentColor defaults to black",
		resolved.currentColor.R == 0 and resolved.currentColor.G == 0 and resolved.currentColor.B == 0,
		`got {resolved.currentColor.R},{resolved.currentColor.G},{resolved.currentColor.B}`
	)

	local rendered = 0
	local seenColors = {}
	svg.setSvgRenderer({
		version = 1,
		render = function(_, _, options)
			rendered += 1
			table.insert(seenColors, options.currentColor)
			return { id = rendered }
		end,
		destroy = function() end,
	})

	local red = Color3.fromRGB(255, 0, 0)
	local blue = Color3.fromRGB(0, 0, 255)
	local white = Color3.fromRGB(255, 255, 255)

	-- Case A: no currentColor in the asset. The requested colour cannot
	-- change a pixel, so every colour hits one cache entry.
	local a1 = svg.renderSvg(fixed, { size = Vector2.new(24, 24), currentColor = red })
	local a2 = svg.renderSvg(fixed, { size = Vector2.new(24, 24), currentColor = blue })
	local a3 = svg.renderSvg(fixed, { size = Vector2.new(24, 24) })
	equal("case A: colour never re-rasterizes a fixed-colour asset", rendered, 1)
	check("case A: all colours share one image", a1.image == a2.image and a2.image == a3.image, "images differ")
	check("case A: the key carries no colour", string.find(a1.cacheKey, "|c", 1, true) == nil, a1.cacheKey)

	-- Case B: tintable. One shared alpha mask; ImageColor3 does the colour.
	rendered = 0
	local b1 = svg.renderSvg(tintable, { size = Vector2.new(24, 24), currentColor = red })
	local b2 = svg.renderSvg(tintable, { size = Vector2.new(24, 24), currentColor = blue })
	local b3 = svg.renderSvg(tintable, { size = Vector2.new(24, 24), currentColor = white })
	equal("case B: three colours of one Lucide icon rasterize once", rendered, 1)
	check("case B: three colours share one image", b1.image == b2.image and b2.image == b3.image, "images differ")
	check("case B: handles report tintable", b1.tintable, "expected tintable")
	check("case B: the key carries no colour", string.find(b1.cacheKey, "|c", 1, true) == nil, b1.cacheKey)

	-- Case C: mixed. The resolved colour reaches the compositor, so it is
	-- part of the identity.
	rendered = 0
	seenColors = {}
	local c1 = svg.renderSvg(mixed, { size = Vector2.new(24, 24), currentColor = red })
	local c2 = svg.renderSvg(mixed, { size = Vector2.new(24, 24), currentColor = blue })
	equal("case C: two colours of a mixed asset rasterize twice", rendered, 2)
	check("case C: two colours are two images", c1.image ~= c2.image, "images shared")
	check("case C: keys differ by colour", c1.cacheKey ~= c2.cacheKey, c1.cacheKey)
	check("case C: the key carries the colour", string.find(c1.cacheKey, "|c255,0,0", 1, true) ~= nil, c1.cacheKey)
	check("case C: handles are not tintable", not c1.tintable, "mixed asset claimed tintable")
	check(
		"case C: the renderer received the resolved colour",
		seenColors[1] ~= nil and seenColors[1].R == 1 and seenColors[1].B == 0,
		"resolved colour missing"
	)

	-- Value semantics: a *different Color3 object* with the same channels must
	-- share the entry, and the default must equal explicit black.
	local c3 = svg.renderSvg(mixed, { size = Vector2.new(24, 24), currentColor = Color3.fromRGB(255, 0, 0) })
	equal("case C: an equal colour value reuses the raster", rendered, 2)
	check("case C: equal colour values share the image", c1.image == c3.image, "images differ")

	local d1 = svg.renderSvg(mixed, { size = Vector2.new(24, 24) })
	local d2 = svg.renderSvg(mixed, { size = Vector2.new(24, 24), currentColor = Color3.new(0, 0, 0) })
	check("case C: omitted colour equals explicit black", d1.image == d2.image, "default drifted from black")

	for _, handle in { a1, a2, a3, b1, b2, b3, c1, c2, c3, d1, d2 } do
		handle.release()
	end
	equal("colour paths: everything released", svg.getSvgRenderCache():stats().entryCount, 0)
end

-- ---------------------------------------------------------------------------

print(`luau: {passed} passed, {failed} failed`)
for _, failure in failures do
	print("  FAIL " .. failure)
end
if failed > 0 then
	error(`{failed} Luau assertion(s) failed`, 0)
end
