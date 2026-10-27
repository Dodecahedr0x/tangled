use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use tangled::capability::{Capability, Invocation};
use tangled::{Cid, Identity};
use tangled_personal::{agents::execute, Store};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn grant(store: &mut Store, agent: &Identity, access: &str, ttl: u64) -> Value {
    execute(store, json!({"action":"grant","audience":agent.did_string(),"collection":"pub.tangled.note","access":access,"ttl_seconds":ttl})).unwrap()
}
fn request(store: &Store, agent: &Identity, grant: &Value, mut args: Value) -> Value {
    let cap = Capability::from_json(&grant["capability"]).unwrap();
    let resource = format!(
        "tangled://{}/{}",
        store.did(),
        args["collection"].as_str().unwrap()
    );
    let nonce: [u8; 16] = blake3::hash(tangled::identity::tid_now().as_bytes()).as_bytes()[..16]
        .try_into()
        .unwrap();
    let mut inv = Invocation::new(&cap, &cap.cmd, &resource, &store.did(), now(), nonce).unwrap();
    inv.args = args.clone();
    inv.sign(agent.signing_key()).unwrap();
    args["invocation"] = json!(hex::encode(inv.encode_cbor().unwrap()));
    args
}

#[test]
fn signed_search_is_scoped_and_receipts_cite_exact_records() {
    let mut s = Store::memory();
    let a = Identity::generate();
    let note = s
        .put("pub.tangled.note", "one", json!({"text":"orchid"}), "cap")
        .unwrap();
    s.put("pub.tangled.secret", "two", json!({"text":"orchid"}), "cap")
        .unwrap();
    let g = grant(&mut s, &a, "read", 3600);
    assert_eq!(g["capability"]["aud"], a.did_string());
    assert_eq!(g["capability"]["cmd"], "repo/read");
    assert!(g.to_string().find("ed25519").is_none());
    let input = request(
        &s,
        &a,
        &g,
        json!({"action":"search","collection":"pub.tangled.note","query":"orchid"}),
    );
    let out = execute(&mut s, input.clone()).unwrap();
    assert_eq!(out["records"].as_array().unwrap().len(), 1);
    assert_eq!(
        out["receipt"]["body"]["citations"][0],
        json!({"uri":note["uri"],"cid":note["cid"]})
    );
    assert!(
        execute(&mut s, input).is_err(),
        "successful invocation replay must fail"
    );
    let bad = request(
        &s,
        &a,
        &g,
        json!({"action":"search","collection":"pub.tangled.secret","query":"orchid"}),
    );
    assert!(execute(&mut s, bad).is_err());
}

#[test]
fn writes_need_write_grants_and_cannot_change_signed_arguments() {
    let mut s = Store::memory();
    let a = Identity::generate();
    let r = grant(&mut s, &a, "read", 3600);
    let args = json!({"action":"write","collection":"pub.tangled.note","rkey":"result","body":{"summary":"hello"}});
    let input = request(&s, &a, &r, args.clone());
    assert!(execute(&mut s, input).is_err());
    let w = grant(&mut s, &a, "write", 3600);
    let mut input = request(&s, &a, &w, args.clone());
    input["body"]["summary"] = json!("tampered");
    assert!(execute(&mut s, input).is_err());
    let input = request(&s, &a, &w, args);
    let out = execute(&mut s, input).unwrap();
    assert_eq!(out["record"]["vis"], "cap");
    assert_eq!(out["record"]["body"]["summary"], "hello");
    assert_eq!(
        out["receipt"]["body"]["citations"][0]["cid"],
        out["record"]["cid"]
    );
}

#[test]
fn wrong_signer_tampered_capability_and_wrong_knot_are_rejected() {
    let mut s = Store::memory();
    let a = Identity::generate();
    let other = Identity::generate();
    let g = grant(&mut s, &a, "read", 3600);
    let args = json!({"action":"search","collection":"pub.tangled.note","query":""});
    let input = request(&s, &other, &g, args.clone());
    assert!(execute(&mut s, input).is_err());
    let mut forged = g.clone();
    forged["capability"]["aud"] = json!(other.did_string());
    let input = request(&s, &other, &forged, args.clone());
    assert!(execute(&mut s, input).is_err());
    let mut input = request(&s, &a, &g, args);
    let bytes = hex::decode(input["invocation"].as_str().unwrap()).unwrap();
    let mut inv = Invocation::decode_cbor(&bytes).unwrap();
    inv.aud = other.did_string();
    inv.sign(a.signing_key()).unwrap();
    input["invocation"] = json!(hex::encode(inv.encode_cbor().unwrap()));
    assert!(execute(&mut s, input).is_err());
}

#[test]
fn revocation_survives_reopening_and_is_signed_protocol_record() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    let a = Identity::generate();
    let g = grant(&mut s, &a, "write", 3600);
    let revoked = execute(
        &mut s,
        json!({"action":"revoke","cid":g["cid"],"reason":"finished"}),
    )
    .unwrap();
    assert_eq!(revoked["collection"], "sys.revocation");
    assert!(revoked["sig"].is_string());
    assert_eq!(
        revoked["rkey"],
        g["cid"].as_str().unwrap().strip_prefix('b').unwrap()
    );
    drop(s);
    let mut s = Store::open(dir.path()).unwrap();
    let input = request(
        &s,
        &a,
        &g,
        json!({"action":"write","collection":"pub.tangled.note","rkey":"denied","body":{}}),
    );
    assert!(execute(&mut s, input).is_err());
    assert!(s.get("pub.tangled.note", "denied").is_err());
}

