use std::sync::{Arc, Mutex};

use reed_core::graph::{block, follow, unblock, unfollow};
use reed_core::handle::parse_peer;
use reed_core::identity::{FileKeyStore, KeyStore, ReedKeys};
use reed_core::outbox::Outbox;
use reed_core::search::{mentions, search};
use reed_core::social::{boost, like, mute, put_profile, unboost, unlike, unmute};
use reed_core::sync::Sync;
use reed_core::types::{KnotUrl, Peer, PostId, Profile, TimelineReason};
use reed_core::{PostApi, Replica};
use tangled::http::KnotClient;
use tangled::MockKnot;

fn keys(ed: u8, x: u8) -> ReedKeys {
    ReedKeys::from_seeds([ed; 32], [x; 32])
}

fn tmp(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "reed-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn identity_generate_import_keystore() {
    let a = ReedKeys::generate();
    let b = ReedKeys::generate();
    assert_ne!(a.did().as_str(), b.did().as_str());

    let json = a.export_json();
    let imported = ReedKeys::from_solana_json(&json).unwrap();
    assert_eq!(a.did(), imported.did());
    assert_eq!(
        a.identity().agreement_public_bytes(),
        imported.identity().agreement_public_bytes()
    );

    let dir = tmp("ks");
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
    let k = keys(1, 2);
    let did = k.did();
    let did_s = did.as_str();
    match parse_peer(&did_s).unwrap() {
        Peer::Did(d) => assert_eq!(d, did),
        Peer::Handle(_) => panic!("expected DID"),
    }
    match parse_peer("alice.example.com").unwrap() {
        Peer::Handle(h) => assert_eq!(h, "alice.example.com"),
        Peer::Did(_) => panic!("expected handle"),
    }
    match parse_peer("acct:alice@example.com").unwrap() {
        Peer::Handle(h) => assert_eq!(h, "acct:alice@example.com"),
        Peer::Did(_) => panic!("expected handle"),
    }
    match parse_peer("@alice@example.com").unwrap() {
        Peer::Handle(h) => assert_eq!(h, "acct:alice@example.com"),
        Peer::Did(_) => panic!("expected handle"),
    }
    match parse_peer("https://example.com/users/alice").unwrap() {
        Peer::Handle(h) => assert_eq!(h, "https://example.com/users/alice"),
        Peer::Did(_) => panic!("expected handle"),
    }
    assert!(parse_peer("").is_err());
}

#[tokio::test]
async fn mockknot_resolves_acct() {
    use reed_core::resolve_peer;
    let alice = keys(1, 2);
    let knot = MockKnot::hosting(&alice.did().to_string());
    let client = KnotClient::mock(knot);
    let peer = parse_peer("acct:alice@example.com").unwrap();
    let did = resolve_peer(&client, &peer).await.unwrap();
    assert!(did.as_str().starts_with("did:sol:"));
    let again = resolve_peer(&client, &parse_peer("acct:alice@example.com").unwrap())
        .await
        .unwrap();
    assert_eq!(did, again);
}

#[test]
fn attested_foreign_marks_post() {
    let alice = keys(1, 2);
    let dir = tmp("attested");
    let mut replica = Replica::open(&dir).unwrap();
    let mut outbox = Outbox::new();
    let api = PostApi {
        keys: alice.clone(),
    };
    let id = api
        .compose_text(&mut replica, &mut outbox, "federated note", None, None)
        .unwrap();
    let post = replica.post(&id).unwrap();
    assert!(!post.attested);
    let env = serde_json::json!({
        "v": 1,
        "type": "sys.foreign",
        "collection": "sys.foreign",
        "rkey": post.rkey,
        "author": alice.did().as_str(),
        "ts": 1,
        "prev": [],
        "vis": "public",
        "body": {
            "protocol": "activitypub",
            "foreign_id": "https://ex.example/users/a/s/1",
            "attested": true,
            "mapped": id.0
        },
        "sig": "00".repeat(32)
    });
    replica
        .apply_record(&serde_json::to_vec(&env).unwrap())
        .unwrap();
    assert!(replica.post(&id).unwrap().attested);
}

