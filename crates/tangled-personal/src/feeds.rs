//! Private RSS/Atom subscriptions, inert imports, and portable reading state.
use crate::{Result, Store};
use quick_xml::{events::Event, Reader, XmlVersion};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const SUBS: &str = "pub.tangled.feedSubscription";
const ITEMS: &str = "pub.tangled.feedItem";
const STATES: &str = "pub.tangled.readingState";
const MAX_XML: usize = 2 * 1024 * 1024;

pub fn execute(store: &mut Store, input: Value) -> Result<Value> {
    match text(&input, "action")? {
        "subscribe" => {
            let url = source_url(text(&input, "url")?)?;
            let key = key(&json!([url]));
            let old = store
                .list(Some(SUBS))?
                .into_iter()
                .find(|r| r["rkey"] == key);
            let mut body = old
                .as_ref()
                .map(|r| r["body"].clone())
                .unwrap_or_else(|| json!({"v":1,"url":url,"title":url,"tags":[]}));
            if input.get("title").is_some() {
                body["title"] = json!(bounded_text(&input, "title", 4096)?);
            }
            if input.get("tags").is_some() {
                body["tags"] = json!(tags(&input)?);
            }
            if let Some(row) = old {
                if row["body"] == body {
                    return Ok(row);
                }
            }
            store.put(SUBS, &key, body, "cap")
        }
        "list" => Ok(json!({"subscriptions":store.list(Some(SUBS))?})),
        "unsubscribe" => {
            let url = source_url(text(&input, "url")?)?;
            store.remove(SUBS, &key(&json!([url])))
        }
        "ingest" => ingest(store, &input),
        "mark" => mark(store, &input),
        "queue" | "digest" => queue(store, &input),
        "opml" => {
            let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><opml version=\"2.0\"><head><title>Reading subscriptions</title></head><body>");
            for sub in store.list(Some(SUBS))? {
                let url = text(&sub["body"], "url")?;
                let title = sub["body"]["title"].as_str().unwrap_or(url);
                xml.push_str(&format!(
                    "<outline type=\"rss\" text=\"{}\" title=\"{}\" xmlUrl=\"{}\"/>",
                    escape(title),
                    escape(title),
                    escape(url)
                ));
            }
            xml.push_str("</body></opml>");
            Ok(json!({"opml":xml}))
        }
        action => Err(format!("Unknown feeds action: {action}")),
    }
}

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .ok_or_else(|| format!("{field} must be a string"))
}
fn bounded_text<'a>(value: &'a Value, field: &str, max: usize) -> Result<&'a str> {
    let value = text(value, field)?;
    if value.len() > max {
        return Err(format!("{field} is too long"));
    }
    Ok(value)
}
fn key(value: &Value) -> String {
    blake3::hash(value.to_string().as_bytes())
        .to_hex()
        .to_string()
}
fn source_url(raw: &str) -> Result<String> {
    if raw.len() > 8192 || raw.chars().any(char::is_whitespace) {
        return Err("Invalid source URL".into());
    }
    let url = url::Url::parse(raw).map_err(|e| format!("Invalid source URL: {e}"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("Source URL must be HTTP(S), without credentials or fragment".into());
    }
    let canonical: String = url.into();
    if canonical.len() > 8192 {
        return Err("Canonical source URL is too long".into());
    }
    Ok(canonical)
}
fn tags(input: &Value) -> Result<Vec<String>> {
    let values = input["tags"].as_array().ok_or("tags must be an array")?;
    if values.len() > 100 {
        return Err("At most 100 tags are allowed".into());
    }
    let mut out = BTreeSet::new();
    for value in values {
        let tag = value.as_str().ok_or("tags must contain strings")?.trim();
        if tag.is_empty() || tag.len() > 128 {
            return Err("Tags must contain 1 to 128 bytes".into());
        }
        out.insert(tag.to_string());
    }
    Ok(out.into_iter().collect())
}
fn flag(input: &Value, name: &str) -> Result<Option<bool>> {
    input
        .get(name)
        .map(|v| {
            v.as_bool()
                .ok_or_else(|| format!("{name} must be a boolean"))
        })
        .transpose()
}
fn escape(value: &str) -> String {
    value.chars().map(|c| if matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}') { c } else { '\u{fffd}' }).collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn ingest(store: &mut Store, input: &Value) -> Result<Value> {
    let url = source_url(text(input, "url")?)?;
    let sub = store.get(SUBS, &key(&json!([url])))?;
    let xml = bounded_text(input, "xml", MAX_XML)?;
    // Complete parsing and validation precedes any repository or blob mutation.
    let (format, entries) = parse_feed(xml)?;
    let current: BTreeSet<String> = store
        .list(Some(ITEMS))?
        .iter()
        .filter_map(|r| r["rkey"].as_str().map(str::to_string))
        .collect();
    let mut seen = current;
    let mut pending = Vec::new();
    let mut skipped = 0;
    for mut body in entries {
        let source_id = body["sourceId"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| body["link"].as_str().filter(|s| !s.is_empty()))
            .map(str::to_string)
            .unwrap_or_else(|| key(&body));
        let rkey = key(&json!([url, source_id]));
        if !seen.insert(rkey.clone()) {
            skipped += 1;
            continue;
        }
        body["v"] = json!(1);
        body["sourceId"] = json!(source_id);
        body["subscription"] = sub["uri"].clone();
        body["sourceUrl"] = json!(url);
        pending.push((rkey, body));
    }
    if pending.is_empty() {
        return Ok(json!({"imported":0,"skipped":skipped,"items":[]}));
    }
    let blob = store.put_blob(xml.as_bytes(), "application/xml")?;
    let mut rows = Vec::new();
    for (rkey, mut body) in pending {
        body["provenance"] =
            json!({"kind":"feed-import","format":format,"sourceUrl":url,"blob":blob});
        rows.push(store.put(ITEMS, &rkey, body, "cap")?);
    }
    Ok(json!({"imported":rows.len(),"skipped":skipped,"items":rows}))
}

fn mark(store: &mut Store, input: &Value) -> Result<Value> {
    let item_uri = bounded_text(input, "item", 2048)?;
    let item = store
        .list(Some(ITEMS))?
        .into_iter()
        .find(|r| r["uri"] == item_uri)
        .ok_or("Unknown feed item URI")?;
    let read = flag(input, "read")?;
    let starred = flag(input, "starred")?;
    let new_tags = input.get("tags").map(|_| tags(input)).transpose()?;
    if read.is_none() && starred.is_none() && new_tags.is_none() {
        return Err("mark requires read, starred, or tags".into());
    }
    let rkey = text(&item, "rkey")?;
    let old = store
        .list(Some(STATES))?
        .into_iter()
        .find(|r| r["rkey"] == rkey);
    let mut body = old
        .as_ref()
        .map(|r| r["body"].clone())
        .unwrap_or_else(|| json!({"v":1,"item":item_uri,"read":false,"starred":false,"tags":[]}));
    if let Some(read) = read {
        body["read"] = json!(read);
    }
    if let Some(starred) = starred {
        body["starred"] = json!(starred);
    }
    if let Some(tags) = new_tags {
        body["tags"] = json!(tags);
    }
    if let Some(row) = old {
        if row["body"] == body {
            return Ok(row);
        }
    }
    store.put(STATES, rkey, body, "cap")
}

fn queue(store: &mut Store, input: &Value) -> Result<Value> {
    let unread = flag(input, "unread")?.unwrap_or(true);
    let starred = flag(input, "starred")?;
    let filter_tags = input
        .get("tags")
        .map(|_| tags(input))
        .transpose()?
        .unwrap_or_default();
    let source = input
        .get("url")
        .map(|_| source_url(text(input, "url")?))
        .transpose()?;
    let limit = match input.get("limit") {
        None => 100,
        Some(v) => v
            .as_u64()
            .filter(|n| *n > 0 && *n <= 1000)
            .ok_or("limit must be 1 to 1000")? as usize,
    };
    let states: BTreeMap<String, Value> = store
        .list(Some(STATES))?
        .into_iter()
        .filter_map(|r| Some((r["body"]["item"].as_str()?.to_string(), r["body"].clone())))
        .collect();
    let subscriptions: BTreeMap<String, Value> = store
        .list(Some(SUBS))?
        .into_iter()
        .filter_map(|r| Some((r["uri"].as_str()?.to_string(), r["body"].clone())))
        .collect();
    let mut rows = Vec::new();
    for mut item in store.list(Some(ITEMS))? {
        let uri = text(&item, "uri")?;
        let state = states
            .get(uri)
            .cloned()
            .unwrap_or_else(|| json!({"v":1,"item":uri,"read":false,"starred":false,"tags":[]}));
        if unread && state["read"] == true {
            continue;
        }
        if starred.is_some_and(|s| state["starred"].as_bool().unwrap_or(false) != s) {
            continue;
        }
        if source
            .as_ref()
            .is_some_and(|s| item["body"]["sourceUrl"] != *s)
        {
            continue;
        }
        let mut effective_tags = BTreeSet::new();
        for body in [
            Some(&state),
            subscriptions.get(item["body"]["subscription"].as_str().unwrap_or("")),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(tags) = body["tags"].as_array() {
                effective_tags.extend(tags.iter().filter_map(Value::as_str).map(str::to_string));
            }
        }
        if !filter_tags.iter().all(|tag| effective_tags.contains(tag)) {
            continue;
        }
        item["reading"] = state;
        item["effectiveTags"] = json!(effective_tags);
        rows.push(item);
    }
    rows.sort_by(|a, b| {
        b["ts"]
            .as_u64()
            .cmp(&a["ts"].as_u64())
            .then_with(|| a["uri"].as_str().cmp(&b["uri"].as_str()))
    });
    rows.truncate(limit);
    if input["action"] == "digest" {
        let text = rows
            .iter()
            .map(|r| {
                format!(
                    "{}\n{}",
                    r["body"]["title"].as_str().unwrap_or("Untitled"),
                    r["body"]["link"].as_str().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        Ok(json!({"count":rows.len(),"text":text,"items":rows}))
    } else {
        Ok(json!({"items":rows}))
    }
}

#[derive(Default)]
struct Element {
    name: String,
    attrs: BTreeMap<String, String>,
    text: String,
    children: Vec<Element>,
}
impl Element {
    fn child(&self, name: &str) -> Option<&Self> {
        self.children.iter().find(|e| e.name == name)
    }
    fn value(&self, name: &str) -> String {
        self.child(name)
            .map(|e| e.text.trim().to_string())
            .unwrap_or_default()
    }
}

fn parse_feed(xml: &str) -> Result<(&'static str, Vec<Value>)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().expand_empty_elements = true;
    reader.config_mut().check_comments = true;
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    let mut nodes = 0;
    let mut retained_text = 0usize;
    loop {
        match reader
            .read_event()
            .map_err(|e| format!("Invalid feed XML: {e}"))?
        {
            Event::Start(e) => {
                nodes += 1;
                if nodes > 20_000 || stack.len() >= 64 {
                    return Err("XML structure exceeds limits".into());
                }
                if stack.is_empty() && root.is_some() {
                    return Err("XML must have one root".into());
                }
                let name = std::str::from_utf8(e.local_name().as_ref())
                    .map_err(|e| e.to_string())?
                    .to_string();
                let mut attrs = BTreeMap::new();
                for attr in e.attributes() {
                    let attr = attr.map_err(|e| format!("Invalid XML attribute: {e}"))?;
                    let name = std::str::from_utf8(attr.key.as_ref())
                        .map_err(|e| e.to_string())?
                        .to_string();
                    attrs.insert(
                        name,
                        attr.normalized_value(XmlVersion::Implicit1_0)
                            .map_err(|e| e.to_string())?
                            .into_owned(),
                    );
                }
                stack.push(Element {
                    name,
                    attrs,
                    ..Element::default()
                });
            }
            Event::End(_) => {
                let element = stack.pop().ok_or("Unmatched XML end")?;
                if let Some(parent) = stack.last_mut() {
                    account_text(&mut retained_text, element.text.len())?;
                    parent.text.push_str(&element.text);
                    parent.children.push(element);
                } else {
                    root = Some(element);
                }
            }
            Event::Text(e) => {
                let value = e.decode().map_err(|e| e.to_string())?;
                if let Some(parent) = stack.last_mut() {
                    account_text(&mut retained_text, value.len())?;
                    parent.text.push_str(&value);
                } else if !value.trim().is_empty() {
                    return Err("Text outside XML root".into());
                }
            }
            Event::CData(e) => {
                let value = e.decode().map_err(|e| e.to_string())?;
                account_text(&mut retained_text, value.len())?;
                stack
                    .last_mut()
                    .ok_or("CDATA outside root")?
                    .text
                    .push_str(&value);
            }
            Event::GeneralRef(e) => {
                let value = if let Some(c) = e.resolve_char_ref().map_err(|e| e.to_string())? {
                    c.to_string()
                } else {
                    match e.decode().map_err(|e| e.to_string())?.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => return Err("Custom XML entities are unsupported".into()),
                    }
                    .to_string()
                };
                account_text(&mut retained_text, value.len())?;
                stack
                    .last_mut()
                    .ok_or("Entity outside root")?
                    .text
                    .push_str(&value);
            }
            Event::DocType(_) => return Err("DTD declarations are unsupported".into()),
            Event::Eof => break,
            Event::Decl(_) | Event::PI(_) | Event::Comment(_) => {}
            Event::Empty(_) => return Err("Unexpected XML empty element".into()),
        }
    }
    if !stack.is_empty() {
        return Err("Truncated feed XML".into());
    }
    let root = root.ok_or("Empty feed XML")?;
    let (format, entries): (&str, Vec<&Element>) = match root.name.as_str() {
        "rss" => (
            "rss",
            root.child("channel")
                .ok_or("RSS channel missing")?
                .children
                .iter()
                .filter(|e| e.name == "item")
                .collect(),
        ),
        "feed" => (
            "atom",
            root.children.iter().filter(|e| e.name == "entry").collect(),
        ),
        _ => return Err("Expected RSS 2.0 or Atom feed".into()),
    };
    if entries.len() > 1000 {
        return Err("Feed exceeds 1000 items".into());
    }
    let mut out = Vec::new();
    for entry in entries {
        let atom = format == "atom";
        let source_id = entry.value(if atom { "id" } else { "guid" });
        let title = entry.value("title");
        let link = if atom {
            entry
                .children
                .iter()
                .find(|e| e.name == "link" && e.attrs.get("rel").is_none_or(|v| v == "alternate"))
                .and_then(|e| e.attrs.get("href"))
                .cloned()
                .unwrap_or_default()
        } else {
            entry.value("link")
        };
        let content = if atom {
            entry.child("content").or_else(|| entry.child("summary"))
        } else {
            entry
                .child("encoded")
                .or_else(|| entry.child("description"))
        };
        let body = content.map(|e| e.text.trim()).unwrap_or("");
        let body_type =
            if atom && content.is_some_and(|e| e.attrs.get("type").is_none_or(|v| v != "html")) {
                "text"
            } else {
                "html"
            };
        let published = entry.value(if atom { "published" } else { "pubDate" });
        let updated = entry.value("updated");
        if title.len() > 4096
            || source_id.len() > 8192
            || link.len() > 8192
            || body.len() > 256 * 1024
            || published.len() > 1024
            || updated.len() > 1024
        {
            return Err("Feed item field exceeds limits".into());
        }
        out.push(json!({"sourceId":source_id,"title":title,"link":link,"body":body,"bodyType":body_type,"publishedAt":published,"updatedAt":updated}));
    }
    Ok((format, out))
}

// Count retained descendant copies as well as decoded text: nesting cannot amplify
// a small input into an unbounded in-memory tree. Custom entities are rejected.
fn account_text(total: &mut usize, added: usize) -> Result<()> {
    *total = total.saturating_add(added);
    if *total > 8 * 1024 * 1024 {
        return Err("Retained XML text exceeds 8 MiB".into());
    }
    Ok(())
}
