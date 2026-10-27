use serde_json::Value;
use std::path::Path;
use crate::Result;
pub struct Store;
impl Store {
    pub fn open(_path: impl AsRef<Path>) -> Result<Self> { Err("storage not initialized".into()) }
    pub fn memory() -> Self { Self }
    pub fn did(&self) -> String { String::new() }
    pub fn list(&mut self, _collection: Option<&str>) -> Result<Vec<Value>> { Err("storage not initialized".into()) }
    pub fn get(&mut self, _collection: &str, _rkey: &str) -> Result<Value> { Err("storage not initialized".into()) }
    pub fn put(&mut self, _collection: &str, _rkey: &str, _body: Value, _visibility: &str) -> Result<Value> { Err("storage not initialized".into()) }
    pub fn remove(&mut self, _collection: &str, _rkey: &str) -> Result<Value> { Err("storage not initialized".into()) }
    pub fn put_blob(&mut self, _bytes: &[u8], _mime: &str) -> Result<String> { Err("storage not initialized".into()) }
    pub fn get_blob(&mut self, _cid: &str) -> Result<Vec<u8>> { Err("storage not initialized".into()) }
    pub fn export(&mut self) -> Result<Vec<u8>> { Err("storage not initialized".into()) }
    pub fn import(&mut self, _bytes: &[u8]) -> Result<()> { Err("storage not initialized".into()) }
    pub fn data_dir(&self) -> Option<&Path> { None }
}