#[test]
fn federated_note_exposes_acct_handle_and_url() {
    let alice = keys(3, 4);
    let dir = tmp("ap-acct");
    let mut replica = Replica::open(&dir).unwrap();
    let mut outbox = Outbox::new();
    let api = PostApi {
        keys: alice.clone(),
    };
    let id = api
        .compose_text(&mut replica, &mut outbox, "hello from mastodon", None, None)
        .unwrap();
    let post = replica.post(&id).unwrap();
    let did = alice.did().as_str();
    let note_url = "https://mastodon.social/@Gargron/114";
    let actor_url = "https://mastodon.social/users/Gargron";
    let env = serde_json::json!({
        "v": 1,
        "type": "sys.foreign",
        "collection": "sys.foreign",
        "rkey": post.rkey,
        "author": did,
        "ts": 1,
        "prev": [],
        "vis": "public",
        "body": {
            "protocol": "activitypub",
            "foreign_id": note_url,
            "foreign": actor_url,
            "acct": "Gargron@mastodon.social",
            "attested": true,
            "mapped": id.0
        },
        "sig": "00".repeat(32)
    });
    replica
        .apply_record(&serde_json::to_vec(&env).unwrap())
        .unwrap();
    let profile = serde_json::json!({
        "v": 1,
        "type": "pub.reed.profile",
        "collection": "pub.reed.profile",
        "rkey": "self",
        "author": did,
        "ts": 2,
        "prev": [],
        "vis": "public",
        "body": {
            "displayName": "Eugen Rochko",
            "bio": "",
            "handle": "Gargron@mastodon.social",
            "url": actor_url
        },
        "sig": "00".repeat(32)
    });
    replica
        .apply_record(&serde_json::to_vec(&profile).unwrap())
        .unwrap();

    let post = replica.post(&id).unwrap();
    assert_eq!(post.handle.as_deref(), Some("Gargron@mastodon.social"));
    assert_eq!(post.source_url.as_deref(), Some(note_url));
    assert!(post.attested);

    let prof = replica.profile(&did).unwrap();
    assert_eq!(prof.handle.as_deref(), Some("Gargron@mastodon.social"));
    assert_eq!(prof.display_name.as_deref(), Some("Eugen Rochko"));
    assert_eq!(prof.source_url.as_deref(), Some(actor_url));
    assert!(
        prof.source_url
            .as_deref()
            .is_some_and(|u| u.starts_with("https://")),
        "profile url: {:?}",
        prof.source_url
    );
}

