//! Private read-later items and an offline, plain-text reader.
use crate::{Result, Store};
use serde_json::{json, Map, Value};
use std::time::{SystemTime, UNIX_EPOCH};

const COLLECTION: &str = "pub.tangled.capture";
const MAX_BYTES: usize = 16 * 1024 * 1024;
const PROTECTED: &[&str] = &[
    "kind",
    "url",
    "blob",
    "mime",
    "size",
    "capturedAt",
    "identity",
    "version",
    "visibility",
    "uri",
    "cid",
    "action",
];

pub fn execute(store: &mut Store, input: Value) -> Result<Value> {
    if !input.is_object() {
        return Err("capture input must be an object".into());
    }
    let action = required_string(&input, "action")?;
    match action {
        "save" | "capture" => save(store, &input),
        "list" => {
            for name in ["read", "starred"] {
                optional_bool(&input, name)?;
            }
            optional_string(&input, "tag")?;
            let items = store.list(Some(COLLECTION))?;
            Ok(Value::Array(
                items
                    .into_iter()
                    .filter(|item| {
                        let body = &item["body"];
                        ["read", "starred"]
                            .iter()
                            .all(|key| input.get(*key).is_none_or(|value| body[*key] == *value))
                            && input.get("tag").is_none_or(|tag| {
                                body["tags"]
                                    .as_array()
                                    .is_some_and(|tags| tags.contains(tag))
                            })
                    })
                    .collect(),
            ))
        }
        "get" => {
            let key = record_key(store, &input)?;
            store.get(COLLECTION, &key)
        }
        "read" | "render" | "download" => {
            let key = record_key(store, &input)?;
            let item = store.get(COLLECTION, &key)?;
            reader(store, &item, action == "download")
        }
        "delete" => {
            let key = record_key(store, &input)?;
            store.remove(COLLECTION, &key)
        }
        "update" | "tag" | "mark-read" | "mark-unread" | "star" => {
            let key = record_key(store, &input)?;
            let item = store.get(COLLECTION, &key)?;
            let patch = match action {
                "update" => input
                    .get("patch")
                    .and_then(Value::as_object)
                    .cloned()
                    .ok_or("patch must be an object")?,
                "tag" => Map::from_iter([(
                    "tags".into(),
                    input.get("tags").cloned().ok_or("tags required")?,
                )]),
                "star" => Map::from_iter([(
                    "starred".into(),
                    Value::Bool(optional_bool(&input, "starred")?.unwrap_or(true)),
                )]),
                _ => Map::from_iter([("read".into(), Value::Bool(action == "mark-read"))]),
            };
            update(store, &key, &item, patch)
        }
        _ => Err(format!("unknown capture action: {action}")),
    }
}

