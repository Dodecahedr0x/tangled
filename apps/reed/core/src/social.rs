use crate::error::{ReedError, Result};
use crate::identity::ReedKeys;
use crate::outbox::Outbox;
use crate::replica::Replica;
use crate::types::{PostId, Profile};
use crate::wire::{
    cid_safe, ensure_schema, put_signed, sign, unsigned, BOOST, LIKE, MUTE, PROFILE,
};
use tangled::Did;

pub fn mute(
    keys: &ReedKeys,
    replica: &mut Replica,
    outbox: &mut Outbox,
    subject: &Did,
) -> Result<()> {
    ensure_schema(
        keys,
        replica,
        outbox,
        MUTE,
        serde_json::json!({ "subject": { "type": "string" } }),
    )?;
    let author = keys.did().as_str();
    let sub = subject.as_str();
    let rkey = tangled::sync::safe_did_rkey(&sub);
    let body = serde_json::json!({ "subject": sub });
    let rec = sign(keys, &unsigned(&author, MUTE, &rkey, "public", body, None))?;
    put_signed(replica, outbox, rec)?;
    Ok(())
}

pub fn unmute(
    keys: &ReedKeys,
    replica: &mut Replica,
    outbox: &mut Outbox,
    subject: &Did,
) -> Result<()> {
    ensure_schema(
        keys,
        replica,
        outbox,
        MUTE,
        serde_json::json!({ "subject": { "type": "string" } }),
    )?;
    let author = keys.did().as_str();
    let sub = subject.as_str();
    let rkey = tangled::sync::safe_did_rkey(&sub);
    let body = serde_json::json!({ "subject": sub, "deleted": true });
    let rec = sign(keys, &unsigned(&author, MUTE, &rkey, "public", body, None))?;
    put_signed(replica, outbox, rec)?;
    Ok(())
}

pub fn like(
    keys: &ReedKeys,
    replica: &mut Replica,
    outbox: &mut Outbox,
    target: &PostId,
) -> Result<()> {
    ensure_schema(
        keys,
        replica,
        outbox,
        LIKE,
        serde_json::json!({ "target": { "type": "string" } }),
    )?;
    target_put(keys, replica, outbox, LIKE, &target.0, false)
}

pub fn unlike(
    keys: &ReedKeys,
    replica: &mut Replica,
    outbox: &mut Outbox,
    target: &PostId,
) -> Result<()> {
    ensure_schema(
        keys,
        replica,
        outbox,
        LIKE,
        serde_json::json!({ "target": { "type": "string" } }),
    )?;
    target_put(keys, replica, outbox, LIKE, &target.0, true)
}

pub fn boost(
    keys: &ReedKeys,
    replica: &mut Replica,
    outbox: &mut Outbox,
    target: &PostId,
) -> Result<()> {
    ensure_schema(
        keys,
        replica,
        outbox,
        BOOST,
        serde_json::json!({ "target": { "type": "string" } }),
    )?;
    target_put(keys, replica, outbox, BOOST, &target.0, false)
}

pub fn unboost(
    keys: &ReedKeys,
    replica: &mut Replica,
    outbox: &mut Outbox,
    target: &PostId,
) -> Result<()> {
    ensure_schema(
        keys,
        replica,
        outbox,
        BOOST,
        serde_json::json!({ "target": { "type": "string" } }),
    )?;
    target_put(keys, replica, outbox, BOOST, &target.0, true)
}

pub fn put_profile(
    keys: &ReedKeys,
    replica: &mut Replica,
    outbox: &mut Outbox,
    profile: &Profile,
) -> Result<()> {
    ensure_schema(
        keys,
        replica,
        outbox,
        PROFILE,
        serde_json::json!({
            "displayName": { "type": "string" },
            "bio": { "type": "string" },
            "avatar": { "type": "string" }
        }),
    )?;
    let author = keys.did().as_str();
    if profile.did != author {
        return Err(ReedError::Identity("profile did mismatch".into()));
    }
    let mut body = serde_json::json!({
        "displayName": profile.display_name.clone().unwrap_or_default(),
        "bio": profile.bio.clone().unwrap_or_default(),
    });
    if let Some(cid) = &profile.avatar_cid {
        body["avatar"] = serde_json::Value::String(cid.clone());
    }
    let rec = sign(
        keys,
        &unsigned(&author, PROFILE, "self", "public", body, None),
    )?;
    put_signed(replica, outbox, rec)?;
    Ok(())
}

fn target_put(
    keys: &ReedKeys,
    replica: &mut Replica,
    outbox: &mut Outbox,
    collection: &str,
    target: &str,
    deleted: bool,
) -> Result<()> {
    let author = keys.did().as_str();
    let rkey = cid_safe(target);
    let mut body = serde_json::json!({ "target": target });
    if deleted {
        body["deleted"] = serde_json::Value::Bool(true);
    }
    let rec = sign(
        keys,
        &unsigned(&author, collection, &rkey, "public", body, None),
    )?;
    put_signed(replica, outbox, rec)?;
    Ok(())
}
