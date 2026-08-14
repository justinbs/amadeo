//! AI as a **state machine over named facts** — ADR 0068.
//!
//! ```text
//! amadeo_behaviour::install(&mut app)?;
//! ```
//!
//! # The whole idea in one file
//!
//! ```text
//! BehaviourMachine
//!   initial "idle"
//!   states
//!     - name "idle"
//!       transitions
//!         - to "pursue"
//!           when "sees_player"
//!     - name "pursue"
//!       transitions
//!         - to "search"
//!           unless "sees_player"
//!     - name "search"
//!       transitions
//!         - to "pursue"
//!           when "sees_player"
//!         - to "idle"
//!           after 6.0
//! ```
//!
//! # This module knows nothing about seeing, walking, or players
//!
//! The **game** writes named facts into [`Facts`] — `"sees_player"` is one line of its own Rust — and
//! the game reads [`Behaviour::state`] and acts on it. There are no registered callbacks and no
//! action functions. Invariant I4: the module knows how to *sequence* behaviour, the game knows what
//! behaviour *means*.
//!
//! That boundary is the expensive half of the decision and the state machine is the cheap half
//! (ADR 0068 §2). Swapping in a behaviour tree later would replace the sequencer and touch neither
//! side of it.
//!
//! # Facts rather than a registry of condition functions, on purpose
//!
//! A table of `fn(&World, Entity) -> bool` would work and would be *less* introspectable: a function
//! pointer cannot be read by `amadeo query`, so "why did it not transition" becomes "read the game's
//! source" — which is what invariant I5 exists to prevent. **Facts are data, and data can be looked
//! at.**
//!
//! # There is no expression language, deliberately
//!
//! A transition tests whether a fact is true, whether one is false, and how long the machine has been
//! in its current state. No comparisons, no arithmetic, no boolean algebra. Each of those is a small
//! language to design, document, parse and debug, and each is a step towards the scripting layer
//! ADR 0011 measured and rejected. A game wanting `health < 0.3` writes one line of Rust that sets
//! `"badly_hurt"`, where it can be typed, tested, and read.

use amadeo_app::{App, Stage, system};
use amadeo_core::{FIXED_DT, StableHash};
use amadeo_ecs::{Component, Entity, World};
use amadeo_events::{Event, WorldEvents};
use amadeo_reflect::{Reflect, RegistryError};
use std::collections::BTreeMap;

/// The label [`run_behaviours`] is registered under.
pub const RUN_BEHAVIOURS: &str = "run_behaviours";

/// One way out of a state.
///
/// The three conditions are ANDed and each is optional: an empty `when` or `unless` is ignored, and
/// `after` needs no sentinel because "at least zero seconds have passed" is always true.
#[derive(Debug, Clone, PartialEq, Default, StableHash, Reflect)]
pub struct Transition {
    /// The state to move to.
    pub to: String,
    /// A fact that must be **true**. Empty means no such requirement.
    pub when: String,
    /// A fact that must be **false**. Empty means no such requirement.
    ///
    /// Separate from `when` rather than a negation flag, so a transition can require one fact and
    /// forbid another — which is what "chase unless you are hurt" is, and what a single field would
    /// have made impossible to say.
    pub unless: String,
    /// How long the machine must have been in the current state, in seconds.
    #[reflect(min = 0.0, max = 3600.0, unit = "s")]
    pub after: f32,
}

impl Transition {
    /// A transition taken as soon as a fact becomes true.
    #[must_use]
    pub fn when(to: &str, fact: &str) -> Self {
        Self {
            to: to.to_string(),
            when: fact.to_string(),
            ..Self::default()
        }
    }

    /// A transition taken once a fact stops being true.
    #[must_use]
    pub fn unless(to: &str, fact: &str) -> Self {
        Self {
            to: to.to_string(),
            unless: fact.to_string(),
            ..Self::default()
        }
    }

    /// A transition taken after a number of seconds in the current state.
    #[must_use]
    pub fn after(to: &str, seconds: f32) -> Self {
        Self {
            to: to.to_string(),
            after: seconds,
            ..Self::default()
        }
    }
}

/// One state and everything that leads out of it.
#[derive(Debug, Clone, PartialEq, Default, StableHash, Reflect)]
pub struct BehaviourState {
    /// What this state is called. A game matches on it.
    pub name: String,
    /// The ways out, **in priority order** — the first whose conditions all hold is taken.
    ///
    /// Order rather than a score, because order is something an author can see in the file and a
    /// score is something they have to simulate in their head.
    pub transitions: Vec<Transition>,
}

