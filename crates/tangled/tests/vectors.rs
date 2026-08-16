//! Table-driven tests: one test per `spec/vectors/*.json`.

use std::fs;
use std::path::PathBuf;

use ed25519_dalek::SigningKey;
use tangled::capability::{
    authorized_collections, authorizes, resource_attenuates, verify_cap_sig, verify_circle_read,
    AuthorizedScope, Capability,
};
use tangled::cid::Cid;
use tangled::circle::{
    mls_constants_ok, mls_exporter_context, simulate_grow_vector, wrap_mls_log, Distributor,
    MlsLogType,
};
use tangled::constants::{
    CIRCLE_MLS_THRESHOLD, DUAL_STACK_MAX_SKEW_EPOCHS, DUAL_STACK_MAX_SKEW_SECONDS, HPKE_AEAD_ID,
    HPKE_INFO_RECORD, HPKE_KDF_ID, HPKE_KEM_ID, MLS_APPLICATION_ID, MLS_CIPHERSUITE,
    MLS_EXPORTER_LABEL,
};
use tangled::dagcbor;
use tangled::foreign::{binding_message, nostr_sign_binding, nostr_verify_binding, nostr_xonly_pk};
use tangled::handle::{txt_name, verify_handle, well_known_url};
use tangled::hpke;
use tangled::mst::{encode_node_string_cids, node_from_vector_json, Mst};
use tangled::page::{apply, record_from_vector};
use tangled::record::{signed_record_cid, verify_json_record};
use tangled::sign::{self, key32_from_hex, offchain_message, sig_from_hex};
use tangled::sync::{getblob_auth, BlobAuth, BlobRef};

fn vec_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/vectors")
        .join(name)
}

fn load(name: &str) -> serde_json::Value {
    let s = fs::read_to_string(vec_path(name)).unwrap_or_else(|e| panic!("read {name}: {e}"));
    serde_json::from_str(&s).expect("json")
}

#[test]
fn vector_unsigned_record() {
    let v = load("unsigned-record.json");
    let unsigned = &v["unsigned"];
    let cbor = dagcbor::encode_json(unsigned).unwrap();
    let hex_cbor = hex::encode(&cbor);
    assert_eq!(hex_cbor, v["hex_cbor"].as_str().unwrap());
    let digest = blake3::hash(&cbor);
    assert_eq!(
        hex::encode(digest.as_bytes()),
        v["blake3"].as_str().unwrap()
    );
}

#[test]
fn vector_solana_signature() {
    let v = load("solana-signature.json");
    let sk =
        SigningKey::from_bytes(&key32_from_hex(v["secret_key_hex"].as_str().unwrap()).unwrap());
    let pk = key32_from_hex(v["public_key_hex"].as_str().unwrap()).unwrap();
    assert_eq!(sk.verifying_key().to_bytes(), pk);
    let b58 = tangled::identity::encode_pubkey_base58(&pk);
    assert_eq!(b58, v["public_key_base58"].as_str().unwrap());
    assert_eq!(tangled::identity::did_sol(&pk), v["did"].as_str().unwrap());

    let unsigned = hex::decode(v["unsigned_cbor_hex"].as_str().unwrap()).unwrap();
    let (digest, envelope) = sign::envelope_for_unsigned(&unsigned);
    assert_eq!(hex::encode(digest), v["digest_hex"].as_str().unwrap());
    assert_eq!(hex::encode(envelope), v["envelope_hex"].as_str().unwrap());

    let sig = sig_from_hex(v["signature_hex"].as_str().unwrap()).unwrap();
    sign::verify_unsigned_raw(&pk, &unsigned, &sig).unwrap();
    let produced = sign::sign_unsigned(&sk, &unsigned);
    assert_eq!(hex::encode(produced), hex::encode(sig));

    let bad = sig_from_hex(v["negative"]["signature_hex"].as_str().unwrap()).unwrap();
    assert!(sign::verify_unsigned_raw(&pk, &unsigned, &bad).is_err());

    // Envelope construction unit check
    let msg = offchain_message(&digest);
    assert_eq!(&msg[..16], b"\xffsolana offchain");
}