#[test]
fn federated_profile_maps_actor_fields_and_avatar_blob() {
    let actor = serde_json::json!({
        "id": "https://mastodon.social/users/Gargron",
        "type": "Person",
        "preferredUsername": "Gargron",
        "name": "Eugen Rochko",
        "summary": "<p>Founder of <a href=\"https://joinmastodon.org\">Mastodon</a></p>",
        "icon": {
            "type": "Image",
            "url": "https://files.mastodon.social/accounts/avatars/gargron.png"
        }
    });
    let actor_id = "https://mastodon.social/users/Gargron";
    let mapped = tangled_ap::actor_to_profile(&actor, actor_id);
    assert_eq!(mapped.display_name, "Eugen Rochko");
    assert_eq!(mapped.handle.as_deref(), Some("Gargron@mastodon.social"));
    assert_eq!(mapped.bio, "Founder of Mastodon");
    assert_eq!(mapped.url, actor_id);
    assert!(mapped.icon_url.is_some());

    let did = tangled::foreign::activitypub_actor_identity(actor_id).did_string();
    let dir = tmp("ap-profile-full");
    let mut replica = Replica::open(&dir).unwrap();
    let png = b"\x89PNG-avatar";
    let cid = tangled::Cid::raw(png).to_string();
    replica.store_blob(&cid, png).unwrap();
    let profile = serde_json::json!({
        "v": 1,
        "type": "pub.reed.profile",
        "collection": "pub.reed.profile",
        "rkey": "self",
        "author": did,
        "ts": 2,
        "prev": [],
        "vis": "public",
        "body": {
            "displayName": mapped.display_name,
            "bio": mapped.bio,
            "handle": mapped.handle,
            "url": mapped.url,
            "avatar": cid
        },
        "sig": "00".repeat(32)
    });
    replica
        .apply_record(&serde_json::to_vec(&profile).unwrap())
        .unwrap();
    let older = serde_json::json!({
        "v": 1,
        "type": "pub.reed.profile",
        "collection": "pub.reed.profile",
        "rkey": "self",
        "author": did,
        "ts": 1,
        "prev": [],
        "vis": "public",
        "body": {
            "displayName": "Old Name",
            "bio": "stale",
            "handle": "old@ex.example",
            "url": actor_id
        },
        "sig": "00".repeat(32)
    });
    replica
        .apply_record(&serde_json::to_vec(&older).unwrap())
        .unwrap();

    let prof = replica.profile(&did).unwrap();
    assert_eq!(prof.display_name.as_deref(), Some("Eugen Rochko"));
    assert_eq!(prof.handle.as_deref(), Some("Gargron@mastodon.social"));
    assert_eq!(prof.bio.as_deref(), Some("Founder of Mastodon"));
    assert_eq!(prof.source_url.as_deref(), Some(actor_id));
    assert_eq!(prof.avatar_cid.as_deref(), Some(cid.as_str()));
    assert_eq!(replica.blob(&cid).as_deref(), Some(png.as_slice()));
}

#[test]
fn ap_like_of_native_post_is_not_attested() {
    let alice = keys(1, 2);
    let dir = tmp("like-not-attested");
    let mut replica = Replica::open(&dir).unwrap();
    let mut outbox = Outbox::new();
    let api = PostApi {
        keys: alice.clone(),
    };
    let id = api
        .compose_text(&mut replica, &mut outbox, "native note", None, None)
        .unwrap();
    assert!(!replica.post(&id).unwrap().attested);
    let env = serde_json::json!({
        "v": 1,
        "type": "sys.foreign",
        "collection": "sys.foreign",
        "rkey": "like-fk1",
        "author": alice.did().as_str(),
        "ts": 2,
        "prev": [],
        "vis": "public",
        "body": {
            "protocol": "activitypub",
            "foreign_id": "https://ex.example/users/b/likes/1",
            "attested": true,
            "mapped": id.0
        },
        "sig": "00".repeat(32)
    });
    replica
        .apply_record(&serde_json::to_vec(&env).unwrap())
        .unwrap();
    assert!(
        !replica.post(&id).unwrap().attested,
        "AP like of a native post must not set Post.attested"
    );
}

#[test]
fn mentions_at_handles() {
    assert_eq!(
        mentions("hi @bob.sol and @carol"),
        vec!["bob.sol".to_string(), "carol".to_string()]
    );
}

