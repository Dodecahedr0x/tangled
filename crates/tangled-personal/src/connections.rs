//! Private source projections and local organization; never a source transport.
use crate::{Result, Store};
use serde_json::{json, Value};

const ITEM: &str = "pub.tangled.sourceItem";
const OVERLAY: &str = "pub.tangled.sourceOverlay";
const CONTACT: &str = "pub.tangled.sourceContact";

/// Executes deterministic operations over supplied activity; no ambient credentials or network.
pub fn execute(store: &mut Store, input: Value) -> Result<Value> {
    if !input.is_object() {
        return Err("Expected a request object".into());
    }
    match string(&input, "action")? {
        "ingest" => ingest(store, &input),
        "get" => store.get(ITEM, string(&input, "rkey")?),
        "list" | "search" => list(store, &input),
        "history" => Ok(store.get(ITEM, string(&input, "rkey")?)?["body"]["revisions"].clone()),
        "original" => original(store, &input),
        "select-revision" => select_revision(store, &input),
        "overlay" => overlay(store, &input),
        "contact" => contact(store, &input),
        "contacts" => contacts(store, &input),
        "reply-preview" => preview(store, &input),
        "reply-export" => export_reply(store, &input),
        _ => Err("Unknown connections action".into()),
    }
}

fn string<'a>(v: &'a Value, field: &str) -> Result<&'a str> {
    v[field]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 65536)
        .ok_or_else(|| format!("Expected nonempty string: {field}"))
}

fn stable_key(value: &Value) -> Result<String> {
    let mut canonical = value.clone();
    canonical.sort_all_objects();
    let bytes = serde_json::to_vec(&canonical).map_err(|e| e.to_string())?;
    Ok(hex::encode(blake3::hash(&bytes).as_bytes()))
}

fn existing(store: &mut Store, collection: &str, key: &str) -> Result<Option<Value>> {
    Ok(store
        .list(Some(collection))?
        .into_iter()
        .find(|v| v["rkey"] == key))
}

