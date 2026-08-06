//! Getting an asset's bytes into memory, without letting the simulation notice.
//!
//! # The one rule everything here obeys — ADR 0021
//!
//! **Gameplay may hold an asset id. It may never observe an asset's *state*.**
//!
//! No simulation system asks whether an asset is loaded, how big it is, or what is in it. That is
//! what makes determinism *structural* rather than conventional: "is it loaded yet" depends on disk
//! speed, file-cache state, and OS scheduling, none of which reproduce. A simulation that can ask
//! does not reproduce; one that cannot ask has nothing to branch on, and an asset arriving at tick
//! 900 instead of tick 300 changes what is on screen and nothing else.
//!
//! Rendering and audio sit *outside* the deterministic zone and are free to look. That is why
//! [`AssetStore`] lives inside [`crate::Assets`], which is a `Service` — services are excluded from
//! `World::state_hash` by trait bound (ADR 0009), so nothing here can reach a replay assertion even
//! by accident.
//!
//! # The barrier
//!
//! ADR 0021's second rule: a scene declares what it needs, and no tick runs until it is resident.
//! [`AssetStore::load_all`] is called before the first tick and never during one, so the very first
//! tick already sees a fully populated world.
//!
//! This is belt-and-braces on top of the rule above, and it is worth having for two reasons that are
//! not determinism: a level does not appear half-textured, and a game that accidentally *does*
//! depend on an asset gets the same answer every run rather than an intermittent one.
//!
//! It is a default, not a constraint. Streaming — loading past the barrier while ticks run — stays
//! permitted precisely because rule 1 makes it harmless. That is the property ADR 0021 bought.
//!
//! # Bytes, not pictures
//!
//! This layer reads files. It does not decode them: turning a `.ppm` into pixels or an `.ogg` into
//! samples is an *importer's* job, and importers are per-kind knowledge that would drag format
//! opinions down into a crate that should not have any (invariant I4). So an asset here is a name,
//! a path, and a `Vec<u8>`.

use crate::{AssetCatalogue, Assets};
use amadeo_jobs::{Inbox, JobPool};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Why one asset could not be loaded.
///
/// **Never fatal.** ADR 0021 requires a missing asset to produce a visible stand-in plus a
/// structured report rather than a crash, "because the agent's only eyes are `render.describe` and
/// `render.capture`" — an agent has to be able to *see* what is broken and keep working.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadFailure {
    /// The id is not in the catalogue at all.
    #[error(
        "no asset is called `{id}`{}.\n\
         Run `amadeo assets` for the ids that do exist",
        suggestion(near)
    )]
    Unknown {
        /// The id that was asked for.
        id: String,
        /// Catalogued ids that look similar.
        near: Vec<String>,
    },

    /// The id is catalogued but the file behind it could not be read.
    ///
    /// The field is `file` rather than `source` because `thiserror` treats a field called `source`
    /// as the underlying error and would try to make a `String` into one.
    #[error("`{id}` is catalogued as `{file}`, but it could not be read: {message}")]
    Unreadable {
        /// The id that was asked for.
        id: String,
        /// Where the catalogue said the file was.
        file: String,
        /// The underlying message.
        message: String,
    },
}

/// Renders a "did you mean" clause, or nothing when there is no near miss.
///
/// A free function rather than inline in the format string because `thiserror`'s attribute is a
/// format expression and a conditional inside one is unreadable.
fn suggestion(near: &[String]) -> String {
    if near.is_empty() {
        String::new()
    } else {
        format!(". Did you mean {}?", near.join(", "))
    }
}

/// One loaded asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedAsset {
    /// Where it came from, relative to the asset root.
    pub source: PathBuf,
    /// The file's contents, undecoded.
    pub bytes: Vec<u8>,
}

impl LoadedAsset {
    /// How many bytes were read.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the file was empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// Assets that have been read into memory, by id.
///
/// Ordered, like everything else in this engine that anything might be generated from — an
/// `assets.list` reply showing residency has to be the same on two machines (invariant I3).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct AssetStore {
    resident: BTreeMap<String, LoadedAsset>,
    failures: BTreeMap<String, LoadFailure>,
}