#[tokio::test]
async fn mockknot_post_follow_reply_quote_like_boost_media_search_outbox() {
    let alice = keys(1, 2);
    let bob = keys(3, 4);
    let alice_did = alice.did();
    let bob_did = bob.did();
    let me = alice_did.as_str();

    let knot = Arc::new(Mutex::new(MockKnot::hosting(&alice_did.to_string())));
    let client = KnotClient::mock_shared(knot.clone());
    let dir = tmp("rep");
    let mut replica = Replica::open(&dir).unwrap();
    let mut outbox = Outbox::new();
    let api = PostApi {
        keys: alice.clone(),
    };

    let post_id = api
        .compose_text(&mut replica, &mut outbox, "hello square", None, None)
        .unwrap();
    let got = replica.post(&post_id).unwrap();
    assert_eq!(got.text, "hello square");
    assert!(got.reply_to.is_none());
    assert!(got.quote_of.is_none());

    follow(&alice, &mut replica, &mut outbox, &bob_did, None).unwrap();

    let reply_id = api
        .compose_text(&mut replica, &mut outbox, "a reply", Some(&post_id), None)
        .unwrap();
    let reply = replica.post(&reply_id).unwrap();
    assert_eq!(
        reply.reply_to.as_ref().map(|p| p.0.as_str()),
        Some(post_id.0.as_str())
    );
    assert!(reply.quote_of.is_none());

    let quote_id = api
        .compose_text(&mut replica, &mut outbox, "quoting", None, Some(&post_id))
        .unwrap();
    let quote = replica.post(&quote_id).unwrap();
    assert_eq!(
        quote.quote_of.as_ref().map(|p| p.0.as_str()),
        Some(post_id.0.as_str())
    );
    assert!(quote.reply_to.is_none(), "quote must not set body reply");

    like(&alice, &mut replica, &mut outbox, &post_id).unwrap();
    boost(&alice, &mut replica, &mut outbox, &post_id).unwrap();
    let liked = replica.home(&me);
    let src = liked
        .iter()
        .find(|i| i.post.id == post_id && matches!(i.reason, TimelineReason::Authored))
        .unwrap();
    assert!(src.post.liked_by_me);
    assert!(src.post.boosted_by_me);
    assert_eq!(src.post.like_count, 1);
    assert_eq!(src.post.boost_count, 1);

    let img = (
        b"\x89PNG-fake".to_vec(),
        "image/png".into(),
        Some("pic".into()),
    );
    let vid = (b"ftypisom".to_vec(), "video/mp4".into(), None);
    let media_id = api
        .compose_with_media(
            &mut replica,
            &mut outbox,
            "media post",
            &[img, vid],
            None,
            None,
        )
        .unwrap();
    let media_post = replica.post(&media_id).unwrap();
    assert_eq!(media_post.media.len(), 2);
    assert_eq!(media_post.media[0].mime, "image/png");
    assert_eq!(media_post.media[1].mime, "video/mp4");

    let five: Vec<(Vec<u8>, String, Option<String>)> = (0..5)
        .map(|i| (vec![i], "image/png".into(), None))
        .collect();
    let err = api
        .compose_with_media(&mut replica, &mut outbox, "too many", &five, None, None)
        .unwrap_err();
    assert!(matches!(err, reed_core::ReedError::MediaLimit));

    replica.set_max_blob(4);
    let err = api
        .compose_with_media(
            &mut replica,
            &mut outbox,
            "big",
            &[(vec![0; 8], "image/png".into(), None)],
            None,
            None,
        )
        .unwrap_err();
    assert!(matches!(err, reed_core::ReedError::MediaTooLarge));
    replica.set_max_blob(134_217_728);

    let hits = search(&replica, "HeLLo SqUaRe");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, post_id);

    let drained = outbox.drain(&client).await.unwrap();
    assert!(drained >= 3, "outbox drained {drained}");

    let quoted = client
        .get_record(&alice_did.to_string(), "pub.tangled.post", &quote.rkey)
        .await
        .unwrap();
    assert_eq!(quoted.record["ref"], post_id.0);
    assert!(quoted.record["body"].get("reply").is_none());

    let replied = client
        .get_record(&alice_did.to_string(), "pub.tangled.post", &reply.rkey)
        .await
        .unwrap();
    assert_eq!(replied.record["body"]["reply"], post_id.0);
    assert!(replied.record.get("ref").is_none());

    let sync_dir = tmp("sync");
    let mut replica2 = Replica::open(&sync_dir).unwrap();
    let mut sync = Sync::mock(knot);
    sync.poll_once(&mut replica2, &alice).await.unwrap();
    assert!(
        replica2.post(&post_id).is_some(),
        "poll_once should ingest public posts"
    );

    let _http = Sync::http(KnotUrl("http://127.0.0.1:9".into())).unwrap();
    let _ = bob;
}

