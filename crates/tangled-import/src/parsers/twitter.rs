use super::*;

/// Accept only JSON or X's static assignment grammar; never evaluate JavaScript.
fn dataset(bytes: &[u8], name: &str) -> Result<Value, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| format!("X dataset {name} is not UTF-8: {e}"))?
        .trim_start_matches('\u{feff}')
        .trim();
    let payload = if text.starts_with('[') || text.starts_with('{') {
        text
    } else {
        let (left, right) = text
            .split_once('=')
            .ok_or_else(|| format!("Invalid X wrapper in {name}"))?;
        let left = left.trim();
        if !left.starts_with("window.YTD.")
            || !left
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$'))
            || left.ends_with('.')
        {
            return Err(format!(
                "Unsafe X JavaScript wrapper in {name}; expected window.YTD assignment"
            ));
        }
        right
            .trim()
            .strip_suffix(';')
            .unwrap_or(right.trim())
            .trim()
    };
    serde_json::from_str(payload)
        .map_err(|e| format!("Invalid JSON payload in X dataset {name}: {e}"))
}

pub(super) fn parse_entries(entries: &Entries, batch: &mut ImportBatch) -> Result<(), String> {
    let mut datasets = Vec::new();
    for (name, bytes) in entries {
        let is_data = name
            .rsplit_once('/')
            .is_some_and(|(parent, _)| parent.rsplit('/').next() == Some("data"))
            || entries.len() == 1;
        if is_data && (name.ends_with(".js") || name.ends_with(".json")) {
            let value = dataset(bytes, name)?;
            if name
                .rsplit('/')
                .next()
                .is_some_and(|f| matches!(f, "account.js" | "account.json"))
            {
                let account = value
                    .as_array()
                    .and_then(|a| a.first())
                    .map(|v| &v["account"]);
                source_account(batch, account.and_then(|v| first(v, &["accountId"])));
            }
            datasets.push((name, value));
        }
    }
    if datasets.is_empty() {
        return Err("No X export datasets found; upload a ZIP containing data/*.js".into());
    }
    let mut occurrences = HashMap::new();
    for (name, value) in datasets {
        let stem = name
            .rsplit('/')
            .next()
            .unwrap_or(name)
            .split('.')
            .next()
            .unwrap_or(name);
        let normalized = stem.split("-part").next().unwrap_or(stem).replace('_', "-");
        let items: Vec<_> = value
            .as_array()
            .map(|a| a.iter().collect())
            .unwrap_or_else(|| vec![&value]);
        for item in items {
            if let Some(dm) = item.get("dmConversation") {
                let conversation = first(dm, &["conversationId"]);
                if let Some(messages) = dm["messages"].as_array() {
                    for event in messages {
                        let record = event.get("messageCreate").unwrap_or(event);
                        let mut imported = object(
                            first(record, &["id"])
                                .unwrap_or_else(|| fallback_id(name, event, &mut occurrences)),
                            if event.get("messageCreate").is_some() {
                                "message"
                            } else {
                                "archive_data"
                            },
                            record.clone(),
                        );
                        imported.conversation = conversation.clone();
                        imported.sender = first(record, &["senderId"]);
                        imported.timestamp = first(record, &["createdAt"]);
                        imported.text = first(record, &["text"]);
                        imported.attachments =
                            attachments(record, entries, &imported.id, &mut batch.warnings);
                        batch.objects.push(imported);
                    }
                }
                batch.objects.push(object(
                    format!(
                        "conversation:{}",
                        conversation.unwrap_or_else(|| fingerprint(dm))
                    ),
                    "conversation",
                    dm.clone(),
                ));
                continue;
            }
            let (kind, wrapper) = match normalized.as_str() {
                "tweets" | "tweet" => ("post", "tweet"),
                "like" | "likes" => ("like", "like"),
                "bookmark" | "bookmarks" => ("bookmark", "bookmark"),
                "following" => ("following", "following"),
                "follower" | "followers" => ("follower", "follower"),
                _ => ("archive_data", ""),
            };
            let record = item.get(wrapper).unwrap_or(item);
            let id = match (
                kind,
                first(record, &["id_str", "id", "tweetId", "accountId"]),
            ) {
                ("archive_data", _) => fallback_id(name, record, &mut occurrences),
                ("post", Some(id)) => id,
                (_, Some(id)) => format!("{kind}:{id}"),
                _ => fallback_id(name, record, &mut occurrences),
            };
            let mut imported = object(id, kind, record.clone());
            imported.sender = if kind == "post" {
                Some(batch.account.clone())
            } else {
                None
            };
            imported.timestamp = first(record, &["created_at", "createdAt"]);
            imported.text = first(record, &["full_text", "fullText", "text"]);
            imported.attachments = attachments(record, entries, &imported.id, &mut batch.warnings);
            batch.objects.push(imported);
        }
    }
    Ok(())
}

fn attachments(
    record: &Value,
    entries: &Entries,
    id: &str,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    let resolve = |url: &str| {
        let basename = url
            .rsplit('/')
            .next()
            .unwrap_or(url)
            .split('?')
            .next()
            .unwrap_or(url);
        let prefixed = format!("{id}-{basename}");
        [prefixed.as_str(), basename].iter().find_map(|wanted| {
            entries
                .keys()
                .find(|name| name.rsplit('/').next() == Some(*wanted))
                .cloned()
        })
    };
    let mut result = Vec::new();
    let mut seen_media = std::collections::HashSet::new();
    for media in [
        record.pointer("/extended_entities/media"),
        record.pointer("/entities/media"),
        record.get("mediaUrls"),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_array)
    .flatten()
    {
        let url = media
            .as_str()
            .or_else(|| media["media_url_https"].as_str())
            .or_else(|| media["media_url"].as_str());
        let identity =
            first(media, &["id_str", "id", "media_key"]).or_else(|| url.map(str::to_owned));
        // Extended entities carry the full video metadata. The basic entities
        // often repeat only its poster and must not generate a false omission.
        if identity.is_some_and(|identity| !seen_media.insert(identity)) {
            continue;
        }
        let mut variants = media
            .pointer("/video_info/variants")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|variant| variant["content_type"] == "video/mp4")
            .collect::<Vec<_>>();
        variants.sort_by_key(|variant| std::cmp::Reverse(variant["bitrate"].as_u64().unwrap_or(0)));
        if let Some(name) = variants
            .iter()
            .filter_map(|variant| variant["url"].as_str())
            .find_map(&resolve)
        {
            if !result.contains(&name) {
                result.push(name);
            }
            continue;
        }
        let missing_video = matches!(media["type"].as_str(), Some("video" | "animated_gif"));
        let supplied = url.and_then(&resolve);
        if missing_video || supplied.is_none() {
            let missing = variants
                .iter()
                .find_map(|variant| variant["url"].as_str())
                .or(url)
                .unwrap_or("unidentified source media");
            let warning = format!("X media is absent from this upload for {id}: {missing}");
            if !warnings.contains(&warning) {
                warnings.push(warning);
            }
        }
        if let Some(name) = supplied {
            if !result.contains(&name) {
                result.push(name);
            }
        }
    }
    result
}
