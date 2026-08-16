//! Offline tests for `just reed-seed` / `reed-seed`.
//!
//! Full HTTP seeding needs a running knot (`just knot`). This file covers
//! keys/path errors and a post+follow drain against in-process `MockKnot`.

use std::sync::{Arc, Mutex};

use reed_core::identity::{FileKeyStore, KeyStore, ReedKeys};
use reed_core::replica::Replica;
use reed_core::seed::{
    keys_path_in, knot_path_in, load_local_keys, publish_posts_and_follows, siws_client_on,
    SeedOpts,
};
use reed_core::sync::Sync;
use tangled::http::KnotClient;
use tangled::MockKnot;

fn keys(ed: u8, x: u8) -> ReedKeys {
    ReedKeys::from_seeds([ed; 32], [x; 32])
}

fn temp_dir(label: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "reed-seed-test-{}-{}-{}",
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
fn keys_path_matches_app_layout() {
    let home = std::path::Path::new("/tmp/reed-home-fixture");
    assert_eq!(keys_path_in(home), home.join("keys.json"));
    assert_eq!(knot_path_in(home), home.join("knot.txt"));
}

#[test]
fn load_local_missing_is_clear_error() {
    let dir = temp_dir("missing");
    let path = keys_path_in(&dir);
    let err = match load_local_keys(&path, false) {
        Ok(_) => panic!("expected missing keys.json to fail"),
        Err(e) => e,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("generate") || msg.contains("Reed") || msg.contains("--create-local"),
        "{msg}"
    );
}

#[test]
fn create_local_writes_compatible_keys_without_clobber() {
    let dir = temp_dir("create");
    let path = keys_path_in(&dir);
    let first = load_local_keys(&path, true).unwrap();
    let again = load_local_keys(&path, true).unwrap();
    assert_eq!(first.did(), again.did());
    let store = FileKeyStore { path: path.clone() };
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.did(), first.did());
}

#[tokio::test]
async fn seed_unreachable_knot_fails() {
    let dir = temp_dir("down");
    let path = keys_path_in(&dir);
    let _ = load_local_keys(&path, true).unwrap();
    let err = reed_core::seed::run(SeedOpts {
        knot: "http://127.0.0.1:9".into(),
        keys_path: path,
        create_local: false,
    })
    .await
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("unreachable") || msg.contains("knot"), "{msg}");
}

#[test]
fn default_ap_acct_is_public_mastodon() {
    let acct = reed_core::seed::DEFAULT_AP_ACCT;
    assert!(acct.starts_with("acct:"), "{acct}");
    assert!(acct.contains('@'), "{acct}");
    assert_eq!(acct, "acct:Gargron@mastodon.social");
}

#[tokio::test]
async fn ap_seed_unreachable_knot_fails() {
    let dir = temp_dir("ap-down");
    let path = keys_path_in(&dir);
    let _ = load_local_keys(&path, true).unwrap();
    let err = reed_core::seed::run_ap(SeedOpts {
        knot: "http://127.0.0.1:9".into(),
        keys_path: path,
        create_local: false,
    })
    .await
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("unreachable") || msg.contains("knot"), "{msg}");
}

/// Seed helpers publish a post and a follow; after `poll_once` Home sees both.
#[tokio::test]
async fn seed_post_and_follow_lands_on_mockknot() {
    let alice = keys(31, 32);
    let bob = keys(33, 34);
    let knot_did = keys(35, 36).did().as_str();

    let mut engine = MockKnot::hosting(&knot_did);
    engine.now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let knot = Arc::new(Mutex::new(engine));

    let bob_client = siws_client_on(KnotClient::mock_shared(knot.clone()), &bob)
        .await
        .unwrap();
    let (bob_posts, bob_n) =
        publish_posts_and_follows(&bob, &bob_client, &["bob says hi"], &[], None)
            .await
            .unwrap();
    assert!(bob_n >= 1, "bob post drained {bob_n}");
    let bob_post = bob_posts.first().expect("bob post id").clone();

    let alice_client = siws_client_on(KnotClient::mock_shared(knot.clone()), &alice)
        .await
        .unwrap();
    let (alice_posts, alice_n) = publish_posts_and_follows(
        &alice,
        &alice_client,
        &["hello square"],
        std::slice::from_ref(&bob.did()),
        None,
    )
    .await
    .unwrap();
    assert!(alice_n >= 2, "post + follow, drained {alice_n}");
    let alice_post = alice_posts.first().expect("alice post id").clone();

    let dir = temp_dir("poll");
    let mut replica = Replica::open(&dir).unwrap();
    let mut sync = Sync::mock(knot);
    sync.poll_once(&mut replica, &alice).await.unwrap();
    assert!(
        replica.post(&alice_post).is_some(),
        "alice post should ingest"
    );
    assert!(
        replica.post(&bob_post).is_some(),
        "follow should pull bob's post"
    );
    let me = alice.did().as_str();
    assert!(
        replica.home(&me).iter().any(|i| i.post.id == bob_post),
        "bob's post should appear on alice home"
    );
}
