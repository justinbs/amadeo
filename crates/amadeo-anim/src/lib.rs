//! Keyframed animation: a clip animates a **reflected field** — ADR 0066.
//!
//! # The one idea
//!
//! A track says which component and which field, by name:
//!
//! ```text
//! track
//!   component "Transform"
//!   field "translation"
//!   interpolation Linear
//!   keys
//!     - time 0.0
//!       value 0.0 1.0 0.0
//!     - time 1.5
//!       value 0.0 2.5 0.0
//! ```
//!
//! **Nothing in this crate knows about any component type**, and that is the whole design. A
//! `Component` is `Reflect` by trait bound (ADR 0013), so reading one as a value, patching one field
//! and rebuilding it is machinery that already existed — it is exactly what ADR 0029's prefab
//! overrides do. The consequence is that adding an animatable property is never engine work: a
//! light's intensity, a material's colour, a sprite's region on a tilesheet and a UI panel's paint
//! all animate today, and so will anything added after this crate was written.
//!
//! # Animation is simulation, not presentation
//!
//! The reflex from `GlobalTransform` and `ComputedRect` is to make animation output derived, and here
//! that would be wrong. A clip that moves a `Transform` is a **moving platform you can stand on**:
//! physics reads it this tick, a save has to restore it, and `docs/04` §14 requires hitboxes on
//! frames to reproduce exactly. So [`AnimationPlayer`]'s clock is hashed and so is everything it
//! writes, and [`animate`] runs in `Simulation`.
//!
//! The derived half arrives with skinning, where a pose becomes joint matrices only a shader reads.
//!
//! # Wiring it up
//!
//! Three lines, and none of them is optional:
//!
//! ```
//! use amadeo_anim::{ANIMATE, Animatable, AnimationPlayer, ClipCache};
//! use amadeo_ecs::World;
//!
//! # use amadeo_core::StableHash;
//! # use amadeo_reflect::Reflect;
//! # #[derive(Debug, Clone, Copy, PartialEq, Default, StableHash, Reflect)]
//! # struct Lamp { /// How bright.
//! # brightness: f32 }
//! # impl amadeo_ecs::Component for Lamp {}
//! let mut world = World::new();
//! world.insert_service(ClipCache::new());
//!
//! // The allow-list. A clip can only write component types named here — see `Animatable` for why
//! // that is a structural necessity and also a good idea.
//! let mut animatable = Animatable::new();
//! animatable.allow::<Lamp>();
//! world.insert_service(animatable);
//!
//! assert!(world.service::<Animatable>().expect("installed").allows("Lamp"));
//! ```
//!
//! …plus `app.add_system(Stage::Simulation, system(ANIMATE, animate))`, and the app layer filling
//! the [`ClipCache`] from the `.anim` files a scene declares — which happens there rather than here
//! because a `.anim` is a *scene* file and `amadeo-scene` sits above this crate (invariant I6).
//!
//! # What is deliberately absent
//!
//! Skeletal animation and skinning, blending and blend trees, and a state machine. ADR 0066 §5 has
//! the reasoning for each, including why an animation state machine is **not** going to be the same
//! abstraction as an AI one: an animation transition is a blend over time and an AI transition is
//! instantaneous with side effects, and every large engine keeps them apart on purpose.

mod clip;
mod play;

pub use clip::{AnimationClip, Interpolation, Key, Track};
pub use play::{ANIMATE, Animatable, AnimationFinished, AnimationPlayer, ClipCache, animate};
