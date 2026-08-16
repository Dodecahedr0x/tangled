use serde_json::{json, Value};
use tangled::foreign::{
    activitypub_actor_acct, activitypub_actor_identity, activitypub_foreign_proof,
    activitypub_import_attested, canonical_foreign_id, import_rkey,
};
use tangled::record::sign_json_record;
use tangled::{Cid, KnotEngine};
use tangled_ap::{note_to_post, Inbound};

pub struct Ingested {
    pub did: String,
    pub collection: String,
    pub rkey: String,
}

pub fn ingest_activity(
    engine: &mut KnotEngine,
    translator: &str,
    public_url: &str,
    inbound: &Inbound,
    raw: &[u8],
) -> Result<Ingested, String> {
    activitypub_import_attested(true).map_err(|e| e.to_string())?;
    match inbound {
        Inbound::CreateNote {
            activity_id: _,
            actor,
            note,
        } => ingest_note(engine, translator, actor, note, raw),
        Inbound::Follow {
            activity_id,
            actor,
            object,
        } => ingest_follow(
            engine,
            translator,
            public_url,
            actor,
            object,
            activity_id,
            raw,
        ),
        Inbound::Like {
            activity_id,
            actor,
            object,
        } => ingest_target(
            engine,
            translator,
            actor,
            object,
            activity_id,
            "pub.reed.like",
            raw,
        ),
        Inbound::Announce {
            activity_id,
            actor,
            object,
        } => ingest_target(
            engine,
            translator,
            actor,
            object,
            activity_id,
            "pub.reed.boost",
            raw,
        ),
        Inbound::Undo { .. } => Err("undo not mapped in v1".into()),
    }
}

fn ingest_note(
    engine: &mut KnotEngine,
    translator: &str,
    actor: &str,
    note: &Value,
    raw: &[u8],
) -> Result<Ingested, String> {
    let foreign_id = canonical_foreign_id(
        "activitypub",
        note.get("id").and_then(|i| i.as_str()).unwrap_or(""),
    )
    .map_err(|e| e.to_string())?;
    let rkey = import_rkey("activitypub", &foreign_id).map_err(|e| e.to_string())?;
    let mapped = note_to_post(note).map_err(|e| e.to_string())?;
    let id = activitypub_actor_identity(actor);
    let did = id.did_string();
    remember_identity(engine, &did);
    let reply = mapped
        .reply_to
        .as_deref()
        .and_then(|fid| mapped_cid(engine, fid));
    let mut body = json!({ "text": mapped.text });
    if let Some(cid) = &reply {
        body["reply"] = json!(cid);
    }
    let post = sign_put(engine, &id, &did, "pub.tangled.post", &rkey, body)?;
    let bytes_cid = store_raw(engine, &did, raw);
    let hint = actor_hint(raw, note);
    let acct = ap_acct(actor, hint.as_ref());
    let mut env = json!({
        "v": 1,
        "protocol": "activitypub",
        "foreign_id": foreign_id,
        "foreign": actor,
        "bytes_cid": bytes_cid,
        "translator": translator,
        "attested": true,
        "mapped": post,
        "foreignProof": activitypub_foreign_proof(&id, actor, raw),
    });
    if let Some(acct) = &acct {
        env["acct"] = json!(acct);
    }
    if mapped.lossy || (mapped.reply_to.is_some() && reply.is_none()) {
        env["mapped_lossy"] = json!(true);
    }
    sign_put(engine, &id, &did, "sys.foreign", &rkey, env)?;
    upsert_profile(engine, actor, hint.as_ref(), None);
    Ok(Ingested {
        did,
        collection: "pub.tangled.post".into(),
        rkey,
    })
}