/// A whole state machine, authored.
///
/// # Shared by prefab rather than by asset
///
/// Twenty monsters of one kind are twenty instances of one prefab (ADR 0029), which is already this
/// engine's answer to "many things with the same components". So there is no `.behaviour` file, no
/// cache, and no fifth instance of the missing-asset hazard ADR 0066 had to reason about.
#[derive(Debug, Clone, PartialEq, Default, StableHash, Reflect)]
pub struct BehaviourMachine {
    /// The state to start in. Falls back to the first state if it names none of them.
    pub initial: String,
    /// Every state.
    pub states: Vec<BehaviourState>,
}

impl Component for BehaviourMachine {}

impl BehaviourMachine {
    /// The state with this name.
    #[must_use]
    pub fn state(&self, name: &str) -> Option<&BehaviourState> {
        self.states.iter().find(|state| state.name == name)
    }

    /// Everything wrong with this machine, as readable lines.
    ///
    /// # Why this exists
    ///
    /// Every fault here produces a machine that *runs* and is quietly wrong. A transition naming a
    /// state that does not exist simply never fires, which looks like an AI that will not chase you
    /// rather than like a typo — and that is the hardest kind of defect to attribute. So they are
    /// named, the same way `AnimationClip::problems` names its own.
    #[must_use]
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();

        if self.states.is_empty() {
            problems.push("the machine has no states, so nothing will ever happen".to_string());
            return problems;
        }
        if !self.initial.is_empty() && self.state(&self.initial).is_none() {
            problems.push(format!(
                "the initial state `{}` is not one of the machine's states; it has {}",
                self.initial,
                self.names().join(", ")
            ));
        }

        for state in &self.states {
            if self
                .states
                .iter()
                .filter(|other| other.name == state.name)
                .count()
                > 1
            {
                problems.push(format!(
                    "two states are both called `{}`; the first one wins and the second is \
                     unreachable",
                    state.name
                ));
            }
            for transition in &state.transitions {
                if self.state(&transition.to).is_none() {
                    problems.push(format!(
                        "state `{}` has a transition to `{}`, which is not a state; it will never \
                         fire. The states are {}",
                        state.name,
                        transition.to,
                        self.names().join(", ")
                    ));
                }
            }
        }

        problems.sort();
        problems.dedup();
        problems
    }

    /// Every state's name, in authored order.
    fn names(&self) -> Vec<String> {
        self.states.iter().map(|state| state.name.clone()).collect()
    }
}

/// Where a machine currently is.
///
/// # Written every tick, never authored
///
/// ADR 0037's `CharacterController` / `CharacterMotion` split for the third time, and the test is the
/// same: [`BehaviourMachine`] is what a person types into a scene file and this is what a person has
/// no business typing.
///
/// Hashed, because which state a monster is in is gameplay and a save has to restore it.
#[derive(Debug, Clone, PartialEq, Default, StableHash, Reflect)]
pub struct Behaviour {
    /// The current state's name. Empty until the first tick, then always one of the machine's.
    pub state: String,
    /// How long it has been in that state, in seconds.
    #[reflect(min = 0.0, max = 86400.0, unit = "s")]
    pub elapsed: f32,
}

impl Component for Behaviour {}

/// What a game's own systems know about the world, by name.
///
/// The module never writes one and has no idea what any of them mean. `"sees_player"`,
/// `"heard_something"`, `"at_waypoint"` — each is one line of a game's own Rust, where it can be
/// typed, tested and read (ADR 0068 §2).
///
/// # Hashed rather than derived, and the reason is latching
///
/// Most facts are recomputed every tick and could be derived. Some are not: **"has seen the player at
/// least once"** has to survive, and a derived map could not hold it. Hashing costs a game that
/// recomputes everything a little for nothing, and the alternative loses a real capability.
#[derive(Debug, Clone, PartialEq, Default, StableHash, Reflect)]
pub struct Facts {
    /// What is true right now.
    ///
    /// An unknown fact is **false**, which is what makes a machine referring to a fact the game has
    /// not written yet inert rather than broken.
    pub known: BTreeMap<String, bool>,
}

impl Facts {
    /// Sets a fact.
    pub fn set(&mut self, name: &str, value: bool) {
        self.known.insert(name.to_string(), value);
    }

