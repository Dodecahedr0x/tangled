//! Private notes, self-inbox captures, drafts and queryable reminders.
use crate::{Result, Store};
use serde_json::{json, Map, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub const COLLECTION: &str = "pub.tangled.note";
const MAX_ATTACHMENT: usize = 16 * 1024 * 1024;

/// JSON command interface; all writes use capability-gated visibility.
pub fn execute(store: &mut Store, input: Value) -> Result<Value> {
    let action = string(&input, "action")?;
    match action {
        "create" | "capture" | "convert" => create(store, &input, action),
        "list" | "due" => {
            let state = input
                .get("state")
                .map(|v| v.as_str().ok_or("state must be a string"))
                .transpose()?;
            if let Some(state) = state {
                validate_state(state)?;
            }
            let at = match input.get("at") {
                Some(v) => v.as_u64().ok_or("at must be nonnegative Unix seconds")?,
                None => now()?,
            };
            let mut records = store.list(Some(COLLECTION))?;
            records.retain(|record| {
                let body = &record["body"];
                state.is_none_or(|s| body["state"] == s)
                    && (action != "due" || body["reminderAt"].as_u64().is_some_and(|t| t <= at))
            });
            records.sort_by(|a, b| {
                let field = if action == "due" {
                    "reminderAt"
                } else {
                    "createdAt"
                };
                a["body"][field]
                    .as_u64()
                    .cmp(&b["body"][field].as_u64())
                    .then_with(|| a["uri"].as_str().cmp(&b["uri"].as_str()))
            });
            Ok(Value::Array(records))
        }
        "get" => store.get(COLLECTION, string(&input, "rkey")?),
        "delete" => store.remove(COLLECTION, string(&input, "rkey")?),
        "edit" | "append" | "attach" => update(store, &input, action),
        "attachment" => {
            let record = store.get(COLLECTION, string(&input, "rkey")?)?;
            let index = input["index"]
                .as_u64()
                .ok_or("index must be a nonnegative integer")?;
            let index = usize::try_from(index).map_err(|_| "attachment index is too large")?;
            let attachment = record["body"]["attachments"]
                .get(index)
                .ok_or("attachment not found")?;
            let cid = string(attachment, "cid")?;
            let bytes = store.get_blob(cid)?;
            Ok(json!({"cid":cid,"mime":attachment["mime"],"name":attachment["name"],"bytes":bytes}))
        }
        _ => Err(format!("unknown notes action: {action}")),
    }
}

fn create(store: &mut Store, input: &Value, action: &str) -> Result<Value> {
    let mut body = match input.get("body") {
        Some(v) => v.as_object().ok_or("body must be an object")?.clone(),
        None if action == "convert" => Map::new(),
        None => return Err("body must be an object".into()),
    };
    for field in ["createdAt", "updatedAt", "provenance", "uri", "cid"] {
        if body.contains_key(field) {
            return Err(format!("{field} is managed by notes"));
        }
    }
    if action == "convert" {
        let source = input.get("source").ok_or("source is required")?;
        let record = store.get(string(source, "collection")?, string(source, "rkey")?)?;
        if !body.contains_key("text") {
            let text = record["body"]["text"]
                .as_str()
                .or_else(|| record["body"]["excerpt"].as_str())
                .unwrap_or("");
            body.insert("text".into(), json!(text));
        }
        body.insert(
            "provenance".into(),
            json!({"uri":string(&record,"uri")?,"cid":string(&record,"cid")?}),
        );
    }
    if action == "capture" {
        body.insert("state".into(), json!("inbox"));
    } else {
        body.entry("state").or_insert(json!("note"));
    }
    let timestamp = now()?;
    body.insert("createdAt".into(), json!(timestamp));
    body.insert("updatedAt".into(), json!(timestamp));
    validate(&body)?;
    let records = store.list(Some(COLLECTION))?;
    let key = match input.get("rkey") {
        Some(v) => v
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("rkey must be a nonempty string")?
            .to_string(),
        None => {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| "clock precedes Unix epoch")?
                .as_nanos();
            let seed = format!("{}:{nanos}:{}", store.did(), records.len());
            format!("note-{}", &blake3::hash(seed.as_bytes()).to_hex()[..24])
        }
    };
    if records.iter().any(|v| {
        v["uri"]
            .as_str()
            .is_some_and(|uri| uri.rsplit('/').next() == Some(key.as_str()))
    }) {
        return Err("note already exists; use edit".into());
    }
    check_blobs(store, &body)?;
    store.put(COLLECTION, &key, Value::Object(body), "cap")
}

