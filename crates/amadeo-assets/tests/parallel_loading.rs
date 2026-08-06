//! Loading files across threads produces exactly what loading them one at a time produces.
//!
//! # Why "exactly" is the whole claim
//!
//! Not "the same set of assets" and not "equivalent" — **identical**, including which failures are
//! recorded and the wording of each one. If the two paths could differ, a game would behave
//! differently depending on how many cores the machine had, which is precisely the class of bug
//! ADR 0041 exists to prevent.
//!
//! `AssetStore` derives `PartialEq`, so the assertion can be the whole store rather than a sample of
//! it. That matters: a test comparing only the resident ids would pass while the failure messages
//! diverged, and failure messages are what an agent reads.

use amadeo_assets::{AssetCatalogue, AssetStore, Scan};
use amadeo_jobs::JobPool;
use std::path::{Path, PathBuf};

/// Builds an asset directory with `count` files, plus one catalogued file that is deleted again.
///
/// The deleted one is the point: a store that only ever succeeded would never exercise the failure
/// path, and the failure path is where the two implementations are most likely to disagree.
fn asset_directory(name: &str, count: usize) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a writable directory");

    for index in 0..count {
        let id = format!("thing_{index:03}");
        // Contents differ per file, so a job that delivered the wrong bytes to the wrong key would
        // be caught rather than looking identical.
        std::fs::write(
            directory.join(format!("{id}.txt")),
            format!("contents of {id}").repeat(64),
        )
        .expect("writes");
        std::fs::write(
            directory.join(format!("{id}.txt.ama-meta")),
            format!("id = \"{id}\"\n"),
        )
        .expect("writes");
    }

    // Catalogued, then removed — so the catalogue knows about it and the read fails.
    std::fs::write(directory.join("ghost.txt"), b"boo").expect("writes");
    std::fs::write(directory.join("ghost.txt.ama-meta"), b"id = \"ghost\"\n").expect("writes");
    directory
}

/// Every id the catalogue knows, plus one it does not.
fn ids(count: usize) -> Vec<String> {
    let mut out: Vec<String> = (0..count)
        .map(|index| format!("thing_{index:03}"))
        .collect();
    out.push("ghost".to_string());
    // An id that was never catalogued at all, which is a different failure from an unreadable file.
    out.push("never_existed".to_string());
    out
}

fn scan(directory: &Path) -> Scan {
    AssetCatalogue::scan(directory).expect("scans")
}

#[test]
fn parallel_loading_matches_sequential_loading_exactly() {
    let count = 60;
    let directory = asset_directory("parallel_equal", count);
    let scanned = scan(&directory);
    // Delete the file the catalogue is now holding, so `ghost` fails to read rather than failing to
    // resolve. Both failure kinds are therefore in play.
    std::fs::remove_file(directory.join("ghost.txt")).expect("removes");

    let names = ids(count);
    let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();

    let mut sequential = AssetStore::new();
    sequential.load_all(&scanned.catalogue, &directory, borrowed.iter().copied());

    let pool = JobPool::new(8);
    let mut parallel = AssetStore::new();
    parallel.load_all_in_parallel(
        &scanned.catalogue,
        &directory,
        borrowed.iter().copied(),
        &pool,
    );

    assert_eq!(
        parallel, sequential,
        "the parallel store must be byte-identical to the sequential one"
    );
    // And it actually did the work, rather than both being empty and trivially equal.
    assert_eq!(sequential.len(), count);
    assert_eq!(sequential.failures().count(), 2);
}

#[test]
fn the_worker_count_cannot_reach_the_result() {
    // The same claim from the other direction: not just that parallel matches sequential once, but
    // that no thread count produces anything different. A pool of one is the control.
    let count = 40;
    let directory = asset_directory("parallel_counts", count);
    let scanned = scan(&directory);
    let names = ids(count);

    let load = |workers: usize| {
        let pool = JobPool::new(workers);
        let mut store = AssetStore::new();
        store.load_all_in_parallel(
            &scanned.catalogue,
            &directory,
            names.iter().map(String::as_str),
            &pool,
        );
        store
    };

    let one = load(1);
    for workers in [2, 3, 7] {
        assert_eq!(
            load(workers),
            one,
            "{workers} workers gave a different store"
        );
    }
}

#[test]
fn loading_twice_does_not_reload_what_is_already_resident() {
    // The sequential path skips ids it already holds, so that a second call cannot *replace* bytes
    // something is already using. The parallel path has to skip them before submitting, or it would
    // do the read and then overwrite — invisible until an asset changed on disk mid-run.
    let directory = asset_directory("parallel_twice", 8);
    let scanned = scan(&directory);
    let names = ids(8);
    let pool = JobPool::new(4);

    let mut store = AssetStore::new();
    store.load_all_in_parallel(
        &scanned.catalogue,
        &directory,
        names.iter().map(String::as_str),
        &pool,
    );
    let after_first = store.len();

    // Change a file on disk. A correct second call notices nothing, because the id is resident.
    std::fs::write(directory.join("thing_000.txt"), b"replaced").expect("writes");
    store.load_all_in_parallel(
        &scanned.catalogue,
        &directory,
        names.iter().map(String::as_str),
        &pool,
    );

    assert_eq!(store.len(), after_first);
    let bytes = &store.get("thing_000").expect("resident").bytes;
    assert!(
        bytes.len() > 8,
        "the resident bytes should be the original, not the replacement"
    );
}

#[test]
fn asking_for_nothing_submits_nothing() {
    // A barrier on an empty set of jobs is harmless but pointless, and this is the common case for
    // a scene that declares no assets at all.
    let directory = asset_directory("parallel_empty", 0);
    let scanned = scan(&directory);
    let pool = JobPool::new(4);

    let mut store = AssetStore::new();
    store.load_all_in_parallel(&scanned.catalogue, &directory, std::iter::empty(), &pool);
    assert_eq!(store.len(), 0);
    assert_eq!(pool.pending(), 0);
}