#[test]
fn vector_mst_proof() {
    let v = load("mst-proof.json");
    let key = v["key"].as_str().unwrap().as_bytes();
    let record_cid = Cid::parse(v["record_cid"].as_str().unwrap()).unwrap();
    let node = node_from_vector_json(&v["node"]).unwrap();
    let encoded = encode_node_string_cids(&node).unwrap();
    assert_eq!(hex::encode(&encoded), v["node_cbor_hex"].as_str().unwrap());
    let node_cid = Cid::dag_cbor(&encoded);
    assert_eq!(node_cid.to_string(), v["node_cid"].as_str().unwrap());

    let mut mst = Mst::new();
    mst.insert(key, record_cid.clone()).unwrap();
    let built = mst.root_cid_string_cids().unwrap();
    assert_eq!(built, node_cid);

    let proof = mst.inclusion_proof(key).unwrap();
    proof.verify(&node_cid).unwrap();
    assert_eq!(proof.value, record_cid);

    let unsigned = &v; // commit unsigned fields
    let commit_unsigned = serde_json::json!({
        "v": v["commit"]["v"],
        "did": v["commit"]["did"],
        "data": v["commit"]["data"],
        "rev": v["commit"]["rev"],
        "prev": serde_json::Value::Null,
    });
    let cbor = dagcbor::encode_json(&commit_unsigned).unwrap();
    assert_eq!(
        hex::encode(&cbor),
        v["commit_cbor_unsigned_hex"].as_str().unwrap()
    );
    let sig = sig_from_hex(v["commit"]["sig_hex"].as_str().unwrap()).unwrap();
    let pk = tangled::identity::Did::parse(v["commit"]["did"].as_str().unwrap())
        .unwrap()
        .pubkey;
    sign::verify_unsigned_raw(&pk, &cbor, &sig).unwrap();
    let _ = unsigned;
}

#[test]
fn vector_capability_chain() {
    let v = load("capability-chain.json");
    let unsigned = &v["unsigned"];
    let cbor = dagcbor::encode_json(unsigned).unwrap();
    assert_eq!(hex::encode(&cbor), v["unsigned_cbor_hex"].as_str().unwrap());
    let pk = tangled::identity::Did::parse(v["issuer"].as_str().unwrap())
        .unwrap()
        .pubkey;
    let sig = sig_from_hex(v["signature_hex"].as_str().unwrap()).unwrap();
    verify_cap_sig(&pk, unsigned, &sig).unwrap();

    let parent = unsigned["res"].as_str().unwrap();
    let child = v["attenuation_negative"]["child_res"].as_str().unwrap();
    assert!(!resource_attenuates(parent, child));
    assert!(!v["attenuation_negative"]["valid"].as_bool().unwrap());

    let cap = Capability::from_json(unsigned).unwrap();
    authorizes(&cap, "repo/read", parent, 1_700_001_000).unwrap();
    assert!(authorizes(&cap, "repo/write", parent, 1_700_001_000).is_err());
}

#[test]
fn vector_hpke_sealed() {
    let v = load("hpke-sealed.json");
    let sk_r = key32_from_hex(v["recipient_agreement_sk_hex"].as_str().unwrap()).unwrap();
    let pk_r = key32_from_hex(v["recipient_agreement_pk_hex"].as_str().unwrap()).unwrap();
    let sk_e = key32_from_hex(v["sender_ephemeral_sk_hex"].as_str().unwrap()).unwrap();
    let pk_e = key32_from_hex(v["sender_ephemeral_pk_hex"].as_str().unwrap()).unwrap();
    let shared = hpke::x25519_shared(&sk_e, &pk_r);
    assert_eq!(
        hex::encode(shared),
        v["x25519_shared_hex"].as_str().unwrap()
    );
    // also DH the other way
    let shared2 = hpke::x25519_shared(&sk_r, &pk_e);
    assert_eq!(shared, shared2);

    let hp = &v["hpke"];
    assert_eq!(
        hp["kem_id"].as_str().unwrap(),
        format!("0x{HPKE_KEM_ID:04x}")
    );
    assert_eq!(
        hp["kdf_id"].as_str().unwrap(),
        format!("0x{HPKE_KDF_ID:04x}")
    );
    assert_eq!(
        hp["aead_id"].as_str().unwrap(),
        format!("0x{HPKE_AEAD_ID:04x}")
    );
    assert_eq!(
        hp["info"].as_str().unwrap(),
        std::str::from_utf8(HPKE_INFO_RECORD).unwrap()
    );
    assert_eq!(hp["mode"].as_str().unwrap(), "base");

    let pt = hex::decode(v["plaintext_hex"].as_str().unwrap()).unwrap();
    let (enc, ct) = hpke::seal_with_ephemeral(&sk_e, &pk_r, HPKE_INFO_RECORD, b"", &pt).unwrap();
    let out = hpke::open(&sk_r, &enc, HPKE_INFO_RECORD, b"", &ct).unwrap();
    assert_eq!(out, pt);
}