// Recovery activity contains data, never the recovered-account or backup key bundle.
// Reject recognized credential fields instead of copying or silently discarding them.
fn reject_credentials(value: &Value) -> Result<()> {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let key = key.to_ascii_lowercase().replace(['-', '_'], "");
                if matches!(
                    key.as_str(),
                    "ed25519"
                        | "x25519"
                        | "privatekey"
                        | "secretkey"
                        | "accesstoken"
                        | "refreshtoken"
                        | "password"
                        | "authorization"
                        | "credentials"
                        | "archivekey"
                        | "apikey"
                        | "sessiontoken"
                ) {
                    return Err(
                        "Credential-bearing input is not activity; supply activity.json only"
                            .into(),
                    );
                }
                reject_credentials(value)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                reject_credentials(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn identity(row: &Value) -> Result<Value> {
    if !row.is_object() || !row["object"].is_object() {
        return Err("Expected recovery activity entry with object".into());
    }
    let object = &row["object"];
    for field in ["conversation", "sender", "timestamp", "text"] {
        if !object[field].is_null() && !object[field].is_string() {
            return Err(format!("Expected string or null: {field}"));
        }
    }
    if let Some(files) = object.get("attachments") {
        if !files
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
        {
            return Err("attachments must be native string references".into());
        }
    }
    if object.get("deleted").is_some_and(|v| !v.is_boolean()) {
        return Err("deleted must be boolean".into());
    }
    if row.get("revisionOrder").is_some_and(|v| !v.is_u64()) {
        return Err("revisionOrder must be a nonnegative integer".into());
    }
    Ok(
        json!({"source":string(row,"source")?,"account":string(row,"account")?,
        "kind":string(object,"kind")?,"conversation":object["conversation"],"id":string(object,"id")?}),
    )
}

fn revision(row: &Value) -> Option<u64> {
    row["revisionOrder"]
        .as_u64()
        .or_else(|| row["object"]["data"]["source_revision"].as_u64())
}

fn deleted(row: &Value) -> bool {
    row["object"]["deleted"].as_bool().unwrap_or(false)
}

fn ingest(store: &mut Store, input: &Value) -> Result<Value> {
    let rows = input["activity"]
        .as_array()
        .ok_or("activity must be the array from recovery activity.json")?;
    if rows.len() > 1000
        || serde_json::to_vec(rows).map_err(|e| e.to_string())?.len() > 8 * 1024 * 1024
    {
        return Err("Activity exceeds batch limit (1000 entries / 8 MiB)".into());
    }
    // Validate the complete batch before any writes, including caller-supplied original bytes.
    let mut planned = Vec::new();
    for row in rows {
        reject_credentials(row)?;
        let id = identity(row)?;
        if let Some(original) = row.get("original") {
            original_bytes(original)?;
            string(original, "mime")?;
        }
        planned.push((row, stable_key(&id)?, id));
    }
    let mut result = Vec::new();
    for (row, key, id) in planned {
        let old = existing(store, ITEM, &key)?;
        let mut body = old
            .as_ref()
            .map(|v| v["body"].clone())
            .unwrap_or_else(|| json!({"identity":id,"revisions":[]}));
        let mut revisions = body["revisions"]
            .as_array()
            .ok_or("Invalid stored revision history")?
            .clone();
        if revisions.iter().any(|r| r["imported"] == *row) {
            result.push(old.ok_or("Missing current source record")?);
            continue;
        }
        let raw = serde_json::to_vec(row).map_err(|e| e.to_string())?;
        let original_cid = store.put_blob(&raw, "application/json")?;
        let mut entry = json!({"imported":row,"originalBlob":original_cid});
        if let Some(original) = row.get("original") {
            let bytes = original_bytes(original)?;
            entry["sourceOriginalBlob"] = json!(store.put_blob(&bytes, string(original, "mime")?)?);
            entry["sourceOriginalMime"] = original["mime"].clone();
        }
        let choose = old.is_none()
            || matches!((revision(row), revision(&body["imported"])),(Some(next),Some(prior)) if next>prior);
        revisions.push(entry);
        if choose {
            body["imported"] = row.clone();
            body["currentRevision"] = json!(revisions.len() - 1);
            body["deleted"] = json!(deleted(row));
        }
        body["revisions"] = json!(revisions);
        result.push(store.put(ITEM, &key, body, "cap")?);
    }
    Ok(json!({"items":result}))
}

fn original_bytes(value: &Value) -> Result<Vec<u8>> {
    let bytes = value["bytes"]
        .as_array()
        .ok_or("original.bytes must be a byte array")?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("Source original exceeds 4 MiB".into());
    }
    bytes
        .iter()
        .map(|b| {
            b.as_u64()
                .filter(|n| *n <= 255)
                .map(|n| n as u8)
                .ok_or_else(|| "Invalid original byte".into())
        })
        .collect()
}

fn index(input: &Value) -> Result<usize> {
    input["index"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| "Expected revision index".into())
}

fn original(store: &mut Store, input: &Value) -> Result<Value> {
    let record = store.get(ITEM, string(input, "rkey")?)?;
    let entry = record["body"]["revisions"]
        .as_array()
        .and_then(|r| r.get(index(input).ok()?))
        .ok_or("Unknown revision index")?;
    let source = input["sourceOriginal"].as_bool().unwrap_or(false);
    let field = if source {
        "sourceOriginalBlob"
    } else {
        "originalBlob"
    };
    let cid = entry[field]
        .as_str()
        .ok_or("Original bytes unavailable for this revision")?;
    let bytes = store.get_blob(cid)?;
    Ok(
        json!({"cid":cid,"mime":if source {entry["sourceOriginalMime"].clone()} else {json!("application/json")},"bytes":bytes}),
    )
}

fn select_revision(store: &mut Store, input: &Value) -> Result<Value> {
    let key = string(input, "rkey")?;
    let record = store.get(ITEM, key)?;
    let mut body = record["body"].clone();
    let i = index(input)?;
    let row = body["revisions"]
        .as_array()
        .and_then(|r| r.get(i))
        .ok_or("Unknown revision index")?["imported"]
        .clone();
    body["imported"] = row.clone();
    body["currentRevision"] = json!(i);
    body["deleted"] = json!(deleted(&row));
    store.put(ITEM, key, body, "cap")
}

fn validate_patch(patch: &Value, reserved: &[&str]) -> Result<()> {
    let map = patch.as_object().ok_or("patch must be an object")?;
    if map.keys().any(|k| reserved.contains(&k.as_str())) {
        return Err("Patch cannot replace identity, target, or reviewed reply fields".into());
    }
    for field in ["tags", "groups"] {
        if let Some(value) = map.get(field) {
            if !value.as_array().is_some_and(|a| {
                a.len() <= 256 && a.iter().all(|s| s.as_str().is_some_and(|s| s.len() <= 512))
            }) {
                return Err(format!("{field} must be a string array"));
            }
        }
    }
    for field in ["note", "alias"] {
        if map.get(field).is_some_and(|v| !v.is_string()) {
            return Err(format!("{field} must be a string"));
        }
    }
    if map.get("bookmarked").is_some_and(|v| !v.is_boolean()) {
        return Err("bookmarked must be boolean".into());
    }
    reject_credentials(patch)
}

fn apply_patch(body: &mut Value, patch: &Value) -> Result<()> {
    body.as_object_mut()
        .ok_or("Invalid stored body")?
        .extend(patch.as_object().ok_or("patch must be object")?.clone());
    Ok(())
}

fn overlay(store: &mut Store, input: &Value) -> Result<Value> {
    let key = string(input, "rkey")?;
    let item = store.get(ITEM, key)?;
    validate_patch(&input["patch"], &["target", "reply"])?;
    let mut body = existing(store, OVERLAY, key)?
        .map(|v| v["body"].clone())
        .unwrap_or_else(|| json!({"target":item["uri"]}));
    apply_patch(&mut body, &input["patch"])?;
    store.put(OVERLAY, key, body, "cap")
}

fn contact(store: &mut Store, input: &Value) -> Result<Value> {
    let identity = json!({"source":string(input,"source")?,"account":string(input,"account")?,"sender":string(input,"sender")?});
    validate_patch(&input["patch"], &["identity"])?;
    let key = stable_key(&identity)?;
    let mut body = existing(store, CONTACT, &key)?
        .map(|v| v["body"].clone())
        .unwrap_or_else(|| json!({"identity":identity}));
    apply_patch(&mut body, &input["patch"])?;
    store.put(CONTACT, &key, body, "cap")
}

fn contains(array: &Value, value: &Value) -> bool {
    array.as_array().is_some_and(|a| a.contains(value))
}

fn contacts(store: &mut Store, input: &Value) -> Result<Value> {
    let rows = store
        .list(Some(CONTACT))?
        .into_iter()
        .filter(|row| {
            ["source", "account", "sender"].iter().all(|f| {
                input
                    .get(*f)
                    .is_none_or(|v| v == &row["body"]["identity"][*f])
            }) && input
                .get("group")
                .is_none_or(|v| contains(&row["body"]["groups"], v))
        })
        .collect::<Vec<_>>();
    Ok(json!(rows))
}

fn list(store: &mut Store, input: &Value) -> Result<Value> {
    let overlays = store.list(Some(OVERLAY))?;
    let contact = if let Some(uri) = input.get("contact") {
        Some(
            store
                .list(Some(CONTACT))?
                .into_iter()
                .find(|r| &r["uri"] == uri)
                .ok_or("Unknown source contact URI")?["body"]["identity"]
                .clone(),
        )
    } else {
        None
    };
    let query = if input.get("query").is_some() {
        string(input, "query")?.to_lowercase()
    } else {
        String::new()
    };
    let mut rows = Vec::new();
    for mut row in store.list(Some(ITEM))? {
        let imported = &row["body"]["imported"];
        let object = &imported["object"];
        if row["body"]["deleted"] == true && input["includeDeleted"] != true {
            continue;
        }
        if !["source", "account"]
            .iter()
            .all(|f| input.get(*f).is_none_or(|v| v == &imported[*f]))
            || !["conversation", "sender"]
                .iter()
                .all(|f| input.get(*f).is_none_or(|v| v == &object[*f]))
        {
            continue;
        }
        if let Some(c) = &contact {
            if c["source"] != imported["source"]
                || c["account"] != imported["account"]
                || c["sender"] != object["sender"]
            {
                continue;
            }
        }
        let overlay = overlays.iter().find(|o| o["body"]["target"] == row["uri"]);
        let metadata = overlay.map(|o| &o["body"]);
        if input
            .get("bookmarked")
            .is_some_and(|v| Some(metadata.is_some_and(|m| m["bookmarked"] == true)) != v.as_bool())
        {
            continue;
        }
        if input
            .get("tag")
            .is_some_and(|v| !metadata.is_some_and(|m| contains(&m["tags"], v)))
        {
            continue;
        }
        let searchable = format!(
            "{} {}",
            object["text"].as_str().unwrap_or(""),
            metadata
                .map(|m| m["note"].as_str().unwrap_or(""))
                .unwrap_or("")
        );
        if !searchable.to_lowercase().contains(&query) {
            continue;
        }
        if let Some(overlay) = overlay {
            row["overlay"] = overlay.clone();
        }
        rows.push(row);
    }
    Ok(json!(rows))
}

fn preview(store: &mut Store, input: &Value) -> Result<Value> {
    let key = string(input, "rkey")?;
    let item = store.get(ITEM, key)?;
    if item["body"]["deleted"] == true {
        return Err("Cannot reply to a deleted source item".into());
    }
    let row = &item["body"]["imported"];
    let account = string(input, "account")?;
    let recipient = string(input, "recipient")?;
    let text = string(input, "text")?;
    if row["account"] != account || row["object"]["sender"] != recipient {
        return Err("Explicit account and recipient must match this source item".into());
    }
    let mut body = existing(store, OVERLAY, key)?
        .map(|v| v["body"].clone())
        .unwrap_or_else(|| json!({"target":item["uri"]}));
    body["reply"] = json!({"source":row["source"],"account":account,"recipient":recipient,"conversation":row["object"]["conversation"],"inReplyTo":row["object"]["id"],"kind":row["object"]["kind"],"text":text,"targetUri":item["uri"],"targetCid":item["cid"],"status":"draft"});
    store.put(OVERLAY, key, body, "cap")
}

fn export_reply(store: &mut Store, input: &Value) -> Result<Value> {
    let key = string(input, "rkey")?;
    let item = store.get(ITEM, key)?;
    let overlay = store.get(OVERLAY, key)?;
    if input["confirmed"] != true || overlay["cid"] != string(input, "previewCid")? {
        return Err("Confirm the exact current preview CID before exporting".into());
    }
    let mut intent = overlay["body"]["reply"].clone();
    if intent["status"] != "draft"
        || intent["targetCid"] != item["cid"]
        || item["body"]["deleted"] == true
    {
        return Err("Source changed or no reply draft exists; review a new preview".into());
    }
    intent["type"] = json!("pub.tangled.replyIntent");
    intent["status"] = json!("confirmed");
    intent["previewCid"] = overlay["cid"].clone();
    intent["delivery"] = json!("not-sent");
    Ok(intent)
}
