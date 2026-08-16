use serde_json::{json, Value};
use tangled::foreign::activitypub_actor_acct;

use crate::note::{as_id, plain_text};

/// Reed `pub.reed.profile` fields mapped from an AP actor document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappedProfile {
    pub display_name: String,
    pub handle: Option<String>,
    pub url: String,
    pub bio: String,
    pub icon_url: Option<String>,
}

/// `name` / `preferredUsername` / acct, HTML-stripped `summary`, actor id, icon URL.
pub fn actor_to_profile(actor: &Value, actor_id: &str) -> MappedProfile {
    let preferred = preferred_username(actor);
    let handle = activitypub_actor_acct(actor_id, preferred.as_deref());
    let display_name = display_name(actor)
        .or_else(|| preferred.clone())
        .or_else(|| {
            handle
                .as_deref()
                .and_then(|a| a.split('@').next())
                .map(str::to_string)
        })
        .unwrap_or_default();
    let bio = summary(actor).map(|s| plain_text(&s)).unwrap_or_default();
    let url = self::actor_id(actor).unwrap_or_else(|| actor_id.to_string());
    MappedProfile {
        display_name,
        handle,
        url,
        bio,
        icon_url: icon_url(actor),
    }
}

pub fn person_document(
    id: &str,
    username: &str,
    inbox: &str,
    outbox: &str,
    public_key_pem: &str,
) -> Value {
    json!({
        "@context": [
            "https://www.w3.org/ns/activitystreams",
            "https://w3id.org/security/v1"
        ],
        "id": id,
        "type": "Person",
        "preferredUsername": username,
        "inbox": inbox,
        "outbox": outbox,
        "publicKey": {
            "id": format!("{id}#main-key"),
            "owner": id,
            "publicKeyPem": public_key_pem
        }
    })
}

pub fn actor_inbox(actor: &Value) -> Option<String> {
    actor
        .get("endpoints")
        .and_then(|e| e.get("sharedInbox"))
        .and_then(|s| s.as_str())
        .map(str::to_string)
        .or_else(|| as_id(actor.get("inbox")))
}

pub fn actor_outbox(actor: &Value) -> Option<String> {
    as_id(actor.get("outbox"))
}

pub fn preferred_username(actor: &Value) -> Option<String> {
    actor
        .get("preferredUsername")
        .and_then(|u| u.as_str())
        .map(str::to_string)
}

pub fn display_name(actor: &Value) -> Option<String> {
    actor
        .get("name")
        .and_then(|n| n.as_str())
        .map(str::to_string)
}

pub fn summary(actor: &Value) -> Option<String> {
    actor
        .get("summary")
        .and_then(|s| s.as_str())
        .map(str::to_string)
}

pub fn icon_url(actor: &Value) -> Option<String> {
    let icon = actor.get("icon")?;
    match icon {
        Value::Array(items) => items.iter().find_map(icon_from),
        other => icon_from(other),
    }
}

fn icon_from(icon: &Value) -> Option<String> {
    as_id(Some(icon)).or_else(|| icon.get("url").and_then(|u| u.as_str()).map(str::to_string))
}

pub fn public_key_pem(actor: &Value) -> Option<String> {
    actor
        .get("publicKey")
        .and_then(|k| k.get("publicKeyPem"))
        .and_then(|p| p.as_str())
        .map(str::to_string)
}

pub fn public_key_id(actor: &Value) -> Option<String> {
    actor
        .get("publicKey")
        .and_then(|k| k.get("id"))
        .and_then(|i| i.as_str())
        .map(str::to_string)
}

pub fn public_key_owner(actor: &Value) -> Option<String> {
    actor
        .get("publicKey")
        .and_then(|k| k.get("owner"))
        .and_then(|o| o.as_str())
        .map(str::to_string)
}

pub fn actor_id(actor: &Value) -> Option<String> {
    as_id(Some(actor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_name_html_summary_and_icon() {
        let actor = json!({
            "id": "https://mastodon.social/users/Gargron",
            "type": "Person",
            "preferredUsername": "Gargron",
            "name": "Eugen Rochko",
            "summary": "<p>Founder of <a href=\"https://joinmastodon.org\">Mastodon</a></p>",
            "icon": {
                "type": "Image",
                "url": "https://files.mastodon.social/accounts/avatars/gargron.png"
            }
        });
        let mapped = actor_to_profile(&actor, "https://mastodon.social/users/Gargron");
        assert_eq!(mapped.display_name, "Eugen Rochko");
        assert_eq!(mapped.handle.as_deref(), Some("Gargron@mastodon.social"));
        assert_eq!(mapped.url, "https://mastodon.social/users/Gargron");
        assert_eq!(mapped.bio, "Founder of Mastodon");
        assert_eq!(
            mapped.icon_url.as_deref(),
            Some("https://files.mastodon.social/accounts/avatars/gargron.png")
        );
    }

    #[test]
    fn falls_back_to_preferred_username_and_string_icon() {
        let actor = json!({
            "id": "https://ex.example/users/alice",
            "preferredUsername": "alice",
            "icon": "https://ex.example/alice.png"
        });
        let mapped = actor_to_profile(&actor, "https://ex.example/users/alice");
        assert_eq!(mapped.display_name, "alice");
        assert_eq!(mapped.handle.as_deref(), Some("alice@ex.example"));
        assert_eq!(
            mapped.icon_url.as_deref(),
            Some("https://ex.example/alice.png")
        );
        assert!(mapped.bio.is_empty());
    }
}
