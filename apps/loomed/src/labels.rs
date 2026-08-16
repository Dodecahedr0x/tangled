//! Shared recipient labels for the rail and the thread header.

use loomed_core::Message;

/// How many names to show before cropping to `+ X others`.
pub const MAX_NAMED_RECIPIENTS: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerLabel {
    pub did: String,
    pub handle: Option<String>,
}

impl PeerLabel {
    pub fn new(did: impl Into<String>, handle: Option<String>) -> Self {
        Self {
            did: did.into(),
            handle,
        }
    }
}

/// Crop a long DID so a rail or header does not overflow: `did:sol:…xxxx`.
pub fn crop_did(did: &str) -> String {
    let Some(rest) = did.strip_prefix("did:sol:") else {
        return did.to_string();
    };
    if rest.len() <= 8 {
        return did.to_string();
    }
    format!("did:sol:…{}", &rest[rest.len() - 4..])
}

/// Handle when known, otherwise a cropped DID.
pub fn peer_display(did: &str, handle: Option<&str>) -> String {
    match handle {
        Some(h) if !h.is_empty() => h.to_string(),
        _ => crop_did(did),
    }
}

fn others<'a>(peers: &'a [PeerLabel], self_did: Option<&str>) -> Vec<&'a PeerLabel> {
    peers
        .iter()
        .filter(|p| self_did != Some(p.did.as_str()))
        .collect()
}

/// Recipients excluding `self_did` when set. 1:1 is the other person;
/// a longer list is `alice.sol, bob.sol + 3 others`.
pub fn recipient_label(peers: &[PeerLabel], self_did: Option<&str>) -> String {
    let others = others(peers, self_did);
    if others.is_empty() {
        return self_did
            .map(|d| {
                let handle = peers
                    .iter()
                    .find(|p| p.did == d)
                    .and_then(|p| p.handle.as_deref());
                peer_display(d, handle)
            })
            .unwrap_or_else(|| "chat".into());
    }
    let named: Vec<String> = others
        .iter()
        .take(MAX_NAMED_RECIPIENTS)
        .map(|p| peer_display(&p.did, p.handle.as_deref()))
        .collect();
    let extra = others.len().saturating_sub(MAX_NAMED_RECIPIENTS);
    if extra > 0 {
        format!("{} + {extra} others", named.join(", "))
    } else {
        named.join(", ")
    }
}

/// Total people in the circle (including self when present).
pub fn member_count(peers: &[PeerLabel]) -> usize {
    peers.len()
}

/// Group subtitle, e.g. `3 people`. None for 1:1.
pub fn group_subtitle(member_count: usize) -> Option<String> {
    if member_count >= 3 {
        Some(format!("{member_count} people"))
    } else {
        None
    }
}

/// Unique circle owner + authors + receipt DIDs when member metadata is missing.
pub fn infer_recipients(circle: &str, messages: &[Message]) -> Vec<PeerLabel> {
    let mut dids = Vec::new();
    if let Some(owner) = circle.split('#').next() {
        if owner.starts_with("did:") {
            push_unique(&mut dids, owner);
        }
    }
    for m in messages {
        push_unique(&mut dids, &m.author);
        for r in &m.receipts {
            push_unique(&mut dids, r);
        }
    }
    dids.into_iter()
        .map(|did| PeerLabel::new(did, None))
        .collect()
}

fn push_unique(dids: &mut Vec<String>, did: &str) {
    if !did.is_empty() && !dids.iter().any(|d| d == did) {
        dids.push(did.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loomed_core::{CircleRef, Message, MessageId};

    #[test]
    fn crop_did_keeps_short_ids() {
        assert_eq!(crop_did("did:sol:bob"), "did:sol:bob");
        assert_eq!(crop_did("did:sol:alice"), "did:sol:alice");
    }

    #[test]
    fn crop_did_shortens_long_keys() {
        let did = "did:sol:7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU";
        assert_eq!(crop_did(did), "did:sol:…gAsU");
    }

    #[test]
    fn peer_display_prefers_handle() {
        assert_eq!(
            peer_display("did:sol:alice", Some("alice.sol")),
            "alice.sol"
        );
        assert_eq!(peer_display("did:sol:bob", None), "did:sol:bob");
    }

    #[test]
    fn one_to_one_is_the_other_person() {
        let peers = [
            PeerLabel::new("did:sol:alice", Some("alice.sol".into())),
            PeerLabel::new("did:sol:bob", Some("bob.sol".into())),
        ];
        assert_eq!(recipient_label(&peers, Some("did:sol:alice")), "bob.sol");
    }

    #[test]
    fn group_crops_with_others() {
        let peers = [
            PeerLabel::new("did:sol:alice", Some("alice.sol".into())),
            PeerLabel::new("did:sol:bob", Some("bob.sol".into())),
            PeerLabel::new("did:sol:cara", Some("cara.sol".into())),
            PeerLabel::new("did:sol:dan", None),
            PeerLabel::new("did:sol:eve", None),
        ];
        assert_eq!(
            recipient_label(&peers, Some("did:sol:zz")),
            "alice.sol, bob.sol + 3 others"
        );
        assert_eq!(
            recipient_label(&peers, Some("did:sol:alice")),
            "bob.sol, cara.sol + 2 others"
        );
    }

    #[test]
    fn group_subtitle_for_three_or_more() {
        assert_eq!(group_subtitle(2), None);
        assert_eq!(group_subtitle(3).as_deref(), Some("3 people"));
        assert_eq!(group_subtitle(6).as_deref(), Some("6 people"));
    }

    #[test]
    fn infer_recipients_from_circle_and_messages() {
        let msg = Message {
            id: MessageId("cid1".into()),
            circle: CircleRef("did:sol:alice#r1".into()),
            author: "did:sol:bob".into(),
            text: Some("ping".into()),
            blob_cid: None,
            ts_us: 1,
            receipts: vec!["did:sol:alice".into()],
        };
        let peers = infer_recipients("did:sol:alice#r1", &[msg]);
        let dids: Vec<_> = peers.iter().map(|p| p.did.as_str()).collect();
        assert_eq!(dids, ["did:sol:alice", "did:sol:bob"]);
    }
}