fn update(store: &mut Store, input: &Value, action: &str) -> Result<Value> {
    let key = string(input, "rkey")?;
    let record = store.get(COLLECTION, key)?;
    let mut body = record["body"]
        .as_object()
        .ok_or("note body is not an object")?
        .clone();
    match action {
        "edit" => {
            let patch = input["patch"]
                .as_object()
                .ok_or("patch must be an object")?;
            for (field, value) in patch {
                if matches!(
                    field.as_str(),
                    "createdAt" | "updatedAt" | "provenance" | "uri" | "cid"
                ) {
                    return Err(format!("{field} is managed by notes"));
                }
                // Null clears optional standard fields. Unknown extension values are preserved verbatim.
                if value.is_null()
                    && matches!(field.as_str(), "title" | "reminderAt" | "attachments")
                {
                    body.remove(field);
                } else {
                    body.insert(field.clone(), value.clone());
                }
            }
        }
        "append" => {
            let text = string(input, "text")?;
            let existing = body
                .get("text")
                .and_then(Value::as_str)
                .ok_or("note text must be a string")?;
            body.insert(
                "text".into(),
                json!(if existing.is_empty() {
                    text.to_string()
                } else {
                    format!("{existing}\n{text}")
                }),
            );
        }
        "attach" => {
            let name = string(input, "name")?;
            let mime = string(input, "mime")?;
            validate_attachment_metadata(name, mime)?;
            let values = input["bytes"]
                .as_array()
                .ok_or("bytes must be an array of byte integers")?;
            if values.len() > MAX_ATTACHMENT {
                return Err("attachment exceeds 16 MiB".into());
            }
            let bytes: Vec<u8> = values
                .iter()
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| u8::try_from(n).ok())
                        .ok_or("bytes must contain integers from 0 to 255".to_string())
                })
                .collect::<Result<_>>()?;
            // Validate before adding a blob so malformed notes cannot leave orphan uploads.
            validate(&body)?;
            check_blobs(store, &body)?;
            let cid = store.put_blob(&bytes, mime)?;
            body.entry("attachments")
                .or_insert(json!([]))
                .as_array_mut()
                .ok_or("attachments must be an array")?
                .push(json!({"cid":cid,"mime":mime,"name":name}));
        }
        _ => return Err("unsupported update".into()),
    }
    body.insert("updatedAt".into(), json!(now()?));
    validate(&body)?;
    check_blobs(store, &body)?;
    store.put(COLLECTION, key, Value::Object(body), "cap")
}

fn validate(body: &Map<String, Value>) -> Result<()> {
    body.get("text")
        .and_then(Value::as_str)
        .ok_or("text must be a string")?;
    validate_state(
        body.get("state")
            .and_then(Value::as_str)
            .ok_or("state must be a string")?,
    )?;
    if let Some(title) = body.get("title") {
        let title = title.as_str().ok_or("title must be a string")?;
        if title.chars().count() > 512 {
            return Err("title exceeds 512 characters".into());
        }
    }
    for field in ["createdAt", "updatedAt", "reminderAt"] {
        if let Some(v) = body.get(field) {
            v.as_u64()
                .ok_or(format!("{field} must be nonnegative Unix seconds"))?;
        }
    }
    if let Some(provenance) = body.get("provenance") {
        let uri = string(provenance, "uri")?;
        if !uri.starts_with("tangled://did:") {
            return Err("provenance uri must be a Tangled record URI".into());
        }
        tangled::Cid::parse(string(provenance, "cid")?)
            .map_err(|e| format!("invalid provenance CID: {e}"))?;
    }
    if let Some(attachments) = body.get("attachments") {
        for attachment in attachments
            .as_array()
            .ok_or("attachments must be an array")?
        {
            tangled::Cid::parse(string(attachment, "cid")?)
                .map_err(|e| format!("invalid attachment CID: {e}"))?;
            validate_attachment_metadata(string(attachment, "name")?, string(attachment, "mime")?)?;
        }
    }
    Ok(())
}

fn check_blobs(store: &mut Store, body: &Map<String, Value>) -> Result<()> {
    if let Some(attachments) = body.get("attachments").and_then(Value::as_array) {
        for attachment in attachments {
            store.get_blob(string(attachment, "cid")?)?;
        }
    }
    Ok(())
}

fn validate_attachment_metadata(name: &str, mime: &str) -> Result<()> {
    if name.is_empty() || name.chars().count() > 512 {
        return Err("attachment name must be 1 to 512 characters".into());
    }
    if mime.len() > 255
        || !mime.split_once('/').is_some_and(|(kind, subtype)| {
            !kind.is_empty() && !subtype.is_empty() && !subtype.contains('/')
        })
        || mime
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    {
        return Err("mime must be a media type without whitespace".into());
    }
    Ok(())
}

fn validate_state(state: &str) -> Result<()> {
    if !matches!(state, "note" | "draft" | "inbox") {
        return Err("state must be note, draft, or inbox".into());
    }
    Ok(())
}

fn string<'a>(input: &'a Value, field: &str) -> Result<&'a str> {
    input
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{field} must be a string"))
}

fn now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "clock precedes Unix epoch")?
        .as_secs())
}
