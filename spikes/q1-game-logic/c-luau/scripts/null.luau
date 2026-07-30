--!strict
-- A do-nothing script, used to separate two costs that are easy to confuse:
--
--   * the marshalling boundary -- building the context table, writing 9 fields per enemy, calling
--     into the VM, and reading 7 fields back per enemy
--   * actually executing Luau
--
-- Running the benchmark with this script measures the first alone. Subtract it from the real
-- script's time and what remains is the second.
--
-- It still has to write `vx`/`vy`, because the host reads them back unconditionally and a missing
-- field would be a marshalling error rather than a zero.

local function update(ctx)
	for index = 1, ctx.count do
		local enemy = ctx.enemies[index]
		enemy.vx = 0.0
		enemy.vy = 0.0
	end
end

return update
