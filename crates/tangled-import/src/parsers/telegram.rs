use super::*;

pub(super) fn parse_entries(entries: &Entries, batch: &mut ImportBatch) -> Result<(), String> {
    let mut found = false;
    let mut occurrences = HashMap::new();
    for (name, bytes) in entries.iter().filter(|(n, _)| {
        let basename = n.rsplit('/').next().unwrap_or(n);
        (entries.len() == 1 && n.to_ascii_lowercase().ends_with(".json"))
            || basename == "result.json"
    }) {
        let data: Value = serde_json::from_slice(bytes)
            .map_err(|e| format!("Invalid Telegram JSON in {name}: {e}"))?;
        let chats: Vec<&Value> = if let Some(chats) = data["chats"]["list"].as_array() {
            chats.iter().collect()
        } else if data["messages"].is_array() {
            vec![&data]
        } else if data.get("personal_information").is_some() || data.get("contacts").is_some() {
            Vec::new()
        } else {
            continue;
        };
        found = true;
        source_account(batch, first(&data["personal_information"], &["user_id"]));
        batch.objects.push(object(
            format!("metadata:{name}"),
            "archive_metadata",
            data.clone(),
        ));
        for chat in chats {
            let conversation = conversation_id(chat, batch);
            let Some(messages) = chat["messages"].as_array() else {
                continue;
            };
            for message in messages {
                let id = match first(message, &["id"]) {
                    Some(id) => format!("{conversation}:{id}"),
                    None => fallback_id(&conversation, message, &mut occurrences),
                };
                let mut imported = object(
                    id,
                    if message["type"] == "service" {
                        "system"
                    } else {
                        "message"
                    },
                    message.clone(),
                );
                imported.conversation = Some(conversation.clone());
                imported.sender = first(message, &["from_id", "actor_id", "from", "actor"]);
                imported.timestamp = first(message, &["date", "date_unixtime"]);
                imported.text = match &message["text"] {
                    Value::String(s) => Some(s.clone()),
                    Value::Array(parts) => Some(
                        parts
                            .iter()
                            .filter_map(|part| part.as_str().or_else(|| part["text"].as_str()))
                            .collect::<Vec<_>>()
                            .join(""),
                    ),
                    _ => None,
                };
                for key in ["photo", "file", "thumbnail", "video_file", "voice_message"] {
                    if let Some(path) = message[key].as_str().filter(|p| !p.starts_with('(')) {
                        let relative = name.rsplit_once('/').map(|(p, _)| format!("{p}/{path}"));
                        let resolved = relative
                            .filter(|p| entries.contains_key(p))
                            .unwrap_or_else(|| path.to_owned());
                        if !entries.contains_key(&resolved) {
                            batch.warnings.push(format!(
                                "Telegram media is absent from this upload: {resolved}"
                            ));
                        }
                        imported.attachments.push(resolved);
                    }
                }
                batch.objects.push(imported);
            }
        }
    }
    if !found {
        return Err("No Telegram export found; upload result.json or a ZIP containing it".into());
    }
    Ok(())
}

// Desktop exports use bare peer IDs; live Telethon dialog IDs mark peer type.
// Keep user/group/channel namespaces distinct even when bare numeric IDs match.
fn conversation_id(chat: &Value, batch: &mut ImportBatch) -> String {
    let Some(native) = first(chat, &["id"]) else {
        batch.warnings.push(
            "Telegram chat lacks a native peer ID; archive/live overlap cannot be guaranteed."
                .into(),
        );
        return fingerprint(chat);
    };
    let Some(id) = native.parse::<i64>().ok() else {
        batch.warnings.push(format!("Telegram chat {native} has an unrecognized peer ID; archive/live overlap cannot be guaranteed."));
        return native;
    };
    if id < 0 {
        return native;
    }
    let marked = match chat["type"].as_str() {
        Some(
            "personal_chat" | "bot_chat" | "saved_messages" | "replies" | "verification_codes",
        ) => Some(id),
        Some("private_group") => id.checked_neg(),
        Some("private_supergroup" | "public_supergroup" | "private_channel" | "public_channel") => {
            id.checked_add(1_000_000_000_000).and_then(i64::checked_neg)
        }
        _ => None,
    };
    match marked {
        Some(id) => id.to_string(),
        None => {
            batch.warnings.push(format!("Telegram chat {native} lacks a recognized peer type; archive/live overlap cannot be guaranteed."));
            native
        }
    }
}
