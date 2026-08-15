--!nocheck
--[[
	Pure-renderer tests for the Luau rasterizer in `packages/svg/src/raster`.

	These are the Rust unit suites (`flatten.rs`, `stroke.rs`, `edges.rs`,
	`image.rs`) ported assertion by assertion, plus a cross-language golden
	comparison: the same compiled asset rendered by the Rust reference through
	the native binding (see `goldens.luau`, regenerated on every bundle) and by
	this Luau port, compared byte-wise.

	Everything here runs under the standalone `luau` CLI — no Roblox engine.
	The `EditableImage` adapter is exercised against a fake factory; the real
	AssetService path is Studio-only by nature.

	Run with `pnpm test:luau`.
]]

local harness = require("./harness")
local sources = require("./sources")
local fixtures = require("./fixtures")
local goldens = require("./goldens")

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

local format = loader.loadModule("packages/svg/out/ir/format")
local base64 = loader.loadModule("packages/svg/out/ir/base64")
local flattenModule = loader.loadModule("packages/svg/out/raster/flatten")
local strokeModule = loader.loadModule("packages/svg/out/raster/stroke")
local edgesModule = loader.loadModule("packages/svg/out/raster/edges")
local coverageModule = loader.loadModule("packages/svg/out/raster/coverage")
local imageModule = loader.loadModule("packages/svg/out/raster/image")
local renderModule = loader.loadModule("packages/svg/out/raster/render")
local backendModule = loader.loadModule("packages/svg/out/raster/editableImageRenderer")

local FLATNESS_TOLERANCE = flattenModule.FLATNESS_TOLERANCE
local MAX_SUBDIVISION_DEPTH = flattenModule.MAX_SUBDIVISION_DEPTH
local ARC_TOLERANCE = strokeModule.ARC_TOLERANCE

local CAP_BUTT, CAP_ROUND, CAP_SQUARE = format.LINE_CAP_BUTT, format.LINE_CAP_ROUND, format.LINE_CAP_SQUARE
local JOIN_MITER, JOIN_ROUND, JOIN_BEVEL =
	format.LINE_JOIN_MITER, format.LINE_JOIN_ROUND, format.LINE_JOIN_BEVEL

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

local function expectError(name, pattern, fn)
	local ok, message = pcall(fn)
	check(
		name,
		not ok and string.find(tostring(message), pattern, 1, true) ~= nil,
		if ok then "expected an error, but the call succeeded" else `message was {message}`
	)
end

-- Shared geometry helpers ----------------------------------------------------

local function v(x, y)
	return { x = x, y = y }
end

local function contour(points, closed)
	local out = {}
	for _, p in points do
		table.insert(out, v(p[1], p[2]))
	end
	return { points = out, closed = closed }
end

local function isFiniteNumber(n)
	return n == n and n ~= math.huge and n ~= -math.huge
end

local function allFinite(polygons)
	for _, polygon in polygons do
		for _, point in polygon do
			if not (isFiniteNumber(point.x) and isFiniteNumber(point.y)) then
				return false
			end
		end
	end
	return true
end

local function bounds(polygons)
	local minX, minY, maxX, maxY = math.huge, math.huge, -math.huge, -math.huge
	for _, polygon in polygons do
		for _, point in polygon do
			minX = math.min(minX, point.x)
			minY = math.min(minY, point.y)
			maxX = math.max(maxX, point.x)
			maxY = math.max(maxY, point.y)
		end
	end
	return minX, minY, maxX, maxY
end

local function windingsAgree(polygons)
	for _, polygon in polygons do
		local area = strokeModule.signedArea(polygon)
		if math.abs(area) > 1e-6 and area < 0 then
			return false
		end
	end
	return true
end

--[[
	Scan-converts polygons into a flat coverage table, `out[y * width + x + 1]`.
	The Luau twin of the Rust tests' `coverage` helper: it is the only way to
	check the property that actually matters — what area ends up covered.
]]
local function rasterizeCoverage(polygons, evenOdd, width, height)
	local edges = edgesModule.EdgeSet.new()
	for _, polygon in polygons do
		edges:addPolygon(polygon)
	end
	edges:finish()

	local out = table.create(width * height, 0)
	local sampler = coverageModule.ScanlineSupersampler.new()
	sampler:rasterize(edges, evenOdd, width, height, function(y, row, startX, endX)
		for x = startX, endX - 1 do
			out[y * width + x + 1] = row[x + 1]
		end
	end)
	return out
end

local function coverageAt(out, width, x, y)
	return out[y * width + x + 1]
end

local function stroke(contours, style)
	local out = {}
	strokeModule.expand(contours, style, out)
	return out
end

local function style(width, cap, join, miterLimit)
	return { width = width, cap = cap, join = join, miterLimit = miterLimit }
end

-- ---------------------------------------------------------------------------
-- flatten: adaptive cubic subdivision
-- ---------------------------------------------------------------------------

local function flattenCurve(p0, p1, p2, p3)
	local out = { v(p0[1], p0[2]) }
	flattenModule.flattenCubic(p0[1], p0[2], p1[1], p1[2], p2[1], p2[2], p3[1], p3[2], out)
	return out
end

