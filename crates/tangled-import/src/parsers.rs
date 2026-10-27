//! In-memory archive readers. JavaScript is parsed as data and never evaluated.
use crate::model::{ImportBatch, ImportedFile, ImportedObject};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read};

mod telegram;
mod twitter;
mod whatsapp;

type Entries = BTreeMap<String, Vec<u8>>;
const MAX_INPUT: usize = 256 * 1024 * 1024;
const MAX_FILE: u64 = 32 * 1024 * 1024;
const MAX_EXPANDED: u64 = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;

/// Parse native source data without extracting files or executing scripts.
pub fn parse(
    source: &str,
    account: &str,
    filename: &str,
    bytes: &[u8],
) -> Result<ImportBatch, String> {
    if !matches!(source, "telegram" | "twitter" | "whatsapp") {
        return Err(format!("Unsupported import source: {source}"));
    }
    if bytes.len() > MAX_INPUT {
        return Err("Archive exceeds the 256 MiB input limit".into());
    }
    if account.trim().is_empty() {
        return Err("A source account identifier is required".into());
    }
    let zipped = filename.to_ascii_lowercase().ends_with(".zip") || bytes.starts_with(b"PK");
    let entries = if zipped {
        read_zip(bytes)?
    } else {
        if bytes.len() as u64 > MAX_FILE {
            return Err("Export exceeds the 32 MiB individual file limit".into());
        }
        BTreeMap::from([(filename.to_owned(), bytes.to_vec())])
    };
    let mut batch = ImportBatch {
        source: source.into(),
        account: account.into(),
        objects: Vec::new(),
        files: Vec::new(),
        warnings: Vec::new(),
    };
    // Keep unknown entries, too. The store retains the original upload separately.
    if zipped {
        batch.files = entries
            .iter()
            .map(|(name, bytes)| ImportedFile {
                name: name.clone(),
                media_type: media_type(name).into(),
                data_base64: STANDARD.encode(bytes),
            })
            .collect();
    }
    match source {
        "telegram" => telegram::parse_entries(&entries, &mut batch)?,
        "twitter" => twitter::parse_entries(&entries, &mut batch)?,
        "whatsapp" => whatsapp::parse_entries(&entries, &mut batch)?,
        _ => unreachable!(),
    }
    Ok(batch)
}

fn safe_path(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains(['\\', ':', '\0'])
        && name.split('/').all(|p| !matches!(p, "." | ".."))
}

fn read_zip(bytes: &[u8]) -> Result<Entries, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("Invalid ZIP archive: {e}"))?;
    validate_directory(bytes, archive.central_directory_start(), archive.len())?;
    if archive.len() > MAX_ENTRIES {
        return Err("ZIP exceeds the 4096 entry limit".into());
    }
    let mut entries = BTreeMap::new();
    let mut expanded = 0u64;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Cannot read ZIP entry: {e}"))?;
        let name = file.name().to_owned();
        if !safe_path(&name) {
            return Err(format!("Unsafe ZIP path: {name:?}"));
        }
        if file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err(format!("ZIP symlink is not allowed: {name}"));
        }
        if file.is_dir() {
            continue;
        }
        if file.size() > MAX_FILE {
            return Err(format!("ZIP entry exceeds the 32 MiB limit: {name}"));
        }
        expanded = expanded
            .checked_add(file.size())
            .ok_or("ZIP expanded size overflow")?;
        if expanded > MAX_EXPANDED {
            return Err("ZIP exceeds the 128 MiB expanded size limit".into());
        }
        if file.size() > 1024 * 1024 && file.size() / file.compressed_size().max(1) > 200 {
            return Err(format!("Unsafe ZIP compression ratio: {name}"));
        }
        if entries.contains_key(&name) {
            return Err(format!("Duplicate ZIP path: {name}"));
        }
        let mut data = Vec::new();
        (&mut file)
            .take(MAX_FILE + 1)
            .read_to_end(&mut data)
            .map_err(|e| format!("Cannot decode ZIP entry {name}: {e}"))?;
        if data.len() as u64 > MAX_FILE || data.len() as u64 != file.size() {
            return Err(format!("ZIP entry size mismatch or limit exceeded: {name}"));
        }
        entries.insert(name, data);
    }
    if entries.is_empty() {
        return Err("ZIP archive contains no files".into());
    }
    Ok(entries)
}

// zip's name-indexed reader collapses duplicate central-directory paths.
// Count the original directory records as well, so duplicates cannot hide an
// unsafe or distinct entry before it reaches our validation/preservation loop.
fn validate_directory(bytes: &[u8], start: u64, unique: usize) -> Result<(), String> {
    let mut at = usize::try_from(start).map_err(|_| "Invalid ZIP directory offset")?;
    let mut count = 0;
    while bytes.get(at..at.saturating_add(4)) == Some(b"PK\x01\x02") {
        let header = bytes
            .get(at..at.saturating_add(46))
            .ok_or("Truncated ZIP directory")?;
        let field = |i| u16::from_le_bytes([header[i], header[i + 1]]) as usize;
        let length = 46 + field(28) + field(30) + field(32);
        at = at.checked_add(length).ok_or("ZIP directory overflow")?;
        if at > bytes.len() {
            return Err("Truncated ZIP directory entry".into());
        }
        count += 1;
        if count > MAX_ENTRIES {
            return Err("ZIP exceeds the 4096 entry limit".into());
        }
    }
    if count != unique {
        return Err("Duplicate ZIP paths or inconsistent central directory".into());
    }
    Ok(())
}

fn media_type(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "json" => "application/json",
        "js" => "text/javascript",
        "txt" => "text/plain",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "ogg" | "opus" => "audio/ogg",
        "m4a" => "audio/mp4",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn first(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| string(&value[*key]))
}

fn object(id: String, kind: &str, data: Value) -> ImportedObject {
    ImportedObject {
        id,
        kind: kind.into(),
        conversation: None,
        sender: None,
        timestamp: None,
        text: None,
        attachments: Vec::new(),
        data,
    }
}

fn fingerprint(value: &Value) -> String {
    blake3::hash(&serde_json::to_vec(value).expect("JSON values serialize"))
        .to_hex()
        .to_string()
}

fn fallback_id(scope: &str, value: &Value, occurrences: &mut HashMap<String, usize>) -> String {
    let key = format!("{scope}:{}", fingerprint(value));
    let count = occurrences.entry(key.clone()).or_default();
    *count += 1;
    format!("{key}:{}", *count)
}

fn source_account(batch: &mut ImportBatch, native: Option<String>) {
    if let Some(native) = native {
        if batch.account != native {
            batch.warnings.push(format!("Source account mismatch: requested {:?}, archive identifies {:?}; using archive account.", batch.account, native));
        }
        batch.account = native;
    }
}
