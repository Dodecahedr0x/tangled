//! Explicit, static sharing of selected signed content. No network requests.
use crate::{Result, Store};
use serde_json::{json, Map, Value};
use std::{collections::BTreeSet, fs, path::Path};

const COLLECTION: &str = "pub.tangled.publication";
const MAX_ITEMS: usize = 256;
const MAX_BYTES: usize = 16 * 1024 * 1024;

/// Render/export an explicit selection, or sign a separate public publication.
pub fn execute(store: &mut Store, input: Value) -> Result<Value> {
    if !input.is_object() {
        return Err("Publishing input must be an object".into());
    }
    let action = input["action"].as_str().ok_or("Missing action")?;
    if !matches!(action, "render" | "export" | "publish") {
        return Err("Expected render, export, or publish action".into());
    }
    let confirmed = input.get("confirmPublish").and_then(Value::as_bool) == Some(true);
    if action == "publish" && !confirmed {
        return Err("Public publication requires confirmPublish: true".into());
    }
    let output_dir = optional_text(&input, "outputDir", 4096)?;
    if output_dir.is_some() && action != "export" {
        return Err("outputDir is supported only by export".into());
    }
    let title = optional_text(&input, "title", 4096)?.unwrap_or("Shared publication");
    let description = optional_text(&input, "description", MAX_BYTES)?.unwrap_or("");
    let site_url = optional_text(&input, "siteUrl", 8192)?
        .map(safe_link)
        .transpose()?;
    let key = if action == "publish" {
        let key = input["key"]
            .as_str()
            .ok_or("Publishing requires a stable key")?;
        if !tangled::identity::valid_rkey(key) {
            return Err("Invalid publication key".into());
        }
        Some(key)
    } else {
        None
    };
    let selectors = selections(store, &input)?;
    let mut items = Vec::new();
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    let mut total = title.len() + description.len();
    for selector in selectors {
        let (collection, rkey) = locate(store, &selector)?;
        let record = store.get(&collection, &rkey)?;
        if !seen.insert(record["uri"].as_str().unwrap_or("").to_owned()) {
            return Err("Duplicate selected record".into());
        }
        if record["vis"] != "public" && !confirmed {
            return Err(
                "Private source requires confirmPublish: true, including static exports".into(),
            );
        }
        let body = &record["body"];
        let item_title = optional_text(body, "title", 4096)?.unwrap_or("Untitled");
        let mut text = optional_text(body, "text", MAX_BYTES)?
            .unwrap_or("")
            .to_owned();
        let item_description = optional_text(body, "description", MAX_BYTES)?.unwrap_or("");
        if let Some(cid) = optional_text(&selector, "textBlob", 256)? {
            let bytes = store.get_blob(cid)?;
            add_size(&mut total, bytes.len())?;
            text = String::from_utf8(bytes).map_err(|_| "Selected textBlob must be UTF-8")?;
        }
        add_size(
            &mut total,
            item_title.len() + text.len() + item_description.len(),
        )?;
        let mut item = json!({"title":item_title,"text":text,"description":item_description});
        if let Some(link) = optional_text(&selector, "link", 8192)? {
            item["link"] = json!(safe_link(link)?);
        }
        let mut attachments = Vec::new();
        if let Some(selected) = selector.get("attachments") {
            let selected = selected.as_array().ok_or("attachments must be an array")?;
            if selected.len() > 32 {
                return Err("At most 32 attachments may be selected per item".into());
            }
            for attachment in selected {
                let cid =
                    optional_text(attachment, "cid", 256)?.ok_or("Attachment requires cid")?;
                let name =
                    optional_text(attachment, "name", 4096)?.unwrap_or("Download attachment");
                let bytes = store.get_blob(cid)?;
                add_size(&mut total, bytes.len() + name.len())?;
                // The name and MIME from the source are never trusted as executable paths/types.
                let path = format!("attachments/{}.bin", blake3::hash(&bytes).to_hex());
                attachments.push(json!({"name":name,"blob":cid,"path":path,"mime":"application/octet-stream","size":bytes.len()}));
                if !files.iter().any(|f: &Value| f["path"] == path) {
                    files.push(
                        json!({"path":path,"mediaType":"application/octet-stream","bytes":bytes}),
                    );
                }
            }
        }
        if !attachments.is_empty() {
            item["attachments"] = json!(attachments);
        }
        items.push(item);
    }
    let (html, rss) = render(title, description, site_url.as_deref(), &items);
    files.insert(
        0,
        json!({"path":"feed.xml","mediaType":"application/rss+xml","text":rss}),
    );
    files.insert(
        0,
        json!({"path":"index.html","mediaType":"text/html","text":html}),
    );
    let mut output = json!({"html":html,"rss":rss,"count":items.len(),"files":files});
    if let Some(key) = key {
        let existing = store
            .list(Some(COLLECTION))?
            .into_iter()
            .find(|r| r["rkey"] == key);
        let mut body = if let Some(existing) = existing {
            if existing["vis"] != "public" {
                return Err("Choose a new key instead of overwriting a private publication".into());
            }
            existing["body"]
                .as_object()
                .cloned()
                .ok_or("Invalid publication body")?
        } else {
            Map::new()
        };
        // Replace owned fields; preserve already-public extension metadata on causal updates.
        body.insert("version".into(), json!(1));
        body.insert("title".into(), json!(title));
        body.insert("description".into(), json!(description));
        body.insert("items".into(), json!(items));
        body.insert(
            "html".into(),
            json!(store.put_blob(html.as_bytes(), "text/html")?),
        );
        body.insert(
            "rss".into(),
            json!(store.put_blob(rss.as_bytes(), "application/rss+xml")?),
        );
        body.remove("siteUrl");
        if let Some(url) = site_url {
            body.insert("siteUrl".into(), json!(url));
        }
        output["publication"] = store.put(COLLECTION, key, Value::Object(body), "public")?;
    }
    if let Some(directory) = output_dir {
        write_files(Path::new(directory), &files)?;
        output["outputDir"] = json!(directory);
    }
    Ok(output)
}

