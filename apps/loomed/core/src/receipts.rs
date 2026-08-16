use crate::envelope::open_plaintext;
use crate::error::Result;
use crate::identity::LoomedKeys;
use crate::types::{CircleRef, MessageId};
use tangled::circle::CircleState;

pub const RECEIPT_COLLECTION: &str = "pub.loomed.receipt";

pub fn seal_receipt(
    keys: &LoomedKeys,
    circle: &CircleRef,
    circle_state: &CircleState,
    target: &MessageId,
) -> Result<serde_json::Value> {
    crate::envelope::seal_json(
        keys,
        circle,
        circle_state,
        RECEIPT_COLLECTION,
        serde_json::json!({ "circle": circle.0, "target": target.0 }),
        Some(&target.0),
    )
}

pub fn is_receipt(record: &serde_json::Value) -> bool {
    record.get("collection").and_then(|c| c.as_str()) == Some(RECEIPT_COLLECTION)
}

pub fn open_receipt(
    _keys: &LoomedKeys,
    circle_state: &CircleState,
    record: &serde_json::Value,
) -> Result<serde_json::Value> {
    let _ = open_plaintext(circle_state, record)?;
    Ok(record.clone())
}
