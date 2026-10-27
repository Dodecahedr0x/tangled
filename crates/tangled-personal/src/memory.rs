//! Deterministic personal search and signed, portable organizational metadata.
use crate::{Result, Store};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

const SEARCHES: &str = "pub.tangled.savedSearch";
const GROUPS: &str = "pub.tangled.collection";
const ANNOTATIONS: &str = "pub.tangled.annotation";
const FILTERS: &[&str] = &["query", "source", "collection", "tag", "group"];
const TEXT_FIELDS: &[&str] = &[
    "title",
    "text",
    "description",
    "summary",
    "content",
    "name",
    "caption",
    "quote",
    "transcript",
    "ocrText",
    "notes",
    "alt",
];

/// Execute a memory action. All writes default to capability-gated visibility.
pub fn execute(store: &mut Store, input: Value) -> Result<Value> {
    let args = input.as_object().ok_or("input must be an object")?;
    match required(args, "action")? {
        "search" => search(store, args),
        "run-search" => {
            let saved = store.get(SEARCHES, required(args, "rkey")?)?;
            let body = saved["body"].as_object().ok_or("invalid saved search")?;
            search(store, body)
        }
        "list-searches" => Ok(json!({"records": store.list(Some(SEARCHES))?})),
        "list-collections" => Ok(json!({"records": store.list(Some(GROUPS))?})),
        "delete-search" => store.remove(SEARCHES, required(args, "rkey")?),
        "delete-collection" => store.remove(GROUPS, required(args, "rkey")?),
        "delete-annotation" => store.remove(ANNOTATIONS, required(args, "rkey")?),
        "save-search" => {
            let mut body = edited_body(
                store,
                SEARCHES,
                args,
                &[
                    "name",
                    "query",
                    "source",
                    "collection",
                    "tag",
                    "group",
                    "limit",
                    "offset",
                ],
            )?;
            required(&body, "name")?;
            validate_search(&body)?;
            body.entry("query").or_insert(json!(""));
            store.put(
                SEARCHES,
                required(args, "rkey")?,
                Value::Object(body),
                "cap",
            )
        }
        "put-collection" => {
            let mut body = edited_body(
                store,
                GROUPS,
                args,
                &["name", "description", "items", "tags"],
            )?;
            required(&body, "name")?;
            body.entry("items").or_insert(json!([]));
            for item in body["items"].as_array().ok_or("items must be an array")? {
                validate_reference(item)?;
            }
            validate_tags(&body)?;
            validate_optional_text(&body, "description")?;
            store.put(GROUPS, required(args, "rkey")?, Value::Object(body), "cap")
        }
        "annotate" => {
            let body = edited_body(
                store,
                ANNOTATIONS,
                args,
                &["text", "quote", "target", "tags"],
            )?;
            let text = body
                .get("text")
                .and_then(Value::as_str)
                .ok_or("text must be a string")?;
            if text.chars().count() > 32768 {
                return Err("annotation text exceeds 32768 characters".into());
            }
            validate_reference(body.get("target").ok_or("target is required")?)?;
            validate_tags(&body)?;
            validate_optional_text(&body, "quote")?;
            store.put(
                ANNOTATIONS,
                required(args, "rkey")?,
                Value::Object(body),
                "cap",
            )
        }
        _ => Err("unknown memory action".into()),
    }
}

fn required<'a>(args: &'a Map<String, Value>, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("{key} must be a nonempty string"))
}

fn validate_reference(value: &Value) -> Result<()> {
    let reference = value.as_object().ok_or("reference must be an object")?;
    let uri = required(reference, "uri")?;
    let parts: Vec<_> = uri
        .strip_prefix("tangled://")
        .unwrap_or("")
        .split('/')
        .collect();
    if parts.len() != 3 || !parts[0].starts_with("did:") || parts.iter().any(|s| s.is_empty()) {
        return Err("reference uri must be tangled://<did>/<collection>/<rkey>".into());
    }
    if reference.contains_key("cid") {
        required(reference, "cid")?;
    }
    Ok(())
}

fn validate_optional_text(body: &Map<String, Value>, field: &str) -> Result<()> {
    if body.get(field).is_some_and(|value| !value.is_string()) {
        return Err(format!("{field} must be a string"));
    }
    Ok(())
}

fn validate_tags(body: &Map<String, Value>) -> Result<()> {
    if let Some(tags) = body.get("tags") {
        if !tags.as_array().is_some_and(|tags| {
            tags.iter()
                .all(|tag| tag.as_str().is_some_and(|s| !s.trim().is_empty()))
        }) {
            return Err("tags must be an array of nonempty strings".into());
        }
    }
    Ok(())
}

// Read the existing body before editing, preserving extensions including nested ones.
fn edited_body(
    store: &mut Store,
    collection: &str,
    args: &Map<String, Value>,
    fields: &[&str],
) -> Result<Map<String, Value>> {
    let rkey = required(args, "rkey")?;
    let existing = store
        .list(Some(collection))?
        .into_iter()
        .find(|record| record["rkey"] == rkey);
    let mut body = existing
        .as_ref()
        .and_then(|r| r["body"].as_object())
        .cloned()
        .unwrap_or_default();
    for field in fields {
        if let Some(value) = args.get(*field) {
            if let (Some(old), Some(new)) = (
                body.get_mut(*field).and_then(Value::as_object_mut),
                value.as_object(),
            ) {
                old.extend(new.clone());
            } else {
                body.insert((*field).into(), value.clone());
            }
        }
    }
    Ok(body)
}

