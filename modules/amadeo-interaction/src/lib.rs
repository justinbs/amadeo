//! Looking at things and using them.
//!
//! ```text
//! amadeo_interaction::install(&mut app)?;
//! ```
//!
//! # What this is
//!
//! Put an [`Interactor`] on whatever does the looking and an [`Interactable`] on whatever can be
//! used. Each tick the module sweeps a small sphere forward from the interactor, writes what it
//! found into [`Looking`], and — when the `use` action is pressed — raises [`Interacted`].
//!
//! `docs/05` names this as one of M3's first genre modules, and M3's exit gate asks for picking up
//! and using a flashlight and a key.
//!
//! # It knows nothing about characters or cameras
//!
//! Trap 10 one level along: an [`Interactor`] is whatever carries the component. In a first-person
//! game that is the camera; in a third-person one it is usually the character, so that reach is
//! measured from the body rather than from wherever the camera has swung to; in a point-and-click
//! game it could be a cursor. This module does not depend on `amadeo-camera` or
//! `amadeo-character`, and nothing here would notice if a game had neither.
//!
//! # A sphere, not a ray
//!
//! The sweep has a radius, and a small one is deliberate. A zero-width ray demands that the player
//! aim at a door handle exactly; a sphere forgives, which is what every game that has ever had an
//! interaction prompt does. `Interactor::radius` is how forgiving.
//!
//! # What is hashed and what is not
//!
//! [`Looking`] is **derived**. It is recomputed from scratch every tick out of transforms and the
//! physics index — it never has to survive to the next one — so it goes out of the state hash for
//! `GlobalTransform`'s reason (ADR 0019). What *is* hashed is everything it is computed from, so two
//! machines agree about what you are looking at without it being state.
//!
//! [`Interacted`] is an event, so it is hashed and replays like any other: choosing to use something
//! is a decision a player made and a replay has to reproduce it.

use amadeo_app::{App, Stage, system};
use amadeo_core::StableHash;
use amadeo_ecs::{Component, Entity, World};
use amadeo_events::{Event, WorldEvents};
use amadeo_input::{ActionId, InputState};
use amadeo_physics::{Physics, Shape, ShapeCast};
use amadeo_reflect::{Reflect, RegistryError};
use amadeo_transform::{GlobalTransform, Mat4, Transform};

/// The label [`update_interactions`] is registered under.
pub const UPDATE_INTERACTIONS: &str = "update_interactions";

/// The named action that uses whatever is being looked at.
pub const USE: &str = "use";

/// Something that can look at things and use them.
///
/// Put it on whatever does the looking — see the module docs for why this module has no opinion
/// about what that is.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Interactor {
    /// How far it can reach, in world units.
    #[reflect(min = 0.0, max = 100.0, unit = "world units")]
    pub reach: f32,
    /// The radius of the sphere swept forward.
    ///
    /// Bigger than nothing on purpose: a zero-width ray demands the player aim at a door handle
    /// exactly. This is how much the aim is forgiven, and it is the single number that decides
    /// whether interaction feels generous or fussy.
    #[reflect(min = 0.0, max = 5.0, unit = "world units")]
    pub radius: f32,
}

impl Default for Interactor {
    fn default() -> Self {
        Self {
            reach: 2.5,
            radius: 0.15,
        }
    }
}

impl Component for Interactor {}

/// Something that can be used.
#[derive(Debug, Clone, PartialEq, Default, StableHash, Reflect)]
pub struct Interactable {
    /// What to tell the player they can do — "Open the door", "Pick up the key".
    ///
    /// **The engine does not know what this means**, which is invariant I4 one level up: a game
    /// reads it and draws it, and could equally ignore it and use its own. A `String` rather than an
    /// id because a prompt is content, and one that had to be an asset would make every door in a
    /// game a file.
    pub prompt: String,
    /// Whether it can currently be used.
    ///
    /// A field rather than removing the component, for `UiNode::visible`'s reason: a locked door
    /// that becomes unlocked must not move between archetypes to say so.
    pub enabled: bool,
}

impl Interactable {
    /// Something usable, with a prompt.
    #[must_use]
    pub fn new(prompt: &str) -> Self {
        Self {
            prompt: prompt.to_string(),
            enabled: true,
        }
    }
}

impl Component for Interactable {}

/// What an [`Interactor`] is currently looking at.
///
/// # Derived, and that is the interesting call
///
/// The reflex is that this is gameplay — what you can pick up decides what happens next, and ADR
/// 0063 spent a whole decision on keeping the *UI's* focus hashed for exactly that reason.
///
/// This is different, and the difference is worth being precise about. UI focus **had to be state**
/// because it changes only through input and nothing recomputes it; there is no other record of
/// where the highlight sits. This is recomputed from scratch every tick out of transforms and the
/// physics index, both of which are already hashed, so it never has to survive to the next tick and
/// putting it in the hash would only be hashing the same facts twice.
///
/// It also does **not** depend on the window size, which is what made UI focus dangerous. Two
/// machines running the same world agree about what is under the crosshair without this being state.
#[derive(Debug, Clone, Copy, PartialEq, Default, StableHash, Reflect)]
pub struct Looking {
    /// What is in reach, or `None`.
    pub at: Option<Entity>,
    /// How far away it is, in world units. Meaningless when nothing is in reach.
    #[reflect(min = 0.0, max = 100.0, unit = "world units")]
    pub distance: f32,
}

