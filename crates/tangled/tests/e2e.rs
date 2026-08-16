//! End-to-end multi-node scenarios (spec 20). Each node is a distinct
//! identity + in-process knot. Clients pull from **other** knots.

mod common;

use common::{http_code, identity_named, load_vector, page_from_list_item, Node, NOW};
use tangled::capability::circle_resource_uri;
use tangled::circle::{wrap_mls_log, CircleState, Distributor, MembershipPlan, MlsLogType};
use tangled::constants::CIRCLE_MLS_THRESHOLD;
use tangled::foreign::import_rkey;
use tangled::handle::{parse_well_known, txt_name, verify_handle, well_known_url};
use tangled::hpke::{self, epoch_aead_open, epoch_aead_seal, open_record, seal_record};
use tangled::page::{apply, record_from_unsigned};
use tangled::record::unsigned_without_body_and_sig;
use tangled::sync::safe_did_rkey;
use tangled::Visibility;

#[tokio::test]
async fn public_post_not_on_bob_until_pull() {
    let alice = Node::named("alice");
    let bob = Node::named("bob");
    let carol = Node::named("carol");

    let post = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "3jzfcijpjiz0a",
            "public",
            serde_json::json!({ "text": "hello from alice" }),
        ))
        .await
        .unwrap();
    let cid = post.cid.to_string();
    assert_eq!(post.put.cid.as_deref(), Some(cid.as_str()));

    let alice_head = alice.local().get_head(&alice.did()).await.unwrap();
    assert!(!alice_head.commit.is_empty());
    assert_eq!(alice_head.did, alice.did());

    let err = bob
        .local()
        .get_record(&alice.did(), "pub.tangled.post", "3jzfcijpjiz0a")
        .await
        .unwrap_err();
    let (status, code) = http_code(&err);
    assert_eq!(status, 404);
    assert_eq!(code, "NotFound");

    let pulled = bob
        .to(&alice)
        .get_record(&alice.did(), "pub.tangled.post", "3jzfcijpjiz0a")
        .await
        .unwrap();
    assert_eq!(pulled.record["body"]["text"], "hello from alice");
    assert_eq!(pulled.record["vis"], "public");
    assert_eq!(pulled.record["author"], alice.did());

    let follow_rkey = safe_did_rkey(&alice.did());
    bob.publish(bob.unsigned(
        "tangled.graph.follow",
        &follow_rkey,
        "public",
        serde_json::json!({ "subject": alice.did() }),
    ))
    .await
    .unwrap();
    let follow = alice
        .to(&bob)
        .get_record(&bob.did(), "tangled.graph.follow", &follow_rkey)
        .await
        .unwrap();
    assert_eq!(follow.record["body"]["subject"], alice.did());

    let cap_post = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "3jzfcijpjiz0b",
            "cap",
            serde_json::json!({ "text": "draft" }),
        ))
        .await
        .unwrap();
    let denied = carol
        .to(&alice)
        .get_record(&alice.did(), "pub.tangled.post", "3jzfcijpjiz0b")
        .await
        .unwrap_err();
    let (status, code) = http_code(&denied);
    assert_eq!(status, 401);
    assert_eq!(code, "CapabilityRequired");
    let _ = cap_post;
}

#[tokio::test]
async fn capability_gated_fetch_and_sibling_denied() {
    let alice = Node::named("alice");
    let bob = Node::named("bob");
    let mallory = Node::named("mallory");

    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "draft1",
            "cap",
            serde_json::json!({ "text": "only bob" }),
        ))
        .await
        .unwrap();

    let res = format!("tangled://{}/pub.tangled.post", alice.did());
    let (cap, _cap_json) = alice.issue_cap(&bob.did(), "repo/read", &res).unwrap();
    let header = bob.invocation_header(&cap, "repo/read", &res, &alice.did());

    let got = bob
        .to_with_cap(&alice, header)
        .get_record(&alice.did(), "pub.tangled.post", "draft1")
        .await
        .unwrap();
    assert_eq!(got.record["body"]["text"], "only bob");

    let no_cap = mallory
        .to(&alice)
        .get_record(&alice.did(), "pub.tangled.post", "draft1")
        .await
        .unwrap_err();
    assert_eq!(http_code(&no_cap).0, 401);

    let wrong_res = format!("tangled://{}/pub.tangled.chat", alice.did());
    let (bad, _bad_json) = alice
        .issue_cap(&mallory.did(), "repo/read", &wrong_res)
        .unwrap();
    let bad_header = mallory.invocation_header(&bad, "repo/read", &wrong_res, &alice.did());
    let sibling = mallory
        .to_with_cap(&alice, bad_header)
        .get_record(&alice.did(), "pub.tangled.post", "draft1")
        .await
        .unwrap_err();
    let (status, code) = http_code(&sibling);
    assert_eq!(status, 403);
    assert_eq!(code, "CapabilityInvalid");
}

