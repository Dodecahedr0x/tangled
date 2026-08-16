use std::sync::{Arc, Mutex};

use loomed_core::chat::Chat;
use loomed_core::handle::parse_peer;
use loomed_core::identity::{FileKeyStore, KeyStore, LoomedKeys};
use loomed_core::outbox::Outbox;
use loomed_core::receipts::{is_receipt, seal_receipt};
use loomed_core::replica::Replica;
use loomed_core::search::search;
use loomed_core::sync::Sync;
use loomed_core::types::{KnotUrl, Peer};
use tangled::circle::CircleState;
use tangled::http::{KnotClient, PutRecordRequest};
use tangled::MockKnot;

fn keys(ed: u8, x: u8) -> LoomedKeys {
    LoomedKeys::from_seeds([ed; 32], [x; 32])
}

fn rkey_of(circle: &loomed_core::types::CircleRef) -> &str {
    circle.0.split('#').nth(1).expect("circle ref")
}

fn circle_state(owner: &str, rkey: &str, extra: &[&str]) -> CircleState {
    let mut st = CircleState::create(
        owner.to_string(),
        rkey.to_string(),
        "chat".into(),
        [9u8; 32],
    );
    for (i, m) in extra.iter().enumerate() {
        let mut secret = [10u8; 32];
        secret[0] = i as u8;
        st.add_member(m.to_string(), secret, 1_700_000_000)
            .expect("add member");
    }
    st
}

