//! Playing a clip: the clock, the allow-list, and writing a sampled number into a field.

use crate::clip::AnimationClip;
use amadeo_core::{FIXED_DT, StableHash};
use amadeo_ecs::{Component, Entity, Service, World};
use amadeo_events::{Event, WorldEvents};
use amadeo_reflect::{Reflect, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The label the app layer registers [`animate`] under.
pub const ANIMATE: &str = "animate";

/// Plays a clip on the entity it is attached to.
///
/// # Hashed, and unlike `GlobalTransform` there is no derived half
///
/// The reflex from `GlobalTransform` and `ComputedRect` is to make animation output derived, and
/// here that would be wrong. A clip that moves a `Transform` is a **moving platform you can stand
/// on**: physics reads it, the character controller reads it, a save has to restore where it was.
/// `docs/04` §14 requires hitboxes-on-frames to reproduce exactly, which is the same claim from the
/// gameplay side.
///
/// So the clock is hashed and so is everything it writes (ADR 0066 §3). The derived half arrives
/// with skinning, where a pose becomes joint matrices that only a shader ever reads.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
pub struct AnimationPlayer {
    /// The declared asset id of the clip (ADR 0020).
    ///
    /// A **missing clip means nothing moves**, which is the first case in this engine where a
    /// missing asset changes *simulation* rather than the picture. `ClipCache::failures` is what
    /// makes that visible; see the type's docs.
    pub clip: String,
    /// Seconds into the clip.
    #[reflect(unit = "s")]
    pub time: f32,
    /// How fast it plays. Negative runs it backwards.
    #[reflect(min = -8.0, max = 8.0)]
    pub speed: f32,
    /// Whether it wraps round at the end instead of stopping on the last frame.
    pub looping: bool,
    /// Whether the clock advances at all.
    ///
    /// A field rather than removing the component, for `AudioSource::playing`'s reason: stopping and
    /// starting must not move the entity between archetypes.
    pub playing: bool,
}

impl Default for AnimationPlayer {
    fn default() -> Self {
        Self {
            clip: String::new(),
            time: 0.0,
            speed: 1.0,
            looping: true,
            playing: true,
        }
    }
}

impl AnimationPlayer {
    /// A player that loops a clip from the start.
    #[must_use]
    pub fn looping(clip: &str) -> Self {
        Self {
            clip: clip.to_string(),
            ..Self::default()
        }
    }

    /// A player that runs a clip once and stops on its last frame.
    #[must_use]
    pub fn once(clip: &str) -> Self {
        Self {
            clip: clip.to_string(),
            looping: false,
            ..Self::default()
        }
    }
}

impl Component for AnimationPlayer {}

/// A non-looping clip reached its end.
///
/// Past tense, because it is a fact rather than a request — the naming convention in `CLAUDE.md` §6.
/// Raised once, on the tick the clock stops; a looping player never raises it, because it never
/// finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableHash, Reflect)]
pub struct AnimationFinished {
    /// Which entity's player finished.
    pub entity: Entity,
}

impl Event for AnimationFinished {}

/// Clips by declared id, and the ids that could not be turned into one.
///
/// `TextureCache`'s shape for a fifth time — with the same rule as `SoundCache`: **there is no
/// placeholder clip and there must not be one.** A magenta texture works because nobody ships
/// magenta; every possible stand-in *animation* is indistinguishable from content, and worse than
/// that, a stand-in here would move a gameplay component and change the state hash. So a missing
/// clip is stillness plus a line in [`ClipCache::failures`], and the report is the whole diagnosis.
///
/// # The consequence worth knowing
///
/// This is the first asset in the engine whose absence changes **simulation** rather than the
/// picture: a missing texture draws magenta, a missing sound is silence, a missing clip means a
/// platform does not move and every hash after it differs. ADR 0021's load barrier means the answer
/// is settled before the first tick and is identical on every machine holding the same files, so a
/// replay is safe — but a machine missing the file simulates a different world, and nothing but this
/// report will say so.
#[derive(Debug, Default)]
pub struct ClipCache {
    clips: BTreeMap<String, AnimationClip>,
    failures: BTreeMap<String, String>,
}

impl Service for ClipCache {}

impl ClipCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores a clip under an id, reporting anything wrong with it.
    ///
    /// A clip with problems is still **kept and still played** — every fault
    /// [`AnimationClip::problems`] names produces animation that runs and is subtly wrong, and
    /// refusing to play it would turn a cosmetic mistake into a missing platform. The report is
    /// what makes it findable.
    pub fn insert(&mut self, id: &str, clip: AnimationClip) {
        let problems = clip.problems();
        if problems.is_empty() {
            self.failures.remove(id);
        } else {
            self.failures.insert(id.to_string(), problems.join("; "));
        }
        self.clips.insert(id.to_string(), clip);
    }

    /// Records that an id could not be turned into a clip at all.
    pub fn fail(&mut self, id: &str, why: &str) {
        self.failures.insert(id.to_string(), why.to_string());
    }

    /// The clip stored under an id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&AnimationClip> {
        self.clips.get(id)
    }

    /// Whether an id resolved to a clip.
    #[must_use]
    pub fn is_loaded(&self, id: &str) -> bool {
        self.clips.contains_key(id)
    }

    /// Everything that went wrong, by id, in id order.
    #[must_use]
    pub fn failures(&self) -> &BTreeMap<String, String> {
        &self.failures
    }
}

