//! The application: a world, its schedules, and the fixed-timestep loop that drives them.

use crate::schedule::{Schedule, ScheduleError, Stage, SystemConfig};
use amadeo_core::{FIXED_DT_NANOS, Rng, StableHash, StableHasher, Tick};
use amadeo_ecs::{Commands, Component, ComponentRegistry, Resource, Service, World};
use amadeo_events::{Event, WorldEvents};
use amadeo_reflect::RegistryError;
use std::collections::BTreeMap;

/// The simulation's random number generator, seeded once and advanced deterministically.
///
/// A resource rather than a global, so its state is part of [`World::state_hash`] — two runs that
/// have consumed different numbers of random values have diverged, and that must be detectable.
///
/// Systems needing randomness should [`Rng::fork`] a child stream rather than sharing this one
/// directly, so that system execution order cannot influence the values any single system receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimRng(pub Rng);

impl StableHash for SimRng {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        // The generator's observable state is captured by cloning and drawing, which would consume
        // it. Instead the debug representation is hashed: it is derived purely from the internal
        // state fields, and is stable because `Rng`'s Debug output is derived.
        hasher.write_str(&format!("{:?}", self.0));
    }
}

impl Resource for SimRng {}

/// How many simulation ticks a single real-time frame may run.
///
/// Without a cap, a long stall (a debugger pause, a slow asset load, a laptop resuming from sleep)
/// leaves a huge accumulated backlog. Running all of it takes longer than the stall, producing a
/// deeper backlog next frame — a spiral the game never recovers from.
///
/// Capping means simulated time falls behind real time after a stall rather than trying to catch up.
/// That is the standard trade, and it is invisible in practice. It cannot affect determinism: the cap
/// changes *how many* ticks run, never what happens inside one, and it lives outside the
/// deterministic zone entirely. Replays use [`App::run_ticks`], which ignores wall time.
const MAX_TICKS_PER_FRAME: u32 = 8;