    /// Whether a fact is true. An unknown fact is false.
    #[must_use]
    pub fn is(&self, name: &str) -> bool {
        self.known.get(name).copied().unwrap_or(false)
    }
}

impl Component for Facts {}

/// A machine moved from one state to another.
///
/// Past tense, because it is a fact rather than a request (`CLAUDE.md` §6). This is the "on enter"
/// hook without a callback: a game plays a roar on entering `pursue` by reading these, and execution
/// order stays explicit (ADR 0059's split).
#[derive(Debug, Clone, PartialEq, Eq, StableHash, Reflect)]
pub struct BehaviourChanged {
    /// Whose machine moved.
    pub entity: Entity,
    /// The state it left. Empty on the very first tick, when it was in none.
    pub from: String,
    /// The state it entered.
    pub to: String,
}

impl Event for BehaviourChanged {}

/// Registers the components, the event, and the system.
///
/// # Errors
///
/// [`RegistryError`] if any of the components is already registered under a different type.
pub fn install(app: &mut App) -> Result<(), RegistryError> {
    app.register_component::<BehaviourMachine>()?;
    app.register_component::<Behaviour>()?;
    app.register_component::<Facts>()?;
    app.register_event::<BehaviourChanged>();

    // `Simulation`, because deciding what a monster is doing *is* gameplay: it feeds movement in the
    // same tick, it is hashed, and a replay reproduces it.
    //
    // **No ordering declared against whatever writes the facts**, and that is the game's call rather
    // than an omission. A game that wants this tick's perception says `.before(RUN_BEHAVIOURS)` on
    // its own system; a game happy with last tick's says nothing. Naming a label this module cannot
    // see would couple it to one game's system names.
    app.add_system(Stage::Simulation, system(RUN_BEHAVIOURS, run_behaviours));
    Ok(())
}

/// Advances every machine by one tick.
///
/// **One transition per tick, at most.** A machine that chained transitions within a tick could
/// traverse its whole graph in one frame, which makes "what state is it in" depend on the shape of
/// the graph rather than on time — and makes a cycle an infinite loop rather than an oscillation
/// somebody can see.
pub fn run_behaviours(world: &mut World) {
    let machines: Vec<(Entity, BehaviourMachine, Behaviour, Facts)> = world
        .query::<(&BehaviourMachine, &Behaviour, &Facts)>()
        .map(|(entity, (machine, behaviour, facts))| {
            (entity, machine.clone(), behaviour.clone(), facts.clone())
        })
        .collect();

    let mut moved: Vec<(Entity, Behaviour, Option<BehaviourChanged>)> = Vec::new();

    for (entity, machine, mut behaviour, facts) in machines {
        // The first tick, or a state that has stopped existing because the machine was edited.
        if machine.state(&behaviour.state).is_none() {
            let start = machine
                .state(&machine.initial)
                .or_else(|| machine.states.first());
            let Some(start) = start else {
                continue;
            };
            let change = BehaviourChanged {
                entity,
                from: behaviour.state.clone(),
                to: start.name.clone(),
            };
            moved.push((
                entity,
                Behaviour {
                    state: start.name.clone(),
                    elapsed: 0.0,
                },
                Some(change),
            ));
            continue;
        }

        behaviour.elapsed += FIXED_DT;

        let current = machine.state(&behaviour.state).expect("just checked");
        // **Authored order, first match wins.** A pure function of the file rather than of a search.
        let taken = current.transitions.iter().find(|transition| {
            machine.state(&transition.to).is_some()
                && (transition.when.is_empty() || facts.is(&transition.when))
                && (transition.unless.is_empty() || !facts.is(&transition.unless))
                && behaviour.elapsed >= transition.after
        });

        match taken {
            Some(transition) => {
                let change = BehaviourChanged {
                    entity,
                    from: behaviour.state.clone(),
                    to: transition.to.clone(),
                };
                moved.push((
                    entity,
                    Behaviour {
                        state: transition.to.clone(),
                        elapsed: 0.0,
                    },
                    Some(change),
                ));
            }
            None => moved.push((entity, behaviour, None)),
        }
    }

    let mut changes = Vec::new();
    for (entity, behaviour, change) in moved {
        world.insert(entity, behaviour);
        if let Some(change) = change {
            changes.push(change);
        }
    }
    for change in changes {
        world.send_event(change);
    }
}
