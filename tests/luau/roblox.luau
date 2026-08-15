--!nocheck
--[[
	Just enough Roblox for a UI library to run outside Roblox.

	The Vide suite needs more than the decoder tests do. Those only ever touch
	`Vector2` and `Color3`; a UI binding creates instances, assigns properties,
	parents children and listens for property changes, and Vide's own semantics
	depend on `typeof` telling it which of those a value is.

	Two details here are load-bearing rather than cosmetic:

	- **Instances are userdata.** Vide distinguishes a child instance from a
	  nested property table with `type(value) == "userdata"`, so a table
	  standing in for an instance would be parented as properties instead.
	- **Datatypes are frozen and compare by value.** Roblox datatypes are value
	  types, and Vide's `source` skips an update when the new value equals the
	  old one — but only for a non-table or a frozen table. Get that wrong and
	  the mock would report re-renders the engine would never perform, which is
	  precisely what the snapping tests are checking for.

	Everything else is deliberately thin. What is missing fails loudly, which is
	how a test learns the code under test reached for something unexpected.
]]

local roblox = {}

local realTypeof = typeof

-- Each datatype's metatable is tagged so `__eq` can refuse a cross-type
-- comparison rather than index a field that is not there.
local function datatype(name, fields)
	local mt = { __datatype = name }
	mt.__index = mt
	mt.__eq = function(a, b)
		local other = getmetatable(b)
		if other == nil or other.__datatype ~= name then
			return false
		end
		for _, field in fields do
			if a[field] ~= b[field] then
				return false
			end
		end
		return true
	end
	mt.__tostring = function(value)
		local parts = {}
		for _, field in fields do
			table.insert(parts, tostring(value[field]))
		end
		return `{name}({table.concat(parts, ", ")})`
	end
	return mt
end

local Vector2MT = datatype("Vector2", { "X", "Y" })
local Vector2 = {}
function Vector2.new(x, y)
	return table.freeze(setmetatable({ X = x or 0, Y = y or 0 }, Vector2MT))
end
Vector2.zero = Vector2.new(0, 0)
roblox.Vector2 = Vector2

local Color3MT = datatype("Color3", { "R", "G", "B" })
local Color3 = {}
function Color3.new(r, g, b)
	return table.freeze(setmetatable({ R = r or 0, G = g or 0, B = b or 0 }, Color3MT))
end
function Color3.fromRGB(r, g, b)
	return Color3.new((r or 0) / 255, (g or 0) / 255, (b or 0) / 255)
end
roblox.Color3 = Color3

local UDimMT = datatype("UDim", { "Scale", "Offset" })
local UDim = {}
function UDim.new(scale, offset)
	return table.freeze(setmetatable({ Scale = scale or 0, Offset = offset or 0 }, UDimMT))
end
roblox.UDim = UDim

local UDim2MT = datatype("UDim2", { "X", "Y" })
local UDim2 = {}
function UDim2.new(xScale, xOffset, yScale, yOffset)
	return table.freeze(setmetatable({
		X = UDim.new(xScale, xOffset),
		Y = UDim.new(yScale, yOffset),
	}, UDim2MT))
end
function UDim2.fromOffset(x, y)
	return UDim2.new(0, x, 0, y)
end
function UDim2.fromScale(x, y)
	return UDim2.new(x, 0, y, 0)
end
roblox.UDim2 = UDim2

-- `Content` is an opaque handle in the engine; all these tests need is that the
-- value round-trips the object it was made from, so an assertion can say which
-- EditableImage a label is displaying.
local ContentMT = { __datatype = "Content" }
ContentMT.__index = ContentMT
ContentMT.__eq = function(a, b)
	local other = getmetatable(b)
	return other ~= nil and other.__datatype == "Content" and a.Object == b.Object
end
local Content = {}
function Content.fromObject(object)
	return table.freeze(setmetatable({ Object = object }, ContentMT))
end
Content.none = table.freeze(setmetatable({ Object = nil }, ContentMT))
roblox.Content = Content