do
	-- A cubic whose control points sit on the chord is already a line.
	local points = flattenCurve({ 0, 0 }, { 100, 0 }, { 200, 0 }, { 300, 0 })
	equal("flatten: a straight cubic needs no subdivision", #points, 2)

	points = flattenCurve({ 0, 0 }, { 0, 100 }, { 100, 100 }, { 100, 0 })
	check("flatten: a curved cubic is subdivided", #points > 8, `got {#points} points`)
end

do
	-- The whole reason flattening is not a compile-time step: the same curve
	-- drawn larger must be subdivided more.
	local K = 0.552285
	local function quarterCircle(r)
		return #flattenCurve({ r, 0 }, { r, r * K }, { r * K, r }, { 0, r })
	end
	local small = quarterCircle(8)
	local large = quarterCircle(512)
	check("flatten: subdivision scales with output resolution", large > small * 4, `{small} -> {large}`)
end

do
	-- Every flattened point must be within tolerance of the true curve.
	local p0, p1, p2, p3 = { 10, 200 }, { 60, -80 }, { 180, 300 }, { 240, 20 }
	local polyline = flattenCurve(p0, p1, p2, p3)

	local function distanceToSegment(px, py, ax, ay, bx, by)
		local abx, aby = bx - ax, by - ay
		local lsq = abx * abx + aby * aby
		if lsq <= 0 then
			local dx, dy = px - ax, py - ay
			return math.sqrt(dx * dx + dy * dy)
		end
		local t = math.clamp(((px - ax) * abx + (py - ay) * aby) / lsq, 0, 1)
		local dx, dy = px - (ax + abx * t), py - (ay + aby * t)
		return math.sqrt(dx * dx + dy * dy)
	end

	local worst = 0
	for i = 0, 2000 do
		local t = i / 2000
		local u = 1 - t
		local ex = u * u * u * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t * t * t * p3[1]
		local ey = u * u * u * p0[2] + 3 * u * u * t * p1[2] + 3 * u * t * t * p2[2] + t * t * t * p3[2]
		local nearest = math.huge
		for index = 1, #polyline - 1 do
			local a, b = polyline[index], polyline[index + 1]
			nearest = math.min(nearest, distanceToSegment(ex, ey, a.x, a.y, b.x, b.y))
		end
		worst = math.max(worst, nearest)
	end
	check("flatten: the polyline stays within tolerance of the curve", worst <= FLATNESS_TOLERANCE,
		`worst deviation was {worst}`)
end

do
	local points = flattenCurve({ 0, 0 }, { 0, 100 }, { 100, 100 }, { 100, 0 })
	equal("flatten: output starts at the start", points[1].x, 0)
	equal("flatten: output ends at the end", points[#points].x, 100)
	local ordered = true
	for index = 1, #points - 1 do
		if points[index + 1].x < points[index].x - 1e-4 then
			ordered = false
		end
	end
	check("flatten: output is in curve order", ordered, "x went backwards")
end

do
	local function run()
		return flattenCurve({ 3, 7 }, { -40, 90 }, { 120, -30 }, { 77, 55 })
	end
	local a, b = run(), run()
	local same = #a == #b
	if same then
		for index = 1, #a do
			if a[index].x ~= b[index].x or a[index].y ~= b[index].y then
				same = false
			end
		end
	end
	check("flatten: flattening is deterministic", same, "two runs differed")
end

do
	-- Extreme control points must terminate, not subdivide forever.
	local points = flattenCurve({ 0, 0 }, { 1e9, 1e9 }, { -1e9, 1e9 }, { 1, 0 })
	check(
		"flatten: an extreme curve is bounded by the depth limit",
		#points <= 2 ^ MAX_SUBDIVISION_DEPTH + 1,
		`{#points} points`
	)
	local finite = true
	for _, p in points do
		if not (isFiniteNumber(p.x) and isFiniteNumber(p.y)) then
			finite = false
		end
	end
	check("flatten: extreme curve output is finite", finite, "non-finite point")
end

do
	-- A degenerate "loop" cubic has no chord to measure against and must not
	-- divide by zero.
	local points = flattenCurve({ 50, 50 }, { 150, 0 }, { -50, 0 }, { 50, 50 })
	local finite = true
	for _, p in points do
		if not (isFiniteNumber(p.x) and isFiniteNumber(p.y)) then
			finite = false
		end
	end
	check("flatten: a loop cubic does not produce non-finite points", finite, "non-finite point")
	check("flatten: a loop cubic still subdivides", #points > 4, `{#points} points`)
end

-- ---------------------------------------------------------------------------
-- flatten: contour extraction through PathFlattener
-- ---------------------------------------------------------------------------

local IDENTITY = { sx = 1, ky = 0, kx = 0, sy = 1, tx = 0, ty = 0 }

local function extractContours(transform, drive)
	local out = {}
	local flattener = flattenModule.PathFlattener.new(transform, out)
	drive(flattener)
	local ok = flattener:finish()
	return out, ok
end

do
	local contours = extractContours(IDENTITY, function(f)
		f:moveTo(0, 0)
		f:lineTo(10, 0)
		f:moveTo(20, 20)
		f:lineTo(30, 20)
	end)
	equal("contours: each moveTo starts a new contour", #contours, 2)
	check("contours: unclosed stay open", not contours[1].closed and not contours[2].closed, "closed flag set")
end

do
	local contours = extractContours(IDENTITY, function(f)
		f:moveTo(0, 0)
		f:lineTo(10, 0)
		f:lineTo(10, 10)
		f:lineTo(0, 0)
		f:close()
	end)
	equal("contours: close produces one contour", #contours, 1)
	check("contours: close marks the contour", contours[1].closed, "expected closed")
	-- The explicit return to the start is dropped: wrapping around supplies
	-- that edge.
	equal("contours: the repeated closing point is dropped", #contours[1].points, 3)
end

do
	local contours = extractContours(IDENTITY, function(f)
		f:moveTo(0, 0)
		f:lineTo(10, 0)
		f:close()
		f:lineTo(0, 20)
	end)
	equal("contours: drawing after close reopens", #contours, 2)
	check("contours: first is closed, second open", contours[1].closed and not contours[2].closed, "flags wrong")
	equal("contours: reopened subpath starts at the subpath start x", contours[2].points[1].x, 0)
	equal("contours: reopened subpath continues y", contours[2].points[2].y, 20)
end

do
	local contours = extractContours(IDENTITY, function(f)
		f:moveTo(5, 5)
		f:lineTo(5, 5)
		f:lineTo(5, 5)
	end)
	equal("contours: coincident points are collapsed", #contours, 1)
	equal("contours: a run of coincident points is degenerate", #contours[1].points, 1)
end

do
	local contours = extractContours({ sx = 10, ky = 0, kx = 0, sy = 10, tx = 5, ty = 0 }, function(f)
		f:moveTo(0, 0)
		f:lineTo(1, 2)
	end)
	equal("contours: the transform is applied before flattening x", contours[1].points[1].x, 5)
	equal("contours: transformed line lands at x", contours[1].points[2].x, 15)
	equal("contours: transformed line lands at y", contours[1].points[2].y, 20)
end

do
	local _, ok = extractContours({ sx = 1e300, ky = 0, kx = 0, sy = 1e300, tx = 0, ty = 0 }, function(f)
		f:moveTo(0, 0)
		f:lineTo(1e300, 1e300)
	end)
	check("contours: overflow is reported rather than emitted", not ok, "expected finish() to report failure")
end

-- ---------------------------------------------------------------------------
-- stroke: caps
-- ---------------------------------------------------------------------------

do
	-- A butt cap stops exactly at the endpoint: 10 x 4 precisely.
	local polygons = stroke({ contour({ { 0, 0 }, { 10, 0 } }, false) }, style(4, CAP_BUTT, JOIN_MITER, 4))
	local minX, minY, maxX, maxY = bounds(polygons)
	near("stroke: butt cap min x", minX, 0)
	near("stroke: butt cap max x", maxX, 10)
	near("stroke: butt cap min y", minY, -2)
	near("stroke: butt cap max y", maxY, 2)
end

do
	local polygons = stroke({ contour({ { 0, 0 }, { 10, 0 } }, false) }, style(4, CAP_SQUARE, JOIN_MITER, 4))
	local minX, _, maxX = bounds(polygons)
	near("stroke: square caps extend by half the width (min)", minX, -2)
	near("stroke: square caps extend by half the width (max)", maxX, 12)
end

do
	-- A round cap is a half disc; arcs are inscribed, so it falls at most
	-- ARC_TOLERANCE short of the true circle and never beyond it.
	local polygons = stroke({ contour({ { 0, 0 }, { 10, 0 } }, false) }, style(4, CAP_ROUND, JOIN_MITER, 4))
	local minX, minY, maxX, maxY = bounds(polygons)
	check("stroke: round cap left bound", minX >= -2 - 1e-4 and minX <= -2 + ARC_TOLERANCE, `{minX}`)
	check("stroke: round cap right bound", maxX >= 12 - ARC_TOLERANCE and maxX <= 12 + 1e-4, `{maxX}`)
	near("stroke: round cap min y", minY, -2)
	near("stroke: round cap max y", maxY, 2)

	-- Every cap point sits on its endpoint's circle, within chord tolerance.
	local onCircle = true
	for _, polygon in polygons do
		for _, point in polygon do
			if point.x < 0 or point.x > 10 then
				local cx = if point.x < 0 then 0 else 10
				local dx, dy = point.x - cx, point.y
				local distance = math.sqrt(dx * dx + dy * dy)
				if math.abs(distance - 2) >= 1e-3 then
					onCircle = false
				end
			end
		end
	end
	check("stroke: round cap points sit on the endpoint circle", onCircle, "a point strayed")
end

do
	local point = contour({ { 5, 5 } }, false)
	equal(
		"stroke: a zero-length subpath paints nothing under butt",
		#stroke({ point }, style(4, CAP_BUTT, JOIN_MITER, 4)),
		0
	)

	local square = stroke({ point }, style(4, CAP_SQUARE, JOIN_MITER, 4))
	local minX, minY, maxX, maxY = bounds(square)
	check(
		"stroke: a zero-length square cap paints a square",
		minX == 3 and minY == 3 and maxX == 7 and maxY == 7,
		`{minX},{minY},{maxX},{maxY}`
	)

	local round = stroke({ point }, style(4, CAP_ROUND, JOIN_MITER, 4))
	minX, minY, maxX, maxY = bounds(round)
	for _, pair in { { minX, 3 }, { minY, 3 }, { maxX, 7 }, { maxY, 7 } } do
		check("stroke: a zero-length round cap paints a disc", math.abs(pair[1] - pair[2]) <= ARC_TOLERANCE,
			`{pair[1]} vs {pair[2]}`)
	end
end

do
	-- A dot must actually cover pixels, not merely produce points.
	local polygons = stroke({ contour({ { 8, 8 } }, false) }, style(8, CAP_ROUND, JOIN_MITER, 4))
	local out = rasterizeCoverage(polygons, false, 16, 16)
	near("stroke: a round dot is solid at the centre", coverageAt(out, 16, 8, 8), 1, 1e-3)
	near("stroke: a round dot leaves the corner empty", coverageAt(out, 16, 0, 0), 0, 1e-3)
	local total = 0
	for _, value in out do
		total += value
	end
	check("stroke: a round dot fills a disc", math.abs(total - 50.3) < 1.5, `total was {total}`)
end

-- ---------------------------------------------------------------------------
-- stroke: joins
-- ---------------------------------------------------------------------------

local function hasPointNear(polygons, x, y, tolerance)
	for _, polygon in polygons do
		for _, point in polygon do
			local dx, dy = point.x - x, point.y - y
			if math.sqrt(dx * dx + dy * dy) < (tolerance or 1e-4) then
				return true
			end
		end
	end
	return false
end

do
	-- A right-angle miter reaches radius * sqrt(2) past the corner.
	local polygons = stroke(
		{ contour({ { 0, 10 }, { 10, 10 }, { 10, 0 } }, false) },
		style(4, CAP_BUTT, JOIN_MITER, 4)
	)
	check("stroke: a right-angle miter reaches the expected apex", hasPointNear(polygons, 12, 12),
		"no apex at (12, 12)")
end

do
	-- Past the limit, SVG requires the miter to become a bevel. This corner is
	-- a narrow spike whose ratio is about 13.5.
	local sharp = contour({ { 0, 0 }, { 20, 0 }, { 0, 3 } }, false)
	local generous = stroke({ sharp }, style(2, CAP_BUTT, JOIN_MITER, 20))
	local strict = stroke({ sharp }, style(2, CAP_BUTT, JOIN_MITER, 2))
	local bevelled = stroke({ sharp }, style(2, CAP_BUTT, JOIN_BEVEL, 2))

	local _, _, generousMaxX = bounds(generous)
	local _, _, strictMaxX = bounds(strict)
	local _, _, bevelMaxX = bounds(bevelled)

	check("stroke: a long miter overhangs", generousMaxX > 30, `{generousMaxX}`)
	check(
		"stroke: a miter beyond the limit falls back to bevel",
		math.abs(strictMaxX - bevelMaxX) < 1e-4,
		`{strictMaxX} vs {bevelMaxX}`
	)
end

do
	-- The limit is a ratio; a right angle's is sqrt(2) ~ 1.4142.
	local corner = contour({ { 0, 10 }, { 10, 10 }, { 10, 0 } }, false)
	local above = stroke({ corner }, style(4, CAP_BUTT, JOIN_MITER, 1.5))
	local below = stroke({ corner }, style(4, CAP_BUTT, JOIN_MITER, 1.3))
	check("stroke: miter limit 1.5 > sqrt(2) keeps the apex", hasPointNear(above, 12, 12), "apex missing")
	check("stroke: miter limit 1.3 < sqrt(2) bevels", not hasPointNear(below, 12, 12), "apex present")
end

do
	local polygons = stroke(
		{ contour({ { 0, 10 }, { 10, 10 }, { 10, 0 } }, false) },
		style(4, CAP_BUTT, JOIN_ROUND, 4)
	)
	-- The join wedge is the only polygon containing the vertex itself.
	local wedge
	for _, polygon in polygons do
		for _, point in polygon do
			local dx, dy = point.x - 10, point.y - 10
			if math.sqrt(dx * dx + dy * dy) < 1e-6 then
				wedge = polygon
			end
		end
	end
	check("stroke: round join has a wedge at the vertex", wedge ~= nil, "no wedge found")
	if wedge then
		local onCircle = true
		for _, point in wedge do
			local dx, dy = point.x - 10, point.y - 10
			local distance = math.sqrt(dx * dx + dy * dy)
			if not (distance < 1e-6 or math.abs(distance - 2) < 1e-3) then
				onCircle = false
			end
		end
		check("stroke: a round join stays on the circle about the vertex", onCircle, "a point strayed")
		check("stroke: a round join is many chords", #wedge > 4, `{#wedge} points`)
	end
end

do
	local polygons = stroke(
		{ contour({ { 0, 10 }, { 10, 10 }, { 10, 0 } }, false) },
		style(4, CAP_BUTT, JOIN_BEVEL, 4)
	)
	local wedge
	for _, polygon in polygons do
		for _, point in polygon do
			local dx, dy = point.x - 10, point.y - 10
			if math.sqrt(dx * dx + dy * dy) < 1e-6 then
				wedge = polygon
			end
		end
	end
	check("stroke: a bevel join is a bare triangle", wedge ~= nil and #wedge == 3,
		`{if wedge then #wedge else "none"} points`)
	local _, _, maxX, maxY = bounds(polygons)
	near("stroke: a bevel does not overhang x", maxX, 12)
	near("stroke: a bevel does not overhang y", maxY, 12)
end

do
	-- The three join styles must differ right at the corner and agree
	-- everywhere else.
	local corner = contour({ { 4, 20 }, { 20, 20 }, { 20, 4 } }, false)
	local function render(join)
		return rasterizeCoverage(stroke({ corner }, style(8, CAP_BUTT, join, 8)), false, 32, 32)
	end
	local miter = render(JOIN_MITER)
	local round = render(JOIN_ROUND)
	local bevel = render(JOIN_BEVEL)

	check("stroke: miter fills the corner", coverageAt(miter, 32, 23, 23) > 0.9, `{coverageAt(miter, 32, 23, 23)}`)
	check("stroke: round is cut back at the corner", coverageAt(round, 32, 23, 23) < 0.1,
		`{coverageAt(round, 32, 23, 23)}`)
	check("stroke: bevel is cut back at the corner", coverageAt(bevel, 32, 23, 23) < 0.1,
		`{coverageAt(bevel, 32, 23, 23)}`)
	check(
		"stroke: round covers more than bevel inside the arc",
		coverageAt(round, 32, 22, 21) > coverageAt(bevel, 32, 22, 21),
		"round <= bevel"
	)
	check(
		"stroke: all joins agree well inside the stroke",
		math.abs(coverageAt(miter, 32, 10, 20) - coverageAt(round, 32, 10, 20)) < 1e-3,
		"interior differs"
	)
end

do
	-- Nearly-collinear segments must degrade gracefully, not divide by zero.
	for _, epsilon in { 1e-7, 1e-5, 1e-3 } do
		for _, join in { JOIN_MITER, JOIN_ROUND, JOIN_BEVEL } do
			local polygons = stroke(
				{ contour({ { 0, 0 }, { 10, 0 }, { 20, epsilon } }, false) },
				style(2, CAP_BUTT, join, 4)
			)
			check(`stroke: nearly collinear stays finite (join {join}, eps {epsilon})`, allFinite(polygons), "NaN")
			check(`stroke: nearly collinear windings agree (join {join}, eps {epsilon})`, windingsAgree(polygons),
				"winding flipped")
			local _, _, maxX = bounds(polygons)
			check(`stroke: nearly collinear does not overshoot (join {join}, eps {epsilon})`, maxX < 21, `{maxX}`)
		end
	end
end

do
	-- A path doubling back on itself is the infinite-miter case.
	for _, join in { JOIN_MITER, JOIN_ROUND, JOIN_BEVEL } do
		local polygons = stroke(
			{ contour({ { 0, 0 }, { 10, 0 }, { 0, 0.0001 } }, false) },
			style(2, CAP_BUTT, join, 100)
		)
		check(`stroke: a cusp stays finite (join {join})`, allFinite(polygons), "NaN")
		local _, _, maxX = bounds(polygons)
		check(`stroke: a cusp does not produce an infinite miter (join {join})`, maxX < 100, `{maxX}`)
	end
end

do
	-- At a cusp a round join must still pivot the pen through a full turn.
	local polygons = stroke(
		{ contour({ { 0, 8 }, { 12, 8 }, { 0, 8.0000001 } }, false) },
		style(8, CAP_BUTT, JOIN_ROUND, 4)
	)
	local _, _, maxX = bounds(polygons)
	check("stroke: a round join at a cusp sweeps a full disc", math.abs(maxX - 16) < ARC_TOLERANCE + 1e-3, `{maxX}`)
end

-- ---------------------------------------------------------------------------
-- stroke: closed contours, overlap, degenerate input
-- ---------------------------------------------------------------------------

do
	-- A closed stroke is a band: solid where the pen went, empty inside.
	local polygons = stroke(
		{ contour({ { 4, 4 }, { 28, 4 }, { 28, 28 }, { 4, 28 } }, true) },
		style(4, CAP_BUTT, JOIN_MITER, 4)
	)
	check("stroke: closed band pieces wind together", windingsAgree(polygons), "winding flipped")

	local out = rasterizeCoverage(polygons, false, 32, 32)
	near("stroke: band top edge is solid", coverageAt(out, 32, 16, 4), 1, 1e-3)
	near("stroke: band left edge is solid", coverageAt(out, 32, 4, 16), 1, 1e-3)
	near("stroke: band middle stays hollow", coverageAt(out, 32, 16, 16), 0, 1e-3)
	near("stroke: outside the band is empty", coverageAt(out, 32, 16, 0), 0, 1e-3)
	-- The corners are mitred, so the outer boundary reaches (2, 2).
	near("stroke: mitred outer corner is solid", coverageAt(out, 32, 2, 2), 1, 1e-3)
end

do
	local square = contour({ { 0, 0 }, { 10, 0 }, { 10, 10 }, { 0, 10 } }, true)
	local butt = stroke({ square }, style(2, CAP_BUTT, JOIN_MITER, 4))
	local round = stroke({ square }, style(2, CAP_ROUND, JOIN_MITER, 4))
	local b1 = { bounds(butt) }
	local b2 = { bounds(round) }
	check(
		"stroke: a closed contour has no caps",
		b1[1] == b2[1] and b1[2] == b2[2] and b1[3] == b2[3] and b1[4] == b2[4],
		"bounds differ"
	)
end

do
	-- Every corner is joined, including the one at the contour's start point,
	-- which is the one an off-by-one loses.
	local polygons = stroke(
		{ contour({ { 0, 0 }, { 10, 0 }, { 10, 10 }, { 0, 10 } }, true) },
		style(2, CAP_BUTT, JOIN_MITER, 4)
	)
	for _, apex in { { -1, -1 }, { 11, -1 }, { 11, 11 }, { -1, 11 } } do
		check(`stroke: closed corner at ({apex[1]}, {apex[2]}) is mitred`, hasPointNear(polygons, apex[1], apex[2]),
			"no apex")
	end
end

do
	-- The direction a contour was authored in must not change its stroke.
	local forward = contour({ { 4, 4 }, { 28, 4 }, { 28, 28 }, { 4, 28 } }, true)
	local backward = contour({ { 4, 28 }, { 28, 28 }, { 28, 4 }, { 4, 4 } }, true)
	local a = rasterizeCoverage(stroke({ forward }, style(4, CAP_BUTT, JOIN_MITER, 4)), false, 32, 32)
	local b = rasterizeCoverage(stroke({ backward }, style(4, CAP_BUTT, JOIN_MITER, 4)), false, 32, 32)
	local same = true
	for index = 1, 32 * 32 do
		if math.abs(a[index] - b[index]) >= 1e-3 then
			same = false
		end
	end
	check("stroke: contour orientation does not change the stroke", same, "pixels differ")
end

do
	-- A stroke crossing itself must be solid: not doubly blended, not holed.
	local polygons = stroke(
		{ contour({ { 2, 2 }, { 30, 30 }, { 2, 30 }, { 30, 2 } }, false) },
		style(6, CAP_BUTT, JOIN_MITER, 4)
	)
	local out = rasterizeCoverage(polygons, false, 32, 32)
	local within = true
	for _, value in out do
		if value > 1 + 1e-4 then
			within = false
		end
	end
	check("stroke: a self-crossing stroke never exceeds full coverage", within, "coverage > 1")
	near("stroke: the crossing point is covered once, fully", coverageAt(out, 32, 16, 16), 1, 1e-3)
end

do
	local shapes = {
		contour({ { 0, 0 }, { 10, 0 }, { 10, 10 } }, false),
		contour({ { 10, 10 }, { 10, 0 }, { 0, 0 } }, false),
		contour({ { 0, 0 }, { 10, 0 }, { 10, 10 }, { 0, 10 } }, true),
		contour({ { 0, 10 }, { 10, 10 }, { 10, 0 }, { 0, 0 } }, true),
	}
	local agree = true
	for _, cap in { CAP_BUTT, CAP_ROUND, CAP_SQUARE } do
		for _, join in { JOIN_MITER, JOIN_ROUND, JOIN_BEVEL } do
			for _, shape in shapes do
				if not windingsAgree(stroke({ shape }, style(3, cap, join, 4))) then
					agree = false
				end
			end
		end
	end
	check("stroke: all pieces wind the same way across caps, joins and shapes", agree, "a piece flipped")
end

do
	local line = contour({ { 0, 0 }, { 10, 0 } }, false)
	for _, width in { 0, -1, 0 / 0 } do
		equal(`stroke: width {width} paints nothing`, #stroke({ line }, style(width, CAP_ROUND, JOIN_ROUND, 4)), 0)
	end
	equal("stroke: an empty contour list produces nothing", #stroke({}, style(2, CAP_ROUND, JOIN_ROUND, 4)), 0)

	local thin = stroke({ line }, style(0.01, CAP_ROUND, JOIN_ROUND, 4))
	check("stroke: a very thin stroke still produces geometry", #thin > 0 and allFinite(thin), "missing or NaN")

	local wide = stroke(
		{ contour({ { 0, 0 }, { 1, 0 }, { 1, 1 } }, false) },
		style(10000, CAP_ROUND, JOIN_ROUND, 100)
	)
	check("stroke: a very wide stroke stays finite", allFinite(wide) and windingsAgree(wide), "NaN or flipped")

	local coincident = stroke(
		{ contour({ { 5, 5 }, { 5, 5 }, { 9, 5 }, { 9, 5 } }, false) },
		style(2, CAP_ROUND, JOIN_ROUND, 4)
	)
	check("stroke: coincident points stay finite", allFinite(coincident), "NaN")
	local minX, _, maxX = bounds(coincident)
	check("stroke: coincident points stay bounded", minX >= 4 - 1e-3 and maxX <= 10 + 1e-3, `{minX}..{maxX}`)
end

-- ---------------------------------------------------------------------------
-- edges + coverage: fills and fill rules
-- ---------------------------------------------------------------------------

local function rect(x0, y0, x1, y1)
	return { v(x0, y0), v(x1, y0), v(x1, y1), v(x0, y1) }
end

local function reversed(points)
	local out = {}
	for index = #points, 1, -1 do
		table.insert(out, points[index])
	end
	return out
end

do
	local out = rasterizeCoverage({ rect(2, 2, 6, 6) }, false, 8, 8)
	local exact = true
	for y = 0, 7 do
		for x = 0, 7 do
			local expected = if x >= 2 and x < 6 and y >= 2 and y < 6 then 1 else 0
			if math.abs(coverageAt(out, 8, x, y) - expected) >= 1e-4 then
				exact = false
			end
		end
	end
	check("fill: a pixel-aligned rectangle is fully covered with no fringe", exact, "a pixel was wrong")
end

do
	local out = rasterizeCoverage({ rect(0.5, 0, 3, 1) }, false, 4, 1)
	near("fill: a half-pixel offset edge is half covered", out[1], 0.5)
	near("fill: interior pixel is full", out[2], 1)
	near("fill: last pixel is empty", out[4], 0)
end

do
	local square = rect(1, 1, 5, 5)
	local forward = rasterizeCoverage({ square }, false, 6, 6)
	local backward = rasterizeCoverage({ reversed(square) }, false, 6, 6)
	local same = true
	for index = 1, 36 do
		if forward[index] ~= backward[index] then
			same = false
		end
	end
	check("fill: orientation does not change a simple fill", same, "pixels differ")
end

do
	local outer = rect(0, 0, 8, 8)
	local inner = rect(2, 2, 6, 6)
	local out = rasterizeCoverage({ outer, inner }, false, 8, 8)
	near("fill: nonzero keeps a same-direction inner contour solid", coverageAt(out, 8, 4, 4), 1)

	out = rasterizeCoverage({ outer, reversed(inner) }, false, 8, 8)
	near("fill: nonzero cuts a hole for an opposite-direction inner contour", coverageAt(out, 8, 4, 4), 0)
	near("fill: the ring stays solid", coverageAt(out, 8, 1, 1), 1)
end

do
	-- The case that separates a real even-odd implementation from one faked
	-- with orientation: nested contours wound the *same* way.
	local outer = rect(0, 0, 8, 8)
	for _, inner in { rect(2, 2, 6, 6), reversed(rect(2, 2, 6, 6)) } do
		local out = rasterizeCoverage({ outer, inner }, true, 8, 8)
		near("fill: evenodd cuts a hole regardless of direction", coverageAt(out, 8, 4, 4), 0)
		near("fill: evenodd ring stays solid", coverageAt(out, 8, 1, 1), 1)
	end
end

do
	local a = rect(0, 0, 6, 6)
	local b = rect(3, 0, 9, 6)
	local nonzero = rasterizeCoverage({ a, b }, false, 10, 6)
	local evenodd = rasterizeCoverage({ a, b }, true, 10, 6)
	near("fill: nonzero keeps the overlap solid", coverageAt(nonzero, 10, 4, 2), 1)
	near("fill: evenodd empties the overlap", coverageAt(evenodd, 10, 4, 2), 0)
	near("fill: outside the overlap nonzero agrees", coverageAt(nonzero, 10, 1, 2), 1)
	near("fill: outside the overlap evenodd agrees", coverageAt(evenodd, 10, 1, 2), 1)
end

do
	local out = rasterizeCoverage({ rect(0, 0, 12, 12), rect(2, 2, 10, 10), rect(4, 4, 8, 8) }, true, 12, 12)
	near("fill: three nested contours - outermost ring", coverageAt(out, 12, 1, 6), 1)
	near("fill: three nested contours - second ring is a hole", coverageAt(out, 12, 3, 6), 0)
	near("fill: three nested contours - core is filled again", coverageAt(out, 12, 6, 6), 1)
end

do
	local out = rasterizeCoverage({ rect(0, 0, 3, 3), rect(5, 0, 8, 3) }, false, 8, 3)
	near("fill: disjoint subpaths both fill (left)", coverageAt(out, 8, 1, 0), 1)
	near("fill: disjoint subpaths gap is empty", coverageAt(out, 8, 4, 0), 0)
	near("fill: disjoint subpaths both fill (right)", coverageAt(out, 8, 6, 0), 1)
end

do
	local out = rasterizeCoverage({ rect(-50, -50, 2, 2) }, false, 4, 4)
	near("fill: geometry outside the raster is clipped, top-left stays", coverageAt(out, 4, 0, 0), 1)
	near("fill: clipping does not wrap", coverageAt(out, 4, 3, 0), 0)

	out = rasterizeCoverage({ rect(100, 100, 200, 200) }, false, 4, 4)
	local empty = true
	for _, value in out do
		if value ~= 0 then
			empty = false
		end
	end
	check("fill: geometry entirely outside the raster draws nothing", empty, "stray coverage")
end

do
	local edges = edgesModule.EdgeSet.new()
	edges:addPolygon({ v(0, 1), v(5, 1) })
	edges:addPolygon({ v(2, 2) })
	check("edges: horizontal and degenerate segments contribute nothing", edges:isEmpty(), "edges present")
end

do
	local edges = edgesModule.EdgeSet.new()
	edges:addPolygon({ v(0, 0), v(0 / 0, 4), v(4, 4) })
	edges:finish()
	local sampler = coverageModule.ScanlineSupersampler.new()
	local sane = true
	sampler:rasterize(edges, false, 4, 4, function(_, row, startX, endX)
		for x = startX, endX - 1 do
			if not isFiniteNumber(row[x + 1]) then
				sane = false
			end
		end
	end)
	check("edges: non-finite points are dropped rather than poisoning the scan", sane, "NaN coverage")
end

do
	local out = rasterizeCoverage({ rect(0, 0, 4, 4) }, false, 0, 4)
	equal("coverage: a zero-width raster is a no-op", #out, 0)
end

do
	-- Ten stacked copies of the same square: winding 10, coverage still 1.
	local square = rect(1, 1, 5, 5)
	local polygons = {}
	for _ = 1, 10 do
		table.insert(polygons, square)
	end
	local out = rasterizeCoverage(polygons, false, 6, 6)
	local within = true
	for _, value in out do
		if value > 1 + 1e-4 then
			within = false
		end
	end
	check("coverage: coverage never exceeds one", within, "coverage > 1")
end

do
	local triangle = { v(0, 0), v(32, 0), v(0, 32) }
	local out = rasterizeCoverage({ triangle }, false, 32, 32)
	local total = 0
	for _, value in out do
		total += value
	end
	check("coverage: a triangle covers about half its bounding box", math.abs(total - 512) < 512 * 0.01,
		`total was {total}`)
end

-- ---------------------------------------------------------------------------
-- image: premultiplied compositing and RGBA8 output
-- ---------------------------------------------------------------------------

local Canvas = imageModule.Canvas

local function pixelAt(buf, width, x, y)
	local at = (y * width + x) * 4
	return buffer.readu8(buf, at), buffer.readu8(buf, at + 1), buffer.readu8(buf, at + 2), buffer.readu8(buf, at + 3)
end

do
	local image = Canvas.new(2, 2):finish(false)
	equal("image: buffer is exactly width*height*4", buffer.len(image), 16)
	local blank = true
	for index = 0, 15 do
		if buffer.readu8(image, index) ~= 0 then
			blank = false
		end
	end
	check("image: an untouched canvas is fully transparent", blank, "non-zero byte")
end

do
	local canvas = Canvas.new(2, 1)
	canvas:blendRow(0, { 1, 1 }, 0, 2, 255, 128, 64, 1)
	local r, g, b, a = pixelAt(canvas:finish(false), 2, 0, 0)
	check(
		"image: an opaque fill survives the premultiplication round trip",
		r == 255 and g == 128 and b == 64 and a == 255,
		`{r},{g},{b},{a}`
	)
end

do
	local canvas = Canvas.new(1, 1)
	canvas:blendRow(0, { 0.5 }, 0, 1, 200, 100, 50, 1)
	local r, g, b, a = pixelAt(canvas:finish(false), 1, 0, 0)
	equal("image: half coverage halves the alpha", a, 128)
	-- Colour comes back essentially unchanged: the alpha carried the coverage.
	check("image: half coverage keeps the colour", math.abs(r - 200) <= 1 and math.abs(g - 100) <= 1
		and math.abs(b - 50) <= 1, `{r},{g},{b}`)
end

do
	local canvas = Canvas.new(1, 1)
	canvas:blendRow(0, { 1 }, 0, 1, 255, 0, 0, 1)
	canvas:blendRow(0, { 1 }, 0, 1, 0, 0, 255, 1)
	local r, _, b, a = pixelAt(canvas:finish(false), 1, 0, 0)
	check("image: source-over puts the later shape on top", r == 0 and b == 255 and a == 255, `{r},{b},{a}`)
end

do
	-- Half-covering blue over opaque red gives the even mix, and stays opaque.
	local canvas = Canvas.new(1, 1)
	canvas:blendRow(0, { 1 }, 0, 1, 255, 0, 0, 1)
	canvas:blendRow(0, { 1 }, 0, 1, 0, 0, 255, 0.5)
	local r, g, b, a = pixelAt(canvas:finish(false), 1, 0, 0)
	equal("image: translucent blend stays opaque", a, 255)
	check("image: translucent blend mixes evenly", math.abs(r - 128) <= 1 and g == 0 and math.abs(b - 128) <= 1,
		`{r},{g},{b}`)
end

do
	-- Two half-transparent layers compose to 1 - 0.5^2 = 0.75.
	local canvas = Canvas.new(1, 1)
	canvas:blendRow(0, { 1 }, 0, 1, 255, 255, 255, 0.5)
	canvas:blendRow(0, { 1 }, 0, 1, 255, 255, 255, 0.5)
	local _, _, _, a = pixelAt(canvas:finish(false), 1, 0, 0)
	equal("image: stacked transparency accumulates correctly", a, 191)
end

do
	-- Mask mode records coverage and discards colour entirely.
	local a = Canvas.new(1, 1)
	a:blendRowAlpha(0, { 0.5 }, 0, 1, 1)
	local b = Canvas.new(1, 1)
	b:blendRowAlpha(0, { 0.5 }, 0, 1, 1)
	local bufA, bufB = a:finish(true), b:finish(true)
	local same = true
	for index = 0, 3 do
		if buffer.readu8(bufA, index) ~= buffer.readu8(bufB, index) then
			same = false
		end
	end
	check("image: mask mode is colour-independent", same, "buffers differ")
	local r, g, bl = pixelAt(Canvas.new(1, 1):finish(true), 1, 0, 0)
	check("image: mask RGB is white so ImageColor3 multiplies exactly", r == 255 and g == 255 and bl == 255,
		`{r},{g},{bl}`)
	local _, _, _, alpha = pixelAt(bufA, 1, 0, 0)
	equal("image: mask alpha carries the coverage", alpha, 128)
end

do
	local canvas = Canvas.new(2, 2)
	canvas:blendRow(9, { 1, 1 }, 0, 2, 255, 255, 255, 1)
	canvas:blendRow(0, { 1, 1 }, 0, 99, 255, 255, 255, 1)
	local image = canvas:finish(false)
	local _, _, _, a00 = pixelAt(image, 2, 0, 0)
	local _, _, _, a01 = pixelAt(image, 2, 0, 1)
	check("image: blending outside the canvas is ignored", a00 == 255 and a01 == 0, `{a00},{a01}`)
end

do
	local canvas = Canvas.new(1, 1)
	canvas:blendRow(0, { 1 }, 0, 1, 255, 255, 255, 0)
	local _, _, _, a = pixelAt(canvas:finish(false), 1, 0, 0)
	equal("image: zero alpha paints nothing", a, 0)
end

-- ---------------------------------------------------------------------------
-- Full pipeline: golden comparison against the Rust reference renderer
-- ---------------------------------------------------------------------------

--[[
	Tolerances for cross-language comparison: **zero**.

	The Rust reference computes in f32 and Luau in f64, so a priori some
	last-bit divergence on anti-aliased boundary pixels was expected. Measured
	across the entire golden set (masks and colour, five assets sizes 16-100,
	stroke overrides, all three aspect policies, mixed currentColor), the two
	implementations are byte-identical: max channel diff 0 on every render.
	Exact equality is therefore required — any future non-zero diff is a real
	algorithmic divergence and should fail loudly, with the metrics below
	saying how big it is. Do not relax these without measuring first.
]]
local MAX_CHANNEL_DIFF = 0
local MAX_MEAN_ALPHA_DIFF = 0
local MAX_BAD_PIXEL_RATIO = 0
local MAX_ALPHA_MASS_RATIO = 0

local assetCache = {}
local function assetFor(name)
	if assetCache[name] == nil then
		local fixture = fixtures.assets[name]
		assetCache[name] = internal.createAssetFromBase64(fixture.base64, fixture.hash)
	end
	return assetCache[name]
end

for _, golden in goldens do
	local name = `golden {golden.fixture} {golden.width}x{golden.height}` ..
		(if golden.alphaMask then " mask" else " colour") ..
		(if golden.strokeWidth then ` sw={golden.strokeWidth}{if golden.absoluteStrokeWidth then "abs" else ""}` else "") ..
		(if golden.currentColor then ` cc={golden.currentColor.r},{golden.currentColor.g},{golden.currentColor.b}` else "")

	local asset = assetFor(golden.fixture)
	local expected = base64.decodeBase64(golden.pixels)

	-- Resolve the stroke override exactly the way the runtime does, through
	-- resolveRenderOptions — which is the unit-conversion boundary under test.
	local strokeWidth = nil
	if golden.strokeWidth ~= nil then
		local resolved = svg.resolveRenderOptions(asset, {
			size = Vector2.new(golden.width, golden.height),
			strokeWidth = golden.strokeWidth,
			absoluteStrokeWidth = golden.absoluteStrokeWidth,
		})
		strokeWidth = resolved.strokeWidth
	end

	local cc = golden.currentColor or { r = 0, g = 0, b = 0 }
	local actual = renderModule.rasterize(internal.inspect(asset), {
		pixelWidth = golden.width,
		pixelHeight = golden.height,
		alphaMask = golden.alphaMask,
		currentColorR = cc.r,
		currentColorG = cc.g,
		currentColorB = cc.b,
		strokeWidth = strokeWidth,
	})

	equal(`{name}: buffer length`, buffer.len(actual), buffer.len(expected))
	if buffer.len(actual) ~= buffer.len(expected) then
		continue
	end

	local pixelCount = golden.width * golden.height
	local maxDiff, alphaDiffSum, badPixels = 0, 0, 0
	local expectedMass, actualMass = 0, 0
	local interiorExact = true
	for index = 0, pixelCount - 1 do
		local at = index * 4
		local worst = 0
		for channel = 0, 3 do
			local diff = math.abs(buffer.readu8(actual, at + channel) - buffer.readu8(expected, at + channel))
			if diff > worst then
				worst = diff
			end
		end
		local expectedAlpha = buffer.readu8(expected, at + 3)
		local actualAlpha = buffer.readu8(actual, at + 3)
		expectedMass += expectedAlpha
		actualMass += actualAlpha
		alphaDiffSum += math.abs(expectedAlpha - actualAlpha)
		if worst > maxDiff then
			maxDiff = worst
		end
		if worst > 16 then
			badPixels += 1
		end
		-- Fully covered interior pixels must be exact: only anti-aliased
		-- boundary pixels are allowed rounding slack.
		if expectedAlpha == 255 and worst ~= 0 then
			interiorExact = false
		end
	end

	check(`{name}: max channel diff`, maxDiff <= MAX_CHANNEL_DIFF, `max diff {maxDiff}`)
	check(`{name}: mean alpha diff`, alphaDiffSum / pixelCount <= MAX_MEAN_ALPHA_DIFF,
		`mean {alphaDiffSum / pixelCount}`)
	check(`{name}: bad pixel ratio`, badPixels / pixelCount <= MAX_BAD_PIXEL_RATIO,
		`{badPixels}/{pixelCount}`)
	check(`{name}: fully covered interior pixels are exact`, interiorExact, "an opaque pixel differed")
	if expectedMass > 0 then
		check(`{name}: total alpha mass`, math.abs(actualMass - expectedMass) / expectedMass <= MAX_ALPHA_MASS_RATIO,
			`{actualMass} vs {expectedMass}`)
	else
		equal(`{name}: total alpha mass is zero`, actualMass, 0)
	end
end

-- ---------------------------------------------------------------------------
-- Pipeline error handling
-- ---------------------------------------------------------------------------

do
	local asset = internal.inspect(assetFor("lucideSearch"))
	expectError("rasterize: zero width is rejected", "cannot rasterize", function()
		renderModule.rasterize(asset, {
			pixelWidth = 0,
			pixelHeight = 24,
			alphaMask = true,
			currentColorR = 0,
			currentColorG = 0,
			currentColorB = 0,
		})
	end)
	expectError("rasterize: fractional size is rejected", "cannot rasterize", function()
		renderModule.rasterize(asset, {
			pixelWidth = 24.5,
			pixelHeight = 24,
			alphaMask = true,
			currentColorR = 0,
			currentColorG = 0,
			currentColorB = 0,
		})
	end)
end

-- ---------------------------------------------------------------------------
-- EditableImage adapter, against a fake factory
-- ---------------------------------------------------------------------------

local function fakeImage()
	local image = {
		writes = {},
		destroyed = 0,
	}
	function image:WritePixelsBuffer(origin, size, pixels)
		table.insert(self.writes, { origin = origin, size = size, pixels = pixels })
	end
	function image:Destroy()
		self.destroyed += 1
	end
	return image
end

do
	local created = {}
	local factory = {
		create = function(_, size)
			local image = fakeImage()
			table.insert(created, { size = size, image = image })
			return image
		end,
	}
	local renderer = backendModule.createEditableImageRenderer(factory)
	equal("backend: renderer version is exported", renderer.version, backendModule.EDITABLE_IMAGE_RENDERER_VERSION)

	local asset = assetFor("lucideSearch")
	local image = renderer:render(asset, {
		pixelWidth = 24,
		pixelHeight = 24,
		currentColor = Color3.new(1, 1, 1),
		strokeWidth = nil,
	})
	equal("backend: one image allocated", #created, 1)
	equal("backend: allocation size x", created[1].size.X, 24)
	equal("backend: allocation size y", created[1].size.Y, 24)
	equal("backend: one full-image write", #image.writes, 1)
	equal("backend: write starts at the origin", image.writes[1].origin.X, 0)
	equal("backend: write covers the full size", image.writes[1].size.Y, 24)
	equal("backend: write is width*height*4 bytes", buffer.len(image.writes[1].pixels), 24 * 24 * 4)

	-- Tintable asset: the written raster must be an alpha mask (white RGB).
	local pixels = image.writes[1].pixels
	local white = true
	for index = 0, 24 * 24 - 1 do
		local at = index * 4
		if buffer.readu8(pixels, at) ~= 255 or buffer.readu8(pixels, at + 1) ~= 255
			or buffer.readu8(pixels, at + 2) ~= 255 then
			white = false
		end
	end
	check("backend: a tintable asset rasterizes as a white alpha mask", white, "non-white RGB")

	renderer:destroy(image)
	equal("backend: destroy calls Destroy exactly once", image.destroyed, 1)
end

do
	-- A mixed-currentColor asset must NOT go through the mask path: its fixed
	-- red paint has to survive in the written pixels.
	local factory = {
		create = function()
			return fakeImage()
		end,
	}
	local renderer = backendModule.createEditableImageRenderer(factory)
	local image = renderer:render(assetFor("mixedCurrentColor"), {
		pixelWidth = 24,
		pixelHeight = 24,
		currentColor = Color3.fromRGB(0, 0, 255),
		strokeWidth = nil,
	})
	local pixels = image.writes[1].pixels
	-- (4, 4) sits in the fixed red backdrop; (12, 12) in the currentColor bar.
	local function rgba(x, y)
		local at = (y * 24 + x) * 4
		return buffer.readu8(pixels, at), buffer.readu8(pixels, at + 1), buffer.readu8(pixels, at + 2),
			buffer.readu8(pixels, at + 3)
	end
	local r, g, b = rgba(4, 4)
	check("backend: mixed asset keeps its fixed red paint", r == 255 and g == 0 and b == 0, `{r},{g},{b}`)
	r, g, b = rgba(12, 12)
	check("backend: mixed asset resolves currentColor to the requested blue", r == 0 and g == 0 and b == 255,
		`{r},{g},{b}`)
end

do
	local renderer = backendModule.createEditableImageRenderer({
		create = function()
			return fakeImage()
		end,
	})
	local asset = assetFor("lucideSearch")
	expectError("backend: an oversized raster is rejected, not clamped", "exceeds the EditableImage backend limit",
		function()
			renderer:render(asset, {
				pixelWidth = 2048,
				pixelHeight = 2048,
				currentColor = Color3.new(0, 0, 0),
			})
		end)
	expectError("backend: a zero-size raster is rejected", "cannot rasterize", function()
		renderer:render(asset, {
			pixelWidth = 0,
			pixelHeight = 24,
			currentColor = Color3.new(0, 0, 0),
		})
	end)
end

do
	-- Allocation failure: the factory returns nothing, the renderer must
	-- produce the actionable memory-budget error rather than a nil deref.
	local renderer = backendModule.createEditableImageRenderer({
		create = function()
			return nil
		end,
	})
	expectError("backend: allocation failure is actionable", "editable-image memory budget", function()
		renderer:render(assetFor("lucideSearch"), {
			pixelWidth = 256,
			pixelHeight = 256,
			currentColor = Color3.new(0, 0, 0),
		})
	end)
end

-- ---------------------------------------------------------------------------
-- Diagnostics counters
-- ---------------------------------------------------------------------------

do
	local statsBefore = renderModule.getSoftwareRasterStats().rasterCount
	renderModule.rasterize(internal.inspect(assetFor("lucideSearch")), {
		pixelWidth = 24,
		pixelHeight = 24,
		alphaMask = true,
		currentColorR = 0,
		currentColorG = 0,
		currentColorB = 0,
	})
	local stats = renderModule.getSoftwareRasterStats()
	equal("stats: raster count increments", stats.rasterCount, statsBefore + 1)
	check("stats: flattened point count is recorded", stats.lastPointCount > 0, `{stats.lastPointCount}`)
	check("stats: edge count is recorded", stats.lastEdgeCount > 0, `{stats.lastEdgeCount}`)
end

-- ---------------------------------------------------------------------------

print(`luau raster: {passed} passed, {failed} failed`)
for _, failure in failures do
	print("  FAIL " .. failure)
end
if failed > 0 then
	error(`{failed} Luau raster assertion(s) failed`, 0)
end