fn save(store: &mut Store, input: &Value) -> Result<Value> {
    if input.get("visibility").is_some_and(|value| value != "cap") {
        return Err("captures are private; use an explicit publication workflow to share".into());
    }
    let url = optional_string(input, "url")?;
    if let Some(url) = url {
        validate_url(url)?;
    }
    let text = optional_string(input, "text")?;
    let kind =
        optional_string(input, "kind")?.unwrap_or(if text.is_some() { "text" } else { "bookmark" });
    if !["bookmark", "article", "text", "file", "screenshot"].contains(&kind) {
        return Err("kind must be bookmark, article, text, file, or screenshot".into());
    }
    let title = optional_string(input, "title")?.unwrap_or(url.unwrap_or("Untitled capture"));
    if title.len() > 4096 {
        return Err("title exceeds 4096 bytes".into());
    }
    let tags = tags(input.get("tags"))?;
    let read = optional_bool(input, "read")?.unwrap_or(false);
    let starred = optional_bool(input, "starred")?.unwrap_or(false);
    let source = input
        .get("source")
        .cloned()
        .unwrap_or(json!({"type":"manual"}));
    if !source.is_object() {
        return Err("source must be an object".into());
    }
    let mut body = match input.get("extensions") {
        Some(Value::Object(fields)) => fields.clone(),
        None => Map::new(),
        _ => return Err("extensions must be an object".into()),
    };
    for key in body.keys() {
        if PROTECTED.contains(&key.as_str())
            || ["title", "text", "tags", "read", "starred", "source"].contains(&key.as_str())
        {
            return Err(format!("extension cannot replace capture field: {key}"));
        }
    }
    let (bytes, mime) = match kind {
        "file" | "screenshot" => {
            let mime = required_string(input, "mime")?;
            validate_mime(mime)?;
            if kind == "screenshot" && !mime.starts_with("image/") {
                return Err("screenshot MIME must be an image type".into());
            }
            let values = input
                .get("bytes")
                .and_then(Value::as_array)
                .ok_or("bytes must be an array of integers 0..255")?;
            if values.len() > MAX_BYTES {
                return Err("capture exceeds 16 MiB".into());
            }
            let bytes = values
                .iter()
                .map(|v| {
                    v.as_u64()
                        .filter(|v| *v <= 255)
                        .map(|v| v as u8)
                        .ok_or_else(|| "bytes must contain integers 0..255".to_string())
                })
                .collect::<Result<Vec<u8>>>()?;
            (Some(bytes), Some(mime.to_string()))
        }
        "article" | "text" => {
            let text = text
                .filter(|v| !v.trim().is_empty())
                .ok_or("text required for article or text capture")?;
            if text.len() > MAX_BYTES {
                return Err("capture exceeds 16 MiB".into());
            }
            (
                Some(text.as_bytes().to_vec()),
                Some("text/plain".to_string()),
            )
        }
        _ => {
            if url.is_none() {
                return Err("bookmark requires an HTTP(S) url".into());
            }
            (None, None)
        }
    };
    let identity = if let Some(explicit) = optional_string(input, "idempotency_key")? {
        if explicit.is_empty() {
            return Err("idempotency_key must not be empty".into());
        }
        format!("key:{explicit}")
    } else if let Some(url) = url {
        format!("url:{url}")
    } else {
        format!(
            "content:{kind}:{}:{}",
            mime.as_deref().unwrap_or(""),
            blake3::hash(bytes.as_deref().unwrap_or_default()).to_hex()
        )
    };
    let key = format!("capture-{}", blake3::hash(identity.as_bytes()).to_hex());
    // List errors propagate; a storage failure must never be treated as absence.
    for existing in store.list(Some(COLLECTION))? {
        if existing["uri"]
            .as_str()
            .is_some_and(|uri| uri.ends_with(&format!("/{key}")))
        {
            return Ok(existing);
        }
    }
    body.extend(Map::from_iter([
        ("version".into(), json!(1)),
        ("kind".into(), json!(kind)),
        ("title".into(), json!(title)),
        ("tags".into(), json!(tags)),
        ("read".into(), json!(read)),
        ("starred".into(), json!(starred)),
        ("source".into(), source),
        ("identity".into(), json!(identity)),
        (
            "capturedAt".into(),
            json!(SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs()),
        ),
    ]));
    if let Some(url) = url {
        body.insert("url".into(), json!(url));
    }
    if kind == "text" {
        body.insert("text".into(), json!(text.unwrap_or_default()));
    }
    if let Some(bytes) = bytes {
        let mime = mime.ok_or("missing MIME")?;
        if kind != "text" {
            let cid = store.put_blob(&bytes, &mime)?;
            body.insert("blob".into(), json!(cid));
        }
        body.insert("size".into(), json!(bytes.len()));
        body.insert("mime".into(), json!(mime));
    }
    store.put(COLLECTION, &key, Value::Object(body), "cap")
}

fn update(store: &mut Store, key: &str, item: &Value, patch: Map<String, Value>) -> Result<Value> {
    let mut body = item["body"]
        .as_object()
        .cloned()
        .ok_or("capture body must be an object")?;
    for field in patch.keys() {
        if PROTECTED.contains(&field.as_str()) {
            return Err(format!("capture field is immutable: {field}"));
        }
    }
    let patch_value = Value::Object(patch.clone());
    optional_string(&patch_value, "title")?;
    if patch_value
        .get("title")
        .and_then(Value::as_str)
        .is_some_and(|v| v.len() > 4096)
    {
        return Err("title exceeds 4096 bytes".into());
    }
    optional_bool(&patch_value, "read")?;
    optional_bool(&patch_value, "starred")?;
    if patch.get("source").is_some_and(|v| !v.is_object()) {
        return Err("source must be an object".into());
    }
    let normalized_tags = if patch.contains_key("tags") {
        Some(tags(patch.get("tags"))?)
    } else {
        None
    };
    let text = optional_string(&patch_value, "text")?;
    if let Some(text) = text {
        let kind = body
            .get("kind")
            .and_then(Value::as_str)
            .ok_or("capture kind missing")?;
        if !["text", "article"].contains(&kind) {
            return Err("only article or text captures accept text updates".into());
        }
        if text.trim().is_empty() || text.len() > MAX_BYTES {
            return Err("text must contain 1..16 MiB".into());
        }
        if kind == "article" {
            let cid = store.put_blob(text.as_bytes(), "text/plain")?;
            body.insert("blob".into(), json!(cid));
        }
        body.insert("size".into(), json!(text.len()));
    }
    body.extend(patch);
    if let Some(tags) = normalized_tags {
        body.insert("tags".into(), json!(tags));
    }
    // Article blob is canonical; never retain a competing inline body.
    if body.get("kind").and_then(Value::as_str) == Some("article") {
        body.remove("text");
    }
    store.put(COLLECTION, key, Value::Object(body), "cap")
}

