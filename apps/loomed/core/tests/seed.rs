//! Offline tests for `just loomed-seed` / `loomed-seed`.
//!
//! Full HTTP seeding needs a running knot (`just knot`). This file covers
//! keys/path errors and the same wrap + SIWS + send + poll path as
//! `sealed_poll` / `knot_live`, against an in-process `MockKnot`.

use std::sync::{Arc, Mutex};

use loomed_core::identity::{FileKeyStore, KeyStore, LoomedKeys};
use loomed_core::replica::Replica;
use loomed_core::seed::{
    keys_path_in, load_local_keys, open_and_send, siws_client_on, wrap_for_member, SeedOpts,
};
use loomed_core::sync::Sync;
use loomed_core::Chat;
use tangled::http::KnotClient;
use tangled::MockKnot;

fn keys(ed: u8, x: u8) -> LoomedKeys {
    LoomedKeys::from_seeds([ed; 32], [x; 32])
}

fn temp_dir(label: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "loomed-seed-test-{}-{}-{}",
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
    let home = std::path::Path::new("/tmp/loomed-home-fixture");
    assert_eq!(keys_path_in(home), home.join("keys.json"));
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
        msg.contains("generate") || msg.contains("Loomed") || msg.contains("--create-local"),
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

#[test]
fn wrap_for_member_is_sys_circle_wrap() {
    let owner = keys(1, 2);
    let member = keys(3, 4);
    let chat = Chat {
        keys: owner.clone(),
    };
    let (circle, sys) = chat
        .create_circle(std::slice::from_ref(&member.did()))
        .unwrap();
    let dir = temp_dir("wrap");
    let mut replica = Replica::open(&dir).unwrap();
    replica
        .apply_record(&serde_json::to_vec(&sys).unwrap())
        .unwrap();
    let state = replica.circle_state(&circle).unwrap();
    let wrap = wrap_for_member(&owner, &member, &circle.0, &state).unwrap();
    assert_eq!(wrap["collection"], "sys.circle.wrap");
    assert_eq!(wrap["circle"], circle.0);
    assert_eq!(wrap["author"], owner.did().as_str());
    assert_eq!(wrap["vis"], "sealed");
    assert!(wrap["body"]["enc"].as_str().is_some());
    assert!(wrap["body"]["ct"].as_str().is_some());
}

#[tokio::test]
async fn seed_unreachable_knot_fails() {
    let dir = temp_dir("down");
    let path = keys_path_in(&dir);
    let _ = load_local_keys(&path, true).unwrap();
    let err = loomed_core::seed::run(SeedOpts {
        knot: "http://127.0.0.1:9".into(),
        keys_path: path,
        create_local: false,
    })
    .await
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("unreachable") || msg.contains("knot"), "{msg}");
}

/// Same sequence as `knot_live` / `sealed_poll`: peer owns the circle, wrap for
/// local, SIWS putRecord, local `poll_once` sees plaintext.
#[tokio::test]
async fn seed_path_inbound_after_poll() {
    let local = keys(11, 12);
    let bob = keys(13, 14);
    let carol = keys(15, 16);
    let knot_did = keys(17, 18).did().as_str();

    let mut engine = MockKnot::hosting(&knot_did);
    engine.now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let knot = Arc::new(Mutex::new(engine));

    let bob_client = siws_client_on(KnotClient::mock_shared(knot.clone()), &bob)
        .await
        .unwrap();
    let (dm, n) = open_and_send(
        &bob,
        std::slice::from_ref(&local),
        &["hey from bob", "second from bob"],
        &bob_client,
    )
    .await
    .unwrap();
    assert!(n >= 4, "sys.circle + wrap + 2 chats, drained {n}");

    let carol_client = siws_client_on(KnotClient::mock_shared(knot.clone()), &carol)
        .await
        .unwrap();
    let (group, gn) = open_and_send(
        &carol,
        &[local.clone(), bob.clone()],
        &["group hello"],
        &carol_client,
    )
    .await
    .unwrap();
    assert!(gn >= 4, "sys.circle + 2 wraps + chat, drained {gn}");

    let dir = temp_dir("poll");
    let mut replica = Replica::open(&dir).unwrap();
    let mut sync = Sync::mock(knot);
    sync.poll_once(&mut replica, &local).await.unwrap();
    assert!(
        !replica.conversations().is_empty(),
        "poll_once should ingest public sys.circle via loom_query"
    );

    let dm_msgs = replica.messages(&dm);
    assert!(
        dm_msgs
            .iter()
            .any(|m| m.text.as_deref() == Some("hey from bob")),
        "local should decrypt Bob's DM after poll_once, got {dm_msgs:?}"
    );
    let group_msgs = replica.messages(&group);
    assert!(
        group_msgs
            .iter()
            .any(|m| m.text.as_deref() == Some("group hello")),
        "local should decrypt group after poll_once, got {group_msgs:?}"
    );
}

/// `run()` also seeds a local→bob send. After Refresh the owner must unwrap
/// their own `sys.circle.wrap` and open that outbound chat.
#[tokio::test]
async fn seed_path_owner_opens_own_send_after_poll() {
    let local = keys(21, 22);
    let bob = keys(23, 24);
    let knot_did = keys(25, 26).did().as_str();

    let mut engine = MockKnot::hosting(&knot_did);
    engine.now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let knot = Arc::new(Mutex::new(engine));

    let local_client = siws_client_on(KnotClient::mock_shared(knot.clone()), &local)
        .await
        .unwrap();
    let (circle, n) = open_and_send(
        &local,
        std::slice::from_ref(&bob),
        &["thanks, got it"],
        &local_client,
    )
    .await
    .unwrap();
    assert!(
        n >= 4,
        "sys.circle + owner wrap + bob wrap + chat, drained {n}"
    );

    let dir = temp_dir("owner-poll");
    let mut replica = Replica::open(&dir).unwrap();
    let mut sync = Sync::mock(knot);
    sync.poll_once(&mut replica, &local).await.unwrap();
    assert!(
        replica.circle_state(&circle).is_some(),
        "poll_once should ingest the public sys.circle the local user authored"
    );

    let state = replica.circle_state(&circle).expect("circle after poll");
    assert!(
        state.epoch_secrets.contains_key(&state.epoch),
        "owner wrap must install the epoch secret so open_message can succeed"
    );

    let msgs = replica.messages(&circle);
    assert!(
        msgs.iter().any(
            |m| m.text.as_deref() == Some("thanks, got it") && m.author == local.did().as_str()
        ),
        "owner should decrypt their own seeded send after poll_once, got {msgs:?}"
    );
}
