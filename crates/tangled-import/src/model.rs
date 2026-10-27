use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ImportedObject {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub conversation: Option<String>,
    #[serde(default)]
    pub sender: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub attachments: Vec<String>,
    #[serde(default)]
    pub data: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ImportedFile {
    pub name: String,
    pub media_type: String,
    pub data_base64: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ImportBatch {
    pub source: String,
    pub account: String,
    #[serde(default)]
    pub objects: Vec<ImportedObject>,
    #[serde(default)]
    pub files: Vec<ImportedFile>,
    #[serde(default)]
    pub warnings: Vec<String>,
}