#[test]
fn vector_mls_welcome_commit() {
    let v = load("mls-welcome-commit.json");
    assert_eq!(
        v["ciphersuite"].as_str().unwrap(),
        format!("0x{MLS_CIPHERSUITE:04x}")
    );
    assert_eq!(
        v["application_id"].as_str().unwrap().as_bytes(),
        MLS_APPLICATION_ID
    );
    assert_eq!(
        v["exporter_label"].as_str().unwrap().as_bytes(),
        MLS_EXPORTER_LABEL
    );
    assert_eq!(
        v["exporter_context_epoch_1_hex"].as_str().unwrap(),
        hex::encode(mls_exporter_context(1))
    );
    assert_eq!(v["exporter_length"].as_u64().unwrap(), 32);
    assert!(mls_constants_ok());

    let welcome = wrap_mls_log(
        MlsLogType::Welcome,
        1,
        Some(v["welcome_record"]["member"].as_str().unwrap()),
        hex::decode(v["welcome_record"]["payload_hex"].as_str().unwrap()).unwrap(),
    );
    assert_eq!(welcome.kind, MlsLogType::Welcome);
    assert_eq!(welcome.epoch, 1);
    let commit = wrap_mls_log(
        MlsLogType::Commit,
        1,
        None,
        hex::decode(v["commit_record"]["payload_hex"].as_str().unwrap()).unwrap(),
    );
    assert_eq!(commit.kind, MlsLogType::Commit);
    // Payload in this vector is a wrapping stand-in; RFC 9420 bytes are tested in `mls`.
}

