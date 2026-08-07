//! System schedules and the deterministic ordering that makes them reproducible.

use crate::profile::Profiler;
use amadeo_ecs::World;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::Instant;

/// When during a tick a system runs.
///
/// `Render` and `Present` are outside the deterministic zone: they may be skipped entirely (headless
/// mode, invariant I7) or run at a different rate from simulation. Systems in those stages may read
/// world state but must never write it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// Input sampling and anything that must be settled before gameplay runs.
    PreSimulation,
    /// Gameplay. The bulk of every game's systems.
    Simulation,
    /// Cleanup, derived state, transform propagation.
    PostSimulation,
    /// Drawing. Read-only with respect to simulation state.
    Render,
    /// Buffer swap and presentation.
    Present,
}

impl Stage {
    /// The stages that make up one simulation tick, in order.
    ///
    /// Deliberately excludes `Render` and `Present`, so a headless run executes exactly this list
    /// and nothing else.
    pub const SIMULATION_STAGES: [Stage; 3] = [
        Stage::PreSimulation,
        Stage::Simulation,
        Stage::PostSimulation,
    ];

    /// Every stage, in the order they run.
    pub const ALL: [Stage; 5] = [
        Stage::PreSimulation,
        Stage::Simulation,
        Stage::PostSimulation,
        Stage::Render,
        Stage::Present,
    ];

    /// This stage's name, as it is written in text and on the wire.
    ///
    /// Spelled out rather than derived from `Debug`, because `Debug` output is a diagnostic
    /// convenience that is allowed to change and this is part of the protocol.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Stage::PreSimulation => "PreSimulation",
            Stage::Simulation => "Simulation",
            Stage::PostSimulation => "PostSimulation",
            Stage::Render => "Render",
            Stage::Present => "Present",
        }
    }

    /// Looks a stage up by name. Case-sensitive, matching [`Stage::name`].
    #[must_use]
    pub fn from_name(name: &str) -> Option<Stage> {
        Stage::ALL.into_iter().find(|stage| stage.name() == name)
    }

    /// Whether this stage runs inside the deterministic zone.
    #[must_use]
    pub fn is_deterministic(self) -> bool {
        matches!(
            self,
            Stage::PreSimulation | Stage::Simulation | Stage::PostSimulation
        )
    }
}

/// What can go wrong when resolving system order.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScheduleError {
    /// Two systems were registered under the same label.
    #[error(
        "duplicate system label '{label}' in stage {stage:?}; labels must be unique within a stage"
    )]
    DuplicateLabel {
        /// The repeated label.
        label: String,
        /// Where the collision happened.
        stage: Stage,
    },

    /// A system referenced a label that does not exist in its stage.
    #[error(
        "system '{system}' in stage {stage:?} declares an ordering against '{missing}', which is not \
         registered in that stage; check for a typo or a system registered in a different stage"
    )]
    UnknownLabel {
        /// The system holding the bad reference.
        system: String,
        /// The label that could not be found.
        missing: String,
        /// Where the reference was declared.
        stage: Stage,
    },

    /// Ordering constraints form a cycle, so no valid order exists.
    #[error(
        "ordering cycle in stage {stage:?} among these systems: {involved}; one of the before/after \
         constraints has to give"
    )]
    Cycle {
        /// Where the cycle is.
        stage: Stage,
        /// The systems that could not be ordered, comma separated.
        involved: String,
    },
}

/// A system plus its ordering constraints.
///
/// Built with [`system`] and the fluent [`SystemConfig::before`] / [`SystemConfig::after`] methods.
pub struct SystemConfig {
    label: &'static str,
    before: Vec<&'static str>,
    after: Vec<&'static str>,
    run: Box<dyn FnMut(&mut World)>,
}

impl fmt::Debug for SystemConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Hand-written because a boxed closure has no Debug. Prints the parts that matter for
        // diagnosing an ordering problem.
        f.debug_struct("SystemConfig")
            .field("label", &self.label)
            .field("before", &self.before)
            .field("after", &self.after)
            .finish_non_exhaustive()
    }
}