/// Reads one component off an entity as a reflected value.
type ReadComponent = fn(&World, Entity) -> Option<Value>;

/// Writes a reflected value back onto an entity, replacing the component.
type WriteComponent = fn(&mut World, Entity, &Value) -> bool;

/// The component types a clip is allowed to write, and how to read and write each.
///
/// # Why this exists at all
///
/// A structural fact rather than a preference: **`ComponentRegistry` is owned by `App`, not by the
/// `World`**, so a system — which is handed only a world — cannot reach it. Animation therefore
/// carries its own small table.
///
/// It turns out to be worth having on its own merits. A clip can only reach components a game has
/// deliberately allowed, so it cannot write `RigidBody::kind` or `Collider::shape` and hand the
/// solver a world it disagrees with. An allow-list is a better answer than "anything reflected".
///
/// # A `Service`, so it is outside the state hash
///
/// Which types are animatable is a property of how the game was assembled, not of the world's state
/// — the same reasoning ADR 0009 applies to every other registry-shaped thing.
#[derive(Debug, Default)]
pub struct Animatable {
    readers: BTreeMap<&'static str, ReadComponent>,
    writers: BTreeMap<&'static str, WriteComponent>,
    /// Targets a clip asked for and did not get, as `Component.field`.
    ///
    /// Ordered and de-duplicated, so reading it is reproducible and a fault that happens sixty times
    /// a second is one line rather than a flood.
    missing: BTreeSet<String>,
}

impl Service for Animatable {}

impl Animatable {
    /// An empty allow-list.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Lets a clip animate fields of `T`.
    ///
    /// The two closures capture nothing — each is a monomorphised call on `T` — so they coerce to
    /// plain function pointers and the table costs no allocation. The same trick `App`'s event-swap
    /// table uses.
    pub fn allow<T: Component>(&mut self) -> &mut Self {
        let read: ReadComponent = |world, entity| world.get::<T>(entity).map(Reflect::to_value);
        let write: WriteComponent = |world, entity, value| match T::from_value(value) {
            Ok(built) => {
                if let Some(slot) = world.get_mut::<T>(entity) {
                    *slot = built;
                    true
                } else {
                    false
                }
            }
            Err(_) => false,
        };
        self.readers.insert(T::STATIC_NAME, read);
        self.writers.insert(T::STATIC_NAME, write);
        self
    }

