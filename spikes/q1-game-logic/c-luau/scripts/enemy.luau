--!strict
-- Q1 candidate C: the enemy AI, in Luau.
--
-- THIS IS THE FILE THE LATENCY MEASUREMENT PERTURBS. Nothing is compiled; the host re-reads and
-- re-compiles this file, which is the entire cost model of option C.
--
-- Must match the specification in `scenario/src/lib.rs` exactly, including the order RNG draws are
-- taken in. See ADR 0011 for why "exactly" turns out to be the hard part.

local PATROL = 0
local PURSUE = 1
local SEARCH = 2

local WAYPOINT_RADIUS = 2.0

-- A unit vector from (fx, fy) toward (tx, ty), scaled by speed.
-- Zero when the points nearly coincide, matching `scenario::steer_toward`.
local function steer_toward(fx: number, fy: number, tx: number, ty: number, speed: number): (number, number)
	local dx = tx - fx
	local dy = ty - fy
	local length = math.sqrt(dx * dx + dy * dy)
	if length < 1e-6 then
		return 0.0, 0.0
	end
	return dx / length * speed, dy / length * speed
end

-- The four patrol waypoints around a home position, in visit order.
local function waypoint(hx: number, hy: number, index: number): (number, number)
	local slot = index % 4
	if slot == 0 then
		return hx + WAYPOINT_RADIUS, hy
	elseif slot == 1 then
		return hx, hy + WAYPOINT_RADIUS
	elseif slot == 2 then
		return hx - WAYPOINT_RADIUS, hy
	else
		return hx, hy - WAYPOINT_RADIUS
	end
end

-- Called once per tick by the host.
--
-- `ctx.enemies` is a plain array of tables, mutated in place; the host reads the mutated values
-- back into the world afterwards. `ctx.random(min, max)` draws from the engine's SimRng -- the
-- script must never call `math.random`, which would have its own state and silently void every
-- replay test (invariant I3).
local function update(ctx)
	local config = ctx.config
	local px = ctx.player_x
	local py = ctx.player_y
	local sight_squared = config.sight_range * config.sight_range
	local lose_squared = config.lose_range * config.lose_range

	for index = 1, ctx.count do
		local enemy = ctx.enemies[index]

		local dx = px - enemy.x
		local dy = py - enemy.y
		local distance_squared = dx * dx + dy * dy

		local vx = 0.0
		local vy = 0.0

		if enemy.state == PATROL then
			if distance_squared <= sight_squared then
				-- A transition costs this tick's movement, as in the Rust reference.
				enemy.state = PURSUE
			else
				local tx, ty = waypoint(enemy.home_x, enemy.home_y, enemy.waypoint)
				vx, vy = steer_toward(enemy.x, enemy.y, tx, ty, config.patrol_speed)

				local ax = tx - enemy.x
				local ay = ty - enemy.y
				if math.sqrt(ax * ax + ay * ay) <= config.waypoint_radius then
					enemy.waypoint = (enemy.waypoint + 1) % 4
				end
			end
		elseif enemy.state == PURSUE then
			if distance_squared > lose_squared then
				-- x then y, in that order. Swapping these two lines changes every subsequent
				-- random value in the run and looks like a behaviour bug rather than an ordering one.
				local jitter_x = ctx.random(-config.search_jitter, config.search_jitter)
				local jitter_y = ctx.random(-config.search_jitter, config.search_jitter)
				enemy.state = SEARCH
				enemy.timer = config.search_ticks
				enemy.last_x = px + jitter_x
				enemy.last_y = py + jitter_y
			else
				enemy.last_x = px
				enemy.last_y = py
				vx, vy = steer_toward(enemy.x, enemy.y, px, py, config.pursue_speed)
			end
		else
			if distance_squared <= sight_squared then
				enemy.state = PURSUE
			else
				vx, vy = steer_toward(enemy.x, enemy.y, enemy.last_x, enemy.last_y, config.patrol_speed)
				enemy.timer = enemy.timer - 1
				if enemy.timer <= 0 then
					enemy.timer = 0
					enemy.state = PATROL
				end
			end
		end

		enemy.vx = vx
		enemy.vy = vy
	end
end

return update