impl SystemConfig {
    /// Declares that this system runs before `label`.
    #[must_use]
    pub fn before(mut self, label: &'static str) -> Self {
        self.before.push(label);
        self
    }

    /// Declares that this system runs after `label`.
    #[must_use]
    pub fn after(mut self, label: &'static str) -> Self {
        self.after.push(label);
        self
    }

    /// This system's label.
    #[must_use]
    pub fn label(&self) -> &'static str {
        self.label
    }
}

/// Wraps a function into a labelled system.
///
/// The label is how other systems refer to this one in ordering constraints, and how it appears in
/// diagnostics and in the agent-facing schedule listing.
///
/// ```
/// use amadeo_app::{Stage, system};
/// use amadeo_ecs::World;
///
/// fn apply_gravity(_world: &mut World) {}
///
/// let config = system("apply_gravity", apply_gravity).after("sample_input");
/// assert_eq!(config.label(), "apply_gravity");
/// ```
pub fn system(label: &'static str, run: impl FnMut(&mut World) + 'static) -> SystemConfig {
    SystemConfig {
        label,
        before: Vec::new(),
        after: Vec::new(),
        run: Box::new(run),
    }
}

/// The systems belonging to one stage, plus their resolved execution order.
pub struct Schedule {
    stage: Stage,
    systems: Vec<SystemConfig>,
    /// Indices into `systems`, in execution order. `None` when a system has been added since the
    /// last resolve.
    order: Option<Vec<usize>>,
}

impl fmt::Debug for Schedule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Schedule")
            .field("stage", &self.stage)
            .field("systems", &self.systems)
            .field("resolved", &self.order.is_some())
            .finish()
    }
}

impl Schedule {
    /// Creates an empty schedule for a stage.
    #[must_use]
    pub fn new(stage: Stage) -> Self {
        Self {
            stage,
            systems: Vec::new(),
            order: None,
        }
    }

    /// Adds a system, invalidating any previously resolved order.
    pub fn add(&mut self, config: SystemConfig) {
        self.systems.push(config);
        self.order = None;
    }

