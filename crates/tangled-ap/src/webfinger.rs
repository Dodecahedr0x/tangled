use serde_json::{json, Value};

use crate::error::{Error, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebFinger {
    pub subject: String,
    pub actor: String,
}

/// Parse a JRD (`application/jrd+json`) and pick the ActivityPub self link.
pub fn parse_jrd(jrd: &Value) -> Result<WebFinger> {
    let subject = jrd
        .get("subject")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    let actor =
        self_href(jrd).ok_or_else(|| Error::Protocol("webfinger missing self link".into()))?;
    Ok(WebFinger { subject, actor })
}

pub fn self_href(jrd: &Value) -> Option<String> {
    let links = jrd.get("links")?.as_array()?;
    for link in links {
        let rel = link.get("rel").and_then(|r| r.as_str()).unwrap_or("");
        let ty = link.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if rel == "self" && (ty.contains("activity") || ty.contains("ld+json") || ty.is_empty()) {
            if let Some(href) = link.get("href").and_then(|h| h.as_str()) {
                return Some(href.to_string());
            }
        }
    }
    None
}

pub fn build_jrd(subject: &str, actor: &str) -> Value {
    json!({
        "subject": subject,
        "links": [{
            "rel": "self",
            "type": "application/activity+json",
            "href": actor
        }]
    })
}

pub fn webfinger_path(resource: &str) -> String {
    format!("/.well-known/webfinger?resource={resource}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jrd_parse_and_build() {
        let jrd = json!({
            "subject": "acct:alice@example.com",
            "links": [
                { "rel": "http://webfinger.net/rel/profile-page", "href": "https://example.com/a" },
                {
                    "rel": "self",
                    "type": "application/activity+json",
                    "href": "https://example.com/users/alice"
                }
            ]
        });
        let parsed = parse_jrd(&jrd).unwrap();
        assert_eq!(parsed.subject, "acct:alice@example.com");
        assert_eq!(parsed.actor, "https://example.com/users/alice");
        let built = build_jrd("acct:bob@knot.example", "https://knot.example/users/bob");
        assert_eq!(
            parse_jrd(&built).unwrap().actor,
            "https://knot.example/users/bob"
        );
    }
}
