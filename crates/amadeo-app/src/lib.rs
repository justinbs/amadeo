//! The application layer: schedules, the fixed-timestep loop, and app lifecycle.
//!
//! ```
//! use amadeo_app::{App, Stage, system};
//! use amadeo_core::{FIXED_DT, StableHash};
//! use amadeo_ecs::{Component, World};
//! use amadeo_reflect::Reflect;
//!
//! #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
//! struct Position { x: f32 }
//! #[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
//! struct Velocity { x: f32 }
//!
//! impl Component for Position {}
//! impl Component for Velocity {}
//!
//! // A system reads FIXED_DT, never a measured frame delta.
//! fn integrate(world: &mut World) {
//!     world.for_each_pair_mut::<Position, Velocity>(|_entity, position, velocity| {
//!         position.x += velocity.x * FIXED_DT;
//!     });
//! }
//!
//! let mut app = App::new();
//! app.add_system(Stage::Simulation, system("integrate", integrate));
//!
//! let entity = app.world.spawn();
//! app.world.insert(entity, Position { x: 0.0 });
//! app.world.insert(entity, Velocity { x: 60.0 });
//!
//! // One second of simulated time.
//! app.run_ticks(60).expect("schedule resolves");
//!
//! let position = app.world.get::<Position>(entity).expect("still alive");
//! assert!((position.x - 60.0).abs() < 0.01);
//! ```
//!
//! # The two ways to run
//!
//! [`App::run_ticks`] runs an exact number of ticks and never looks at a clock — the deterministic
//! path used by tests, replays, and headless agent runs. [`App::advance_real_time`] feeds in measured
//! elapsed time and runs whatever whole ticks fit — the path a windowed game uses.
//!
//! Both drive the same per-tick code, which is what makes a headless run and a windowed run agree
//! exactly (invariant I7).
//!
//! # System ordering is a pure function of what is registered
//!
//! Systems declare `before`/`after` constraints and are topologically sorted, with ties broken
//! **alphabetically by label** rather than by registration order. Registration order depends on how
//! the app was assembled, and letting that influence execution order would make simulation results
//! depend on plugin setup — the exact trap invariant I3 exists to close.

mod agent;
mod app;
mod schedule;

pub use agent::{
    AGENT_FLAG, APP_METHODS, AgentError, AgentOptions, TICKS_FLAG, agent_options,
    agent_options_from, serve, serve_if_requested,
};
pub use app::{App, SimRng};
pub use schedule::{Schedule, ScheduleError, Stage, SystemConfig, system};