    /// How many systems are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.systems.len()
    }

    /// Whether this schedule has no systems.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.systems.is_empty()
    }

    /// Whether a label is already registered in this stage.
    ///
    /// Exists for **shared prerequisites between modules**. `amadeo_character::install` and
    /// `amadeo_terrain::install` both need `step_physics` to run, and both used to register it
    /// unconditionally — so a game using a character *and* terrain, which is the ordinary case for an
    /// open world, failed at startup with `DuplicateLabel`. Neither module can reasonably be the one
    /// that owns it, and a game having to know which was which would be exactly the coupling
    /// `install` exists to remove.
    ///
    /// So each asks first. Checking rather than making [`Schedule::add`] idempotent is deliberate: a
    /// genuine label collision between two *different* systems is a real bug and must stay an error.
    #[must_use]
    pub fn contains(&self, label: &str) -> bool {
        self.systems.iter().any(|config| config.label == label)
    }

    /// The system labels in execution order, or an error if the constraints are unsatisfiable.
    ///
    /// Exposed for diagnostics and for the agent-facing schedule listing.
    pub fn resolved_labels(&mut self) -> Result<Vec<&'static str>, ScheduleError> {
        self.resolve()?;
        let order = self.order.as_ref().expect("resolve succeeded");
        Ok(order.iter().map(|&i| self.systems[i].label).collect())
    }

    /// Runs every system in this stage, in resolved order, timing each one.
    ///
    /// # The clock read is safe, and ADR 0009 is why
    ///
    /// `CLAUDE.md` trap 2 forbids `Instant::now()` in gameplay, and this reads it twice per system.
    /// What makes it safe is that the result goes into [`Profiler`](crate::Profiler), which is a
    /// **service** — structurally outside the state hash, so nothing recorded here can reach a
    /// replay, a snapshot or a hash. ADR 0040 has the full argument.
    ///
    /// A world with no profiler installed pays for neither the clock reads nor the lookup, which is
    /// what keeps this honest for anything constructing a `World` directly rather than through
    /// [`App`](crate::App).
    pub fn run(&mut self, world: &mut World) -> Result<(), ScheduleError> {
        self.resolve()?;
        // Cloned so the borrow of `self.order` ends before `self.systems` is borrowed mutably.
        let order = self.order.clone().expect("resolve succeeded");

        // Checked once per stage rather than once per system: the answer cannot change while the
        // stage runs, and a service lookup per system would cost more than the timing it guards.
        if !world.has_service::<Profiler>() {
            for index in order {
                (self.systems[index].run)(world);
            }
            return Ok(());
        }

        for index in order {
            let label = self.systems[index].label;
            let started = Instant::now();
            (self.systems[index].run)(world);
            let elapsed = started.elapsed();
            // Taken and put back around the *record* rather than held across the system: a system
            // is handed the whole world and would otherwise find the profiler missing from it.
            if let Some(profiler) = world.service_mut::<Profiler>() {
                profiler.record(label, elapsed);
            }
        }
        Ok(())
    }

    /// Computes execution order if it is not already cached.
    fn resolve(&mut self) -> Result<(), ScheduleError> {
        if self.order.is_some() {
            return Ok(());
        }
        self.order = Some(self.topological_order()?);
        Ok(())
    }

    /// Orders systems by their constraints, breaking ties **alphabetically by label**.
    ///
    /// # Why alphabetical rather than registration order
    ///
    /// Registration order depends on which module registered first, which depends on plugin setup
    /// order. Using it would make execution order — and therefore simulation results — sensitive to
    /// how the app was assembled, which is the trap invariant I3 warns about. Sorting unconstrained
    /// systems by label makes the schedule a pure function of *what* is registered, never of *when*.
    ///
    /// The consequence worth knowing: adding a system with no constraints can change the relative
    /// order of other unconstrained systems. If order matters, declare it.
    fn topological_order(&self) -> Result<Vec<usize>, ScheduleError> {
        let mut index_by_label: BTreeMap<&'static str, usize> = BTreeMap::new();
        for (index, config) in self.systems.iter().enumerate() {
            if index_by_label.insert(config.label, index).is_some() {
                return Err(ScheduleError::DuplicateLabel {
                    label: config.label.to_string(),
                    stage: self.stage,
                });
            }
        }

        // Edge from -> to means `from` must run first. BTreeSet keeps successor iteration ordered.
        let mut successors: BTreeMap<&'static str, BTreeSet<&'static str>> = BTreeMap::new();
        let mut incoming_count: BTreeMap<&'static str, usize> = index_by_label
            .keys()
            .map(|label| (*label, 0usize))
            .collect();

        let mut add_edge =
            |from: &'static str, to: &'static str, incoming: &mut BTreeMap<_, usize>| {
                if successors.entry(from).or_default().insert(to) {
                    *incoming.entry(to).or_insert(0) += 1;
                }
            };

        for config in &self.systems {
            for target in &config.before {
                if !index_by_label.contains_key(target) {
                    return Err(ScheduleError::UnknownLabel {
                        system: config.label.to_string(),
                        missing: (*target).to_string(),
                        stage: self.stage,
                    });
                }
                add_edge(config.label, target, &mut incoming_count);
            }
            for target in &config.after {
                if !index_by_label.contains_key(target) {
                    return Err(ScheduleError::UnknownLabel {
                        system: config.label.to_string(),
                        missing: (*target).to_string(),
                        stage: self.stage,
                    });
                }
                add_edge(target, config.label, &mut incoming_count);
            }
        }

        // Kahn's algorithm. The ready set is a BTreeSet, so whenever several systems are eligible the
        // alphabetically first one is chosen -- that is what makes the result independent of
        // registration order.
        let mut ready: BTreeSet<&'static str> = incoming_count
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(label, _)| *label)
            .collect();

        let mut order = Vec::with_capacity(self.systems.len());
        while let Some(label) = ready.pop_first() {
            order.push(index_by_label[label]);
            if let Some(next) = successors.get(label) {
                for successor in next {
                    let count = incoming_count
                        .get_mut(successor)
                        .expect("successor was registered");
                    *count -= 1;
                    if *count == 0 {
                        ready.insert(*successor);
                    }
                }
            }
        }

        if order.len() != self.systems.len() {
            // Whatever never reached indegree zero is part of a cycle.
            let mut involved: Vec<&str> = incoming_count
                .iter()
                .filter(|(_, count)| **count > 0)
                .map(|(label, _)| *label)
                .collect();
            involved.sort_unstable();
            return Err(ScheduleError::Cycle {
                stage: self.stage,
                involved: involved.join(", "),
            });
        }

        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records the order systems ran in, via a shared counter resource.
    fn noop(_world: &mut World) {}

    fn labels(schedule: &mut Schedule) -> Vec<&'static str> {
        schedule.resolved_labels().expect("resolvable")
    }

    #[test]
    fn unconstrained_systems_run_alphabetically() {
        let mut schedule = Schedule::new(Stage::Simulation);
        // Registered in deliberately non-alphabetical order.
        schedule.add(system("zebra", noop));
        schedule.add(system("apple", noop));
        schedule.add(system("mango", noop));

        assert_eq!(labels(&mut schedule), vec!["apple", "mango", "zebra"]);
    }

    #[test]
    fn registration_order_does_not_affect_result() {
        // The property that matters: assembling the same systems in a different order must produce
        // the same schedule, or simulation results would depend on plugin setup order.
        let mut forward = Schedule::new(Stage::Simulation);
        forward.add(system("a", noop));
        forward.add(system("b", noop).after("a"));
        forward.add(system("c", noop).after("b"));

        let mut backward = Schedule::new(Stage::Simulation);
        backward.add(system("c", noop).after("b"));
        backward.add(system("b", noop).after("a"));
        backward.add(system("a", noop));

        assert_eq!(labels(&mut forward), labels(&mut backward));
        assert_eq!(labels(&mut forward), vec!["a", "b", "c"]);
    }

    #[test]
    fn after_constraint_is_respected() {
        let mut schedule = Schedule::new(Stage::Simulation);
        // Alphabetically "apply" precedes "sample", so only the constraint can flip them.
        schedule.add(system("apply", noop).after("sample"));
        schedule.add(system("sample", noop));

        assert_eq!(labels(&mut schedule), vec!["sample", "apply"]);
    }

    #[test]
    fn before_constraint_is_respected() {
        let mut schedule = Schedule::new(Stage::Simulation);
        schedule.add(system("zulu", noop).before("alpha"));
        schedule.add(system("alpha", noop));

        assert_eq!(labels(&mut schedule), vec!["zulu", "alpha"]);
    }

    #[test]
    fn before_and_after_can_be_combined() {
        let mut schedule = Schedule::new(Stage::Simulation);
        schedule.add(system("middle", noop).after("first").before("last"));
        schedule.add(system("last", noop));
        schedule.add(system("first", noop));

        assert_eq!(labels(&mut schedule), vec!["first", "middle", "last"]);
    }

    #[test]
    fn duplicate_labels_are_rejected() {
        let mut schedule = Schedule::new(Stage::Simulation);
        schedule.add(system("same", noop));
        schedule.add(system("same", noop));

        assert_eq!(
            schedule.resolved_labels(),
            Err(ScheduleError::DuplicateLabel {
                label: "same".to_string(),
                stage: Stage::Simulation,
            })
        );
    }

    #[test]
    fn unknown_label_reference_is_rejected_with_context() {
        let mut schedule = Schedule::new(Stage::Simulation);
        schedule.add(system("real", noop).after("imaginary"));

        let error = schedule.resolved_labels().expect_err("should fail");
        assert_eq!(
            error,
            ScheduleError::UnknownLabel {
                system: "real".to_string(),
                missing: "imaginary".to_string(),
                stage: Stage::Simulation,
            }
        );
        // The message must be actionable without a debugger -- both authors read these.
        let text = error.to_string();
        assert!(text.contains("real"), "{text}");
        assert!(text.contains("imaginary"), "{text}");
        assert!(text.contains("typo"), "{text}");
    }

    #[test]
    fn cycles_are_reported_with_the_systems_involved() {
        let mut schedule = Schedule::new(Stage::Simulation);
        schedule.add(system("a", noop).after("b"));
        schedule.add(system("b", noop).after("a"));

        let error = schedule.resolved_labels().expect_err("should fail");
        match error {
            ScheduleError::Cycle { stage, involved } => {
                assert_eq!(stage, Stage::Simulation);
                assert!(
                    involved.contains('a') && involved.contains('b'),
                    "{involved}"
                );
            }
            other => panic!("expected a cycle error, got {other:?}"),
        }
    }

    #[test]
    fn longer_cycles_are_detected() {
        let mut schedule = Schedule::new(Stage::Simulation);
        schedule.add(system("a", noop).after("c"));
        schedule.add(system("b", noop).after("a"));
        schedule.add(system("c", noop).after("b"));
        assert!(schedule.resolved_labels().is_err());
    }

    #[test]
    fn duplicate_constraints_are_harmless() {
        let mut schedule = Schedule::new(Stage::Simulation);
        schedule.add(system("b", noop).after("a").after("a"));
        schedule.add(system("a", noop));
        assert_eq!(labels(&mut schedule), vec!["a", "b"]);
    }

    #[test]
    fn systems_actually_run_in_resolved_order() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let log = Rc::new(RefCell::new(Vec::new()));
        let mut schedule = Schedule::new(Stage::Simulation);

        for label in ["third", "first", "second"] {
            let log = Rc::clone(&log);
            let mut config = system(label, move |_world: &mut World| {
                log.borrow_mut().push(label);
            });
            config = match label {
                "second" => config.after("first"),
                "third" => config.after("second"),
                _ => config,
            };
            schedule.add(config);
        }

        let mut world = World::new();
        schedule.run(&mut world).expect("resolvable");
        assert_eq!(*log.borrow(), vec!["first", "second", "third"]);
    }

    #[test]
    fn empty_schedule_runs_cleanly() {
        let mut schedule = Schedule::new(Stage::Simulation);
        assert!(schedule.is_empty());
        assert_eq!(schedule.len(), 0);
        let mut world = World::new();
        assert!(schedule.run(&mut world).is_ok());
    }

    #[test]
    fn adding_a_system_invalidates_the_cached_order() {
        let mut schedule = Schedule::new(Stage::Simulation);
        schedule.add(system("b", noop));
        assert_eq!(labels(&mut schedule), vec!["b"]);

        schedule.add(system("a", noop));
        assert_eq!(labels(&mut schedule), vec!["a", "b"]);
    }

    #[test]
    fn render_stages_are_outside_the_deterministic_zone() {
        assert!(Stage::Simulation.is_deterministic());
        assert!(Stage::PreSimulation.is_deterministic());
        assert!(Stage::PostSimulation.is_deterministic());
        assert!(!Stage::Render.is_deterministic());
        assert!(!Stage::Present.is_deterministic());

        // A headless tick must execute exactly the deterministic stages.
        assert!(
            Stage::SIMULATION_STAGES
                .iter()
                .all(|s| s.is_deterministic())
        );
        assert_eq!(Stage::SIMULATION_STAGES.len(), 3);
    }
}
