# Shared contract

Crate: crates/tangled-personal. Exports Store, Result<T> = std::result::Result<T,String>, modules memory,capture,notes,feeds,agents,devices,publish,connections. Feature entry point: pub fn execute(store: &mut crate::Store, input: serde_json::Value) -> crate::Result<serde_json::Value>. Input is an object with action string; output is JSON. Document all actions/examples. Never panic on user input.

Store API (synchronous, methods require &mut self except did/data_dir):
- Store::open(path: impl AsRef<std::path::Path>) -> Result<Self>: persistent local signed knot, private owner key files and exclusive process lock.
- Store::memory() -> Self: ephemeral isolated owner repository for tests.
- store.did() -> String
- store.list(collection: Option<&str>) -> Result<Vec<serde_json::Value>>: authorized current signed envelopes excluding tombstones, each with body plus derived uri and cid fields.
- store.get(collection: &str,rkey: &str) -> Result<serde_json::Value>
- store.put(collection: &str,rkey: &str,body: serde_json::Value,visibility: &str) -> Result<serde_json::Value>: signed create/causal successor, prev current CID; use cap unless explicitly publishing.
- store.remove(collection: &str,rkey: &str) -> Result<serde_json::Value>: signed tombstone.
- store.put_blob(bytes: &[u8],mime: &str) -> Result<String>; store.get_blob(cid: &str) -> Result<Vec<u8>>.
- store.export() -> Result<Vec<u8>>: authorized full CAR with blobs; store.import(bytes: &[u8]) -> Result<()>: verified import, no silent rollback.
- store.data_dir() -> Option<&std::path::Path>

Use generic pub.tangled.* collections, never app-specific namespaces. Extend rather than change existing normative v1 schemas. JSON schemas permit unknown extension fields, which edits preserve. Stable references are Tangled URIs; exact evidence adds a CID. Source imports do not become fake native users or public activity. Private data is cap-gated, not claimed encrypted from a local operator. Public blob sharing must explicitly copy selected bytes into a public publication.

Available deps: serde,serde_json,blake3,hex,tangled,std. Request other deps from parent. Network operations must be explicit, bounded HTTP(S), without ambient credentials. Parsing takes supplied bytes for deterministic tests. Shared store implementation will land while feature agents work; temporary stubs intentionally return errors. Work on your own module/tests/docs/schema only; parent resolves wiring. Record failing tests, implement, test, commit. Report exact commit, commands/results, limits.