#[tokio::test]
async fn probe_closed_port_is_unreachable() {
    let sync = Sync::http(KnotUrl("http://127.0.0.1:9".into())).unwrap();
    let err = sync.probe().await.unwrap_err();
    assert!(
        matches!(err, reed_core::ReedError::KnotUnreachable),
        "{err}"
    );
}

#[test]
fn mute_block_filter_edit_delete() {
    let alice = keys(1, 2);
    let bob = keys(3, 4);
    let me = alice.did().as_str();
    let dir = tmp("filter");
    let mut replica = Replica::open(&dir).unwrap();
    let mut outbox = Outbox::new();

    follow(&alice, &mut replica, &mut outbox, &bob.did(), None).unwrap();

    let bob_api = PostApi { keys: bob.clone() };
    let bob_post = bob_api
        .compose_text(&mut replica, &mut outbox, "bob says hi", None, None)
        .unwrap();
    assert!(
        replica.home(&me).iter().any(|i| i.post.id == bob_post),
        "followed author should appear on home"
    );

    mute(&alice, &mut replica, &mut outbox, &bob.did()).unwrap();
    assert!(
        replica.home(&me).iter().all(|i| i.post.id != bob_post),
        "muted author hidden on home"
    );
    assert!(
        replica.discover(&me).iter().all(|i| i.post.id != bob_post),
        "muted author hidden on discover"
    );

    unmute(&alice, &mut replica, &mut outbox, &bob.did()).unwrap();
    assert!(replica.home(&me).iter().any(|i| i.post.id == bob_post));

    block(&alice, &mut replica, &mut outbox, &bob.did()).unwrap();
    assert!(replica.home(&me).iter().all(|i| i.post.id != bob_post));
    unblock(&alice, &mut replica, &mut outbox, &bob.did()).unwrap();
    assert!(replica.home(&me).iter().any(|i| i.post.id == bob_post));

    let api = PostApi {
        keys: alice.clone(),
    };
    let own = api
        .compose_text(&mut replica, &mut outbox, "draft", None, None)
        .unwrap();
    api.edit_text(&mut replica, &mut outbox, &own, "edited")
        .unwrap();
    assert_eq!(replica.post(&own).unwrap().text, "edited");
    api.delete(&mut replica, &mut outbox, &own).unwrap();
    let after = replica.post(&own).unwrap();
    assert!(after.deleted);
    assert!(replica.home(&me).iter().all(|i| i.post.id != own));

    unfollow(&alice, &mut replica, &mut outbox, &bob.did()).unwrap();
    unlike(&alice, &mut replica, &mut outbox, &bob_post).ok();
    unboost(&alice, &mut replica, &mut outbox, &bob_post).ok();
    put_profile(
        &alice,
        &mut replica,
        &mut outbox,
        &Profile {
            did: me.clone(),
            handle: None,
            display_name: Some("Ada".into()),
            bio: Some("wires".into()),
            avatar_cid: None,
            source_url: None,
        },
    )
    .unwrap();
    let prof = replica.profile(&me).unwrap();
    assert_eq!(prof.display_name.as_deref(), Some("Ada"));

    let thread = replica.thread(&PostId(bob_post.0.clone()));
    assert!(!thread.is_empty());
}