impl Component for Looking {
    /// Recomputed every tick from hashed state — see the type's docs for why that is the right call
    /// here and the opposite call from ADR 0063's.
    const DERIVED: bool = true;
}

/// Something was used.
///
/// Past tense, because it is a fact rather than a request (`CLAUDE.md` §6). Carries **both** ends:
/// a game with two players in the same room needs to know which of them opened the door.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableHash, Reflect)]
pub struct Interacted {
    /// Who used it.
    pub interactor: Entity,
    /// What they used.
    pub target: Entity,
}

impl Event for Interacted {}

/// Registers the components, the event, and the system.
///
/// # It goes in `PostSimulation`, and both halves of that are load-bearing
///
/// **After `propagate_transforms`**, because an interactor is usually a child — a camera on a
/// character — and its world position and facing come from composing the chain. Running in
/// `Simulation` would read last tick's composed transform, which is a whole tick of aim lag on
/// something a player is pointing by hand.
///
/// **And therefore after `step_physics`**, which the cast requires: a backend answers from a
/// spatial index the step builds, and asking earlier queries an empty one (ADR 0054). That falls out
/// of being in a later stage rather than being declared.
///
/// # Errors
///
/// [`RegistryError`] if any of the components is already registered under a different type.
pub fn install(app: &mut App) -> Result<(), RegistryError> {
    app.register_component::<Interactor>()?;
    app.register_component::<Interactable>()?;
    app.register_component::<Looking>()?;
    app.register_event::<Interacted>();

    app.add_system(
        Stage::PostSimulation,
        system(UPDATE_INTERACTIONS, update_interactions)
            .after(amadeo_transform::PROPAGATE_TRANSFORMS),
    );
    Ok(())
}

/// Sweeps forward from every [`Interactor`], records what it found, and raises [`Interacted`].
///
/// Does nothing without a [`Physics`] service, which is the headless case — and against
/// `NullPhysics` every cast reports clear, so nothing is ever in reach. That is deliberately the
/// control case this module's tests assert, the same trade `modules/amadeo-character` makes.
pub fn update_interactions(world: &mut World) {
    let used = world
        .resource::<InputState>()
        .is_some_and(|input| input.just_pressed(ActionId::new(USE)));

    // Everything that could be looked at, so a hit on scenery is not mistaken for one on a door.
    // Collected first because the cast below needs the world while this reads it.
    let usable: Vec<Entity> = world
        .query::<(&Interactable,)>()
        .filter(|(_, (interactable,))| interactable.enabled)
        .map(|(entity, _)| entity)
        .collect();

    let interactors: Vec<(Entity, Interactor, [f32; 3], [f32; 3])> = world
        .query::<(&Interactor,)>()
        .filter_map(|(entity, (interactor,))| {
            // The **composed** transform, because an interactor is usually a child — a camera on a
            // character — and where it is pointing is the whole chain multiplied out. Falling back
            // to the local one covers a root entity, and covers a game that never registered
            // `propagate_transforms`; for a root the two are the same thing.
            let matrix = match world.get::<GlobalTransform>(entity) {
                Some(global) => global.to_mat4(),
                None => {
                    let local = world.get::<Transform>(entity)?;
                    Mat4::from_transform(local.translation, local.rotation, [1.0, 1.0, 1.0])
                }
            };
            let from = [
                matrix.columns[3][0],
                matrix.columns[3][1],
                matrix.columns[3][2],
            ];
            Some((entity, *interactor, from, forward_of(&matrix)))
        })
        .collect();

    let mut found: Vec<(Entity, Looking)> = Vec::new();
    let mut chosen: Vec<(Entity, Entity)> = Vec::new();

    for (entity, interactor, from, forward) in interactors {
        let motion = [
            forward[0] * interactor.reach,
            forward[1] * interactor.reach,
            forward[2] * interactor.reach,
        ];

        let hit = world.service::<Physics>().and_then(|physics| {
            physics.cast_shape(
                &ShapeCast::new(
                    Shape::Sphere {
                        radius: interactor.radius,
                    },
                    from,
                    motion,
                )
                // Never itself. A sweep that begins inside the interactor's own collider has no
                // reliable answer — `modules/amadeo-camera` learned that as an intermittent
                // flicker, and here it would report the player using themselves.
                .ignoring(entity),
            )
        });

        // A hit on a wall is a hit. It is only an *interaction* if what was hit can be used, and
        // that check is what stops a player opening a door through it.
        let looking = match hit {
            Some(hit) if hit.entity.is_some_and(|target| usable.contains(&target)) => Looking {
                at: hit.entity,
                distance: hit.fraction * interactor.reach,
            },
            _ => Looking::default(),
        };

        if used && let Some(target) = looking.at {
            chosen.push((entity, target));
        }
        found.push((entity, looking));
    }

    for (entity, looking) in found {
        world.insert(entity, looking);
    }
    for (interactor, target) in chosen {
        world.send_event(Interacted { interactor, target });
    }
}

/// The direction a transform faces, in world space.
///
/// **Negative Z**, which is ADR 0018's convention and the one the camera uses to build its view. A
/// sign error here would make every interaction happen behind the player, which is a bug that reads
/// as interaction simply not working.
fn forward_of(matrix: &Mat4) -> [f32; 3] {
    [
        -matrix.columns[2][0],
        -matrix.columns[2][1],
        -matrix.columns[2][2],
    ]
}
