use rsa::RsaPrivateKey;
use serde_json::Value;
use tangled::foreign::activitypub_actor_identity;
use tangled::KnotEngine;
use tangled_ap::httpsig::{http_date, sign_request};
use tangled_ap::{announce_activity, create_note, follow_activity, like_activity, post_to_note};

use super::ApState;

#[derive(Clone, Debug)]
pub struct RemoteActor {
    pub actor: String,
    pub inbox: String,
    pub did: String,
}

pub fn outbound_activity(ap: &ApState, record: &Value) -> Option<Value> {
    let collection = record.get("collection")?.as_str()?;
    let vis = record
        .get("vis")
        .and_then(|v| v.as_str())
        .unwrap_or("public");
    if vis != "public" {
        return None;
    }
    let author = record.get("author")?.as_str()?;
    let cid = record
        .get("sig")
        .and_then(|s| s.as_str())
        .map(|_| record_cid_hint(record))
        .unwrap_or_else(|| "unknown".into());
    let name = author_name(author);
    let actor = format!("{}/users/{name}", ap.public_url);
    match collection {
        "pub.tangled.post" => {
            let text = record
                .get("body")
                .and_then(|b| b.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            let reply = record
                .get("body")
                .and_then(|b| b.get("reply"))
                .and_then(|r| r.as_str())
                .map(|c| format!("{}/notes/{c}", ap.public_url));
            let note_id = format!("{}/notes/{cid}", ap.public_url);
            let note = post_to_note(&note_id, &actor, text, reply.as_deref(), None);
            Some(create_note(&format!("{note_id}/activity"), &actor, &note))
        }
        "tangled.graph.follow" => {
            let subject = record.get("body")?.get("subject")?.as_str()?;
            let object = actor_for_did(ap, subject);
            Some(follow_activity(
                &format!("{}/follows/{cid}", ap.public_url),
                &actor,
                &object,
            ))
        }
        "pub.reed.like" => {
            let target = record.get("body")?.get("target")?.as_str()?;
            Some(like_activity(
                &format!("{}/likes/{cid}", ap.public_url),
                &actor,
                &object_iri(ap, target),
            ))
        }
        "pub.reed.boost" => {
            let target = record.get("body")?.get("target")?.as_str()?;
            Some(announce_activity(
                &format!("{}/boosts/{cid}", ap.public_url),
                &actor,
                &object_iri(ap, target),
            ))
        }
        _ => None,
    }
}

pub fn deliveries_for(ap: &ApState, record: &Value) -> Vec<String> {
    let collection = record
        .get("collection")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    match collection {
        "pub.tangled.post" | "pub.reed.boost" => ap
            .followers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|r| r.inbox.clone())
            .collect(),
        "tangled.graph.follow" => {
            let subject = record
                .get("body")
                .and_then(|b| b.get("subject"))
                .and_then(|s| s.as_str())
                .unwrap_or("");
            inbox_for_did(ap, subject).into_iter().collect()
        }
        "pub.reed.like" => {
            let target = record
                .get("body")
                .and_then(|b| b.get("target"))
                .and_then(|s| s.as_str())
                .unwrap_or("");
            inbox_for_object(ap, target).into_iter().collect()
        }
        _ => Vec::new(),
    }
}

pub fn outbox_notes(engine: &KnotEngine, public_url: &str, name: &str) -> Vec<Value> {
    let public_url = public_url.trim_end_matches('/');
    engine
        .stored
        .iter()
        .rev()
        .filter(|r| r.collection == "pub.tangled.post" && r.vis == "public" && !r.deleted)
        .filter(|r| name == "gateway" || r.did.ends_with(name) || author_name(&r.did) == name)
        .take(40)
        .filter_map(|r| {
            let text = r.record.get("body")?.get("text")?.as_str()?;
            let actor = format!("{public_url}/users/{name}");
            let note_id = format!("{public_url}/notes/{}", r.cid);
            Some(post_to_note(&note_id, &actor, text, None, None))
        })
        .collect()
}

pub async fn deliver_signed(
    rsa: &RsaPrivateKey,
    public_url: &str,
    activity: &Value,
    inboxes: &[String],
    allow_loopback: bool,
) {
    let body = match serde_json::to_vec(activity) {
        Ok(b) => b,
        Err(_) => return,
    };
    let client = match reqwest::Client::builder().build() {
        Ok(c) => c,
        Err(_) => return,
    };
    let date = http_date(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    );
    let actor = activity
        .get("actor")
        .and_then(|a| a.as_str())
        .unwrap_or(public_url);
    let key_id = format!("{actor}#main-key");
    for inbox in inboxes {
        if !super::delivery_url_allowed(inbox, allow_loopback) {
            continue;
        }
        let Ok(url) = reqwest::Url::parse(inbox) else {
            continue;
        };
        let host = url.host_str().unwrap_or("").to_string();
        let path = if url.query().is_some() {
            format!("{}?{}", url.path(), url.query().unwrap_or(""))
        } else {
            url.path().to_string()
        };
        let (signature, digest) = sign_request(rsa, &key_id, "POST", &path, &host, &date, &body);
        let _ = client
            .post(inbox)
            .header("Host", host)
            .header("Date", &date)
            .header("Digest", digest)
            .header("Signature", signature)
            .header("Content-Type", "application/activity+json")
            .body(body.clone())
            .send()
            .await;
    }
}

fn actor_for_did(ap: &ApState, did: &str) -> String {
    let remotes = ap.remotes.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = remotes.values().find(|r| r.did == did) {
        return r.actor.clone();
    }
    drop(remotes);
    format!("{}/users/{}", ap.public_url, author_name(did))
}

fn inbox_for_did(ap: &ApState, did: &str) -> Option<String> {
    ap.remotes
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .find(|r| r.did == did)
        .map(|r| r.inbox.clone())
}

fn inbox_for_object(ap: &ApState, target: &str) -> Option<String> {
    if target.starts_with("http://") || target.starts_with("https://") {
        let actor = activitypub_actor_identity(target).did_string();
        return inbox_for_did(ap, &actor);
    }
    None
}

fn object_iri(ap: &ApState, target: &str) -> String {
    if target.starts_with("http://") || target.starts_with("https://") {
        target.to_string()
    } else {
        format!("{}/notes/{target}", ap.public_url)
    }
}

fn author_name(did: &str) -> String {
    did.strip_prefix("did:sol:")
        .unwrap_or(did)
        .chars()
        .take(32)
        .collect()
}

fn record_cid_hint(record: &Value) -> String {
    record
        .get("rkey")
        .and_then(|r| r.as_str())
        .unwrap_or("item")
        .to_string()
}