fn validate_search(args: &Map<String, Value>) -> Result<(usize, usize)> {
    for field in FILTERS {
        if let Some(value) = args.get(*field) {
            let text = value
                .as_str()
                .ok_or_else(|| format!("{field} must be a string"))?;
            if text.len() > 65536 {
                return Err(format!("{field} exceeds 65536 bytes"));
            }
        }
    }
    let number = |key: &str, default| -> Result<usize> {
        match args.get(key) {
            None => Ok(default),
            Some(value) => value
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| format!("{key} must be a nonnegative integer")),
        }
    };
    let limit = number("limit", 20)?;
    if !(1..=1000).contains(&limit) {
        return Err("limit must be between 1 and 1000".into());
    }
    Ok((limit, number("offset", 0)?))
}

fn tags(body: &Value) -> Vec<String> {
    body["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_lowercase)
        .collect()
}

fn visible_text(record: &Value) -> String {
    let body = &record["body"];
    if record["vis"] == "sealed" || body["encrypted"] == true {
        return String::new();
    }
    // Only strings explicitly described as human-readable content are indexed. Never
    // serialize arbitrary bodies, envelope signatures, credentials or ciphertext.
    let mut parts: Vec<&str> = TEXT_FIELDS
        .iter()
        .filter_map(|field| body[*field].as_str())
        .collect();
    if let Some(values) = body["tags"].as_array() {
        parts.extend(values.iter().filter_map(Value::as_str));
    }
    parts.join("\n")
}

fn source_matches(record: &Value, filter: &str) -> bool {
    if record["author"].as_str() == Some(filter) {
        return true;
    }
    let source = &record["body"]["source"];
    source.as_str() == Some(filter)
        || ["kind", "id", "url", "uri", "provider"]
            .iter()
            .any(|key| source[*key].as_str() == Some(filter))
}

fn search(store: &mut Store, args: &Map<String, Value>) -> Result<Value> {
    let (limit, offset) = validate_search(args)?;
    let filter = |key| {
        args.get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    };
    let query = filter("query").unwrap_or("").to_lowercase();
    let terms: Vec<&str> = query.split_whitespace().collect();
    let records = store.list(None)?;
    let group = if let Some(key) = filter("group") {
        Some(
            store.get(GROUPS, key)?["body"]["items"]
                .as_array()
                .ok_or("invalid collection items")?
                .clone(),
        )
    } else {
        None
    };
    let mut annotations: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
    for annotation in records.iter().filter(|r| r["collection"] == ANNOTATIONS) {
        if let Some(uri) = annotation["body"]["target"]["uri"].as_str() {
            annotations.entry(uri).or_default().push(annotation);
        }
    }
    let mut hits = Vec::new();
    for record in &records {
        // Metadata records remain portable records, but are not source search hits.
        if [SEARCHES, GROUPS]
            .iter()
            .any(|c| record["collection"] == *c)
        {
            continue;
        }
        if filter("collection").is_some_and(|c| record["collection"] != c) {
            continue;
        }
        if filter("source").is_some_and(|s| !source_matches(record, s)) {
            continue;
        }
        if group
            .as_ref()
            .is_some_and(|items| !items.iter().any(|item| item["uri"] == record["uri"]))
        {
            continue;
        }
        if record["vis"] == "sealed" || record["body"]["encrypted"] == true {
            continue;
        }
        let mut searchable = visible_text(record);
        let mut record_tags = tags(&record["body"]);
        for annotation in annotations
            .get(record["uri"].as_str().unwrap_or(""))
            .into_iter()
            .flatten()
        {
            searchable.push('\n');
            searchable.push_str(&visible_text(annotation));
            record_tags.extend(tags(&annotation["body"]));
        }
        if searchable.trim().is_empty() {
            continue;
        }
        if filter("tag").is_some_and(|tag| !record_tags.contains(&tag.to_lowercase())) {
            continue;
        }
        let normalized = searchable.to_lowercase();
        if !terms.iter().all(|term| normalized.contains(term)) {
            continue;
        }
        let title = record["body"]["title"]
            .as_str()
            .or_else(|| record["body"]["name"].as_str())
            .unwrap_or("");
        let title_lower = title.to_lowercase();
        let score: usize = terms
            .iter()
            .map(|term| {
                normalized.matches(term).count() + 3 * usize::from(title_lower.contains(term))
            })
            .sum();
        hits.push(json!({"uri":record["uri"], "cid":record["cid"], "collection":record["collection"], "source":record["body"]["source"], "title":title, "snippet": snippet(&searchable, &terms), "score":score}));
    }
    hits.sort_by(|a, b| {
        b["score"]
            .as_u64()
            .cmp(&a["score"].as_u64())
            .then_with(|| a["uri"].as_str().cmp(&b["uri"].as_str()))
    });
    let total = hits.len();
    let results: Vec<_> = hits.into_iter().skip(offset).take(limit).collect();
    Ok(
        json!({"query":filter("query").unwrap_or(""), "total":total, "offset":offset, "results":results}),
    )
}

// Map the lowercase byte match back to original Unicode characters. Lowercasing
// may expand characters, so offsets from the normalized string cannot slice text.
fn snippet(text: &str, terms: &[&str]) -> String {
    let normalized = text.to_lowercase();
    let byte = terms
        .iter()
        .filter_map(|term| normalized.find(term))
        .min()
        .unwrap_or(0);
    let mut lower_bytes = 0;
    let position = text
        .chars()
        .position(|ch| {
            lower_bytes += ch.to_lowercase().map(char::len_utf8).sum::<usize>();
            lower_bytes > byte
        })
        .unwrap_or(0);
    text.chars()
        .skip(position.saturating_sub(40))
        .take(240)
        .collect()
}