fn ingest_follow(
    engine: &mut KnotEngine,
    translator: &str,
    public_url: &str,
    actor: &str,
    object: &str,
    activity_id: &str,
    raw: &[u8],
) -> Result<Ingested, String> {
    let id = activitypub_actor_identity(actor);
    let did = id.did_string();
    remember_identity(engine, &did);
    let subject = local_subject(public_url, object, translator);
    let rkey = tangled::sync::safe_did_rkey(&subject);
    sign_put(
        engine,
        &id,
        &did,
        "tangled.graph.follow",
        &rkey,
        json!({ "v": 1, "subject": subject }),
    )?;
    let foreign_id = canonical_foreign_id(
        "activitypub",
        if activity_id.is_empty() {
            object
        } else {
            activity_id
        },
    )
    .map_err(|e| e.to_string())?;
    let env_rkey = import_rkey("activitypub", &foreign_id).map_err(|e| e.to_string())?;
    let bytes_cid = store_raw(engine, &did, raw);
    sign_put(engine, &id, &did, "sys.foreign", &env_rkey, {
        let mut env = json!({
            "v": 1,
            "protocol": "activitypub",
            "foreign_id": foreign_id,
            "foreign": actor,
            "bytes_cid": bytes_cid,
            "translator": translator,
            "attested": true,
            "foreignProof": activitypub_foreign_proof(&id, actor, raw),
        });
        if let Some(acct) = activitypub_actor_acct(actor, None) {
            env["acct"] = json!(acct);
        }
        env
    })?;
    let hint = actor_hint(raw, &json!({}));
    upsert_profile(engine, actor, hint.as_ref(), None);
    Ok(Ingested {
        did,
        collection: "tangled.graph.follow".into(),
        rkey,
    })
}

fn ingest_target(
    engine: &mut KnotEngine,
    translator: &str,
    actor: &str,
    object: &str,
    activity_id: &str,
    collection: &str,
    raw: &[u8],
) -> Result<Ingested, String> {
    let id = activitypub_actor_identity(actor);
    let did = id.did_string();
    remember_identity(engine, &did);
    let target = mapped_cid(engine, object).unwrap_or_else(|| object.to_string());
    let foreign_id = canonical_foreign_id(
        "activitypub",
        if activity_id.is_empty() {
            object
        } else {
            activity_id
        },
    )
    .map_err(|e| e.to_string())?;
    let rkey = import_rkey("activitypub", &foreign_id).map_err(|e| e.to_string())?;
    sign_put(
        engine,
        &id,
        &did,
        collection,
        &rkey,
        json!({ "target": target }),
    )?;
    let bytes_cid = store_raw(engine, &did, raw);
    sign_put(engine, &id, &did, "sys.foreign", &rkey, {
        let mut env = json!({
            "v": 1,
            "protocol": "activitypub",
            "foreign_id": foreign_id,
            "foreign": actor,
            "bytes_cid": bytes_cid,
            "translator": translator,
            "attested": true,
            "mapped": target,
            "foreignProof": activitypub_foreign_proof(&id, actor, raw),
        });
        if let Some(acct) = activitypub_actor_acct(actor, None) {
            env["acct"] = json!(acct);
        }
        env
    })?;
    let hint = actor_hint(raw, &json!({}));
    upsert_profile(engine, actor, hint.as_ref(), None);
    Ok(Ingested {
        did,
        collection: collection.into(),
        rkey,
    })
}

fn actor_hint(raw: &[u8], note: &Value) -> Option<Value> {
    if let Ok(v) = serde_json::from_slice::<Value>(raw) {
        if v.get("actor").is_some_and(|a| a.is_object()) {
            return v.get("actor").cloned();
        }
        if v.get("attributedTo").is_some_and(|a| a.is_object()) {
            return v.get("attributedTo").cloned();
        }
    }
    note.get("attributedTo").filter(|a| a.is_object()).cloned()
}

fn ap_acct(actor: &str, hint: Option<&Value>) -> Option<String> {
    let preferred = hint.and_then(tangled_ap::actor::preferred_username);
    activitypub_actor_acct(actor, preferred.as_deref())
}