-- Enums are only ever read for their identity (Vide's instance defaults), so
-- each path materializes a distinct, stable token.
local function lazyEnum(path)
	return setmetatable({}, {
		__index = function(self, key)
			local value = lazyEnum(`{path}.{key}`)
			rawset(self, key, value)
			return value
		end,
		__tostring = function()
			return path
		end,
	})
end
roblox.Enum = lazyEnum("Enum")

-- ---------------------------------------------------------------------------
-- Signals, connections and instances
-- ---------------------------------------------------------------------------

local SignalMT = {}
SignalMT.__index = SignalMT

local ConnectionMT = {}
ConnectionMT.__index = ConnectionMT

function ConnectionMT:Disconnect()
	self.Connected = false
end

local function newSignal()
	return setmetatable({ listeners = {} }, SignalMT)
end

function SignalMT:Connect(listener)
	local connection = setmetatable({ Connected = true, listener = listener }, ConnectionMT)
	table.insert(self.listeners, connection)
	return connection
end

function SignalMT:Fire(...)
	-- Copied first: a listener may disconnect during dispatch, exactly as one
	-- may in the engine.
	for _, connection in table.clone(self.listeners) do
		if connection.Connected then
			connection.listener(...)
		end
	end
end

--[[
	The instance registry, so `typeof` can answer "Instance" for a userdata.

	Weak-keyed: a mock instance that nothing references should be collectable,
	just as a destroyed one is.
]]
local instances = setmetatable({}, { __mode = "k" })

local DEFAULT_PROPERTIES = {
	AbsoluteSize = Vector2.zero,
	ImageColor3 = Color3.new(1, 1, 1),
	Name = "",
	Parent = nil,
}

local function newInstance(className)
	local properties = table.clone(DEFAULT_PROPERTIES)
	properties.ClassName = className
	properties.Name = className

	local children = {}
	local signals = {}
	local instance = newproxy(true)

	local methods = {}

	function methods:GetPropertyChangedSignal(property)
		local signal = signals[property]
		if signal == nil then
			signal = newSignal()
			signals[property] = signal
		end
		return signal
	end

	function methods:Destroy()
		properties.Parent = nil
		instances[instance] = nil
	end

	function methods:Clone()
		return newInstance(className)
	end

	function methods:GetChildren()
		return table.clone(children)
	end

	local metatable = getmetatable(instance)
	metatable.__index = function(_, key)
		local method = methods[key]
		if method ~= nil then
			return function(_, ...)
				return method(instance, ...)
			end
		end
		return properties[key]
	end
	metatable.__newindex = function(_, key, value)
		if properties[key] == value then
			return
		end
		if key == "Parent" then
			local previous = properties.Parent
			if previous ~= nil then
				local siblings = instances[previous].children
				local index = table.find(siblings, instance)
				if index ~= nil then
					table.remove(siblings, index)
				end
			end
			if value ~= nil then
				table.insert(instances[value].children, instance)
			end
		end
		properties[key] = value
		local signal = signals[key]
		if signal ~= nil then
			signal:Fire()
		end
	end
	metatable.__tostring = function()
		return properties.Name
	end

	instances[instance] = { properties = properties, children = children }
	return instance
end

roblox.Instance = { new = newInstance }

--[[
	`typeof`, extended to recognize the mocks.

	Vide branches on this in three places — parenting children, connecting
	events and disposing of cleanup values — so an incomplete answer here would
	silently take a different code path rather than fail.
]]
function roblox.typeof(value)
	local kind = realTypeof(value)
	if kind == "userdata" and instances[value] ~= nil then
		return "Instance"
	end
	if kind == "table" then
		local mt = getmetatable(value)
		if mt == SignalMT then
			return "RBXScriptSignal"
		elseif mt == ConnectionMT then
			return "RBXScriptConnection"
		elseif mt ~= nil and mt.__datatype ~= nil then
			return mt.__datatype
		end
	end
	return kind
end

--[[
	A `game` stand-in.

	Vide asks for two services. `ReflectionService` is how its JSX factory maps
	`"imagelabel"` to `"ImageLabel"`, and getting that right matters: the class
	name is what selects Vide's own instance defaults, so a mock that returned
	no classes would quietly test a differently-configured ImageLabel.
]]
local CLASSES = {
	"Frame",
	"ImageButton",
	"ImageLabel",
	"ScreenGui",
	"TextButton",
	"TextLabel",
	"UIListLayout",
	"UIPadding",
}

roblox.heartbeat = newSignal()

roblox.game = {
	GetService = function(_, name)
		if name == "ReflectionService" then
			return {
				GetClasses = function()
					local classes = {}
					for _, className in CLASSES do
						table.insert(classes, { Name = className })
					end
					return classes
				end,
			}
		elseif name == "RunService" then
			return { Heartbeat = roblox.heartbeat }
		end
		error(`the Roblox mock has no {name}`, 0)
	end,
}

--[[
	The globals a module under test should see.

	Returned as a fresh table each call so a suite can add to it — the harness
	uses it as the base environment for every module it loads.
]]
function roblox.globals()
	return {
		Color3 = roblox.Color3,
		Content = roblox.Content,
		Enum = roblox.Enum,
		Instance = roblox.Instance,
		UDim = roblox.UDim,
		UDim2 = roblox.UDim2,
		Vector2 = roblox.Vector2,
		game = roblox.game,
		typeof = roblox.typeof,
	}
end

return roblox
