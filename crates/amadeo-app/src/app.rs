//! The application: a world, its schedules, and the fixed-timestep loop that drives them.

use crate::profile::Profiler;
use crate::schedule::{Schedule, ScheduleError, Stage, SystemConfig};
use amadeo_assets::{Assets, ScanError};
use amadeo_core::{FIXED_DT_NANOS, Rng, StableHash, Tick};
use amadeo_ecs::{Commands, Component, ComponentRegistry, Resource, Service, World};
use amadeo_events::{Event, WorldEvents};
use amadeo_reflect::{
    FieldInfo, Reflect, ReflectError, RegistryError, Replication, TypeInfo, TypeKind, Value,
};
use amadeo_render::{
    ArchMesh, BoxMesh, Camera, CylinderMesh, Environment, EnvironmentCache, GltfPart, Material,
    MaterialCache, Mesh, MeshCache, MeshData, PlaneMesh, SphereMesh, StairMesh, Vertex, WedgeMesh,
};
use amadeo_scene::PrefabLibrary;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The simulation's random number generator, seeded once and advanced deterministically.
///
/// A resource rather than a global, so its state is part of [`World::state_hash`] — two runs that
/// have consumed different numbers of random values have diverged, and that must be detectable.
///
/// Systems needing randomness should [`Rng::fork`] a child stream rather than sharing this one
/// directly, so that system execution order cannot influence the values any single system receives.
///
/// # How its state is hashed, and what used to be wrong with it
///
/// The two words behind [`Rng::state`] are hashed directly. That is worth a note because it did
/// not used to be: this impl previously hashed `format!("{:?}", rng)`, on the reasoning that a
/// derived `Debug` is a faithful function of the fields. It is — but it made **every committed
/// replay depend on the exact text of a `Debug` impl**, so renaming a private field or adding
/// `#[derive]` ordering would have invalidated all of them for a reason no one would ever connect
/// to the failure. Hashing the state directly removes that coupling entirely.
#[derive(Debug, Clone, PartialEq, Eq, StableHash)]
pub struct SimRng(pub Rng);

impl Resource for SimRng {}

/// One `u64` field of [`SimRng`]'s schema.
///
/// A small helper because `FieldInfo` has no constructor — the derive builds it literally, and this
/// is the one place in the engine that writes a schema by hand.
fn raw_state_field(name: &str, docs: &str) -> FieldInfo {
    FieldInfo {
        name: name.to_string(),
        type_name: "u64".to_string(),
        docs: docs.to_string(),
        range: None,
        unit: None,
        replication: Replication::default(),
    }
}

impl Reflect for SimRng {
    /// Hand-written rather than derived, because the two words it exposes are private to `Rng` and
    /// reached through [`Rng::state`] — see the note there for why `Rng` itself cannot be
    /// `Reflect` (invariant I6: `amadeo-reflect` sits above `amadeo-core`).
    const STATIC_NAME: &'static str = "SimRng";

    fn type_name() -> String {
        Self::STATIC_NAME.to_string()
    }

    fn type_info() -> TypeInfo {
        TypeInfo {
            name: Self::type_name(),
            docs: "The simulation's random number generator. Both fields are the generator's raw \
                   internal state; editing either changes every random value from here on."
                .to_string(),
            version: 1,
            kind: TypeKind::Struct {
                fields: vec![
                    raw_state_field("state", "The evolving LCG state. Advances on every draw."),
                    raw_state_field(
                        "increment",
                        "The stream selector, always odd. Two generators with different \
                         increments produce unrelated sequences from the same seed.",
                    ),
                ],
            },
        }
    }

    fn to_value(&self) -> Value {
        let [state, increment] = self.0.state();
        Value::structure([
            ("state", Value::U64(state)),
            ("increment", Value::U64(increment)),
        ])
    }

    fn from_value(value: &Value) -> Result<Self, ReflectError> {
        let Value::Struct(fields) = value else {
            return Err(ReflectError::mismatch("SimRng", "struct", value));
        };

        let read = |name: &str| -> Result<u64, ReflectError> {
            let field = fields.get(name).ok_or_else(|| ReflectError::MissingField {
                type_name: "SimRng".to_string(),
                field: name.to_string(),
                required: "state, increment".to_string(),
            })?;
            u64::from_value(field)
        };

        let state = read("state")?;
        let increment = read("increment")?;
        Ok(SimRng(Rng::from_state([state, increment])))
    }
}