    /// Whether clips may animate this component.
    #[must_use]
    pub fn allows(&self, component: &str) -> bool {
        self.writers.contains_key(component)
    }

    /// Every `Component.field` a clip asked for and did not get, in order.
    ///
    /// ADR 0060's rule, third application: a subsystem that can quietly do nothing must be able to
    /// say why. A track naming a component nobody allowed, or a field that does not exist, animates
    /// nothing — and "nothing moved" is indistinguishable from a clip authored with no motion in it
    /// unless something keeps this list.
    #[must_use]
    pub fn missing(&self) -> &BTreeSet<String> {
        &self.missing
    }
}

/// Advances every player and writes what its clip says — ADR 0066.
///
/// # It runs in `Simulation`, not `PostSimulation`
///
/// A clip writes a `Transform` that physics, the character controller and `propagate_transforms` all
/// read **this** tick. Running it after them would apply this tick's animation to next tick's
/// physics, which reads as a platform you sink into.
///
/// Does nothing without an [`Animatable`] service, which is the headless case for a game with no
/// animation — and, since the allow-list is explicit, also the case where somebody forgot to install
/// one. That is why a missing target is recorded by name rather than skipped.
pub fn animate(world: &mut World) {
    if !world.has_service::<Animatable>() || !world.has_service::<ClipCache>() {
        return;
    }

    // Advance every clock first, and collect what each player now wants applied. Two passes because
    // the second one needs the world mutably while the clips it reads live in a service.
    let mut finished: Vec<Entity> = Vec::new();
    let mut playing: Vec<(Entity, String, f32)> = Vec::new();

    for (entity, (player,)) in world.query::<(&AnimationPlayer,)>() {
        if !player.playing || player.clip.is_empty() {
            continue;
        }
        playing.push((entity, player.clip.clone(), player.time));
    }

    for (entity, clip_id, _) in &playing {
        let Some(duration) = world
            .service::<ClipCache>()
            .and_then(|cache| cache.get(clip_id))
            .map(|clip| clip.duration)
        else {
            continue;
        };

        let Some(player) = world.get_mut::<AnimationPlayer>(*entity) else {
            continue;
        };
        let advanced = player.time + FIXED_DT * player.speed;

        player.time = if player.looping && duration > 0.0 {
            // `rem_euclid` rather than a subtraction, so a clip playing backwards past zero wraps to
            // the end rather than running off into negative time. Exactly specified by IEEE 754,
            // unlike anything that would reach for a transcendental.
            advanced.rem_euclid(duration)
        } else {
            let clamped = advanced.clamp(0.0, duration);
            // The edge, not the state: a stopped player sitting at the end must not raise this
            // every tick for the rest of the game.
            if clamped != player.time && (clamped >= duration || clamped <= 0.0) {
                finished.push(*entity);
            }
            clamped
        };
    }

    // Now apply. The clip cache and the allow-list both come out of the world for the duration,
    // because writing a component needs the world mutably — `collect_ui` has the same shape.
    world.with_service_taken::<ClipCache, ()>(|world, cache| {
        world.with_service_taken::<Animatable, ()>(|world, animatable| {
            for (entity, clip_id, _) in &playing {
                let Some(clip) = cache.get(clip_id) else {
                    continue;
                };
                let Some(time) = world
                    .get::<AnimationPlayer>(*entity)
                    .map(|player| player.time)
                else {
                    continue;
                };

                for track in &clip.tracks {
                    let Some(sampled) = track.sample(time) else {
                        continue;
                    };
                    apply(
                        world,
                        animatable,
                        *entity,
                        &track.component,
                        &track.field,
                        &sampled,
                    );
                }
            }
        });
    });

    for entity in finished {
        world.send_event(AnimationFinished { entity });
    }
}