#[test]
fn expiry_blocks_actions_without_protocol_clock_skew_extension() {
    let mut s = Store::memory();
    let a = Identity::generate();
    let g = grant(&mut s, &a, "read", 1);
    let until = g["capability"]["exp"].as_u64().unwrap();
    while now() <= until {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let input = request(
        &s,
        &a,
        &g,
        json!({"action":"search","collection":"pub.tangled.note","query":""}),
    );
    assert!(execute(&mut s, input).is_err());
}

#[test]
fn malformed_grants_fail_and_activity_report_uses_signed_storage() {
    let mut s = Store::memory();
    let a = Identity::generate();
    for collection in [
        "pub.tangled.note/../secret",
        "sys.id",
        "pub.tangled.agent.grant",
        "pub.tangled.agent.activity",
    ] {
        assert!(execute(&mut s,json!({"action":"grant","audience":a.did_string(),"collection":collection,"access":"write","ttl_seconds":3600})).is_err());
    }
    assert!(execute(&mut s,json!({"action":"grant","audience":"bad","collection":"pub.tangled.note","access":"read","ttl_seconds":3600})).is_err());
    assert!(execute(&mut s,json!({"action":"grant","audience":a.did_string(),"collection":"pub.tangled.note","access":"read","ttl_seconds":0})).is_err());
    let g = grant(&mut s, &a, "read", 3600);
    let input = request(
        &s,
        &a,
        &g,
        json!({"action":"search","collection":"pub.tangled.note","query":""}),
    );
    execute(&mut s, input).unwrap();
    let report = execute(&mut s, json!({"action":"report"})).unwrap();
    assert_eq!(report["activities"].as_array().unwrap().len(), 1);
    let listed = execute(&mut s, json!({"action":"list"})).unwrap();
    assert_eq!(listed["grants"].as_array().unwrap().len(), 1);
    assert!(Cid::parse(listed["grants"][0]["body"]["cid"].as_str().unwrap()).is_ok());
}

#[test]
fn write_grant_cannot_read_or_write_a_sibling_collection() {
    let mut s = Store::memory();
    let a = Identity::generate();
    let g = grant(&mut s, &a, "write", 3600);
    let input = request(
        &s,
        &a,
        &g,
        json!({"action":"search","collection":"pub.tangled.note","query":""}),
    );
    assert!(execute(&mut s, input).is_err());
    let input = request(
        &s,
        &a,
        &g,
        json!({"action":"write","collection":"pub.tangled.secret","rkey":"wrong","body":{"secret":"oops"}}),
    );
    assert!(execute(&mut s, input).is_err());
    assert!(s.get("pub.tangled.secret", "wrong").is_err());
}

#[test]
fn invocation_cap_reference_is_bound_to_the_signed_certificate() {
    let mut s = Store::memory();
    let a = Identity::generate();
    let g = grant(&mut s, &a, "read", 3600);
    let mut input = request(
        &s,
        &a,
        &g,
        json!({"action":"search","collection":"pub.tangled.note","query":""}),
    );
    let mut inv =
        Invocation::decode_cbor(&hex::decode(input["invocation"].as_str().unwrap()).unwrap())
            .unwrap();
    inv.cap = Cid::raw(b"unrelated").to_string();
    inv.sign(a.signing_key()).unwrap();
    input["invocation"] = json!(hex::encode(inv.encode_cbor().unwrap()));
    assert!(execute(&mut s, input).is_err());
}

#[test]
fn search_replay_is_rejected_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    let a = Identity::generate();
    let g = grant(&mut s, &a, "read", 3600);
    let input = request(
        &s,
        &a,
        &g,
        json!({"action":"search","collection":"pub.tangled.note","query":""}),
    );
    execute(&mut s, input.clone()).unwrap();
    drop(s);
    let mut s = Store::open(dir.path()).unwrap();
    assert!(execute(&mut s, input).is_err());
}

#[test]
fn grant_signature_verifies_with_owner_public_key() {
    let mut s = Store::memory();
    let a = Identity::generate();
    let g = grant(&mut s, &a, "read", 3600);
    let cap = Capability::from_json(&g["capability"]).unwrap();
    let pk = tangled::identity::Did::parse(&s.did()).unwrap().pubkey;
    tangled::capability::verify_cap_sig(&pk, &cap.to_unsigned_json(), &cap.sig.unwrap()).unwrap();
    assert_eq!(cap.cid().unwrap().to_string(), g["cid"]);
}

#[test]
fn write_only_agent_cannot_overwrite_existing_owner_records() {
    let mut s = Store::memory();
    let a = Identity::generate();
    let original = s
        .put(
            "pub.tangled.note",
            "owner-note",
            json!({"text":"private","extension":{"keep":true}}),
            "cap",
        )
        .unwrap();
    let g = grant(&mut s, &a, "write", 3600);
    let input = request(
        &s,
        &a,
        &g,
        json!({"action":"write","collection":"pub.tangled.note","rkey":"owner-note","body":{"summary":"replace"}}),
    );
    assert!(execute(&mut s, input).is_err());
    assert_eq!(
        s.get("pub.tangled.note", "owner-note").unwrap()["cid"],
        original["cid"]
    );
}

#[test]
fn receipts_retain_verifiable_signed_invocations() {
    let mut s = Store::memory();
    let a = Identity::generate();
    let g = grant(&mut s, &a, "read", 3600);
    let input = request(
        &s,
        &a,
        &g,
        json!({"action":"search","collection":"pub.tangled.note","query":""}),
    );
    let out = execute(&mut s, input).unwrap();
    let bytes = hex::decode(out["receipt"]["body"]["invocation_cbor"].as_str().unwrap()).unwrap();
    let inv = Invocation::decode_cbor(&bytes).unwrap();
    inv.verify(now(), &s.did()).unwrap();
    assert_eq!(
        Cid::dag_cbor(&bytes).to_string(),
        out["receipt"]["body"]["invocation"]
    );
}