/// A registered event type's name paired with the function that swaps its buffers.
///
/// The function is a plain pointer rather than a boxed closure: each one is a monomorphised
/// `World::swap_events::<T>` that captures nothing, so no allocation is needed. The name is carried
/// alongside purely for diagnostics and duplicate detection, since a function pointer cannot be
/// compared back to the type it came from.
type EventSwap = (&'static str, fn(&mut World));

/// A world plus the schedules that drive it.
///
/// # Running it
///
/// Two entry points, for two different purposes:
///
/// - [`App::run_ticks`] runs an exact number of ticks with no reference to real time. This is what
///   tests, replays, and headless agent runs use, and it is fully deterministic.
/// - [`App::advance_real_time`] accumulates elapsed wall time and runs however many whole ticks
///   fit. This is what a windowed game uses.
///
/// Both call the same per-tick code, so a headless run and a windowed run produce identical
/// simulation state (invariant I7).
#[derive(Debug)]
pub struct App {
    /// All simulation state.
    pub world: World,
    /// Every component type this app can describe, build from a scene file, or show to an agent.
    ///
    /// It lives here, on the app, rather than being built separately and passed around, because
    /// ADR 0016 makes the game binary the agent's host: whoever holds the `App` must be able to hand
    /// over a registry without going looking for one. Keeping registration and spawning in the same
    /// place is also what stops a component from working perfectly at runtime while being invisible
    /// to `describe` — the failure ADR 0013 made `Component: Reflect` a compiler-enforced bound to
    /// prevent, one level further up.
    registry: ComponentRegistry,
    /// Systems, grouped by stage. `BTreeMap` so stages iterate in declared order.
    schedules: BTreeMap<Stage, Schedule>,
    /// One buffer-swap entry per registered event type. See [`EventSwap`].
    event_swaps: Vec<EventSwap>,
    /// Unspent real time, in nanoseconds. Outside the deterministic zone.
    accumulated_nanos: u64,
    /// The seed this app was built with.
    ///
    /// Kept because a replay only reproduces against the seed that recorded it, and the agent host
    /// has to be able to say so rather than silently replaying against the wrong one.
    seed: u64,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Creates an app with an empty world and a seeded RNG.
    #[must_use]
    pub fn new() -> Self {
        Self::with_seed(0)
    }

    /// Creates an app whose [`SimRng`] starts from `seed`.
    ///
    /// The seed is part of what makes a run reproducible, so a replay file records it and a replay
    /// must be played back with the same value.
    #[must_use]
    pub fn with_seed(seed: u64) -> Self {
        let mut world = World::new();
        world.insert_resource(SimRng(Rng::new(seed)));
        // Installed by default: a system that wants to spawn or despawn from inside a query needs
        // this, and having to remember to add it would be a confusing first failure.
        world.insert_service(Commands::new());
        Self {
            world,
            registry: ComponentRegistry::new(),
            schedules: BTreeMap::new(),
            event_swaps: Vec::new(),
            accumulated_nanos: 0,
            seed,
        }
    }

    /// The seed this app was built with.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Registers a component type, so scenes can build it and the agent can see it.
    ///
    /// Registration is what puts a type into `amadeo describe` and lets a `.scene` file name it.
    /// A component that is never registered still works at runtime — it just cannot be authored in
    /// text or inspected, which is invariant I8's whole concern.
    ///
    /// Engine components are not registered automatically. A game names the ones it uses, including
    /// `Transform2d` and `Parent`, so the schema describes that game rather than everything the
    /// engine could theoretically offer.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if another type is already registered under the same canonical name. That
    /// is a real ambiguity — a scene file naming it could mean either — so it is refused rather
    /// than resolved arbitrarily.
    pub fn register_component<T: Component>(&mut self) -> Result<(), RegistryError> {
        self.registry.register::<T>()
    }

    /// The registered component types.
    ///
    /// This is what `describe`, `inspect`, and scene loading are all given.
    #[must_use]
    pub fn components(&self) -> &ComponentRegistry {
        &self.registry
    }

    /// Registers a system into a stage.
    pub fn add_system(&mut self, stage: Stage, config: SystemConfig) -> &mut Self {
        self.schedules
            .entry(stage)
            .or_insert_with(|| Schedule::new(stage))
            .add(config);
        self
    }

    /// Registers an event type and arranges for its buffers to be swapped each tick.
    pub fn register_event<T: Event>(&mut self) -> &mut Self {
        self.world.register_event::<T>();

        let name = std::any::type_name::<T>();
        if !self
            .event_swaps
            .iter()
            .any(|(existing, _)| *existing == name)
        {
            // A non-capturing closure coerces to a plain function pointer.
            let swap: fn(&mut World) = |world| world.swap_events::<T>();
            self.event_swaps.push((name, swap));
        }
        self
    }

    /// Inserts a resource into the world. Convenience for chaining during setup.
    pub fn insert_resource<T: Resource>(&mut self, value: T) -> &mut Self {
        self.world.insert_resource(value);
        self
    }

    /// Inserts an engine service into the world. Convenience for chaining during setup.
    ///
    /// Services are excluded from [`App::state_hash`]. Anything render-side, cached, or
    /// device-backed belongs here rather than in a resource.
    pub fn insert_service<T: Service>(&mut self, value: T) -> &mut Self {
        self.world.insert_service(value);
        self
    }

    /// The current simulation tick.
    #[must_use]
    pub fn tick(&self) -> Tick {
        self.world.tick()
    }

    /// The registered event type names, for diagnostics and the agent-facing listing.
    #[must_use]
    pub fn registered_events(&self) -> Vec<&'static str> {
        self.event_swaps.iter().map(|(name, _)| *name).collect()
    }

    /// The systems in a stage, in resolved execution order.
    ///
    /// Returns an empty list for a stage with no systems. Surfaced so the schedule is inspectable
    /// rather than opaque — the agent interface layer will expose this directly.
    pub fn resolved_order(&mut self, stage: Stage) -> Result<Vec<&'static str>, ScheduleError> {
        match self.schedules.get_mut(&stage) {
            Some(schedule) => schedule.resolved_labels(),
            None => Ok(Vec::new()),
        }
    }

    /// Runs exactly one simulation tick.
    ///
    /// The order here is the deterministic zone from ADR 0005:
    ///
    /// 1. `PreSimulation`, `Simulation`, `PostSimulation`, in that order, with **queued commands
    ///    flushed after each stage** — so a spawn queued during `PreSimulation` exists by the time
    ///    `Simulation` runs, rather than a stage later
    /// 2. event buffers swap, so this tick's events become readable next tick
    /// 3. the tick counter advances
    ///
    /// `Render` and `Present` are not touched, which is why this is identical headless or windowed.
    pub fn step(&mut self) -> Result<(), ScheduleError> {
        for stage in Stage::SIMULATION_STAGES {
            if let Some(schedule) = self.schedules.get_mut(&stage) {
                schedule.run(&mut self.world)?;
            }
            // Flushing per stage rather than once per tick keeps stage boundaries meaningful: a
            // system in a later stage sees the structural changes an earlier stage requested.
            self.world.flush_commands();
        }

        for (_, swap) in &self.event_swaps {
            swap(&mut self.world);
        }

        self.world.advance_tick();
        Ok(())
    }

    /// Runs `ticks` simulation steps, ignoring real time entirely.
    ///
    /// The deterministic entry point: same starting state and same inputs always produce the same
    /// result. Used by tests, replays, and headless agent runs.
    pub fn run_ticks(&mut self, ticks: u64) -> Result<(), ScheduleError> {
        for _ in 0..ticks {
            self.step()?;
        }
        Ok(())
    }

    /// Feeds elapsed real time in and runs whatever whole ticks it affords.
    ///
    /// Returns how many ticks ran, capped at 8 per call. When the cap is hit the excess backlog is
    /// discarded rather than carried, which is what prevents a catch-up spiral after a long stall.
    /// The cap cannot affect determinism: it changes how many ticks run, never what happens inside
    /// one, and replays use [`App::run_ticks`] which ignores wall time entirely.
    pub fn advance_real_time(&mut self, elapsed_nanos: u64) -> Result<u32, ScheduleError> {
        self.accumulated_nanos = self.accumulated_nanos.saturating_add(elapsed_nanos);

        let mut ticks_run = 0;
        while self.accumulated_nanos >= FIXED_DT_NANOS && ticks_run < MAX_TICKS_PER_FRAME {
            self.step()?;
            self.accumulated_nanos -= FIXED_DT_NANOS;
            ticks_run += 1;
        }

        if ticks_run == MAX_TICKS_PER_FRAME {
            // Drop the rest of the backlog. Simulated time now lags real time, which is preferable
            // to never catching up at all.
            self.accumulated_nanos %= FIXED_DT_NANOS;
        }

        Ok(ticks_run)
    }

    /// How far the current frame has progressed toward the next tick, in `0.0..1.0`.
    ///
    /// Rendering uses this to interpolate between the previous and current simulation state, so a
    /// 60 Hz simulation still looks smooth on a faster display. Never readable from simulation code —
    /// it is derived from wall time and is not reproducible.
    #[must_use]
    pub fn render_interpolation(&self) -> f32 {
        self.accumulated_nanos as f32 / FIXED_DT_NANOS as f32
    }

    /// Runs the `Render` and `Present` stages.
    ///
    /// Separate from [`App::step`] because rendering happens at a different rate and may be skipped
    /// entirely. Systems in these stages must not write simulation state.
    pub fn render(&mut self) -> Result<(), ScheduleError> {
        for stage in [Stage::Render, Stage::Present] {
            if let Some(schedule) = self.schedules.get_mut(&stage) {
                schedule.run(&mut self.world)?;
            }
        }
        Ok(())
    }

    /// A fingerprint of all simulation state. Shorthand for [`World::state_hash`].
    #[must_use]
    pub fn state_hash(&self) -> u64 {
        self.world.state_hash()
    }
}