#[test]
fn identity_generate_import_keystore() {
    let a = LoomedKeys::generate();
    let b = LoomedKeys::generate();
    assert_ne!(a.did().as_str(), b.did().as_str());

    let json = a.export_json();
    let imported = LoomedKeys::from_solana_json(&json).unwrap();
    assert_eq!(a.did(), imported.did());
    assert_eq!(
        a.identity().agreement_public_bytes(),
        imported.identity().agreement_public_bytes()
    );

    let dir = std::env::temp_dir().join(format!("loomed-ks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("keys.json");
    let store = FileKeyStore { path: path.clone() };
    assert!(store.load().unwrap().is_none());
    store.save(&a).unwrap();
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.did(), a.did());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[test]
fn parse_peer_did_and_handle() {
    let keys = keys(1, 2);
    let did = keys.did();
    let did_s = did.as_str();
    match parse_peer(&did_s).unwrap() {
        Peer::Did(d) => assert_eq!(d, did),
        Peer::Handle(_) => panic!("expected DID"),
    }
    match parse_peer("alice.example.com").unwrap() {
        Peer::Handle(h) => assert_eq!(h, "alice.example.com"),
        Peer::Did(_) => panic!("expected handle"),
    }
    assert!(parse_peer("").is_err());
}

#[tokio::test]
async fn mockknot_dm_group_blob_search_receipt_outbox() {
    let alice = keys(1, 2);
    let bob = keys(3, 4);
    let carol = keys(5, 6);
    let alice_did = alice.did();
    let bob_did = bob.did();
    let carol_did = carol.did();

    let knot = Arc::new(Mutex::new(MockKnot::hosting(&alice_did.to_string())));
    let client = KnotClient::mock_shared(knot.clone());
    let chat = Chat {
        keys: alice.clone(),
    };

    let (dm, sys_dm) = chat.create_circle(std::slice::from_ref(&bob_did)).unwrap();
    assert_eq!(sys_dm["collection"], "sys.circle");
    let members = sys_dm["body"]["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);

    let mut outbox = Outbox::new();
    outbox.enqueue_record(sys_dm.clone()).unwrap();

    let st = circle_state(
        &alice_did.to_string(),
        rkey_of(&dm),
        &[&bob_did.to_string()],
    );
    let sealed = chat.seal_text(&dm, &st, "hello bob").unwrap();
    assert_eq!(sealed["collection"], "pub.tangled.chat");
    assert_eq!(sealed["vis"], "sealed");
    outbox.enqueue_record(sealed.clone()).unwrap();

    let blob = client.put_blob(b"\x89PNG-fake".to_vec()).await.unwrap();
    let img = chat.seal_image(&dm, &st, &blob.cid).unwrap();
    outbox.enqueue_record(img.clone()).unwrap();

    let drained = outbox.drain(&client).await.unwrap();
    assert!(drained >= 3, "outbox drained {drained}");

    let (group, sys_g) = chat
        .create_circle(&[bob_did.clone(), carol_did.clone()])
        .unwrap();
    assert_eq!(sys_g["body"]["members"].as_array().unwrap().len(), 3);
    let gst = circle_state(
        &alice_did.to_string(),
        rkey_of(&group),
        &[&bob_did.to_string(), &carol_did.to_string()],
    );
    let gsealed = chat.seal_text(&group, &gst, "hello group").unwrap();

    let opened_dm = chat.open_message(&st, &sealed).unwrap();
    assert_eq!(opened_dm.text.as_deref(), Some("hello bob"));
    let opened_img = chat.open_message(&st, &img).unwrap();
    assert_eq!(opened_img.blob_cid.as_deref(), Some(blob.cid.as_str()));
    let opened_g = chat.open_message(&gst, &gsealed).unwrap();
    assert_eq!(opened_g.text.as_deref(), Some("hello group"));

    let dir = std::env::temp_dir().join(format!("loomed-rep-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut replica = Replica::open(&dir).unwrap();
    replica
        .apply_record(&serde_json::to_vec(&sys_dm).unwrap())
        .unwrap();
    replica
        .apply_record(&serde_json::to_vec(&opened_dm).unwrap())
        .unwrap();
    replica
        .apply_record(&serde_json::to_vec(&opened_img).unwrap())
        .unwrap();
    replica
        .apply_record(&serde_json::to_vec(&opened_g).unwrap())
        .unwrap();

    assert!(!replica.conversations().is_empty());
    let hits = search(&replica, "HeLLo BoB");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].text.as_deref(), Some("hello bob"));

    let receipt = seal_receipt(&bob, &dm, &st, &opened_dm.id).unwrap();
    assert!(is_receipt(&receipt));
    assert_eq!(receipt["collection"], "pub.loomed.receipt");
    assert_eq!(receipt["ref"], opened_dm.id.0);
    replica
        .apply_record(&serde_json::to_vec(&receipt).unwrap())
        .unwrap();
    let msgs = replica.messages(&dm);
    let got = msgs.iter().find(|m| m.id == opened_dm.id).unwrap();
    assert!(got.receipts.iter().any(|d| d == &bob_did.to_string()));

    let sync_dir = std::env::temp_dir().join(format!("loomed-sync-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&sync_dir);
    let mut replica2 = Replica::open(&sync_dir).unwrap();
    let mut sync = Sync::mock(knot);
    sync.poll_once(&mut replica2, &alice).await.unwrap();
    assert!(
        !replica2.conversations().is_empty(),
        "poll_once should ingest public sys.circle"
    );

    let _http = Sync::http(KnotUrl("http://127.0.0.1:9".into())).unwrap();
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[tokio::test]
async fn sealed_poll() {
    let alice = keys(1, 2);
    let bob = keys(3, 4);
    let alice_did = alice.did();
    let bob_did = bob.did();

    let mut engine = MockKnot::hosting(&alice_did.to_string());
    engine.now = unix_secs();
    let knot = Arc::new(Mutex::new(engine));
    let client = KnotClient::mock_shared(knot.clone());
    let chat = Chat {
        keys: alice.clone(),
    };

    let (dm, sys_dm) = chat.create_circle(std::slice::from_ref(&bob_did)).unwrap();
    let st = circle_state(
        &alice_did.to_string(),
        rkey_of(&dm),
        &[&bob_did.to_string()],
    );
    let sealed = chat.seal_text(&dm, &st, "secret dm").unwrap();

    let secret = *st.epoch_secrets.get(&st.epoch).expect("epoch secret");
    let (enc, ct) = tangled::hpke::wrap_epoch_secret(
        &bob.identity().agreement_public_bytes(),
        &dm.0,
        st.epoch,
        &secret,
    )
    .unwrap();
    let wrap_rkey = format!(
        "{}-{}",
        st.epoch,
        tangled::identity::encode_pubkey_base58(&bob.identity().pubkey_bytes())
    );
    let wrap = serde_json::json!({
        "v": 1,
        "type": tangled::Cid::dag_cbor(b"sys.circle.wrap").to_string(),
        "collection": "sys.circle.wrap",
        "rkey": wrap_rkey,
        "author": alice_did.to_string(),
        "ts": unix_secs() * 1_000_000,
        "prev": [],
        "vis": "sealed",
        "circle": dm.0,
        "epoch": st.epoch,
        "body": {
            "alg": "hpke-record",
            "enc": hex::encode(&enc),
            "ct": hex::encode(&ct),
            "epoch": st.epoch,
        }
    });
    let wrap = tangled::record::sign_json_record(alice.identity().signing_key(), &wrap)
        .unwrap()
        .json;

    client
        .put_record(&PutRecordRequest { record: sys_dm })
        .await
        .unwrap();
    client
        .put_record(&PutRecordRequest { record: wrap })
        .await
        .unwrap();
    client
        .put_record(&PutRecordRequest { record: sealed })
        .await
        .unwrap();

    let dir = std::env::temp_dir().join(format!("loomed-sealed-poll-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut replica = Replica::open(&dir).unwrap();
    let mut sync = Sync::mock(knot);
    sync.poll_once(&mut replica, &bob).await.unwrap();

    let msgs = replica.messages(&dm);
    assert!(
        msgs.iter().any(|m| m.text.as_deref() == Some("secret dm")),
        "Bob's replica should contain Alice's sealed DM plaintext, got {msgs:?}"
    );
}

#[test]
fn send_text() {
    let alice = keys(1, 2);
    let bob = keys(3, 4);
    let chat = Chat {
        keys: alice.clone(),
    };
    let dir = std::env::temp_dir().join(format!("loomed-send-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut replica = Replica::open(&dir).unwrap();
    let mut outbox = Outbox::new();
    let (circle, sys) = chat
        .create_circle(std::slice::from_ref(&bob.did()))
        .unwrap();
    replica
        .apply_record(&serde_json::to_vec(&sys).unwrap())
        .unwrap();
    let id = chat
        .send_text(&mut replica, &mut outbox, &circle, "hello")
        .unwrap();
    let msgs = replica.messages(&circle);
    assert!(
        msgs.iter()
            .any(|m| m.id == id && m.text.as_deref() == Some("hello")),
        "replica should contain plaintext, got {msgs:?}"
    );
    assert!(!outbox.is_empty(), "outbox should hold the sealed record");
}