fn optional_text<'a>(value: &'a Value, field: &str, limit: usize) -> Result<Option<&'a str>> {
    match value.get(field) {
        None => Ok(None),
        Some(value) => {
            let text = value
                .as_str()
                .ok_or_else(|| format!("{field} must be a string"))?;
            if text.len() > limit {
                return Err(format!("{field} exceeds {limit} bytes"));
            }
            Ok(Some(text))
        }
    }
}

fn add_size(total: &mut usize, amount: usize) -> Result<()> {
    *total = total.checked_add(amount).ok_or("Selection is too large")?;
    if *total > MAX_BYTES {
        return Err("Selected content exceeds 16 MiB".into());
    }
    Ok(())
}

fn selections(store: &mut Store, input: &Value) -> Result<Vec<Value>> {
    let records = input.get("records");
    let source = input.get("source");
    let single = input.get("uri").is_some() || input.get("rkey").is_some();
    let collection = input.get("collection");
    if records.is_some() {
        if source.is_some() || single || collection.is_some() {
            return Err("Choose records, source, or a collection selection".into());
        }
        let records = records
            .and_then(Value::as_array)
            .ok_or("records must be an array")?;
        if records.is_empty() || records.len() > MAX_ITEMS {
            return Err("Select 1..256 records explicitly".into());
        }
        return Ok(records.clone());
    }
    if let Some(source) = source {
        if single || collection.is_some() {
            return Err("Ambiguous source selection".into());
        }
        return Ok(vec![source.clone()]);
    }
    if single {
        return Ok(vec![input.clone()]);
    }
    if let Some(collection) = collection {
        let collection = collection
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Invalid collection")?;
        let records = store.list(Some(collection))?;
        if records.is_empty() || records.len() > MAX_ITEMS {
            return Err("Selected collection must contain 1..256 records".into());
        }
        return Ok(records
            .into_iter()
            .map(|r| json!({"uri":r["uri"]}))
            .collect());
    }
    Err("Select records, source, uri, collection/rkey, or an entire collection".into())
}