#[tokio::test]
async fn circle_read_membership_grant() {
    let alice = Node::named("alice");
    let bob = Node::named("bob");
    let mallory = Node::named("mallory");
    let circle_rkey = "circ1";
    let envelope = format!("{}#{circle_rkey}", alice.did());

    alice
        .publish(alice.unsigned(
            "sys.circle",
            circle_rkey,
            "public",
            serde_json::json!({
                "name": "friends",
                "members": [alice.did(), bob.did()],
                "epoch": 0,
                "distributor": "hpke",
                "threshold": CIRCLE_MLS_THRESHOLD,
            }),
        ))
        .await
        .unwrap();

    let mut chat = alice.unsigned(
        "pub.tangled.chat",
        "msg1",
        "sealed",
        serde_json::json!({ "alg": "epoch-aead", "epoch": 0, "ct": "00" }),
    );
    chat["circle"] = serde_json::Value::String(envelope.clone());
    alice.publish(chat).await.unwrap();

    let cres = circle_resource_uri(&alice.did(), circle_rkey);
    let (grant, _grant_json) = alice.issue_cap(&bob.did(), "circle/read", &cres).unwrap();
    let header = bob.invocation_header(&grant, "circle/read", &cres, &alice.did());

    let got = bob
        .to_with_cap(&alice, header)
        .get_record(&alice.did(), "pub.tangled.chat", "msg1")
        .await
        .unwrap();
    assert_eq!(got.record["circle"], envelope);
    assert_eq!(got.record["vis"], Visibility::Sealed.as_str());

    let denied = mallory
        .to(&alice)
        .get_record(&alice.did(), "pub.tangled.chat", "msg1")
        .await
        .unwrap_err();
    assert_eq!(http_code(&denied).0, 401);
}

#[tokio::test]
async fn sealed_hpke_third_party_cannot_read() {
    let alice = Node::named("alice");
    let bob = Node::named("bob");
    let mallory = Node::named("mallory");

    let plaintext = b"secret for bob";
    let mut unsigned = alice.unsigned("pub.tangled.post", "seal1", "sealed", serde_json::json!({}));
    let aad = unsigned_without_body_and_sig(&unsigned).unwrap();
    let (enc, ct) = seal_record(&bob.agreement_pk(), &aad, plaintext).unwrap();
    unsigned["body"] = serde_json::json!({
        "alg": "hpke-record",
        "enc": hex::encode(&enc),
        "ct": hex::encode(&ct),
    });
    alice.publish(unsigned).await.unwrap();

    let res = format!("tangled://{}", alice.did());
    let (cap, _cap_json) = alice.issue_cap(&bob.did(), "repo/read", &res).unwrap();
    let bob_h = bob.invocation_header(&cap, "repo/read", &res, &alice.did());
    let rec = bob
        .to_with_cap(&alice, bob_h)
        .get_record(&alice.did(), "pub.tangled.post", "seal1")
        .await
        .unwrap();

    let mut opened_unsigned = rec.record.clone();
    opened_unsigned.as_object_mut().unwrap().remove("sig");
    let aad2 = unsigned_without_body_and_sig(&opened_unsigned).unwrap();
    let enc = hex::decode(rec.record["body"]["enc"].as_str().unwrap()).unwrap();
    let ct = hex::decode(rec.record["body"]["ct"].as_str().unwrap()).unwrap();
    let pt = open_record(
        &bob.identity.agreement_secret().to_bytes(),
        &enc,
        &aad2,
        &ct,
    )
    .unwrap();
    assert_eq!(pt, plaintext);

    let (mcap, _mjson) = alice.issue_cap(&mallory.did(), "repo/read", &res).unwrap();
    let mh = mallory.invocation_header(&mcap, "repo/read", &res, &alice.did());
    let stolen = mallory
        .to_with_cap(&alice, mh)
        .get_record(&alice.did(), "pub.tangled.post", "seal1")
        .await
        .unwrap();
    let enc_m = hex::decode(stolen.record["body"]["enc"].as_str().unwrap()).unwrap();
    let ct_m = hex::decode(stolen.record["body"]["ct"].as_str().unwrap()).unwrap();
    let mut mu = stolen.record.clone();
    mu.as_object_mut().unwrap().remove("sig");
    let aad_m = unsigned_without_body_and_sig(&mu).unwrap();
    assert!(open_record(
        &mallory.identity.agreement_secret().to_bytes(),
        &enc_m,
        &aad_m,
        &ct_m,
    )
    .is_err());
}

