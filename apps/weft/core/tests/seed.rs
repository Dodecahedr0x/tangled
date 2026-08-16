//! Offline tests for `just weft-seed` / `weft-seed`.
//!
//! Process orchestration lives in the justfile. This file covers path
//! layout, unreachable knots, and public-record drain against in-process
//! `MockKnot`.

use std::sync::{Arc, Mutex};

use tangled::http::KnotClient;
use tangled::{Identity, MockKnot};
use weft_core::seed::{
    knot_path_in, persist_knot_if_missing, publish_public, siws_client_on, SeedOpts,
};
use weft_core::Explorer;

fn identity(ed: u8, x: u8) -> Identity {
    Identity::from_seeds([ed; 32], [x; 32])
}

fn temp_dir(label: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "weft-seed-test-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        label
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn knot_path_matches_app_layout() {
    let home = std::path::Path::new("/tmp/weft-home-fixture");
    assert_eq!(knot_path_in(home), home.join("knot.txt"));
}

#[test]
fn persist_knot_if_missing_does_not_clobber() {
    let dir = temp_dir("knot");
    let path = knot_path_in(&dir);
    persist_knot_if_missing(&path, "http://127.0.0.1:3000").unwrap();
    persist_knot_if_missing(&path, "http://127.0.0.1:9").unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap().trim(),
        "http://127.0.0.1:3000"
    );
}

#[tokio::test]
async fn seed_unreachable_knot_fails() {
    let dir = temp_dir("down");
    let err = weft_core::seed::run(SeedOpts {
        knot: "http://127.0.0.1:9".into(),
        knot_path: knot_path_in(&dir),
    })
    .await
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("unreachable") || msg.contains("knot"), "{msg}");
}

/// Seed helpers publish public posts; `node.stats` then has non-zero counts.
#[tokio::test]
async fn seed_public_records_show_in_stats() {
    let ada = identity(41, 42);
    let bob = identity(43, 44);
    let knot_did = identity(45, 46).did_string();

    let mut engine = MockKnot::hosting(&knot_did);
    engine.now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let knot = Arc::new(Mutex::new(engine));

    let ada_client = siws_client_on(KnotClient::mock_shared(knot.clone()), &ada)
        .await
        .unwrap();
    let ada_n = publish_public(&ada, &ada_client, &["ada on the mesh", "ada again"])
        .await
        .unwrap();
    assert!(ada_n >= 2, "ada posts drained {ada_n}");

    let bob_client = siws_client_on(KnotClient::mock_shared(knot.clone()), &bob)
        .await
        .unwrap();
    let bob_n = publish_public(&bob, &bob_client, &["bob on the mesh"])
        .await
        .unwrap();
    assert!(bob_n >= 1, "bob post drained {bob_n}");

    let s = Explorer::mock(knot).local_stats().await.unwrap();
    assert!(s.counts.records >= 3, "records {}", s.counts.records);
    assert!(
        s.counts.identities >= 2,
        "identities {}",
        s.counts.identities
    );
    assert!(s.counts.repos >= 2, "repos {}", s.counts.repos);
    assert!(s.topology.hosted.iter().any(|h| h.did == ada.did_string()));
    assert!(s.topology.hosted.iter().any(|h| h.did == bob.did_string()));
}
