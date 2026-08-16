use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::note::{as_id, type_name};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inbound {
    CreateNote {
        activity_id: String,
        actor: String,
        note: Value,
    },
    Follow {
        activity_id: String,
        actor: String,
        object: String,
    },
    Like {
        activity_id: String,
        actor: String,
        object: String,
    },
    Announce {
        activity_id: String,
        actor: String,
        object: String,
    },
    Undo {
        activity_id: String,
        actor: String,
        object: String,
    },
}

pub fn parse_inbound(activity: &Value) -> Result<Inbound> {
    let ty = type_name(activity);
    let activity_id = as_id(Some(activity)).unwrap_or_default();
    let actor = as_id(activity.get("actor")).unwrap_or_default();
    match ty.as_str() {
        "Create" => {
            let note = activity
                .get("object")
                .cloned()
                .ok_or_else(|| Error::Protocol("Create missing object".into()))?;
            if type_name(&note) != "Note" {
                return Err(Error::Protocol("Create object is not a Note".into()));
            }
            let actor = if actor.is_empty() {
                as_id(note.get("attributedTo")).unwrap_or_default()
            } else {
                actor
            };
            Ok(Inbound::CreateNote {
                activity_id,
                actor,
                note,
            })
        }
        "Follow" => Ok(Inbound::Follow {
            activity_id,
            actor,
            object: as_id(activity.get("object")).unwrap_or_default(),
        }),
        "Like" => Ok(Inbound::Like {
            activity_id,
            actor,
            object: as_id(activity.get("object")).unwrap_or_default(),
        }),
        "Announce" => Ok(Inbound::Announce {
            activity_id,
            actor,
            object: as_id(activity.get("object")).unwrap_or_default(),
        }),
        "Undo" => Ok(Inbound::Undo {
            activity_id,
            actor,
            object: as_id(activity.get("object")).unwrap_or_default(),
        }),
        other => Err(Error::Protocol(format!("unsupported activity {other}"))),
    }
}

pub fn like_activity(id: &str, actor: &str, object: &str) -> Value {
    json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": id,
        "type": "Like",
        "actor": actor,
        "object": object,
    })
}

pub fn announce_activity(id: &str, actor: &str, object: &str) -> Value {
    json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": id,
        "type": "Announce",
        "actor": actor,
        "object": object,
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
    })
}

pub fn follow_activity(id: &str, actor: &str, object: &str) -> Value {
    json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": id,
        "type": "Follow",
        "actor": actor,
        "object": object,
    })
}

pub fn accept_follow(id: &str, actor: &str, follow: &Value) -> Value {
    json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": id,
        "type": "Accept",
        "actor": actor,
        "object": follow,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::note_to_post;

    #[test]
    fn like_and_announce_mapping() {
        let like = json!({
            "type": "Like",
            "id": "https://ex.example/likes/1",
            "actor": "https://ex.example/users/a",
            "object": "https://ex.example/users/b/statuses/2"
        });
        match parse_inbound(&like).unwrap() {
            Inbound::Like { object, actor, .. } => {
                assert_eq!(object, "https://ex.example/users/b/statuses/2");
                assert_eq!(actor, "https://ex.example/users/a");
            }
            other => panic!("{other:?}"),
        }
        let boost = json!({
            "type": "Announce",
            "id": "https://ex.example/boosts/1",
            "actor": "https://ex.example/users/a",
            "object": "https://ex.example/users/b/statuses/2"
        });
        match parse_inbound(&boost).unwrap() {
            Inbound::Announce { object, .. } => {
                assert_eq!(object, "https://ex.example/users/b/statuses/2");
            }
            other => panic!("{other:?}"),
        }
        let create = json!({
            "type": "Create",
            "id": "https://ex.example/acts/1",
            "actor": "https://ex.example/users/a",
            "object": {
                "type": "Note",
                "id": "https://ex.example/users/a/statuses/1",
                "content": "hi"
            }
        });
        match parse_inbound(&create).unwrap() {
            Inbound::CreateNote { note, .. } => {
                assert_eq!(note_to_post(&note).unwrap().text, "hi");
            }
            other => panic!("{other:?}"),
        }
        let out_like = like_activity(
            "https://k.example/like/1",
            "https://k.example/users/g",
            "https://ex.example/users/b/statuses/2",
        );
        assert_eq!(out_like["type"], "Like");
        let out_ann = announce_activity(
            "https://k.example/boost/1",
            "https://k.example/users/g",
            "https://ex.example/users/b/statuses/2",
        );
        assert_eq!(out_ann["type"], "Announce");
    }
}
