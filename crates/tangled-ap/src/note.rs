use serde_json::{json, Value};

use crate::error::{Error, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappedAttachment {
    pub url: String,
    pub mime: Option<String>,
    pub alt: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappedPost {
    pub text: String,
    pub reply_to: Option<String>,
    pub attachments: Vec<MappedAttachment>,
    pub lossy: bool,
}

/// `Note.content` → `pub.tangled.post` `text`; `inReplyTo` → `reply`.
pub fn note_to_post(note: &Value) -> Result<MappedPost> {
    let ty = type_name(note);
    if ty != "Note" && ty != "note" {
        return Err(Error::Protocol(format!("expected Note, got {ty}")));
    }
    let raw = note
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    let (text, html_lossy) = html_to_text(&raw);
    let reply_to = as_id(note.get("inReplyTo"));
    let attachments = attachments(note);
    let lossy = html_lossy || !attachments.is_empty();
    Ok(MappedPost {
        text,
        reply_to,
        attachments,
        lossy,
    })
}

/// Native public post → ActivityPub `Note`.
pub fn post_to_note(
    id: &str,
    attributed_to: &str,
    text: &str,
    in_reply_to: Option<&str>,
    published: Option<&str>,
) -> Value {
    let mut note = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": id,
        "type": "Note",
        "attributedTo": attributed_to,
        "content": text,
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
    });
    if let Some(reply) = in_reply_to {
        note["inReplyTo"] = json!(reply);
    }
    if let Some(ts) = published {
        note["published"] = json!(ts);
    }
    note
}

pub fn create_note(activity_id: &str, actor: &str, note: &Value) -> Value {
    json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": activity_id,
        "type": "Create",
        "actor": actor,
        "object": note,
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
    })
}

fn attachments(note: &Value) -> Vec<MappedAttachment> {
    let Some(att) = note.get("attachment") else {
        return Vec::new();
    };
    let items: Vec<&Value> = match att {
        Value::Array(a) => a.iter().collect(),
        other => vec![other],
    };
    items
        .into_iter()
        .filter_map(|a| {
            let url = a
                .get("url")
                .and_then(|u| u.as_str())
                .or_else(|| a.get("href").and_then(|h| h.as_str()))?
                .to_string();
            Some(MappedAttachment {
                url,
                mime: a
                    .get("mediaType")
                    .and_then(|m| m.as_str())
                    .map(str::to_string),
                alt: a.get("name").and_then(|n| n.as_str()).map(str::to_string),
            })
        })
        .collect()
}

pub(crate) fn type_name(v: &Value) -> String {
    match v.get("type") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a.iter().find_map(|x| x.as_str()).unwrap_or("").to_string(),
        _ => String::new(),
    }
}

pub(crate) fn as_id(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o.get("id").and_then(|i| i.as_str()).map(str::to_string),
        _ => None,
    }
}

/// Strip tags and unescape a few entities. Used for Note content and actor summary.
pub fn plain_text(s: &str) -> String {
    html_to_text(s).0
}

fn html_to_text(s: &str) -> (String, bool) {
    if !s.contains('<') {
        return (s.to_string(), false);
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            let mut tag = String::new();
            for t in chars.by_ref() {
                if t == '>' {
                    break;
                }
                tag.push(t);
            }
            let name = tag
                .trim()
                .trim_start_matches('/')
                .split(|ch: char| ch.is_whitespace() || ch == '/')
                .next()
                .unwrap_or("");
            if (name.eq_ignore_ascii_case("br") || name.eq_ignore_ascii_case("p"))
                && !out.ends_with('\n')
            {
                out.push('\n');
            }
        } else {
            out.push(c);
        }
    }
    let text = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .trim()
        .to_string();
    (text, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_post_roundtrip() {
        let note = json!({
            "type": "Note",
            "id": "https://ex.example/users/a/statuses/1",
            "content": "<p>hello <b>square</b></p>",
            "inReplyTo": "https://ex.example/users/b/statuses/9",
            "attachment": [{
                "type": "Document",
                "url": "https://ex.example/media/1.png",
                "mediaType": "image/png",
                "name": "pic"
            }]
        });
        let mapped = note_to_post(&note).unwrap();
        assert_eq!(mapped.text, "hello square");
        assert_eq!(
            mapped.reply_to.as_deref(),
            Some("https://ex.example/users/b/statuses/9")
        );
        assert_eq!(mapped.attachments.len(), 1);
        assert_eq!(mapped.attachments[0].mime.as_deref(), Some("image/png"));
        assert!(mapped.lossy);

        let back = post_to_note(
            "https://knot.example/notes/1",
            "https://knot.example/users/g",
            &mapped.text,
            mapped.reply_to.as_deref(),
            None,
        );
        assert_eq!(back["type"], "Note");
        assert_eq!(back["content"], "hello square");
        assert_eq!(back["inReplyTo"], "https://ex.example/users/b/statuses/9");
        let again = note_to_post(&back).unwrap();
        assert_eq!(again.text, "hello square");
        assert_eq!(again.reply_to, mapped.reply_to);
    }
}