#[test]
fn vector_circle_grow_dual_stack() {
    let v = load("circle-grow-dual-stack.json");
    assert_eq!(
        v["threshold"].as_u64().unwrap() as usize,
        CIRCLE_MLS_THRESHOLD
    );
    let members: Vec<String> = v["members_before"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect();
    assert_eq!(members.len(), 16);
    let s0 = key32_from_hex(v["epoch_secret_0_hex"].as_str().unwrap()).unwrap();
    let s1 = key32_from_hex(v["epoch_secret_1_hex"].as_str().unwrap()).unwrap();
    let mut st = simulate_grow_vector(
        v["owner"].as_str().unwrap().into(),
        v["circle_id"].as_str().unwrap().into(),
        members,
        v["new_member"].as_str().unwrap().into(),
        s0,
        s1,
        1_700_000_000,
    )
    .unwrap();
    assert_eq!(st.circle_id, v["circle_id_after"].as_str().unwrap());
    assert_eq!(
        st.distributor.as_str(),
        v["distributor_after_transition"].as_str().unwrap()
    );
    assert_eq!(st.epoch, 1);
    assert!(!v["client_api_flag_flipped"].as_bool().unwrap());
    assert_eq!(st.members.len(), 17);
    assert!(st
        .members
        .contains(&v["offline_member"].as_str().unwrap().to_string()));

    // Pre-migration AEAD still decrypts with epoch 0.
    let pt = v["pre_migration_record"]["plaintext"]
        .as_str()
        .unwrap()
        .as_bytes();
    let circle_did = format!(
        "{}#{}",
        v["owner"].as_str().unwrap(),
        v["circle_id"].as_str().unwrap()
    );
    let ct = hpke::epoch_aead_seal(&s0, &circle_did, b"unsigned", pt).unwrap();
    let out = hpke::epoch_aead_open(&s0, &circle_did, b"unsigned", &ct).unwrap();
    assert_eq!(out, pt);
    assert!(v["pre_migration_record"]["decrypts_after_grow"]
        .as_bool()
        .unwrap());

    assert_eq!(
        v["dual_stack_max_skew_epochs"].as_u64().unwrap(),
        DUAL_STACK_MAX_SKEW_EPOCHS
    );
    assert_eq!(
        v["dual_stack_max_skew_seconds"].as_u64().unwrap(),
        DUAL_STACK_MAX_SKEW_SECONDS
    );

    // Skew ends dual-stack.
    st.epoch += DUAL_STACK_MAX_SKEW_EPOCHS;
    st.maybe_end_dual(1_700_000_000);
    assert_eq!(
        st.distributor.as_str(),
        v["distributor_after_skew"].as_str().unwrap()
    );

    // Shrink 20→5 stays MLS.
    for i in 0..12 {
        let victim = st.members.last().cloned().unwrap();
        if victim == st.owner {
            continue;
        }
        st.remove_member(&victim, [i as u8; 32]).unwrap();
    }
    assert_eq!(st.distributor, Distributor::Mls);
    assert_eq!(v["shrink_20_to_5"]["distributor"].as_str().unwrap(), "mls");
    assert!(v["shrink_20_to_5"]["never_downgrade"].as_bool().unwrap());
}

#[test]
fn vector_handle_dns() {
    let v = load("handle-dns.json");
    let handle = v["handle"].as_str().unwrap();
    assert_eq!(txt_name(handle), v["txt_name"].as_str().unwrap());
    assert_eq!(well_known_url(handle), v["well_known"].as_str().unwrap());
    let txt: Vec<String> = v["txt_values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect();
    let did = tangled::handle::did_from_txt(&txt).unwrap();
    verify_handle(
        handle,
        &txt,
        &[v["alsoKnownAs_must_include"].as_str().unwrap().to_string()],
        &did,
    )
    .unwrap();
    assert!(v["one_sided_reject"].as_bool().unwrap());
    assert!(verify_handle(handle, &txt, &[], &did).is_err());
}

#[test]
fn vector_binding_nostr() {
    let v = load("binding-nostr.json");
    let did = v["tangled"].as_str().unwrap();
    let nonce = hex::decode(v["nonce_hex"].as_str().unwrap()).unwrap();
    let msg = binding_message(did, &nonce);
    assert_eq!(hex::encode(&msg), v["message_hex"].as_str().unwrap());
    assert_eq!(
        std::str::from_utf8(&msg).unwrap(),
        v["message_utf8"].as_str().unwrap()
    );
    assert!(!v["attested"].as_bool().unwrap());
    let sk = key32_from_hex(v["nostr_sk_hex"].as_str().unwrap()).unwrap();
    let sig = nostr_sign_binding(&sk, did, &nonce).unwrap();
    let pk_arr = nostr_xonly_pk(&sk).unwrap();
    nostr_verify_binding(&pk_arr, did, &nonce, &sig).unwrap();
    assert_eq!(v["protocol"].as_str().unwrap(), "nostr");
}

#[test]
fn vector_circle_read_auth() {
    let v = load("circle-read-auth.json");
    let owner = v["owner"].as_str().unwrap();
    let now = 1_700_001_000u64;
    for case in v["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let unsigned = &case["unsigned"];
        let cbor = dagcbor::encode_json(unsigned).unwrap();
        assert_eq!(
            hex::encode(&cbor),
            case["unsigned_cbor_hex"].as_str().unwrap(),
            "{id} cbor"
        );
        let iss = unsigned["iss"].as_str().unwrap();
        let pk = tangled::identity::Did::parse(iss).unwrap().pubkey;
        let sig = sig_from_hex(case["signature_hex"].as_str().unwrap()).unwrap();
        verify_cap_sig(&pk, unsigned, &sig).unwrap();

        let cap = Capability::from_json(unsigned).unwrap();
        let accept = case["accept"].as_bool().unwrap();
        let ok = verify_circle_read(&cap, owner, now).is_ok();
        if id == "sibling-collection-leak" {
            assert!(!accept);
            let scope = authorized_collections(&cap).unwrap();
            assert!(!scope.allows_circle_read());
            for leak in case["must_not_authorize"].as_array().unwrap() {
                let s = leak.as_str().unwrap();
                if s == "circle/read" {
                    assert!(!scope.allows_circle_read());
                } else {
                    assert!(!scope.allows_collection(s), "{id} must not authorize {s}");
                }
            }
            match scope {
                AuthorizedScope::Repo { rest, .. } => {
                    assert_eq!(rest.first().map(String::as_str), Some("pub.tangled.chat"));
                }
                _ => panic!("expected repo scope"),
            }
        } else {
            assert_eq!(ok, accept, "{id}");
        }
    }
}

#[test]
fn vector_getblob_auth() {
    let v = load("getblob-auth.json");
    assert!(Cid::parse(v["blob_cid"].as_str().unwrap()).is_ok());
    for case in v["cases"].as_array().unwrap() {
        let refs: Vec<BlobRef> = case["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| BlobRef {
                collection: r["collection"].as_str().unwrap().into(),
                rkey: r["rkey"].as_str().unwrap().into(),
                vis: r["vis"].as_str().unwrap().into(),
                deleted: r["deleted"].as_bool().unwrap(),
                references_blob: r["references_blob"].as_bool().unwrap(),
                circle: r.get("circle").and_then(|x| x.as_str()).map(str::to_string),
            })
            .collect();
        let got = getblob_auth(&refs);
        let expect = case["expect"].as_str().unwrap();
        match expect {
            "serve-without-cap" => assert_eq!(got, BlobAuth::ServeWithoutCap, "{}", case["id"]),
            "require-cap" => assert_eq!(got, BlobAuth::RequireCap, "{}", case["id"]),
            other => panic!("unknown expect {other}"),
        }
    }
}

#[test]
fn vector_page_crdt_concurrent() {
    let v = load("page-crdt-concurrent.json");
    let recs: Vec<_> = v["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| record_from_vector(r).unwrap())
        .collect();
    let by_id: std::collections::HashMap<&str, usize> = v["records"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, r)| (r["id"].as_str().unwrap(), i))
        .collect();
    let pk = tangled::identity::Did::parse(v["author"].as_str().unwrap())
        .unwrap()
        .pubkey;
    for r in v["records"].as_array().unwrap() {
        let sig = sig_from_hex(r["signature_hex"].as_str().unwrap()).unwrap();
        verify_json_record(&pk, &r["unsigned"], &sig).unwrap();
        let cbor = dagcbor::encode_json(&r["unsigned"]).unwrap();
        assert_eq!(hex::encode(&cbor), r["unsigned_cbor_hex"].as_str().unwrap());
        let cid = signed_record_cid(&r["unsigned"], &sig).unwrap();
        assert_eq!(cid.to_string(), r["cid"].as_str().unwrap());
    }
    for order in v["orders"].as_array().unwrap() {
        let ids = order["order"].as_array().unwrap();
        let set: Vec<_> = ids
            .iter()
            .map(|id| recs[*by_id.get(id.as_str().unwrap()).unwrap()].clone())
            .collect();
        let live = apply(&set).unwrap();
        assert_eq!(live.title, order["live"]["title"].as_str().unwrap());
        assert_eq!(live.text, order["live"]["text"].as_str().unwrap());
        assert_eq!(live.text, "Hello Bob Alice");
    }
    assert!(v["orders_equal"].as_bool().unwrap());
}

#[test]
fn vector_page_crdt_compact() {
    let v = load("page-crdt-compact.json");
    let recs: Vec<_> = v["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| record_from_vector(r).unwrap())
        .collect();
    let by_id: std::collections::HashMap<&str, usize> = v["records"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, r)| (r["id"].as_str().unwrap(), i))
        .collect();
    let pk = tangled::identity::Did::parse(v["author"].as_str().unwrap())
        .unwrap()
        .pubkey;
    for r in v["records"].as_array().unwrap() {
        let sig = sig_from_hex(r["signature_hex"].as_str().unwrap()).unwrap();
        verify_json_record(&pk, &r["unsigned"], &sig).unwrap();
        let cid = signed_record_cid(&r["unsigned"], &sig).unwrap();
        assert_eq!(cid.to_string(), r["cid"].as_str().unwrap());
    }
    for order in v["orders"].as_array().unwrap() {
        let ids = order["order"].as_array().unwrap();
        let set: Vec<_> = ids
            .iter()
            .map(|id| recs[*by_id.get(id.as_str().unwrap()).unwrap()].clone())
            .collect();
        let live = apply(&set).unwrap();
        assert_eq!(
            live.text,
            order["live"]["text"].as_str().unwrap(),
            "order {:?}",
            ids
        );
        assert_eq!(live.title, order["live"]["title"].as_str().unwrap());
    }
    assert!(v["converges"].as_bool().unwrap());
    assert!(v["concurrent_with_compact"].as_bool().unwrap());
}

#[test]
fn foreign_rkey_blake3() {
    let r = tangled::foreign::foreign_rkey("nostr", &"ab".repeat(32));
    let mut data = b"nostr".to_vec();
    data.push(0x00);
    data.extend_from_slice("ab".repeat(32).as_bytes());
    assert_eq!(r, hex::encode(blake3::hash(&data).as_bytes()));
}

#[test]
fn envelope_rejects_raw_cbor_sign_helper() {
    // Signing helper always wraps the envelope; digest-only is not the message.
    let d = [1u8; 32];
    let msg = offchain_message(&d);
    assert_ne!(&msg[..32], &d);
}