#[tokio::test]
async fn sealed_blob_requires_cap() {
    let alice = Node::named("alice");
    let bob = Node::named("bob");
    let mallory = Node::named("mallory");

    let ct_bytes = b"ciphertext-not-plaintext";
    let blob = alice.local().put_blob(ct_bytes.to_vec()).await.unwrap();
    let mut rec = alice.unsigned(
        "pub.tangled.media",
        "photo1",
        "sealed",
        serde_json::json!({
            "alg": "epoch-aead",
            "epoch": 0,
            "blob": blob.cid,
        }),
    );
    rec["circle"] = serde_json::Value::String(format!("{}#album", alice.did()));
    alice.publish(rec).await.unwrap();

    let denied = mallory
        .to(&alice)
        .get_blob(&alice.did(), &blob.cid)
        .await
        .unwrap_err();
    assert_eq!(http_code(&denied).0, 401);

    let res = format!("tangled://{}", alice.did());
    let (cap, _cap_json) = alice.issue_cap(&bob.did(), "repo/read", &res).unwrap();
    let h = bob.invocation_header(&cap, "repo/read", &res, &alice.did());
    let bytes = bob
        .to_with_cap(&alice, h)
        .get_blob(&alice.did(), &blob.cid)
        .await
        .unwrap();
    assert_eq!(bytes, ct_bytes);
}

