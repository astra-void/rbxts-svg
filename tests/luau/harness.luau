--!nocheck
--[[
	A minimal loader for roblox-ts output, so the Luau half of `@rbxts/svg` can
	actually be executed outside Roblox.

	roblox-ts emits modules that expect two things from their environment: a
	`script` global identifying the module's own position in an Instance tree,
	and `_G[script]` holding a runtime table whose `import` resolves sibling
	modules. Neither exists under the standalone `luau` CLI, and its `_G` is
	frozen, so the modules are loaded with `loadstring` into a per-module
	environment instead.

	The code under test is exactly the Luau that `rbxtsc` produced — the sources
	are embedded verbatim by `bundle.mjs` because the CLI has no filesystem
	access. Only `TS.import` is shimmed, because it is the only runtime helper
	the package uses; anything else fails loudly rather than quietly resolving
	to something different.
]]

local harness = {}

--[[
	Creates a loader over a `path -> source` table, as produced by bundle.mjs.

	`extraGlobals` supplies Roblox globals the standalone CLI lacks (`Vector2`
	and friends). Only what the code under test actually reaches for should be
	provided: anything missing then surfaces as a clear error instead of as a
	silently different code path.

	`packages` maps a package name (`"@rbxts/vide"`) to an already-loaded value,
	which is what `TS.getModule` resolves against. Cross-package imports are
	otherwise unresolvable here: `TS.getModule(script, "@rbxts", "vide").src`
	walks a `node_modules` folder that only exists inside a synced Rojo tree.
]]
function harness.new(sources, extraGlobals, packages)
	local self = {}
	local loaded = {}

	-- Stands in for the frozen `_G`. Modules only ever use it to find their
	-- runtime table via `_G[script]`.
	local fakeGlobals = {}
	local baseGlobals = setmetatable(extraGlobals or {}, { __index = getfenv(1) })

	local TS = {}

	local function makeNode(path, name, parent)
		return { __path = path, __name = name, Parent = parent, __children = {} }
	end

	-- Directory and file nodes are indistinguishable, exactly as in Roblox
	-- where `script.Parent.foo` may be a ModuleScript or a Folder.
	local function child(node, name)
		if not node.__children[name] then
			node.__children[name] = makeNode(node.__path .. "/" .. name, name, node)
		end
		return node.__children[name]
	end

	local function requireNode(node)
		if loaded[node] ~= nil then
			return loaded[node]
		end

		-- A folder that is itself a module resolves to its `init` file, which is
		-- how roblox-ts packages are laid out: `out/init.luau` is the module for
		-- the `out` folder, and `out/asset.luau` is its child.
		local source = sources[node.__path] or sources[node.__path .. "/init"]
		if source == nil then
			error(`no such module: {node.__path}`, 0)
		end

		fakeGlobals[node] = TS

		local chunk, compileError = loadstring(source, "@" .. node.__path)
		if chunk == nil then
			error(`failed to compile {node.__path}: {compileError}`, 0)
		end

		setfenv(
			chunk,
			setmetatable({ script = node, _G = fakeGlobals }, { __index = baseGlobals })
		)

		local result = chunk()
		loaded[node] = result
		return result
	end

	function TS.import(_caller, base, ...)
		local node = base
		for _, name in { ... } do
			node = child(node, name)
		end
		return requireNode(node)
	end

	--[[
		Resolves a dependency to a preloaded value.

		roblox-ts emits `TS.import(script, TS.getModule(script, "@rbxts", "svg").out)`
		for a cross-package import; which folder inside the package is named
		(`out`, `src`) is a packaging detail, so the returned stand-in answers
		to any field with itself and requires to the preloaded value.
	]]
	function TS.getModule(_caller, scope, name)
		local key = `{scope}/{name}`
		local value = packages and packages[key]
		if value == nil then
			error(`no package provided to the harness: {key}`, 0)
		end

		local node = setmetatable({ __path = key }, { __index = function(node) return node end })
		loaded[node] = value
		return node
	end

	-- One shared node per top-level directory, so that a package loaded via
	-- `loadPackage` and a module loaded via `loadModule` resolve to the *same*
	-- node tree — and therefore share module instances, exactly as two
	-- `require`s of one ModuleScript do in Roblox.
	local roots = {}

	local function nodeFor(path)
		local node
		for segment in string.gmatch(path, "[^/]+") do
			if node == nil then
				if roots[segment] == nil then
					roots[segment] = makeNode(segment, segment, nil)
				end
				node = roots[segment]
			else
				node = child(node, segment)
			end
		end
		return node
	end

	--[[
		Loads a package by the path of its output directory.

		The returned node is the folder itself, so `script.Parent` inside a child
		module points at the package root exactly as it would in Roblox.
	]]
	function self.loadPackage(outDir)
		return requireNode(nodeFor(outDir))
	end

	--[[
		Loads a single module by path, without loading its package's `init`.

		The full parent chain is materialized, so a module that imports
		relatively (`script.Parent.Parent...`) resolves exactly as it would
		inside Roblox. Used for React-free `@rbxts/svg-react` modules — the
		standalone CLI has no React — and for the rasterizer's internals.
	]]
	function self.loadModule(path)
		return requireNode(nodeFor(path))
	end

	return self
end

return harness
