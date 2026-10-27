use super::*;
use serde_json::json;

fn decode(bytes: &[u8], name: &str) -> Result<String, String> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let encoding = if bytes.starts_with(&[0xff, 0xfe]) {
            encoding_rs::UTF_16LE
        } else {
            encoding_rs::UTF_16BE
        };
        let (text, _, invalid) = encoding.decode(bytes);
        if invalid {
            return Err(format!("Invalid UTF-16 in WhatsApp chat {name}"));
        }
        Ok(text.into_owned())
    } else {
        std::str::from_utf8(bytes)
            .map(|s| s.trim_start_matches('\u{feff}').to_owned())
            .map_err(|e| format!("WhatsApp chat {name} must be UTF-8 or BOM-marked UTF-16: {e}"))
    }
}

fn header(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start_matches(['\u{200e}', '\u{200f}', '\u{feff}']);
    let (stamp, content) = if let Some(rest) = line.strip_prefix('[') {
        rest.split_once("] ")?
    } else {
        line.split_once(" - ")?
    };
    let (date, clock) = stamp.split_once(',').or_else(|| stamp.split_once(' '))?;
    let parts: Vec<_> = date.split(['/', '.', '-']).collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|p| p.is_empty() || p.len() > 4 || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let clock = clock.trim();
    if !clock.contains(':')
        || !clock.chars().all(|c| {
            c.is_ascii_digit()
                || matches!(
                    c,
                    ':' | ' ' | 'A' | 'P' | 'M' | 'a' | 'p' | 'm' | '\u{202f}' | '\u{a0}'
                )
        })
    {
        return None;
    }
    Some((stamp, content))
}

pub(super) fn parse_entries(entries: &Entries, batch: &mut ImportBatch) -> Result<(), String> {
    let mut occurrences = HashMap::new();
    let mut found = false;
    for (name, bytes) in entries
        .iter()
        .filter(|(n, _)| n.to_ascii_lowercase().ends_with(".txt"))
    {
        let text = decode(bytes, name)?;
        let mut pending: Option<(String, String, String)> = None;
        let mut preamble = Vec::new();
        let mut headers = 0;
        for line in text.lines() {
            if let Some((stamp, content)) = header(line) {
                headers += 1;
                if let Some((stamp, content, raw)) = pending.take() {
                    push(batch, entries, name, stamp, content, raw, &mut occurrences);
                }
                pending = Some((stamp.into(), content.into(), line.into()));
            } else if let Some((_, content, raw)) = &mut pending {
                content.push('\n');
                content.push_str(line);
                raw.push('\n');
                raw.push_str(line);
            } else {
                preamble.push(line);
            }
        }
        if let Some((stamp, content, raw)) = pending {
            push(batch, entries, name, stamp, content, raw, &mut occurrences);
        }
        if headers > 0 {
            found = true;
            if !preamble.is_empty() {
                let raw = preamble.join("\n");
                let mut imported =
                    object(format!("preamble:{name}"), "system", json!({"raw": raw}));
                imported.text = Some(raw);
                imported.conversation = Some(name.clone());
                batch.objects.push(imported);
            }
        }
    }
    if !found {
        return Err(
            "No recognizable WhatsApp chat headers found; upload an exported .txt chat or ZIP"
                .into(),
        );
    }
    batch.warnings.push("WhatsApp text exports omit stable account/message IDs and timezone. Sender names and source date strings are preserved; day/month order is not guessed. Exact export reimports are stable. Changed or overlapping exports retain separate possible-duplicate captures because native message IDs are unavailable.".into());
    Ok(())
}

// WhatsApp lacks native IDs. The fingerprint includes the conversation, literal
// timestamp, sender and content, and a repeat-occurrence index preserves identical
// consecutive messages. A content-scoped capture preserves uncertain candidates
// across changed/overlapping files rather than silently collapsing distinct messages.
#[allow(clippy::too_many_arguments)]
fn push(
    batch: &mut ImportBatch,
    entries: &Entries,
    name: &str,
    stamp: String,
    content: String,
    raw: String,
    occurrences: &mut HashMap<String, usize>,
) {
    let (sender, text) = match content.split_once(": ") {
        Some((sender, text)) => (Some(sender.to_owned()), text.to_owned()),
        None => (None, content),
    };
    let data =
        json!({"raw": raw, "timestamp_original": stamp, "sender_name": sender, "text": text});
    // Text exports have no native IDs. Namespace uncertain captures by the
    // chat file's content so partially overlapping exports cannot erase repeats.
    // Exact file reimports stay stable; changed files may retain possible duplicates.
    let capture = entries
        .get(name)
        .map(|bytes| blake3::hash(bytes).to_hex().to_string())
        .unwrap_or_default();
    let key = fingerprint(&json!([name, capture, stamp, sender, text]));
    let count = occurrences.entry(key.clone()).or_default();
    *count += 1;
    let mut imported = object(
        format!("{key}:{}", *count),
        if sender.is_some() {
            "message"
        } else {
            "system"
        },
        data,
    );
    imported.sender = sender;
    imported.timestamp = Some(stamp);
    imported.text = Some(text.clone());
    imported.conversation = Some(name.to_owned());
    let parent = name.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
    for candidate in entries.keys().filter(|c| !c.ends_with(".txt")) {
        let (folder, base) = candidate.rsplit_once('/').unwrap_or(("", candidate));
        if folder == parent && text.contains(base) {
            imported.attachments.push(candidate.clone());
        }
    }
    batch.objects.push(imported);
}