fn locate(store: &Store, selector: &Value) -> Result<(String, String)> {
    if !selector.is_object() {
        return Err("Each selection must be an object".into());
    }
    if let Some(uri) = selector.get("uri") {
        if selector.get("collection").is_some() || selector.get("rkey").is_some() {
            return Err("Choose either URI or collection/rkey".into());
        }
        let prefix = format!("tangled://{}/", store.did());
        let local = uri
            .as_str()
            .and_then(|uri| uri.strip_prefix(&prefix))
            .ok_or("URI must identify this local owner")?;
        let (collection, key) = local.split_once('/').ok_or("Invalid selected URI")?;
        if collection.is_empty() || key.is_empty() || key.contains('/') {
            return Err("Invalid selected URI".into());
        }
        return Ok((collection.into(), key.into()));
    }
    let collection = selector["collection"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Selection requires collection")?;
    let key = selector["rkey"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Selection requires rkey")?;
    Ok((collection.into(), key.into()))
}

fn safe_link(link: &str) -> Result<String> {
    if link.chars().any(char::is_control) || link.trim() != link {
        return Err("Link contains whitespace or control characters".into());
    }
    let url = url::Url::parse(link).map_err(|_| "Invalid HTTP(S) link")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Links must be HTTP(S) without credentials".into());
    }
    Ok(url.to_string())
}

/// Escape XML 1.0/HTML text and attributes, replacing illegal XML code points.
fn escape(value: &str) -> String {
    let mut output = String::new();
    for c in value.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            '\t' | '\n' | '\r' => output.push(c),
            c if c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}' => output.push(c),
            _ => output.push('\u{fffd}'),
        }
    }
    output
}

fn render(
    title: &str,
    description: &str,
    site_url: Option<&str>,
    items: &[Value],
) -> (String, String) {
    let mut html = format!("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; base-uri 'none'; form-action 'none'\"><title>{}</title></head><body><h1>{}</h1><p>{}</p>", escape(title), escape(title), escape(description));
    let mut rss = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><rss version=\"2.0\"><channel><title>{}</title><link>{}</link><description>{}</description>", escape(title), escape(site_url.unwrap_or("")), escape(&escape(description)));
    for item in items {
        let title = escape(item["title"].as_str().unwrap_or(""));
        let text = item["text"].as_str().unwrap_or("");
        let description = item["description"].as_str().unwrap_or("");
        html.push_str(&format!(
            "<article><h2>{title}</h2><p>{}</p><pre>{}</pre>",
            escape(description),
            escape(text)
        ));
        rss.push_str(&format!(
            "<item><title>{title}</title><description>{}</description>",
            escape(&escape(&format!("{description}\n{text}")))
        ));
        // Digest only selected public content, never a private URI or exact source envelope.
        let guid = blake3::hash(item.to_string().as_bytes()).to_hex();
        rss.push_str(&format!("<guid isPermaLink=\"false\">{guid}</guid>"));
        if let Some(link) = item["link"].as_str() {
            html.push_str(&format!(
                "<p><a rel=\"noreferrer noopener\" href=\"{}\">Open link</a></p>",
                escape(link)
            ));
            rss.push_str(&format!("<link>{}</link>", escape(link)));
        }
        if let Some(attachments) = item["attachments"].as_array() {
            for attachment in attachments {
                html.push_str(&format!(
                    "<p><a download href=\"{}\">{}</a></p>",
                    escape(attachment["path"].as_str().unwrap_or("")),
                    escape(attachment["name"].as_str().unwrap_or(""))
                ));
            }
        }
        html.push_str("</article>");
        rss.push_str("</item>");
    }
    html.push_str("</body></html>");
    rss.push_str("</channel></rss>");
    (html, rss)
}

fn write_files(directory: &Path, files: &[Value]) -> Result<()> {
    // A new directory avoids silently overwriting somebody's files or following existing symlinks.
    fs::create_dir(directory).map_err(|e| format!("Create new export directory: {e}"))?;
    let result = (|| {
        for file in files {
            let relative = file["path"].as_str().ok_or("Invalid generated path")?;
            let path = directory.join(relative);
            if relative.starts_with("attachments/") && !directory.join("attachments").exists() {
                fs::create_dir(directory.join("attachments")).map_err(|e| e.to_string())?;
            }
            let bytes = if let Some(text) = file["text"].as_str() {
                text.as_bytes().to_vec()
            } else {
                serde_json::from_value::<Vec<u8>>(file["bytes"].clone())
                    .map_err(|e| e.to_string())?
            };
            use std::io::Write;
            let mut target = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|e| e.to_string())?;
            target.write_all(&bytes).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if result.is_err() {
        // Only files created for this explicit export live in this newly created directory.
        let _ = fs::remove_dir_all(directory);
    }
    result
}