/// Whether gameplay is suspended — ADR 0065.
///
/// # What it does
///
/// While `paused` is true, [`App::step`] skips every system in `Simulation` and `PostSimulation`
/// **except** those registered with [`SystemConfig::while_paused`]. `PreSimulation` still runs in
/// full, because input has to be sampled or nothing could ever unpause. `Render` and `Present` are
/// untouched, because the menu has to be drawn.
///
/// **The tick keeps advancing.** That is deliberate and load-bearing: menu navigation is hashed
/// state driven by hashed input, and `amadeo-input` records input per tick, so a frozen tick would
/// leave a keypress in a menu with nowhere in a replay to live. It also means `advance_real_time`
/// keeps consuming its backlog on cheap paused ticks, so there is nothing banked to burst through on
/// unpause.
///
/// # Why it is hashed rather than a service
///
/// Whether you are paused is gameplay state: a save should restore it, and a replay must reproduce
/// it or two runs disagree about which systems ran. Being an ordinary reflected resource is also
/// what makes it visible to `amadeo query` and restorable from a snapshot, with nothing built.
///
/// # The engine never writes it
///
/// A game decides that Escape means pause — invariant I4 one level up, the same split ADR 0061 uses
/// for footsteps and ADR 0063 for buttons. A game with no pause never inserts this, and pays one
/// `Option` lookup per tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct Paused {
    /// True while the gameplay stages are being skipped.
    pub paused: bool,
}