#[tokio::test]
async fn circle_grow_dual_stack_same_id() {
    let owner = Node::named("owner");
    let new_member = Node::named("member17");
    let mut fillers = Vec::new();
    for i in 0..(CIRCLE_MLS_THRESHOLD - 1) {
        fillers.push(identity_named(&format!("fill-{i}")));
    }

    let owner_party = tangled::mls::MlsParty::new(owner.identity.pubkey_bytes()).unwrap();
    let member_party = tangled::mls::MlsParty::new(new_member.identity.pubkey_bytes()).unwrap();
    let mut mls_sess = tangled::mls::MlsSession::create(&owner_party).unwrap();

    let circle_id = "growcirc";
    let secret0 = [0x11u8; 32];
    let mut st = CircleState::create(owner.did(), circle_id.into(), "party".into(), secret0);
    for f in &fillers {
        st.add_member(f.did_string(), [0x10; 32], NOW).unwrap();
    }
    assert_eq!(st.members.len(), CIRCLE_MLS_THRESHOLD);
    assert_eq!(st.distributor, Distributor::Hpke);

    let mls_out = mls_sess.add(&member_party, st.epoch + 1).unwrap();
    let secret1 = mls_out.exporter;
    let plan = st.add_member(new_member.did(), secret1, NOW).unwrap();
    assert_eq!(st.distributor, Distributor::Dual);
    assert_eq!(st.circle_id, circle_id);
    assert!(st.mls_group.is_some());
    let MembershipPlan::SeamlessGrow {
        new_epoch,
        wrap_to,
        welcome_members,
    } = plan
    else {
        panic!("expected seamless grow, got {plan:?}");
    };
    assert_eq!(new_epoch, st.epoch);
    assert!(wrap_to.contains(&new_member.did()));
    assert!(welcome_members.contains(&owner.did()));

    owner
        .publish(owner.unsigned(
            "sys.circle",
            circle_id,
            "public",
            serde_json::json!({
                "name": "party",
                "members": st.members,
                "epoch": st.epoch,
                "distributor": st.distributor.as_str(),
                "threshold": CIRCLE_MLS_THRESHOLD,
            }),
        ))
        .await
        .unwrap();

    let circle_did = format!("{}#{circle_id}", owner.did());
    let (enc, ct) =
        hpke::wrap_epoch_secret(&new_member.agreement_pk(), &circle_did, st.epoch, &secret1)
            .unwrap();
    let mut wrap = owner.unsigned(
        "sys.circle.wrap",
        &format!(
            "{}-{}",
            st.epoch,
            tangled::identity::encode_pubkey_base58(&new_member.identity.pubkey_bytes())
        ),
        "sealed",
        serde_json::json!({
            "alg": "hpke-record",
            "enc": hex::encode(&enc),
            "ct": hex::encode(&ct),
            "epoch": st.epoch,
        }),
    );
    wrap["circle"] = serde_json::Value::String(circle_did.clone());
    wrap["epoch"] = serde_json::json!(st.epoch);
    let wrap_rkey = wrap["rkey"].as_str().unwrap().to_string();
    owner.publish(wrap).await.unwrap();

    let welcome = wrap_mls_log(
        MlsLogType::Welcome,
        st.epoch,
        Some(&new_member.did()),
        mls_out.welcome.clone(),
    );
    let mut mls = owner.unsigned(
        "sys.circle.mls",
        &format!("welcome-{}", st.epoch),
        "sealed",
        serde_json::json!({
            "kind": welcome.kind.as_str(),
            "epoch": welcome.epoch,
            "member": welcome.member,
            "payload": hex::encode(&welcome.payload),
        }),
    );
    mls["circle"] = serde_json::Value::String(circle_did.clone());
    owner.publish(mls).await.unwrap();

    let mut chat_u = owner.unsigned(
        "pub.tangled.chat",
        "after-grow",
        "sealed",
        serde_json::json!({}),
    );
    chat_u["circle"] = serde_json::Value::String(circle_did.clone());
    chat_u["epoch"] = serde_json::json!(st.epoch);
    let aad = unsigned_without_body_and_sig(&chat_u).unwrap();
    let ct = epoch_aead_seal(&secret1, &circle_did, &aad, b"hello dual-stack").unwrap();
    chat_u["body"] = serde_json::json!({
        "alg": "epoch-aead",
        "epoch": st.epoch,
        "ct": hex::encode(&ct),
    });
    owner.publish(chat_u).await.unwrap();

    let cres = circle_resource_uri(&owner.did(), circle_id);
    let (g, _gj) = owner
        .issue_cap(&new_member.did(), "circle/read", &cres)
        .unwrap();
    let h = new_member.invocation_header(&g, "circle/read", &cres, &owner.did());
    let client = new_member.to_with_cap(&owner, h);

    let defn = new_member
        .to(&owner)
        .get_record(&owner.did(), "sys.circle", circle_id)
        .await
        .unwrap();
    assert_eq!(defn.record["body"]["distributor"], "dual");
    assert_eq!(defn.record["rkey"], circle_id);

    let wrap_got = client
        .get_record(&owner.did(), "sys.circle.wrap", &wrap_rkey)
        .await
        .unwrap();
    let opened = hpke::unwrap_epoch_secret(
        &new_member.identity.agreement_secret().to_bytes(),
        &hex::decode(wrap_got.record["body"]["enc"].as_str().unwrap()).unwrap(),
        &circle_did,
        st.epoch,
        &hex::decode(wrap_got.record["body"]["ct"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(opened, secret1);

    let mls_got = client
        .get_record(
            &owner.did(),
            "sys.circle.mls",
            &format!("welcome-{}", st.epoch),
        )
        .await
        .unwrap();
    let welcome_bytes = hex::decode(mls_got.record["body"]["payload"].as_str().unwrap()).unwrap();
    assert_eq!(welcome_bytes, mls_out.welcome);
    let joined = tangled::mls::join_welcome(member_party, &welcome_bytes).unwrap();
    assert_eq!(joined.exporter(st.epoch).unwrap(), secret1);

    let chat = client
        .get_record(&owner.did(), "pub.tangled.chat", "after-grow")
        .await
        .unwrap();
    let mut cu = chat.record.clone();
    cu.as_object_mut().unwrap().remove("sig");
    let aad_r = unsigned_without_body_and_sig(&cu).unwrap();
    let pt = epoch_aead_open(
        &opened,
        &circle_did,
        &aad_r,
        &hex::decode(chat.record["body"]["ct"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(pt, b"hello dual-stack");

    let owner_pt = epoch_aead_open(
        st.epoch_secrets.get(&st.epoch).unwrap(),
        &circle_did,
        &aad_r,
        &hex::decode(chat.record["body"]["ct"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(owner_pt, b"hello dual-stack");

    st.remove_member(&new_member.did(), [0x33; 32]).unwrap();
    assert_eq!(st.distributor, Distributor::Mls);
    assert_ne!(st.distributor, Distributor::Hpke);
}

#[tokio::test]
async fn wiki_concurrent_crdt_then_stale_compact() {
    let alice = Node::named("wiki-alice");
    let bob = Node::named("wiki-bob");
    let concurrent = load_vector("page-crdt-concurrent.json");
    let compact_v = load_vector("page-crdt-compact.json");
    let wiki_did = concurrent["author"].as_str().unwrap();

    let by_id = |v: &serde_json::Value, id: &str| -> (String, serde_json::Value) {
        let rec = v["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"].as_str() == Some(id))
            .unwrap();
        let mut json = rec["unsigned"].clone();
        json["sig"] = serde_json::Value::String(rec["signature_hex"].as_str().unwrap().into());
        (rec["cid"].as_str().unwrap().into(), json)
    };

    let (cid_r0, r0) = by_id(&concurrent, "R0");
    let (cid_a, rec_a) = by_id(&concurrent, "A");
    let (cid_b, rec_b) = by_id(&concurrent, "B");
    assert_eq!(r0["author"], wiki_did);

    alice.ingest(r0.clone()).unwrap();
    alice.ingest(rec_a.clone()).unwrap();
    bob.ingest(r0).unwrap();
    bob.ingest(rec_b.clone()).unwrap();

    let alice_local = alice
        .local()
        .list_records(wiki_did, Some("pub.tangled.page"))
        .await
        .unwrap();
    let cids: Vec<_> = alice_local.records.iter().map(|r| r.cid.as_str()).collect();
    assert!(cids.contains(&cid_r0.as_str()));
    assert!(cids.contains(&cid_a.as_str()));
    assert!(
        !cids.contains(&cid_b.as_str()),
        "Bob's op must not be on Alice until pull"
    );

    let bob_ops = bob
        .to(&alice)
        .list_records(wiki_did, Some("pub.tangled.page"))
        .await
        .unwrap();
    assert!(bob_ops.records.iter().any(|r| r.cid == cid_a));

    let alice_ops = alice
        .to(&bob)
        .list_records(wiki_did, Some("pub.tangled.page"))
        .await
        .unwrap();
    assert!(alice_ops.records.iter().any(|r| r.cid == cid_b));

    alice.ingest(rec_b).unwrap();
    bob.ingest(rec_a).unwrap();

    let mut alice_set: Vec<_> = alice
        .local()
        .list_records(wiki_did, Some("pub.tangled.page"))
        .await
        .unwrap()
        .records
        .iter()
        .map(page_from_list_item)
        .collect();
    let mut bob_set: Vec<_> = bob
        .local()
        .list_records(wiki_did, Some("pub.tangled.page"))
        .await
        .unwrap()
        .records
        .iter()
        .map(page_from_list_item)
        .collect();
    alice_set.sort_by(|a, b| a.cid.cmp(&b.cid));
    bob_set.sort_by(|a, b| a.cid.cmp(&b.cid));
    let live_a = apply(&alice_set).unwrap();
    let live_b = apply(&bob_set).unwrap();
    assert_eq!(live_a.text, live_b.text);
    assert_eq!(live_a.text, "Hello Bob Alice");

    let (cid_k, rec_k) = by_id(&compact_v, "K");
    let (cid_l, rec_l) = by_id(&compact_v, "L");
    let (cid_r0c, rec_r0c) = by_id(&compact_v, "R0");
    let (cid_ac, rec_ac) = by_id(&compact_v, "A");
    let (cid_bc, rec_bc) = by_id(&compact_v, "B");
    alice.ingest(rec_k.clone()).unwrap();
    bob.ingest(rec_l.clone()).unwrap();

    let pulled_k = bob
        .to(&alice)
        .get_record(wiki_did, "pub.tangled.page", "Home")
        .await
        .unwrap();
    assert_eq!(pulled_k.record["body"]["compact"], 1);
    assert_eq!(
        pulled_k.record["body"]["frontier"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let strip = |mut rec: serde_json::Value| {
        rec.as_object_mut().unwrap().remove("sig");
        rec
    };
    let r0 = record_from_unsigned(&cid_r0c, &strip(rec_r0c)).unwrap();
    let a = record_from_unsigned(&cid_ac, &strip(rec_ac)).unwrap();
    let b = record_from_unsigned(&cid_bc, &strip(rec_bc)).unwrap();
    let k = record_from_unsigned(&cid_k, &strip(rec_k)).unwrap();
    let l = record_from_unsigned(&cid_l, &strip(rec_l)).unwrap();
    let merged = apply(&[r0, a, b, k, l]).unwrap();
    assert_eq!(merged.text, "Hello Bob! Alice");
}

#[tokio::test]
async fn foreign_id_import_idempotent() {
    let alice = Node::named("alice");
    let event_id = "ab".repeat(32);
    let rkey = import_rkey("nostr", &event_id.to_uppercase()).unwrap();
    assert_eq!(rkey, import_rkey("nostr", &event_id).unwrap());
    assert_eq!(rkey.len(), 64);

    let body = serde_json::json!({
        "protocol": "nostr",
        "foreign_id": event_id,
        "attested": false,
    });
    let first = alice
        .publish(alice.unsigned("sys.foreign", &rkey, "public", body.clone()))
        .await
        .unwrap();
    let second = alice
        .publish(alice.unsigned("sys.foreign", &rkey, "public", body))
        .await
        .unwrap();
    assert_eq!(first.cid, second.cid);

    let listed = alice
        .local()
        .list_records(&alice.did(), Some("sys.foreign"))
        .await
        .unwrap();
    let same_key: Vec<_> = listed.records.iter().filter(|r| r.rkey == rkey).collect();
    assert_eq!(same_key.len(), 2);
    assert_eq!(same_key[0].cid, same_key[1].cid);

    let other = import_rkey("atproto", "at://did:plc:x/app.bsky.feed.post/y#bafyabc").unwrap();
    assert_ne!(other, rkey);
}

#[tokio::test]
async fn handle_txt_and_well_known_fixtures() {
    let alice = Node::named("alice");
    let v = load_vector("handle-dns.json");
    let handle = v["handle"].as_str().unwrap();
    assert_eq!(txt_name(handle), v["txt_name"].as_str().unwrap());
    assert_eq!(well_known_url(handle), v["well_known"].as_str().unwrap());

    let did = alice.did();
    verify_handle(
        handle,
        &[
            did.clone(),
            "head=bafyr4ihfp7ogii2jece4vcykrewmmuopi2g75icntrqzuia3fozesum7i4".into(),
        ],
        &[handle.to_string()],
        &did,
    )
    .unwrap();

    let one_sided = verify_handle(handle, std::slice::from_ref(&did), &[], &did);
    assert!(one_sided.is_err());

    assert_eq!(parse_well_known(&did).unwrap(), did);
    assert_eq!(
        parse_well_known(&format!(r#"{{"did":"{did}"}}"#)).unwrap(),
        did
    );

    alice
        .publish(alice.unsigned(
            "sys.id",
            "self",
            "public",
            serde_json::json!({
                "v": 1,
                "pubkey": hex::encode(alice.identity.pubkey_bytes()),
                "agreement": hex::encode(alice.identity.agreement_public_bytes()),
                "alsoKnownAs": [handle],
            }),
        ))
        .await
        .unwrap();
    {
        let mut g = alice.store.lock().unwrap();
        g.handle_resolver
            .txt
            .insert(txt_name(handle), vec![did.clone()]);
        g.handle_resolver
            .well_known
            .insert(well_known_url(handle), did.clone());
    }
    let resolved = alice
        .local()
        .identity_resolve(Some(handle), None)
        .await
        .unwrap();
    assert_eq!(resolved.did, did);
}

#[tokio::test]
async fn distinct_knots_do_not_share_store() {
    let a = Node::named("a");
    let b = Node::named("b");
    a.publish(a.unsigned(
        "pub.tangled.post",
        "only-a",
        "public",
        serde_json::json!({ "text": "a" }),
    ))
    .await
    .unwrap();
    assert!(b
        .local()
        .get_record(&a.did(), "pub.tangled.post", "only-a")
        .await
        .is_err());
    assert!(b
        .to(&a)
        .get_record(&a.did(), "pub.tangled.post", "only-a")
        .await
        .is_ok());
}

#[test]
fn vis_enum_matches_wire() {
    assert_eq!(Visibility::Public.as_str(), "public");
    assert_eq!(Visibility::Cap.as_str(), "cap");
    assert_eq!(Visibility::Sealed.as_str(), "sealed");
}