/// Last-ts-wins `pub.reed.profile` / `self` for a hosted AP DID.
pub fn upsert_profile(
    engine: &mut KnotEngine,
    actor: &str,
    hint: Option<&Value>,
    avatar_cid: Option<&str>,
) {
    let id = activitypub_actor_identity(actor);
    let did = id.did_string();
    remember_identity(engine, &did);
    let mapped = hint
        .map(|h| tangled_ap::actor_to_profile(h, actor))
        .unwrap_or_else(|| {
            let acct = activitypub_actor_acct(actor, None);
            tangled_ap::MappedProfile {
                display_name: acct
                    .as_deref()
                    .and_then(|a| a.split('@').next())
                    .unwrap_or("")
                    .to_string(),
                handle: acct,
                url: actor.to_string(),
                bio: String::new(),
                icon_url: None,
            }
        });
    let mut body = json!({
        "displayName": mapped.display_name,
        "bio": mapped.bio,
        "url": mapped.url,
    });
    if let Some(acct) = &mapped.handle {
        body["handle"] = json!(acct);
    }
    if let Some(cid) = avatar_cid.filter(|c| !c.is_empty()) {
        body["avatar"] = json!(cid);
    }
    let new_ts = engine.now.saturating_mul(1_000_000);
    if let Some(existing) = engine.stored.iter().rev().find(|r| {
        r.did == did && r.collection == "pub.reed.profile" && r.rkey == "self" && !r.deleted
    }) {
        let old_ts = existing
            .record
            .get("ts")
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        if old_ts > new_ts {
            return;
        }
        if existing.record.get("body") == Some(&body) {
            return;
        }
        let old_avatar = existing
            .record
            .get("body")
            .and_then(|b| b.get("avatar"))
            .and_then(|v| v.as_str());
        if old_ts == new_ts && old_avatar.is_some() && body.get("avatar").is_none() {
            return;
        }
    }
    let _ = sign_put(engine, &id, &did, "pub.reed.profile", "self", body);
}

fn sign_put(
    engine: &mut KnotEngine,
    id: &tangled::Identity,
    author: &str,
    collection: &str,
    rkey: &str,
    body: Value,
) -> Result<String, String> {
    let ts = engine.now.saturating_mul(1_000_000);
    let unsigned = json!({
        "v": 1,
        "type": Cid::dag_cbor(collection.as_bytes()).to_string(),
        "collection": collection,
        "rkey": rkey,
        "author": author,
        "ts": ts,
        "prev": [],
        "vis": "public",
        "body": body,
    });
    let signed = sign_json_record(id.signing_key(), &unsigned).map_err(|e| e.to_string())?;
    let cid = signed.cid.to_string();
    engine
        .ingest_record(signed.json)
        .map_err(|e| e.to_string())?;
    Ok(cid)
}

pub fn store_blob(engine: &mut KnotEngine, did: &str, raw: &[u8]) -> String {
    store_raw(engine, did, raw)
}

fn store_raw(engine: &mut KnotEngine, did: &str, raw: &[u8]) -> String {
    let cid = Cid::raw(raw).to_string();
    engine
        .blobs
        .insert((did.to_string(), cid.clone()), raw.to_vec());
    engine.blocks.insert(cid.clone(), raw.to_vec());
    cid
}

fn remember_identity(engine: &mut KnotEngine, did: &str) {
    engine.identities.entry(did.to_string()).or_insert_with(|| {
        tangled::http::IdentityResolveResponse {
            did: did.to_string(),
            id_cid: None,
            endpoints: vec![engine
                .knot_base
                .clone()
                .unwrap_or_else(|| format!("inproc://{did}"))],
            document: None,
        }
    });
}

fn mapped_cid(engine: &KnotEngine, foreign_id: &str) -> Option<String> {
    if let Ok(id) = canonical_foreign_id("activitypub", foreign_id) {
        if let Ok(rkey) = import_rkey("activitypub", &id) {
            if let Some(rec) = engine
                .stored
                .iter()
                .rev()
                .find(|r| r.collection == "sys.foreign" && r.rkey == rkey && !r.deleted)
            {
                if let Some(m) = rec
                    .record
                    .get("body")
                    .and_then(|b| b.get("mapped"))
                    .and_then(|m| m.as_str())
                {
                    return Some(m.to_string());
                }
            }
        }
    }
    let prefix_notes = "/notes/";
    if let Some(idx) = foreign_id.rfind(prefix_notes) {
        return Some(foreign_id[idx + prefix_notes.len()..].to_string());
    }
    None
}

fn local_subject(public_url: &str, object: &str, knot_did: &str) -> String {
    let prefix = format!("{}/users/", public_url.trim_end_matches('/'));
    if let Some(name) = object.strip_prefix(&prefix) {
        if name == "gateway" {
            return knot_did.to_string();
        }
        if let Ok(did) = tangled::Did::parse(&format!("did:sol:{name}")) {
            return did.as_str();
        }
        if name.starts_with("did:sol:") {
            return name.to_string();
        }
    }
    if object.starts_with("did:sol:") {
        return object.to_string();
    }
    activitypub_actor_identity(object).did_string()
}