#[test]
fn edit_text_preserves_media_cids() {
    let alice = keys(5, 6);
    let dir = tmp("edit-media");
    let mut replica = Replica::open(&dir).unwrap();
    let mut outbox = Outbox::new();
    let api = PostApi {
        keys: alice.clone(),
    };
    let img = (
        b"\x89PNG-edit".to_vec(),
        "image/png".into(),
        Some("keep".into()),
    );
    let vid = (b"ftypedit".to_vec(), "video/mp4".into(), None);
    let id = api
        .compose_with_media(
            &mut replica,
            &mut outbox,
            "caption",
            &[img, vid],
            None,
            None,
        )
        .unwrap();
    let before = replica.post(&id).unwrap();
    let media_cids: Vec<String> = before.media.iter().map(|m| m.cid.clone()).collect();
    assert_eq!(media_cids.len(), 2);

    api.edit_text(&mut replica, &mut outbox, &id, "new caption")
        .unwrap();
    let after = replica.post(&id).unwrap();
    assert_eq!(after.text, "new caption");
    assert_eq!(after.media.len(), 2);
    assert_eq!(
        after
            .media
            .iter()
            .map(|m| m.cid.clone())
            .collect::<Vec<_>>(),
        media_cids
    );
    assert_eq!(after.media[0].mime, "image/png");
    assert_eq!(after.media[0].alt.as_deref(), Some("keep"));
    assert_eq!(after.media[1].mime, "video/mp4");
}

#[tokio::test]
async fn poll_once_uses_per_follow_knot() {
    let alice = keys(7, 8);
    let bob = keys(9, 10);
    let alice_did = alice.did();
    let bob_did = bob.did();

    let alice_knot = Arc::new(Mutex::new(MockKnot::hosting(&alice_did.to_string())));
    let bob_knot = Arc::new(Mutex::new(MockKnot::hosting(&bob_did.to_string())));
    let bob_client = KnotClient::mock_shared(bob_knot.clone());

    let mut bob_replica = Replica::open(&tmp("bob-remote")).unwrap();
    let mut bob_outbox = Outbox::new();
    let bob_api = PostApi { keys: bob.clone() };
    let bob_post = bob_api
        .compose_text(
            &mut bob_replica,
            &mut bob_outbox,
            "hosted elsewhere",
            None,
            None,
        )
        .unwrap();
    bob_outbox.drain(&bob_client).await.unwrap();

    let mut replica = Replica::open(&tmp("alice-home")).unwrap();
    let mut outbox = Outbox::new();
    let remote = "mock://bob-knot";
    follow(&alice, &mut replica, &mut outbox, &bob_did, Some(remote)).unwrap();

    let mut sync = Sync::mock(alice_knot).with_remote(remote, bob_knot);
    sync.poll_once(&mut replica, &alice).await.unwrap();
    let got = replica
        .post(&bob_post)
        .expect("poll_once should fetch the follow from its knot");
    assert_eq!(got.text, "hosted elsewhere");
}

#[test]
fn inbound_social_lists_follows_likes_boosts_not_own() {
    use reed_core::SocialKind;

    let alice = keys(1, 2);
    let bob = keys(3, 4);
    let me = alice.did().as_str();
    let dir = tmp("inbound-social");
    let mut replica = Replica::open(&dir).unwrap();
    let mut outbox = Outbox::new();
    let api = PostApi {
        keys: alice.clone(),
    };
    let post = api
        .compose_text(&mut replica, &mut outbox, "hello square", None, None)
        .unwrap();

    follow(&alice, &mut replica, &mut outbox, &bob.did(), None).unwrap();
    like(&alice, &mut replica, &mut outbox, &post).unwrap();
    boost(&alice, &mut replica, &mut outbox, &post).unwrap();
    assert!(
        replica.inbound_social(&me).is_empty(),
        "own follow/like/boost must not appear"
    );

    follow(&bob, &mut replica, &mut outbox, &alice.did(), None).unwrap();
    like(&bob, &mut replica, &mut outbox, &post).unwrap();
    boost(&bob, &mut replica, &mut outbox, &post).unwrap();
    let events = replica.inbound_social(&me);
    assert!(
        events
            .iter()
            .any(|e| e.kind == SocialKind::Follow && e.actor == bob.did().as_str()),
        "bob follow missing: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| e.kind == SocialKind::Like && e.actor == bob.did().as_str()),
        "bob like missing: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| e.kind == SocialKind::Boost && e.actor == bob.did().as_str()),
        "bob boost missing: {events:?}"
    );
    assert!(
        events.iter().all(|e| e.actor != me),
        "own actions leaked: {events:?}"
    );
}