impl AssetStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> AssetStore {
        AssetStore::default()
    }

    /// Loads every named id, reading each file relative to `root`.
    ///
    /// **Keeps going after a failure.** A level referring to five missing textures should report
    /// five, not the first — an agent fixing them cannot ask a follow-up question, and one problem
    /// per round trip is the functional defect `amadeo_scene::validate` exists to avoid.
    ///
    /// Ids are visited in sorted order, so two runs read the same files in the same sequence. That
    /// does not matter for correctness — rule 1 means nothing observes it — but it makes a trace
    /// comparable, which is worth having for free.
    pub fn load_all<'a>(
        &mut self,
        catalogue: &AssetCatalogue,
        root: &Path,
        ids: impl IntoIterator<Item = &'a str>,
    ) {
        // Collected and sorted rather than trusting the caller's order.
        let mut wanted: Vec<&str> = ids.into_iter().collect();
        wanted.sort_unstable();
        wanted.dedup();

        for id in wanted {
            // Already resident. Loading twice would be wasted work, and would also mean a second
            // call could *replace* bytes something is already using.
            if self.resident.contains_key(id) {
                continue;
            }

            let Some(entry) = catalogue.get(id) else {
                self.failures.insert(
                    id.to_string(),
                    LoadFailure::Unknown {
                        id: id.to_string(),
                        near: catalogue
                            .similar_to(id)
                            .into_iter()
                            .map(str::to_string)
                            .collect(),
                    },
                );
                continue;
            };

            match std::fs::read(root.join(&entry.source)) {
                Ok(bytes) => {
                    // A previous failure for this id is cleared, so a reload that succeeds does not
                    // leave a stale complaint behind.
                    self.failures.remove(id);
                    self.resident.insert(
                        id.to_string(),
                        LoadedAsset {
                            source: entry.source.clone(),
                            bytes,
                        },
                    );
                }
                Err(error) => {
                    self.failures.insert(
                        id.to_string(),
                        LoadFailure::Unreadable {
                            id: id.to_string(),
                            file: entry.source.to_string_lossy().replace('\\', "/"),
                            message: error.to_string(),
                        },
                    );
                }
            }
        }
    }

    /// [`AssetStore::load_all`], reading the files across threads — ADR 0041.
    ///
    /// # It produces exactly what the sequential version produces
    ///
    /// Not "equivalent" or "the same set" — **identical**, including which failures are recorded and
    /// what they say. `parallel_loading_matches_sequential_loading_exactly` asserts it, and that is
    /// the only interesting claim here: if the two could differ, a game would behave differently
    /// depending on how many cores it ran on.
    ///
    /// Three things make that true, and all three are ADR 0041's rules rather than luck:
    ///
    /// 1. **A job owns its inputs.** Each one gets an id and a path, reads the file, and returns
    ///    bytes. It cannot see the store, the catalogue or the world.
    /// 2. **Results come back through an [`Inbox`], which drains in key order.** The id is the key,
    ///    so the store is filled in sorted order however the reads finished — which is the same
    ///    order [`AssetStore::load_all`] fills it in.
    /// 3. **There is a barrier.** Nothing returns until every read has finished, so from outside
    ///    this is a slow function that got faster, and the simulation cannot tell it was threaded.
    ///
    /// ADR 0021's rule is what makes the whole thing safe to begin with: gameplay may not ask
    /// whether an asset has loaded, so there is no way to observe *when* any of this happened.
    ///
    /// # When it is worth it
    ///
    /// When there are many files or big ones. Reading a file is mostly waiting on the operating
    /// system, so this parallelises better than arithmetic does — but two small files are not worth
    /// a barrier, and the sequential path stays the right default for a handful of assets.
    pub fn load_all_in_parallel<'a>(
        &mut self,
        catalogue: &AssetCatalogue,
        root: &Path,
        ids: impl IntoIterator<Item = &'a str>,
        pool: &JobPool,
    ) {
        let mut wanted: Vec<&str> = ids.into_iter().collect();
        wanted.sort_unstable();
        wanted.dedup();

        // Catalogue lookups happen here, on the calling thread. A job cannot borrow the catalogue --
        // it is `'static` by construction -- and resolving up front also means an unknown id is
        // recorded in exactly the order and with exactly the wording the sequential path uses.
        let inbox: Inbox<String, Result<Vec<u8>, String>> = Inbox::new();
        let mut submitted = 0_usize;
        let mut sources: BTreeMap<String, PathBuf> = BTreeMap::new();

        for id in wanted {
            if self.resident.contains_key(id) {
                continue;
            }
            let Some(entry) = catalogue.get(id) else {
                self.failures.insert(
                    id.to_string(),
                    LoadFailure::Unknown {
                        id: id.to_string(),
                        near: catalogue
                            .similar_to(id)
                            .into_iter()
                            .map(str::to_string)
                            .collect(),
                    },
                );
                continue;
            };

            sources.insert(id.to_string(), entry.source.clone());
            let owned_id = id.to_string();
            let file = root.join(&entry.source);
            let inbox = inbox.clone();
            pool.submit(move || {
                // `map_err` to a String rather than carrying `std::io::Error`, which is not `Send`
                // in every form and would tie this crate's job type to one error representation.
                let result = std::fs::read(&file).map_err(|error| error.to_string());
                inbox.deliver(owned_id, result);
            });
            submitted += 1;
        }

        if submitted == 0 {
            return;
        }

        // **The barrier.** After this every read has finished, so what follows is ordinary
        // single-threaded code filling a map.
        pool.wait_for_idle();

        for (id, result) in inbox.drain() {
            match result {
                Ok(bytes) => {
                    let source = sources.get(&id).cloned().unwrap_or_default();
                    self.failures.remove(&id);
                    self.resident.insert(id, LoadedAsset { source, bytes });
                }
                Err(message) => {
                    let file = sources
                        .get(&id)
                        .map(|path| path.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_default();
                    self.failures
                        .insert(id.clone(), LoadFailure::Unreadable { id, file, message });
                }
            }
        }
    }

    /// A loaded asset's bytes.
    ///
    /// **Not callable from a simulation system**, by ADR 0021's rule. Renderers and audio may use
    /// it freely; they sit outside the deterministic zone. Nothing enforces that at the type level
    /// today, which is recorded as a real gap in the module docs.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&LoadedAsset> {
        self.resident.get(id)
    }

    /// Whether an asset is in memory.
    #[must_use]
    pub fn is_resident(&self, id: &str) -> bool {
        self.resident.contains_key(id)
    }

    /// Every resident id, in order.
    pub fn resident_ids(&self) -> impl Iterator<Item = &str> {
        self.resident.keys().map(String::as_str)
    }

    /// How many assets are resident.
    #[must_use]
    pub fn len(&self) -> usize {
        self.resident.len()
    }

    /// Whether nothing is loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.resident.is_empty()
    }

    /// Everything that failed to load, by id.
    ///
    /// The structured half of ADR 0021's "visible stand-in plus a structured report".
    pub fn failures(&self) -> impl Iterator<Item = (&str, &LoadFailure)> {
        self.failures
            .iter()
            .map(|(id, failure)| (id.as_str(), failure))
    }

    /// Whether anything failed.
    #[must_use]
    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }
}

