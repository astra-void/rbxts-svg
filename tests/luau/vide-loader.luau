--!nocheck
--[[
	Loads the real `@rbxts/vide` into a mocked Roblox environment.

	Vide is a pure Luau library — its only engine dependencies are instance
	creation, a handful of datatypes and `typeof` — which is what makes running
	the actual reactive library here possible, and worth doing. A hand-written
	stand-in for `source`/`effect`/`cleanup` would agree with whatever the
	binding happened to do; the real one has opinions about scope ownership,
	cleanup ordering and strict-mode double evaluation that the binding has to
	be correct against.

	Its modules require each other by relative path (`require "./graph"`), which
	the CLI resolves against a real file on disk. These are embedded strings, so
	they are given a `require` of their own that resolves the same names out of
	the bundle. `../test/mock` never resolves, and never needs to: it is Vide's
	fallback for running without a `game`, and here there is one.
]]

local videSources = require("./vide-sources")

--[[
	Returns a freshly loaded Vide.

	Fresh per call because Vide keeps process-wide state — the scope stack, the
	update queue, the strict-mode flag — and a suite that toggles strict mode
	should not leak that into the next one.

	@param globals the base environment, from `roblox.globals()`.
]]
return function(globals)
	local baseGlobals = setmetatable(globals, { __index = getfenv(1) })
	local loaded = {}

	local function videRequire(path)
		local name = string.match(path, "([^/]+)$")
		if loaded[name] ~= nil then
			return loaded[name]
		end

		local source = videSources[name]
		if source == nil then
			error(`@rbxts/vide has no module {path}`, 0)
		end

		local chunk, compileError = loadstring(source, `@vide/{name}`)
		if chunk == nil then
			error(`failed to compile vide/{name}: {compileError}`, 0)
		end

		setfenv(chunk, setmetatable({ require = videRequire }, { __index = baseGlobals }))

		local result = chunk()
		loaded[name] = result
		return result
	end

	-- `lib` rather than `init`: `init.luau` is the Roblox entry point and
	-- resolves its siblings through `script`, which does not exist here. Vide's
	-- own assertion in that file points at `lib` for exactly this case.
	return videRequire("./lib")
end