fn reader(store: &mut Store, item: &Value, download: bool) -> Result<Value> {
    let body = &item["body"];
    let original_mime = body["mime"].as_str().unwrap_or("text/plain");
    let bytes = if let Some(cid) = body["blob"].as_str() {
        store.get_blob(cid)?
    } else if let Some(text) = body["text"].as_str() {
        text.as_bytes().to_vec()
    } else {
        body["url"].as_str().unwrap_or_default().as_bytes().to_vec()
    };
    if download {
        return Ok(json!({"uri":item["uri"],"cid":item["cid"],"mime":original_mime,"bytes":bytes}));
    }
    let text = if original_mime.starts_with("text/") {
        String::from_utf8(bytes).map_err(|_| "saved text is not valid UTF-8".to_string())?
    } else {
        format!("Saved attachment ({original_mime}, {} bytes). Use download to retrieve the original bytes.",bytes.len())
    };
    // This is data with an explicit plain-text MIME. Clients must use textContent,
    // a textarea, or terminal output, never insert it as HTML.
    Ok(
        json!({"uri":item["uri"],"cid":item["cid"],"title":body["title"],"url":body["url"],"text":text,"mime":"text/plain","originalMime":original_mime,"blob":body["blob"]}),
    )
}

fn record_key(store: &Store, input: &Value) -> Result<String> {
    let uri = required_string(input, "uri")?;
    let prefix = format!("tangled://{}/{COLLECTION}/", store.did());
    let key = uri
        .strip_prefix(&prefix)
        .ok_or("capture URI must belong to this owner and collection")?;
    if key.is_empty() || key.contains('/') || key.contains('?') || key.contains('#') {
        return Err("invalid capture URI".into());
    }
    Ok(key.to_string())
}
fn required_string<'a>(input: &'a Value, key: &str) -> Result<&'a str> {
    optional_string(input, key)?.ok_or_else(|| format!("{key} must be a string"))
}
fn optional_string<'a>(input: &'a Value, key: &str) -> Result<Option<&'a str>> {
    input
        .get(key)
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| format!("{key} must be a string"))
        })
        .transpose()
}
fn optional_bool(input: &Value, key: &str) -> Result<Option<bool>> {
    input
        .get(key)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| format!("{key} must be a boolean"))
        })
        .transpose()
}
fn tags(input: Option<&Value>) -> Result<Vec<String>> {
    let Some(input) = input else {
        return Ok(Vec::new());
    };
    let values = input.as_array().ok_or("tags must be an array of strings")?;
    if values.len() > 128 {
        return Err("at most 128 tags allowed".into());
    }
    let mut tags = Vec::new();
    for value in values {
        let tag = value.as_str().ok_or("tags must be strings")?.trim();
        if tag.is_empty() || tag.len() > 256 {
            return Err("tags must contain 1..256 bytes".into());
        }
        if !tags.iter().any(|v| v == tag) {
            tags.push(tag.to_string());
        }
    }
    Ok(tags)
}
fn validate_url(url: &str) -> Result<()> {
    let authority = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .and_then(|v| v.split(['/', '?', '#']).next())
        .unwrap_or_default();
    if authority.is_empty()
        || authority.contains('@')
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err("url must be HTTP(S) with a host and no embedded credentials".into());
    }
    Ok(())
}
fn validate_mime(mime: &str) -> Result<()> {
    if !mime.contains('/')
        || mime.len() > 256
        || mime.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err("mime must be a type/subtype without whitespace".into());
    }
    Ok(())
}