impl Assets {
    /// Loads the named assets, at the barrier, before any tick runs.
    ///
    /// Failures are recorded rather than returned, because ADR 0021 requires a missing asset to be
    /// survivable: the game keeps running, draws a placeholder, and reports what is wrong through
    /// the protocol. Ask [`AssetStore::failures`] — or `assets.list`, which shows them — for what
    /// went wrong.
    ///
    /// Does nothing if the catalogue was built by hand and has no root behind it.
    pub fn load<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>) {
        let Some(root) = &self.root else {
            return;
        };
        let root = root.path.clone();
        // Split borrow: `load_all` needs the catalogue while the store is held mutably, and they are
        // two fields of the same struct.
        let Assets {
            catalogue, store, ..
        } = self;
        store.load_all(catalogue, &root, ids);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sidecar;

    /// A directory with two real files in it, plus the catalogue naming them.
    struct Fixture {
        root: PathBuf,
        catalogue: AssetCatalogue,
    }

    impl Fixture {
        fn new(name: &str) -> Fixture {
            let root = std::env::temp_dir().join(format!(
                "amadeo-load-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("textures")).expect("temp dir");
            std::fs::write(root.join("textures/wall.ppm"), "P3 wall").expect("write");
            std::fs::write(root.join("textures/floor.ppm"), "P3 floor").expect("write");

            let mut catalogue = AssetCatalogue::new();
            catalogue
                .insert(Sidecar::new("wall"), Path::new("textures/wall.ppm"))
                .expect("distinct");
            catalogue
                .insert(Sidecar::new("floor"), Path::new("textures/floor.ppm"))
                .expect("distinct");
            // Catalogued, but the file was never written.
            catalogue
                .insert(Sidecar::new("ghost"), Path::new("textures/ghost.ppm"))
                .expect("distinct");

            Fixture { root, catalogue }
        }

        fn load(&self, ids: &[&str]) -> AssetStore {
            let mut store = AssetStore::new();
            store.load_all(&self.catalogue, &self.root, ids.iter().copied());
            store
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_declared_asset_is_resident_with_its_bytes() {
        let fixture = Fixture::new("basic");
        let store = fixture.load(&["wall"]);

        assert!(store.is_resident("wall"));
        assert_eq!(store.get("wall").expect("loaded").bytes, b"P3 wall");
        assert!(!store.has_failures());
    }

    #[test]
    fn only_what_was_asked_for_is_loaded() {
        // The barrier loads a scene's declared set, not the whole catalogue -- otherwise a project
        // with a gigabyte of assets pays for all of it to open one level.
        let fixture = Fixture::new("subset");
        let store = fixture.load(&["wall"]);

        assert!(store.is_resident("wall"));
        assert!(!store.is_resident("floor"));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn a_missing_asset_is_recorded_rather_than_fatal() {
        // ADR 0021: a missing asset must produce a report and a placeholder, not a crash, because
        // the agent has to be able to see what is broken and keep working.
        let fixture = Fixture::new("missing");
        let store = fixture.load(&["wall", "ghost"]);

        // The good one still loaded.
        assert!(store.is_resident("wall"));
        assert!(!store.is_resident("ghost"));

        let failures: Vec<&str> = store.failures().map(|(id, _)| id).collect();
        assert_eq!(failures, ["ghost"]);
    }

    #[test]
    fn every_failure_is_reported_not_just_the_first() {
        // One problem per round trip is a functional defect for an agent.
        let fixture = Fixture::new("many");
        let store = fixture.load(&["ghost", "nope", "wall"]);

        assert_eq!(store.failures().count(), 2);
        // And the one that could load, did -- a bad id does not poison the batch.
        assert!(store.is_resident("wall"));
    }

    #[test]
    fn an_unknown_id_suggests_what_was_probably_meant() {
        // Pillar 5: the error carries the fix, because an agent cannot ask a follow-up question.
        let fixture = Fixture::new("nearmiss");
        let store = fixture.load(&["wal"]);

        let (_, failure) = store.failures().next().expect("one failure");
        let message = failure.to_string();

        assert!(message.contains("Did you mean wall?"), "got: {message}");
        assert!(message.contains("amadeo assets"), "got: {message}");
    }

    #[test]
    fn an_unknown_id_with_no_near_miss_omits_the_suggestion() {
        let fixture = Fixture::new("nomiss");
        let store = fixture.load(&["completely_unrelated"]);

        let (_, failure) = store.failures().next().expect("one failure");
        assert!(!failure.to_string().contains("Did you mean"), "{failure}");
    }

    #[test]
    fn loading_twice_does_not_reread() {
        // A second call must not replace bytes something is already holding.
        let fixture = Fixture::new("twice");
        let mut store = fixture.load(&["wall"]);

        std::fs::write(fixture.root.join("textures/wall.ppm"), "CHANGED").expect("write");
        store.load_all(&fixture.catalogue, &fixture.root, ["wall"]);

        assert_eq!(store.get("wall").expect("loaded").bytes, b"P3 wall");
    }

    #[test]
    fn the_order_ids_are_visited_does_not_change_the_result() {
        // Invariant I3. Nothing observes load order under rule 1, but making it stable costs one
        // sort and makes a trace comparable.
        let fixture = Fixture::new("order");
        let forwards = fixture.load(&["floor", "wall"]);
        let backwards = fixture.load(&["wall", "floor"]);

        assert_eq!(forwards, backwards);
    }

    #[test]
    fn a_repeated_id_is_loaded_once() {
        let fixture = Fixture::new("dupes");
        let store = fixture.load(&["wall", "wall", "wall"]);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn the_store_is_not_part_of_the_state_hash() {
        // The structural guarantee, stated as a test. `Assets` is a Service, and ADR 0009 excludes
        // services from the hash by trait bound -- so loading an asset cannot move a replay.
        use amadeo_ecs::World;

        let mut bare = World::new();
        let baseline = bare.state_hash();

        bare.insert_service(Assets::default());
        assert_eq!(bare.state_hash(), baseline);

        let fixture = Fixture::new("hash");
        let mut assets = Assets::from_catalogue(AssetCatalogue::new());
        assets
            .store
            .load_all(&fixture.catalogue, &fixture.root, ["wall"]);
        assert!(assets.store.is_resident("wall"));

        let mut loaded = World::new();
        loaded.insert_service(assets);
        assert_eq!(loaded.state_hash(), baseline);
    }
}