impl Resource for Paused {}

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
    /// Assets whose file names a component it then failed to build, by id.
    ///
    /// See [`App::asset_problems`]. Ordered, so reporting it is reproducible (invariant I3).
    asset_problems: BTreeMap<String, String>,
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
        // Also installed by default, so `profile.frame` always has something to report and a game
        // never has to opt in to being measurable. A service, so nothing it records can reach the
        // state hash (ADR 0040, ADR 0009).
        world.insert_service(Profiler::new());
        Self {
            world,
            registry: ComponentRegistry::new(),
            schedules: BTreeMap::new(),
            event_swaps: Vec::new(),
            accumulated_nanos: 0,
            seed,
            asset_problems: BTreeMap::new(),
        }
    }

    /// Assets whose file names a component it then failed to build, and why. In id order.
    ///
    /// # Why this exists, and what it is not
    ///
    /// **Not** a list of missing assets — those are ADR 0021's business and are survivable by
    /// design: a texture that has not loaded draws a placeholder, a mesh that has not loaded draws
    /// nothing, and neither is worth complaining about. This is the narrower and much more
    /// actionable case: a file that **says** it holds a `Material` and does not hold a valid one.
    ///
    /// That is nearly always one of two things — a typo'd field name, or a field the component has
    /// grown since the file was written (**Q32**). Both are fixed in seconds once you know which
    /// file, and were previously invisible: the asset was skipped in silence, and whatever depended
    /// on it failed later somewhere unrelated. When `Environment` gained a `sky` field, every
    /// `.environment` file stopped parsing and the symptom was a *missing service* three layers away.
    ///
    /// Empty is the normal state. A game that wants to be loud about it can print this at startup;
    /// `games/scarp` does.
    pub fn asset_problems(&self) -> impl Iterator<Item = (&str, &str)> {
        self.asset_problems
            .iter()
            .map(|(id, reason)| (id.as_str(), reason.as_str()))
    }

    /// Whether any asset named a component it could not build.
    #[must_use]
    pub fn has_asset_problems(&self) -> bool {
        !self.asset_problems.is_empty()
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
    /// `Transform` and `Parent`, so the schema describes that game rather than everything the
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

    /// Takes the registry out, leaving an empty one behind.
    ///
    /// Only for the handful of operations that need the world **mutably** and the registry
    /// **shared** at the same time — restoring a snapshot is the one caller today. `App` owns both,
    /// so the borrow checker refuses the obvious spelling; this is the same take-and-put-back shape
    /// `World::with_service_taken` uses, and it must always be paired with [`App::put_registry`].
    ///
    /// Deliberately not public API for games: an unpaired call would leave the app unable to
    /// describe its own components. [`App::capture_snapshot`] and [`App::restore_snapshot`] are the
    /// paired forms a game actually wants.
    pub(crate) fn take_registry(&mut self) -> ComponentRegistry {
        std::mem::take(&mut self.registry)
    }

    /// Puts a registry taken by [`App::take_registry`] back.
    pub(crate) fn put_registry(&mut self, registry: ComponentRegistry) {
        self.registry = registry;
    }

    /// Captures the whole world to a snapshot (ADR 0028).
    ///
    /// # Why this is on `App` rather than called directly
    ///
    /// `amadeo_snapshot::capture` needs the component registry, and `App` owns it — so every caller
    /// outside this crate had to build an empty registry — which silently produces a snapshot that
    /// restores to a *different* world and then fails its own hash check — because the registry
    /// accessor is crate-private, an unpaired call to it leaving the app unable to describe itself.
    ///
    /// Found by writing a game test for terrain edits surviving a save: there was no supported way
    /// for a **game** to save or load at all, which is a gap worth closing on its own — M3's build
    /// list has "save/load built on snapshots" and this is what it stands on.
    #[must_use]
    pub fn capture_snapshot(&self) -> amadeo_snapshot::Snapshot {
        amadeo_snapshot::capture(&self.world, &self.registry)
    }

    /// Puts a captured world back.
    ///
    /// **A snapshot restores components and resources, never services** (ADR 0009), so a subsystem
    /// caching derived state has to notice and rebuild. `PhysicsBackend::reset` exists for exactly
    /// that reason, and terrain streaming does the same by comparing
    /// `TerrainEdits::revision` — hash equality after a restore is necessary and not sufficient.
    ///
    /// # Errors
    ///
    /// [`amadeo_snapshot::RestoreError`] if the snapshot names a component this build does not have,
    /// holds a value that will not fit its type, or does not hash to what it recorded — the last
    /// being the check that catches a restore into a world that was not what the snapshot expected.
    pub fn restore_snapshot(
        &mut self,
        snapshot: &amadeo_snapshot::Snapshot,
    ) -> Result<(), amadeo_snapshot::RestoreError> {
        // Taken and put back, because `restore` needs the world mutably and the registry shared and
        // this type owns both — the same shape `World::with_service_taken` uses.
        let registry = self.take_registry();
        let result = amadeo_snapshot::restore(&mut self.world, &registry, snapshot);
        self.put_registry(registry);
        result
    }

    /// Puts a **save** back: the same file, read leniently (ADR 0069).
    ///
    /// The difference from [`App::restore_snapshot`] is what happens when the file was written by a
    /// build whose components have since changed shape. A snapshot refuses; a save fills in what
    /// this build expects, drops what it no longer has, applies `redirects` for anything renamed,
    /// and reports every one of those in the returned [`SaveReport`](amadeo_snapshot::SaveReport).
    ///
    /// **When nothing has changed shape this is the strict path, unchanged** — hard errors and the
    /// state hash enforced — so an ordinary load by a player who has not updated loses no checking
    /// at all.
    ///
    /// Everything [`App::restore_snapshot`] says about services applies here too: a restore puts
    /// components and resources back and never a service, so a subsystem caching derived state has
    /// to notice and rebuild.
    ///
    /// # Errors
    ///
    /// [`amadeo_snapshot::RestoreError`] for what leniency cannot explain away — a file whose entity
    /// slots do not add up, and, when the layout matches, anything a snapshot restore would refuse.
    pub fn restore_save(
        &mut self,
        snapshot: &amadeo_snapshot::Snapshot,
        redirects: &amadeo_snapshot::Redirects,
    ) -> Result<amadeo_snapshot::SaveReport, amadeo_snapshot::RestoreError> {
        let registry = self.take_registry();
        let result = amadeo_snapshot::restore_save(&mut self.world, &registry, snapshot, redirects);
        self.put_registry(registry);
        result
    }

    /// Whether a system label is already registered in a stage.
    ///
    /// For **modules that share a prerequisite**. `amadeo_character::install` and
    /// `amadeo_terrain::install` both need `step_physics` in the schedule, and both registering it
    /// is a `DuplicateLabel` error — so a game with a character walking on streamed terrain, which is
    /// the ordinary open-world case, refused to start. Each asks this first and registers only if
    /// nobody has.
    #[must_use]
    pub fn has_system(&self, stage: Stage, label: &str) -> bool {
        self.schedules
            .get(&stage)
            .is_some_and(|schedule| schedule.contains(label))
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

    /// Scans an asset directory and installs the catalogue.
    ///
    /// `relative` is resolved against the nearest `amadeo.toml` rather than against the working
    /// directory, so a game finds the same assets whether it was started by `cargo run`, by the CLI,
    /// or by double-clicking the executable. See `amadeo_assets::resolve` for the full rule and why
    /// it is the marker file rather than an environment variable.
    ///
    /// Called **before** the first tick, and never during one. That is ADR 0021's load barrier: the
    /// simulation never observes an asset arriving, so there is nothing about load timing for it to
    /// branch on and nothing for a replay to disagree about.
    ///
    /// The catalogue goes in as a [`Service`], so it stays out of [`App::state_hash`] by construction
    /// rather than by anyone remembering (ADR 0009).
    ///
    /// # Errors
    ///
    /// [`amadeo_assets::ScanError`] listing every duplicate id, malformed sidecar, and unreadable
    /// path — including a missing asset directory, which is an error rather than an empty catalogue
    /// so that a mistyped path cannot look like a project with no assets.
    pub fn scan_assets(&mut self, relative: impl AsRef<Path>) -> Result<&mut Self, ScanError> {
        let assets = Assets::scan(relative.as_ref())?;
        self.world.insert_service(assets);
        Ok(self)
    }

    /// What the game knows about assets on disk, if a catalogue was installed.
    #[must_use]
    pub fn assets(&self) -> Option<&Assets> {
        self.world.service::<Assets>()
    }

    /// Reads the named assets into memory — ADR 0021's load barrier.
    ///
    /// Call before the first tick. Nothing enforces that today, and nothing needs to: under
    /// ADR 0021's first rule the simulation never observes whether an asset is resident, so a late
    /// load changes what is drawn and nothing that a replay compares.
    ///
    /// A missing asset is **not** an error. It is recorded and the game keeps running, because
    /// ADR 0021 requires a visible stand-in plus a structured report rather than a crash — an agent
    /// whose only eyes are the protocol has to be able to see what is broken and carry on. Ask
    /// [`amadeo_assets::AssetStore::failures`], or `assets.list`, for what went wrong.
    ///
    /// Does nothing when no catalogue was installed.
    pub fn load_assets<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>) -> &mut Self {
        if let Some(assets) = self.world.service_mut::<Assets>() {
            assets.load(ids);
        }
        self
    }

    /// Loads everything a scene declares it needs.
    ///
    /// The barrier in the form a game actually uses it: a scene's `assets` block says what it
    /// requires, and this makes all of it resident before the scene is instantiated.
    pub fn load_scene_assets(&mut self, document: &amadeo_scene::SceneDocument) -> &mut Self {
        let required = document.required_assets();
        self.load_assets(required.iter().map(String::as_str))
    }

    /// Loads a scene's assets, then instantiates it into the world.
    ///
    /// The two halves in the order ADR 0021's barrier requires: everything the scene declares is
    /// resident *before* any entity referring to it exists, so no tick ever runs against a
    /// half-loaded world.
    ///
    /// # Why a game could not do this itself before
    ///
    /// `amadeo_scene::instantiate` needs the world mutably and the registry shared, and `App` owns
    /// both — so the borrow checker refuses the obvious spelling and every game would have had to
    /// rediscover the take-and-put-back workaround. Invariant I1 says text files are the source of
    /// truth; making the *only* path to that awkward was a real gap, and it stood until a game
    /// actually tried to load a scene rather than build its world in code.
    ///
    /// # Errors
    ///
    /// [`InstantiateError`](amadeo_scene::InstantiateError) if any entity names a component this
    /// app has not registered, or gives one a value it cannot hold. **Atomic**: a failure despawns
    /// everything it created, because a half-loaded scene looks like it worked.
    pub fn load_scene(
        &mut self,
        document: &amadeo_scene::SceneDocument,
    ) -> Result<amadeo_scene::Instantiated, amadeo_scene::InstantiateError> {
        self.load_scene_assets(document);

        // Built after loading, from the same ids the barrier just made resident. A prefab that
        // failed to load simply is not here, and `instantiate_with` says which one by name — which
        // is a better message than anything this layer could produce, because it also knows which
        // entity asked for it.
        let prefabs = self.prefab_library(document);

        let registry = self.take_registry();
        let result = amadeo_scene::instantiate_with(document, &registry, &prefabs, &mut self.world);
        self.put_registry(registry);

        // After instantiation, because the ids come off the components the scene just created. A
        // scene that authors none of these does no work here.
        self.load_environments();
        self.load_meshes();
        self.load_materials();
        self.load_clips();
        result
    }

    /// Every non-empty asset id named by a component of type `C`, across the whole world.
    ///
    /// `pick` hands back the ids one component names — an array rather than a single value because
    /// a `Mesh` names two (its geometry and its material) and a `Camera` names one. Empty ids are
    /// dropped, since "empty" is how every one of these fields spells "none".
    fn ids_named_by<C: Component, const N: usize>(
        &self,
        pick: impl Fn(&C) -> [String; N],
    ) -> BTreeSet<String> {
        self.world
            .entities()
            .into_iter()
            .filter_map(|entity| self.world.get::<C>(entity))
            .flat_map(pick)
            .filter(|id| !id.is_empty())
            .collect()
    }

    /// Reads assets whose file is a scene document with a single root carrying one `T`.
    ///
    /// The shape every render asset shares — an `Environment` (ADR 0034), a `Material` (ADR 0033),
    /// and a mesh's procedural shape (ADR 0035). Written once with a type parameter rather than
    /// three times, because three copies of the same thirty lines is where they start to drift.
    ///
    /// # Nothing here is fatal
    ///
    /// An id that does not resolve, will not parse, or holds no `T` is skipped and simply does not
    /// appear in the result. ADR 0021 requires a missing asset to be visible and survivable rather
    /// than a crash, and each caller's cache decides what "absent" looks like — a default material,
    /// or a mesh that draws nothing.
    ///
    /// Ids that name a *different* kind are skipped for the same reason, which is what lets a mesh
    /// asset be read as "a `BoxMesh`, or failing that a `PlaneMesh`" without either attempt being an
    /// error.
    ///
    /// # One of those skips is not like the others, and it is recorded
    ///
    /// A document that **has** a `T` which then fails to build is a different thing entirely from a
    /// document that has no `T`. The first is a file that says `Material` and does not hold a valid
    /// one — a typo'd field, or a field the type has grown since the file was written. The second is
    /// ordinary and expected.
    ///
    /// Only the first is remembered, in [`App::asset_problems`]. **This is Q32's actual cost**: when
    /// `Environment` gained a `sky` field, every `.environment` file stopped parsing, every one was
    /// skipped in silence, and the failure surfaced three layers away as a test complaining that a
    /// *service* had not been installed. Nothing in that message mentioned a field, a file, or a
    /// schema. The churn of adding a field was never the problem; this was.
    /// **Public because a component-shaped asset is a general idea, not a renderer one.** A
    /// `.material`, an `.environment` and a `.theme` are all one component in a scene file, and the
    /// crate that owns each type sits *below* `amadeo-scene` and cannot parse it (I6). This layer
    /// can see both, so this is where the reading happens — and a game with an asset kind the engine
    /// has never heard of can use it too.
    pub fn read_component_assets<T: Component>(
        &mut self,
        wanted: &BTreeSet<String>,
    ) -> Vec<(String, T)> {
        if wanted.is_empty() {
            return Vec::new();
        }

        // The load barrier (ADR 0021): the bytes have to be resident before anything reads them. A
        // scene that declared the id in its `assets` block has already loaded it and this is a
        // no-op; one that forgot gets it loaded here rather than silently rendering without it.
        self.load_assets(wanted.iter().map(String::as_str));

        let mut found = Vec::new();
        for id in wanted {
            // The document is parsed out of the borrow before anything below can want `self`
            // mutably, which recording a problem does. Cloning the text costs a few kilobytes once
            // per asset at load; threading a second pass through here to avoid it would cost a
            // reader's attention every time.
            let document = {
                let Some(assets) = self.assets() else {
                    break;
                };
                let Some(loaded) = assets.store.get(id) else {
                    continue;
                };
                let Ok(text) = std::str::from_utf8(&loaded.bytes) else {
                    continue;
                };
                match amadeo_scene::parse(text) {
                    Ok(document) => document,
                    Err(_) => continue,
                }
            };
            // `walk` rather than indexing the first entity, so a file that wraps the root in a
            // parent still works.
            let Some(value) = document
                .walk()
                .into_iter()
                .find_map(|entity| entity.components.get(T::STATIC_NAME))
            else {
                continue;
            };
            let built = match T::from_value(value) {
                Ok(built) => built,
                Err(error) => {
                    // The one skip worth complaining about — see this function's docs.
                    self.asset_problems.insert(
                        id.clone(),
                        format!(
                            "`{id}` holds a `{}` that could not be read: {error}",
                            T::STATIC_NAME
                        ),
                    );
                    continue;
                }
            };
            // A previous attempt on this id may have failed and been recorded — a mesh asset is
            // tried as a `BoxMesh` and then a `PlaneMesh`, and only the one that matches proves the
            // file is fine. Succeeding clears it.
            self.asset_problems.remove(id);
            found.push((id.clone(), built));
        }
        found
    }

    /// Turns every mesh id a [`Mesh`] names into geometry — ADR 0035.
    ///
    /// A mesh asset carries **either** a procedural shape or vertex data, and both end up as one
    /// [`MeshData`]. The glTF importer is a third producer that changes nothing here except adding a
    /// branch, which is the property ADR 0035 was written to buy.
    ///
    /// **The vertex-data half has never been built, and that is the engine's largest gap.** ADR 0035
    /// promised it; five sessions of renderer work sit on top of an authoring surface whose only
    /// expressible object is an axis-aligned box. Session 20's engine review measured the
    /// consequence: **23 of 23 `.mesh` assets in this repository are `BoxMesh`**, `PlaneMesh` and
    /// `ArchMesh` are used by no game, and every material's texture slots are empty. A renderer with
    /// cascaded shadows, PBR and IBL whose content language has one noun is why the games look the
    /// way they do — see `docs/12-the-bar.md` §3.
    ///
    /// Tessellation happens **here, once**, rather than per frame — the same place ADR 0026 puts
    /// image decoding, and for the same reason.
    pub fn load_meshes(&mut self) -> &mut Self {
        let wanted = self.ids_named_by(|mesh: &Mesh| [mesh.mesh.clone()]);
        if wanted.is_empty() {
            return self;
        }

        // Each shape kind is asked for the same ids; a file holding one is skipped by the others,
        // which is exactly what `read_component_assets` promises about a mismatched kind.
        let mut built: Vec<(String, MeshData)> = Vec::new();
        for (id, shape) in self.read_component_assets::<BoxMesh>(&wanted) {
            built.push((id, shape.tessellate()));
        }
        for (id, shape) in self.read_component_assets::<PlaneMesh>(&wanted) {
            built.push((id, shape.tessellate()));
        }
        // The curved one (session 20). Added because a review measured that every mesh in every game
        // here was an axis-aligned box, which is most of why the result read as a test scene.
        for (id, shape) in self.read_component_assets::<ArchMesh>(&wanted) {
            built.push((id, shape.tessellate()));
        }
        // ADR 0074's parametric set. Each is one more branch here, which is exactly the property
        // ADR 0035 was written to buy and the first time it has been cashed for anything but a box.
        for (id, shape) in self.read_component_assets::<CylinderMesh>(&wanted) {
            built.push((id, shape.tessellate()));
        }
        for (id, shape) in self.read_component_assets::<SphereMesh>(&wanted) {
            built.push((id, shape.tessellate()));
        }
        for (id, shape) in self.read_component_assets::<WedgeMesh>(&wanted) {
            built.push((id, shape.tessellate()));
        }
        for (id, shape) in self.read_component_assets::<StairMesh>(&wanted) {
            built.push((id, shape.tessellate()));
        }
        // The third producer, and the one ADR 0035 predicted: geometry read out of a glTF file
        // (ADR 0039). Nothing above this line changes because of it, which is the property that
        // ADR being written early was for.
        built.extend(self.read_gltf_meshes(&wanted));

        if built.is_empty() {
            return self;
        }
        if !self.world.has_service::<MeshCache>() {
            self.world.insert_service(MeshCache::new());
        }
        if let Some(cache) = self.world.service_mut::<MeshCache>() {
            for (id, data) in built {
                cache.insert(id, data);
            }
        }
        self
    }

    /// Reads geometry out of glTF files for every `.mesh` asset that points into one — ADR 0039.
    ///
    /// # Each source file is parsed once, however many parts name it
    ///
    /// A level exported from Blender is one `.glb` and thirty `.mesh` files pointing into it.
    /// Parsing per part would read and decode the whole file thirty times, so the parts are grouped
    /// by source first. That is the only reason this is not three lines inside
    /// [`App::load_meshes`].
    ///
    /// # Nothing here is fatal
    ///
    /// A part whose source will not resolve, will not parse, or names an index the file does not
    /// have is skipped, and `MeshCache` treats it as a mesh that never loaded — an entity naming it
    /// draws nothing. ADR 0021 requires a missing asset to be survivable, and `MeshCache::get`
    /// explains why a missing *mesh* has no honest stand-in the way a missing texture does.
    fn read_gltf_meshes(&mut self, wanted: &BTreeSet<String>) -> Vec<(String, MeshData)> {
        let parts = self.read_component_assets::<GltfPart>(wanted);
        if parts.is_empty() {
            return Vec::new();
        }

        // The glTF files themselves are assets too, and the barrier applies to them exactly as it
        // does to anything else (ADR 0021): their bytes have to be resident before this reads them.
        let sources: BTreeSet<String> = parts.iter().map(|(_, part)| part.source.clone()).collect();
        self.load_assets(sources.iter().map(String::as_str));

        // Parsed once per source, keyed by id. A `BTreeMap` rather than a hash map for the reason
        // every registry in this engine uses one: iteration order reaches the mesh cache.
        let mut documents: BTreeMap<String, amadeo_gltf::GltfDocument> = BTreeMap::new();
        for source in &sources {
            let Some(assets) = self.assets() else {
                break;
            };
            let Some(loaded) = assets.store.get(source) else {
                continue;
            };
            let Ok(document) = amadeo_gltf::read(&loaded.bytes) else {
                continue;
            };
            documents.insert(source.clone(), document);
        }

        let mut built = Vec::new();
        for (id, part) in parts {
            let Some(document) = documents.get(&part.source) else {
                continue;
            };
            let Some(mesh) = document.meshes.get(part.mesh as usize) else {
                continue;
            };
            let Some(primitive) = mesh.primitives.get(part.primitive as usize) else {
                continue;
            };
            // A file that exports tangents is trusted over anything computed here: it is the frame
            // the model's normal map was baked against (ADR 0047). Only a file that omits them gets
            // a generated frame, and glTF's own spec expects a client to do exactly that.
            //
            // All-or-nothing rather than per-vertex, because a tangent frame is only consistent
            // across a surface if one method produced the whole of it -- mixing two would put a
            // visible lighting seam wherever they met.
            let has_tangents = primitive
                .vertices
                .iter()
                .all(|vertex| vertex.tangent.is_some());

            let mut data = MeshData {
                vertices: primitive
                    .vertices
                    .iter()
                    .map(|vertex| Vertex {
                        position: vertex.position,
                        normal: vertex.normal,
                        uv: vertex.uv,
                        tangent: vertex.tangent.unwrap_or_default(),
                    })
                    .collect(),
                indices: primitive.indices.clone(),
            };

            // **Faceting first, tangents second, and the order is load-bearing** (Q33).
            //
            // `flat_shade` splits every shared vertex so each triangle can carry its own normal.
            // Tangents are averaged over the triangles sharing a vertex — so generating them first
            // and splitting afterwards would copy a frame that had been smoothed across edges this
            // has just decided are sharp, leaving the tangent basis smooth where the normals are not.
            //
            // It also discards the file's own tangents, deliberately: they were baked against the
            // smooth normals that just went away, so they no longer describe this surface.
            let flat = part.flat;
            if flat {
                data.flat_shade();
            }
            if flat || !has_tangents {
                data.generate_tangents();
            }

            built.push((id, data));
        }
        built
    }

    /// Turns every material id a [`Mesh`] names into the material behind it — ADR 0033.
    ///
    /// **Only materials named by a `Mesh` that exists right now.** For anything spawned later, see
    /// [`App::load_material`].
    pub fn load_materials(&mut self) -> &mut Self {
        let wanted = self.ids_named_by(|mesh: &Mesh| [mesh.material.clone()]);
        self.load_material_ids(wanted)
    }

    /// Loads one material by id, whether or not anything names it yet.
    ///
    /// # Why this is needed at all
    ///
    /// [`App::load_materials`] scans the `Mesh` components in the world, which is complete for a
    /// world built entirely from a scene file — and silently incomplete the moment anything spawns
    /// at **runtime**. Terrain streaming is the first such thing: chunk entities appear as the
    /// player approaches, long after loading is done, so the material they name was never read and
    /// every chunk drew with [`Material::default`] — a plain white surface, over a world that was
    /// otherwise entirely correct.
    ///
    /// That is the same failure `games/atrium` hit in session 9 from a different direction, and it
    /// has the same shape both times: **a missing asset is survivable by design (ADR 0021), so the
    /// only symptom is that something looks wrong.**
    ///
    /// Idempotent, and a miss is not an error: an id that does not resolve leaves the cache alone
    /// and whatever names it draws with the default.
    pub fn load_material(&mut self, id: &str) -> &mut Self {
        if id.is_empty() {
            return self;
        }
        self.load_material_ids(BTreeSet::from([id.to_string()]))
    }

    /// The shared half of [`App::load_materials`] and [`App::load_material`].
    fn load_material_ids(&mut self, wanted: BTreeSet<String>) -> &mut Self {
        let found: Vec<(String, Material)> = self.read_component_assets(&wanted);
        if found.is_empty() {
            return self;
        }
        if !self.world.has_service::<MaterialCache>() {
            self.world.insert_service(MaterialCache::new());
        }
        if let Some(cache) = self.world.service_mut::<MaterialCache>() {
            for (id, material) in found {
                cache.insert(id, material);
            }
        }
        self
    }

    /// Turns every clip id an [`amadeo_anim::AnimationPlayer`] names into the animation behind it —
    /// ADR 0066.
    ///
    /// Called automatically by [`App::load_scene`], and **self-installing**, which is the same
    /// departure [`App::load_environments`] makes and for a stronger reason. A clip that never loads
    /// does not merely look wrong: it means a platform does not move, and **every state hash after
    /// it differs**. This is the first asset in the engine whose absence changes *simulation* rather
    /// than the picture, so leaving it behind a setup line somebody could forget is not a risk worth
    /// taking.
    ///
    /// # It does not install `Animatable`, deliberately
    ///
    /// Which component types a clip may write is a decision about the *game* — ADR 0066 §4 — and
    /// there is no defensible default for it. An unallowed target is reported by name in
    /// `Animatable::missing`, so forgetting that one is loud rather than silent.
    ///
    /// # Nothing here is fatal
    ///
    /// An id that does not resolve is recorded in `ClipCache::failures` and its player simply holds
    /// still. ADR 0021 requires a missing asset to be visible and survivable rather than a crash;
    /// what makes it visible here is that report, and `App::asset_problems` for a file that parsed
    /// but held no clip.
    pub fn load_clips(&mut self) -> &mut Self {
        let wanted =
            self.ids_named_by(|player: &amadeo_anim::AnimationPlayer| [player.clip.clone()]);
        if wanted.is_empty() {
            return self;
        }
        let found: Vec<(String, amadeo_anim::AnimationClip)> = self.read_component_assets(&wanted);

        if !self.world.has_service::<amadeo_anim::ClipCache>() {
            self.world.insert_service(amadeo_anim::ClipCache::new());
        }
        let Some(cache) = self.world.service_mut::<amadeo_anim::ClipCache>() else {
            return self;
        };

        for (id, clip) in &found {
            cache.insert(id, clip.clone());
        }
        // An id nobody could turn into a clip. Recorded here rather than left to be noticed, because
        // the symptom is a thing that does not move — which reads as an authoring mistake in the
        // clip rather than as a missing file.
        for id in &wanted {
            if !found.iter().any(|(found_id, _)| found_id == id) {
                cache.fail(
                    id,
                    "no asset with this id holds an `AnimationClip`; check the scene's `assets` \
                     block and that the file is a `.anim`",
                );
            }
        }
        self
    }

    /// Turns every environment id a camera names into the look behind it — ADR 0034.
    ///
    /// Called automatically by [`App::load_scene`], and public because a game that spawns its camera
    /// in code rather than in a scene file has to be able to do the same thing. Idempotent: calling
    /// it twice re-reads the same bytes and produces the same cache.
    ///
    /// # Why this lives here rather than in `amadeo-render`
    ///
    /// An environment's file is a *scene* file (ADR 0034), and `amadeo-scene` sits **above**
    /// `amadeo-render` in the crate graph — so by invariant I6 the renderer cannot parse its own
    /// asset. It owns the type and the cache; this layer, which can see both crates, does the
    /// reading. The same split `TextureCache` has, for the same reason.
    ///
    /// # Nothing here is fatal
    ///
    /// An id that does not resolve, will not parse, or holds no `Environment` is skipped, and the
    /// camera renders with the default look — which is the picture a camera with no environment
    /// draws anyway. ADR 0021 requires a missing asset to be visible and survivable rather than a
    /// crash, and `EnvironmentCache::is_loaded` is what tells "asked for nothing" apart from "asked
    /// for something that is not there".
    pub fn load_environments(&mut self) -> &mut Self {
        let wanted = self.ids_named_by(|camera: &Camera| [camera.environment.clone()]);
        let found: Vec<(String, Environment)> = self.read_component_assets(&wanted);
        if found.is_empty() {
            return self;
        }
        // Installed on first use rather than at construction, so a game that never asks for a look
        // never carries the service — and so a game that does gets it without a setup step.
        if !self.world.has_service::<EnvironmentCache>() {
            self.world.insert_service(EnvironmentCache::new());
        }
        let names_a_sky = found.iter().any(|(_, look)| !look.sky.is_empty());
        if let Some(cache) = self.world.service_mut::<EnvironmentCache>() {
            for (id, environment) in found {
                cache.insert(id, environment);
            }
        }

        // And the cache that turns a named sky into the light it casts (ADR 0049). Installed here,
        // beside the looks, because a look is the only thing that names a sky — so if one does, the
        // service it needs is present, and if none does, nothing is carried.
        //
        // **Automatic rather than a line in each game's setup, and that is a deliberate departure
        // from how `TextureCache` is installed.** Every game inserts that one by hand, which is a
        // step that can be forgotten — and it was: image-based lighting was built, wired and tested
        // and then rendered *nothing* on the Scarp, because no service existed to prefilter into and
        // the frame quietly fell back to the neutral sky. Nothing failed and nothing said so. A
        // capability that goes silently inert when a setup line is missing is the shape of defect
        // this project keeps rediscovering, so this one installs itself.
        if names_a_sky && !self.world.has_service::<amadeo_render::SkyCache>() {
            self.world.insert_service(amadeo_render::SkyCache::new());
        }
        self
    }

    /// Parses every prefab a scene instances, from the bytes the load barrier made resident.
    ///
    /// Prefabs are scene files, so this is the same parser. A prefab that will not parse is skipped
    /// rather than reported here: `instantiate_with` will say `UnknownPrefab` naming the entity that
    /// wanted it, which is the more useful half of the message.
    ///
    /// Recursive: a prefab may instance another, so anything a prefab itself requires is pulled in
    /// too — with a visited set, because a prefab cycle is refused at instantiation and should not
    /// hang here first.
    fn prefab_library(&mut self, document: &amadeo_scene::SceneDocument) -> PrefabLibrary {
        let mut library = PrefabLibrary::new();
        let mut wanted: Vec<String> = document
            .walk()
            .into_iter()
            .filter_map(|entity| entity.prefab.clone())
            .collect();

        while let Some(id) = wanted.pop() {
            if library.get(&id).is_some() {
                continue;
            }
            let Some(assets) = self.assets() else {
                continue;
            };
            let Some(loaded) = assets.store.get(&id) else {
                continue;
            };
            let Ok(text) = std::str::from_utf8(&loaded.bytes) else {
                continue;
            };
            let Ok(parsed) = amadeo_scene::parse(text) else {
                continue;
            };

            // A prefab's own requirements, which the outer scene never mentioned.
            for nested in parsed.walk() {
                if let Some(inner) = &nested.prefab {
                    wanted.push(inner.clone());
                }
            }
            let nested_assets = parsed.required_assets();
            library.insert(id, parsed);
            self.load_assets(nested_assets.iter().map(String::as_str));
        }

        library
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

    /// The systems in a stage that keep running while the game is paused (ADR 0065).
    ///
    /// A subset of [`App::resolved_order`], in the same order. Surfaced by `schedule.list` so that
    /// **"why did my system not run" is answerable without reading the game's source** — invariant
    /// I5's standard applied to a scheduling rule rather than to data.
    pub fn while_paused_order(&mut self, stage: Stage) -> Result<Vec<&'static str>, ScheduleError> {
        match self.schedules.get_mut(&stage) {
            Some(schedule) => schedule.while_paused_labels(),
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
    ///
    /// # Pausing (ADR 0065)
    ///
    /// A [`Paused`] resource set to true makes `Simulation` and `PostSimulation` run only the
    /// systems that declared [`SystemConfig::while_paused`]. Steps 2 and 3 still happen — **the tick
    /// never stops** — and `PreSimulation` still runs in full, so input is sampled and the game can
    /// unpause.
    pub fn step(&mut self) -> Result<(), ScheduleError> {
        // Read once, at the top, so pausing takes effect on the *next* tick rather than half-way
        // through this one. Both are deterministic; this one is the one a person can reason about,
        // because "which systems ran this tick" then does not depend on where in the schedule the
        // toggle happened to sit.
        let paused = self
            .world
            .resource::<Paused>()
            .is_some_and(|state| state.paused);

        for stage in Stage::SIMULATION_STAGES {
            // `PreSimulation` always runs in full. It is definitionally the stage before gameplay,
            // and a game whose input sampling stopped could never unpause itself.
            let skip_unflagged = paused && stage != Stage::PreSimulation;
            if let Some(schedule) = self.schedules.get_mut(&stage) {
                schedule.run(&mut self.world, skip_unflagged)?;
            }
            // Flushing per stage rather than once per tick keeps stage boundaries meaningful: a
            // system in a later stage sees the structural changes an earlier stage requested.
            self.world.flush_commands();
        }

        for (_, swap) in &self.event_swaps {
            swap(&mut self.world);
        }

        self.world.advance_tick();
        if let Some(profiler) = self.world.service_mut::<Profiler>() {
            profiler.record_tick();
        }
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
                // Never paused. A pause menu that stopped being drawn the moment it opened would be
                // a difficult thing to close.
                schedule.run(&mut self.world, false)?;
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
