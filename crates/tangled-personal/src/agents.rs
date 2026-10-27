//! External agents use signed, collection-scoped protocol capabilities.
use crate::{Result, Store};
use serde_json::{json, Value};
use tangled::capability::Capability;

const GRANTS: &str = "pub.tangled.agent.grant";
const ACTIVITY: &str = "pub.tangled.agent.activity";

/// Owner controls: grant/revoke/list/report. Delegated controls: search/write.
/// Delegated requests include a hex-encoded DAG-CBOR Invocation whose signed
/// args are exactly the input object with `invocation` removed.
pub fn execute(store: &mut Store, input: Value) -> Result<Value> {
    let action = text(&input, "action")?;
    match action {
        "grant" => {
            let collection = text(&input, "collection")?;
            check_collection(collection)?;
            let command = match text(&input, "access")? {
                "read" => "repo/read",
                "write" => "repo/write",
                _ => return Err("access must be read or write".into()),
            };
            let mut grant = store.issue_agent_capability(
                text(&input, "audience")?,
                collection,
                command,
                input["ttl_seconds"]
                    .as_u64()
                    .ok_or("ttl_seconds must be an integer")?,
            )?;
            let cid = text(&grant, "cid")?.to_string();
            let record = store.put(GRANTS, &cid[1..], grant.clone(), "cap")?;
            grant["record"] = record;
            Ok(grant)
        }
        "revoke" => {
            let cid = text(&input, "cid")?;
            tangled::Cid::parse(cid).map_err(|e| e.to_string())?;
            let grant = store.get(GRANTS, &cid[1..])?;
            if grant["body"]["cid"] != cid {
                return Err("Unknown agent capability".into());
            }
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs();
            store.put("sys.revocation", &cid[1..], json!({"cid":cid,"ts":ts,"reason":input.get("reason").and_then(Value::as_str).unwrap_or("owner revoked")}), "cap")
        }
        "list" => Ok(json!({"grants":store.list(Some(GRANTS))?})),
        "report" => Ok(json!({"activities":store.list(Some(ACTIVITY))?})),
        "search" | "write" => delegated(store, input),
        _ => Err("Unknown agents action; use grant, revoke, list, search, write, report".into()),
    }
}

fn delegated(store: &mut Store, mut args: Value) -> Result<Value> {
    let encoded = text(&args, "invocation")?.to_string();
    if encoded.len() > 2 * 1024 * 1024 {
        return Err("Invocation exceeds 1 MiB".into());
    }
    args.as_object_mut()
        .ok_or("Input must be an object")?
        .remove("invocation");
    let bytes = hex::decode(encoded).map_err(|_| "invocation must be hex DAG-CBOR")?;
    let collection = text(&args, "collection")?;
    check_collection(collection)?;
    let action = text(&args, "action")?;
    let command = if action == "search" {
        "repo/read"
    } else {
        "repo/write"
    };
    let inv = store.verify_agent_invocation(&bytes, &args, collection, command)?;
    let cap = inv.leaf().map_err(|e| e.to_string())?;
    let cid = cap.cid().map_err(|e| e.to_string())?.to_string();
    let grant = store.get(GRANTS, &cid[1..])?;
    // Match the signed capability to its current signed owner grant record.
    let recorded =
        Capability::from_json(&grant["body"]["capability"]).map_err(|e| e.to_string())?;
    if recorded != *cap || grant["body"]["cid"] != cid {
        return Err("Capability is not an issued workspace grant".into());
    }
    let receipt_key =
        hex::encode(blake3::hash(&[cap.aud.as_bytes(), &inv.nonce].concat()).as_bytes());
    if store.contains_key(ACTIVITY, &receipt_key) {
        return Err("Agent invocation nonce was already used".into());
    }
    let (mut output, citations) = if action == "search" {
        let query = text(&args, "query")?;
        if query.len() > 4096 {
            return Err("Search query exceeds 4096 bytes".into());
        }
        let records: Vec<_> = store
            .list(Some(collection))?
            .into_iter()
            .filter(|r| {
                r["body"]
                    .to_string()
                    .to_lowercase()
                    .contains(&query.to_lowercase())
            })
            .take(100)
            .collect();
        let citations: Vec<_> = records.iter().map(citation).collect();
        (json!({"records":records}), citations)
    } else {
        let body = args
            .get("body")
            .filter(|b| b.is_object())
            .ok_or("Write body must be an object")?;
        if serde_json::to_vec(body).map_err(|e| e.to_string())?.len() > 256 * 1024 {
            return Err("Agent result exceeds 256 KiB".into());
        }
        // This local owner-side service signs the authorized result, never gives
        // its root signing key to the agent, and always uses private visibility.
        let rkey = text(&args, "rkey")?;
        if store.contains_key(collection, rkey) {
            return Err(
                "Agent results must use a new record key; existing records cannot be overwritten"
                    .into(),
            );
        }
        let record = store.put(collection, rkey, body.clone(), "cap")?;
        let citations = vec![citation(&record)];
        (json!({"record":record}), citations)
    };
    let invocation_cid = tangled::Cid::dag_cbor(&bytes).to_string();
    let receipt = store.put(ACTIVITY, &receipt_key, json!({"actor":cap.aud,"action":action,"capability":cid,"invocation":invocation_cid,"invocation_cbor":hex::encode(&bytes),"request":args,"citations":citations}), "cap")?;
    output["receipt"] = receipt;
    Ok(output)
}

fn citation(record: &Value) -> Value {
    json!({"uri":record["uri"],"cid":record["cid"]})
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} must be a string"))
}
fn check_collection(collection: &str) -> Result<()> {
    if !collection.starts_with("pub.tangled.")
        || collection.len() > 256
        || matches!(collection, GRANTS | ACTIVITY)
        || !collection
            .split('.')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
    {
        return Err(
            "Agent scope must be one pub.tangled collection outside workspace control records"
                .into(),
        );
    }
    Ok(())
}
