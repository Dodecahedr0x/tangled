use crate::error::Result;
use crate::identity::ReedKeys;
use crate::outbox::Outbox;
use crate::replica::Replica;
use crate::wire::{put_signed, sign, unsigned, BLOCK, FOLLOW};
use tangled::Did;

pub fn follow(
    keys: &ReedKeys,
    replica: &mut Replica,
    outbox: &mut Outbox,
    subject: &Did,
    knot: Option<&str>,
) -> Result<()> {
    let author = keys.did().as_str();
    let sub = subject.as_str();
    let rkey = tangled::sync::safe_did_rkey(&sub);
    let mut body = serde_json::json!({ "v": 1, "subject": sub });
    if let Some(k) = knot {
        body["knot"] = serde_json::Value::String(k.to_string());
    }
    let rec = sign(
        keys,
        &unsigned(&author, FOLLOW, &rkey, "public", body, None),
    )?;
    put_signed(replica, outbox, rec)?;
    Ok(())
}

pub fn unfollow(
    keys: &ReedKeys,
    replica: &mut Replica,
    outbox: &mut Outbox,
    subject: &Did,
) -> Result<()> {
    graph_delete(keys, replica, outbox, FOLLOW, subject)
}

pub fn block(
    keys: &ReedKeys,
    replica: &mut Replica,
    outbox: &mut Outbox,
    subject: &Did,
) -> Result<()> {
    let author = keys.did().as_str();
    let sub = subject.as_str();
    let rkey = tangled::sync::safe_did_rkey(&sub);
    let body = serde_json::json!({ "v": 1, "subject": sub });
    let rec = sign(keys, &unsigned(&author, BLOCK, &rkey, "public", body, None))?;
    put_signed(replica, outbox, rec)?;
    Ok(())
}

pub fn unblock(
    keys: &ReedKeys,
    replica: &mut Replica,
    outbox: &mut Outbox,
    subject: &Did,
) -> Result<()> {
    graph_delete(keys, replica, outbox, BLOCK, subject)
}

fn graph_delete(
    keys: &ReedKeys,
    replica: &mut Replica,
    outbox: &mut Outbox,
    collection: &str,
    subject: &Did,
) -> Result<()> {
    let author = keys.did().as_str();
    let sub = subject.as_str();
    let rkey = tangled::sync::safe_did_rkey(&sub);
    let body = serde_json::json!({ "v": 1, "subject": sub, "deleted": true });
    let rec = sign(
        keys,
        &unsigned(&author, collection, &rkey, "public", body, None),
    )?;
    put_signed(replica, outbox, rec)?;
    Ok(())
}