/// Writes sampled numbers into one field of one component.
fn apply(
    world: &mut World,
    animatable: &mut Animatable,
    entity: Entity,
    component: &str,
    field: &str,
    sampled: &[f32],
) {
    let target = format!("{component}.{field}");

    let (Some(read), Some(write)) = (
        animatable.readers.get(component).copied(),
        animatable.writers.get(component).copied(),
    ) else {
        animatable.missing.insert(target);
        return;
    };

    let Some(Value::Struct(mut fields)) = read(world, entity) else {
        // Either the entity does not have the component, or it reflects as something other than a
        // struct and so has no named fields to patch.
        animatable.missing.insert(target);
        return;
    };

    let Some(existing) = fields.get(field) else {
        animatable.missing.insert(target);
        return;
    };

    let Some(patched) = coerce(existing, sampled) else {
        animatable.missing.insert(target);
        return;
    };

    fields.insert(field.to_string(), patched);
    if !write(world, entity, &Value::Struct(fields)) {
        animatable.missing.insert(target);
    }
}

/// Sampled numbers, shaped like the value already in the field.
///
/// # Why the target decides the shape, and not the track
///
/// ADR 0066 §2. A track carrying a bare list of numbers can animate a scalar, a vector, a colour and
/// a tilesheet index with no variant per shape — and the *component's* schema stays the single
/// description of what its fields are, rather than being restated in every clip that touches one.
///
/// Returns `None` when the numbers cannot fill the shape, which is reported by name rather than
/// half-applied: a translation animated by a track with two numbers in it would otherwise move on
/// two axes and leave the third at whatever it happened to hold.
fn coerce(existing: &Value, sampled: &[f32]) -> Option<Value> {
    match existing {
        Value::F32(_) => Some(Value::F32(*sampled.first()?)),
        Value::F64(_) => Some(Value::F64(f64::from(*sampled.first()?))),
        // Rounded rather than truncated, so a linear track passing through 2.999 on its way to 3
        // reaches 3 rather than showing 2 for the last fraction of the segment. A tilesheet index
        // usually wants `Interpolation::Step` anyway, which never produces a fraction at all.
        Value::I64(_) => Some(Value::I64(sampled.first()?.round() as i64)),
        Value::U64(_) => Some(Value::U64(sampled.first()?.round().max(0.0) as u64)),
        Value::List(items) => {
            if sampled.len() < items.len() {
                return None;
            }
            let patched: Option<Vec<Value>> = items
                .iter()
                .zip(sampled)
                .map(|(item, number)| coerce(item, std::slice::from_ref(number)))
                .collect();
            Some(Value::List(patched?))
        }
        // A bool, a string, a struct, a map or an enum. Nothing here is a number to move between two
        // values, and picking a rule — halfway is true? — would be inventing behaviour nobody asked
        // for. Reported instead.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::{Interpolation, Key, Track};

    /// A component with one of each shape a track can write into.
    #[derive(Debug, Clone, Copy, PartialEq, Default, StableHash, Reflect)]
    struct Thing {
        /// A scalar.
        height: f32,
        /// A vector.
        place: [f32; 3],
        /// A whole number — a tilesheet index, in spirit.
        frame: i64,
        /// Something a track cannot animate.
        awake: bool,
    }

    impl Component for Thing {}

    fn clip(field: &str, keys: &[(f32, &[f32])], interpolation: Interpolation) -> AnimationClip {
        AnimationClip {
            duration: 2.0,
            tracks: vec![Track {
                component: "Thing".to_string(),
                field: field.to_string(),
                interpolation,
                keys: keys
                    .iter()
                    .map(|(time, value)| Key {
                        time: *time,
                        value: value.to_vec(),
                    })
                    .collect(),
            }],
        }
    }

    /// A world with one animated `Thing`, and that entity.
    fn world_with(clip: AnimationClip, player: AnimationPlayer) -> (World, Entity) {
        let mut world = World::new();
        let mut cache = ClipCache::new();
        cache.insert("test", clip);
        world.insert_service(cache);

        let mut animatable = Animatable::new();
        animatable.allow::<Thing>();
        world.insert_service(animatable);
        world.register_event::<AnimationFinished>();

        let entity = world.spawn();
        world.insert(entity, Thing::default());
        world.insert(entity, player);
        (world, entity)
    }

    fn thing(world: &World, entity: Entity) -> Thing {
        *world.get::<Thing>(entity).expect("still there")
    }

    #[test]
    fn a_scalar_field_moves() {
        let (mut world, entity) = world_with(
            clip(
                "height",
                &[(0.0, &[0.0]), (1.0, &[10.0])],
                Interpolation::Linear,
            ),
            AnimationPlayer::looping("test"),
        );

        // Half a second in, at 60 Hz, is thirty ticks.
        for _ in 0..30 {
            animate(&mut world);
        }
        let height = thing(&world, entity).height;
        assert!(
            (height - 5.0).abs() < 0.2,
            "half a second into a one-second ramp should be about halfway, got {height}"
        );
    }

    #[test]
    fn a_vector_field_moves_on_every_axis() {
        // The shape that catches a coercion which fills the first element and leaves the rest.
        let (mut world, entity) = world_with(
            clip(
                "place",
                &[(0.0, &[0.0, 0.0, 0.0]), (1.0, &[3.0, 6.0, 9.0])],
                Interpolation::Linear,
            ),
            AnimationPlayer::looping("test"),
        );
        for _ in 0..60 {
            animate(&mut world);
        }

        let place = thing(&world, entity).place;
        for (axis, expected) in place.iter().zip([3.0, 6.0, 9.0]) {
            assert!((axis - expected).abs() < 0.2, "got {place:?}");
        }
    }

    #[test]
    fn a_whole_number_field_gets_whole_numbers() {
        let (mut world, entity) = world_with(
            clip(
                "frame",
                &[(0.0, &[0.0]), (1.0, &[4.0])],
                Interpolation::Step,
            ),
            AnimationPlayer::looping("test"),
        );
        animate(&mut world);
        assert_eq!(thing(&world, entity).frame, 0);

        for _ in 0..60 {
            animate(&mut world);
        }
        assert_eq!(thing(&world, entity).frame, 4);
    }

    #[test]
    fn a_field_that_is_not_a_number_is_reported_rather_than_guessed_at() {
        // Halfway between false and true is not a thing, and inventing a rule for it would be
        // behaviour nobody asked for that nobody could find later.
        let (mut world, entity) = world_with(
            clip(
                "awake",
                &[(0.0, &[0.0]), (1.0, &[1.0])],
                Interpolation::Linear,
            ),
            AnimationPlayer::looping("test"),
        );
        animate(&mut world);

        assert!(!thing(&world, entity).awake);
        assert!(
            world
                .service::<Animatable>()
                .expect("installed")
                .missing()
                .contains("Thing.awake")
        );
    }

    #[test]
    fn a_component_nobody_allowed_is_reported_by_name() {
        // The failure this crate is most likely to hit in a real game: the allow-list is explicit,
        // so forgetting a type means nothing moves — which is indistinguishable from a clip with no
        // motion in it unless something says so.
        let (mut world, _) = world_with(
            AnimationClip {
                duration: 1.0,
                tracks: vec![Track {
                    component: "Transform".to_string(),
                    field: "translation".to_string(),
                    interpolation: Interpolation::Linear,
                    keys: vec![Key {
                        time: 0.0,
                        value: vec![1.0],
                    }],
                }],
            },
            AnimationPlayer::looping("test"),
        );
        animate(&mut world);

        assert!(
            world
                .service::<Animatable>()
                .expect("installed")
                .missing()
                .contains("Transform.translation")
        );
    }

    #[test]
    fn a_looping_clip_wraps_and_never_finishes() {
        let (mut world, entity) = world_with(
            clip(
                "height",
                &[(0.0, &[0.0]), (1.0, &[1.0])],
                Interpolation::Linear,
            ),
            AnimationPlayer::looping("test"),
        );
        // The clip is two seconds long; run three.
        for _ in 0..180 {
            animate(&mut world);
        }

        let time = world
            .get::<AnimationPlayer>(entity)
            .expect("still there")
            .time;
        assert!(
            (0.0..2.0).contains(&time),
            "should have wrapped, got {time}"
        );

        world.swap_events::<AnimationFinished>();
        assert!(world.read_events::<AnimationFinished>().is_empty());
    }

    #[test]
    fn a_one_shot_stops_on_its_last_frame_and_says_so_once() {
        // Once, not every tick after. A player parked at the end that kept announcing itself would
        // make "did this animation finish" unanswerable by reading events.
        let (mut world, entity) = world_with(
            clip(
                "height",
                &[(0.0, &[0.0]), (2.0, &[8.0])],
                Interpolation::Linear,
            ),
            AnimationPlayer::once("test"),
        );

        let mut announcements = 0;
        for _ in 0..240 {
            animate(&mut world);
            world.swap_events::<AnimationFinished>();
            announcements += world.read_events::<AnimationFinished>().len();
        }

        assert_eq!(announcements, 1);
        assert_eq!(
            world
                .get::<AnimationPlayer>(entity)
                .expect("still there")
                .time,
            2.0,
            "a one-shot holds its last frame"
        );
        assert!((thing(&world, entity).height - 8.0).abs() < 0.01);
    }

    #[test]
    fn a_missing_clip_leaves_everything_alone_and_is_findable() {
        // **The consequence worth pinning**: this is the first asset whose absence changes
        // simulation rather than the picture, so the report is the whole diagnosis.
        let (mut world, entity) = world_with(
            clip("height", &[(0.0, &[5.0])], Interpolation::Linear),
            AnimationPlayer::looping("not_an_asset"),
        );
        if let Some(cache) = world.service_mut::<ClipCache>() {
            cache.fail("not_an_asset", "no asset declares this id");
        }
        animate(&mut world);

        assert_eq!(thing(&world, entity).height, 0.0);
        assert!(
            world
                .service::<ClipCache>()
                .expect("installed")
                .failures()
                .contains_key("not_an_asset")
        );
    }

    #[test]
    fn a_stopped_player_does_not_move() {
        let (mut world, entity) = world_with(
            clip(
                "height",
                &[(0.0, &[0.0]), (1.0, &[10.0])],
                Interpolation::Linear,
            ),
            AnimationPlayer {
                playing: false,
                ..AnimationPlayer::looping("test")
            },
        );
        for _ in 0..60 {
            animate(&mut world);
        }
        assert_eq!(thing(&world, entity).height, 0.0);
    }

    #[test]
    fn playing_a_clip_reproduces_exactly() {
        // Invariant I3. The clock is `+ - * /` and `rem_euclid`, all pinned by IEEE 754, and the
        // sample is a lerp — nothing here can differ between two runs or two machines.
        let run = || {
            let (mut world, _) = world_with(
                clip(
                    "place",
                    &[(0.0, &[0.0, 0.0, 0.0]), (1.3, &[3.0, 6.0, 9.0])],
                    Interpolation::Linear,
                ),
                AnimationPlayer::looping("test"),
            );
            for _ in 0..137 {
                animate(&mut world);
            }
            world.state_hash()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn animation_output_is_in_the_state_hash_because_it_is_gameplay() {
        // **The claim ADR 0066 §3 makes, checked rather than argued.** A clip that moves a transform
        // is a moving platform, so the world after it must not hash the same as the world before.
        // If this ever passes, animation has quietly become presentation.
        let (mut world, _) = world_with(
            clip(
                "height",
                &[(0.0, &[0.0]), (1.0, &[10.0])],
                Interpolation::Linear,
            ),
            AnimationPlayer::looping("test"),
        );
        let before = world.state_hash();
        for _ in 0..30 {
            animate(&mut world);
        }
        assert_ne!(before, world.state_hash());
    }
}
