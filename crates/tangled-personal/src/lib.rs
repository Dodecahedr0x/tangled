//! Application-independent personal workflows over signed Tangled repositories.
pub mod store;
pub use store::Store;
pub type Result<T> = std::result::Result<T, String>;
pub mod memory;
pub mod capture;
pub mod notes;
pub mod feeds;
pub mod agents;
pub mod devices;
pub mod publish;
pub mod connections;

/// Route the same JSON workflow contract used by the command client.
pub fn execute(feature: &str, store: &mut Store, input: serde_json::Value) -> Result<serde_json::Value> {
    use serde_json::json;
    match feature {
        "info" => Ok(json!({"did":store.did(),"data":store.data_dir(),"version":env!("CARGO_PKG_VERSION")})),
        "memory" => memory::execute(store,input),
        "capture" => capture::execute(store,input),
        "notes" => notes::execute(store,input),
        "records" => {
            let action = input["action"].as_str().ok_or("Missing action")?;
            let collection = || input["collection"].as_str().ok_or_else(||"Missing collection".to_string());
            let rkey = || input["rkey"].as_str().ok_or_else(||"Missing rkey".to_string());
            match action {
                "list" => Ok(json!({"records":store.list(input.get("collection").map(|v|v.as_str().ok_or("Invalid collection")).transpose()?)?})),
                "get" => store.get(collection()?,rkey()?),
                "put" => store.put(collection()?,rkey()?,input.get("body").cloned().ok_or("Missing body")?,input.get("visibility").map(|v|v.as_str().ok_or("Invalid visibility")).transpose()?.unwrap_or("cap")),
                "delete" => store.remove(collection()?,rkey()?),
                _ => Err(format!("Unknown records action: {action}")),
            }
        }
        _ => Err(format!("Unknown feature: {feature}")),
    }
}
