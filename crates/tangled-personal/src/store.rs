mod delegation;
pub(crate) mod replica;
use crate::Result;
use serde_json::{json, Value};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tangled::capability::{cap_authorization_header, Capability, Invocation};
use tangled::http::{KnotRequest, KnotResponse, Method};
use tangled::{Cid, Identity, KnotEngine};

/// A single-owner adapter over the protocol's existing repository engine.
/// Files use the same format as tangled-knot; the adapter owns an exclusive lock.
pub struct Store {
    engine: KnotEngine,
    identity: Identity,
    path: Option<PathBuf>,
    _lock: Option<File>,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        std::fs::create_dir_all(path).map_err(err)?;
        private_dir(path)?;
        let lock = private_file(&path.join("personal.lock"), false)?;
        lock.try_lock()
            .map_err(|_| "Repository is in use by another personal client".to_string())?;
        let identity_path = path.join("identity.json");
        let identity = match std::fs::read(&identity_path) {
            Ok(bytes) => {
                let value: Value = serde_json::from_slice(&bytes).map_err(err)?;
                Identity::from_seeds(seed(&value, "ed25519")?, seed(&value, "x25519")?)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if path.join("describe.json").exists() {
                    return Err("Existing repository is missing identity.json; restore its identity instead of generating a replacement".into());
                }
                let id = Identity::generate();
                let (ed, x) = id.to_seeds();
                let value = json!({"ed25519":hex::encode(ed),"x25519":hex::encode(x),"did":id.did_string()});
                use std::io::Write;
                let mut file = private_file(&identity_path, true)?;
                file.write_all(&serde_json::to_vec(&value).map_err(err)?)
                    .map_err(err)?;
                file.sync_all().map_err(err)?;
                id
            }
            Err(e) => return Err(err(e)),
        };
        private_permissions(&identity_path, 0o600)?;
        let mut engine = KnotEngine::open(path, &identity.did_string()).map_err(err)?;
        if engine
            .describe
            .as_ref()
            .is_some_and(|d| d.did != identity.did_string())
        {
            return Err("Repository and identity disagree".into());
        }
        engine.register_identity(&identity);
        engine.now = now();
        Ok(Self {
            engine,
            identity,
            path: Some(path.to_path_buf()),
            _lock: Some(lock),
        })
    }

    pub fn memory() -> Self {
        let identity = Identity::generate();
        let mut engine = KnotEngine::hosting(&identity.did_string());
        engine.now = now();
        engine.register_identity(&identity);
        Self {
            engine,
            identity,
            path: None,
            _lock: None,
        }
    }

    pub fn did(&self) -> String {
        self.identity.did_string()
    }
    pub fn data_dir(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn list(&mut self, collection: Option<&str>) -> Result<Vec<Value>> {
        // Materialize only authenticated current tips, never historical superseded bodies.
        let did = self.did();
        let mut rows: Vec<_> = self
            .engine
            .stored
            .iter()
            .filter(|r| {
                r.did == did
                    && collection.is_none_or(|c| c == r.collection)
                    && !r.deleted
                    && self
                        .engine
                        .tips
                        .get(&(r.did.clone(), r.collection.clone(), r.rkey.clone()))
                        == Some(&r.cid)
            })
            .map(|r| {
                let mut value = r.record.clone();
                value["cid"] = json!(r.cid);
                value["uri"] = json!(format!("tangled://{}/{}/{}", r.did, r.collection, r.rkey));
                value
            })
            .collect();
        rows.sort_by(|a, b| a["uri"].as_str().cmp(&b["uri"].as_str()));
        Ok(rows)
    }

    pub fn get(&mut self, collection: &str, rkey: &str) -> Result<Value> {
        self.list(Some(collection))?
            .into_iter()
            .find(|r| r["rkey"] == rkey)
            .ok_or_else(|| format!("Record not found: {collection}/{rkey}"))
    }

    pub fn put(
        &mut self,
        collection: &str,
        rkey: &str,
        body: Value,
        visibility: &str,
    ) -> Result<Value> {
        self.write(collection, rkey, body, visibility)
    }

    fn write(
        &mut self,
        collection: &str,
        rkey: &str,
        body: Value,
        visibility: &str,
    ) -> Result<Value> {
        if collection.is_empty()
            || collection.len() > 256
            || !collection
                .split('.')
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        {
            return Err("Invalid collection name".into());
        }
        if !tangled::identity::valid_rkey(rkey) {
            return Err("Invalid record key".into());
        }
        if !matches!(visibility, "cap" | "public") {
            return Err(
                "Personal records support cap or explicitly selected public visibility".into(),
            );
        }
        if !body.is_object() && !body.is_null() {
            return Err("Record body must be an object".into());
        }
        let did = self.did();
        let key = (did.clone(), collection.to_string(), rkey.to_string());
        let prev: Vec<_> = self.engine.tips.get(&key).cloned().into_iter().collect();
        let last_ts = self
            .engine
            .stored
            .iter()
            .filter(|r| r.did == did)
            .filter_map(|r| r.record["ts"].as_u64())
            .max()
            .unwrap_or(0);
        let ts = now().saturating_mul(1000).max(last_ts.saturating_add(1));
        let schema = json!({"$schema":"https://json-schema.org/draft/2020-12/schema","title":collection,"type":"object","additionalProperties":true});
        let schema_bytes = serde_json::to_vec(&schema).map_err(err)?;
        let schema_cid = self.put_blob(&schema_bytes, "application/schema+json")?;
        let unsigned = json!({"v":1,"type":schema_cid,"collection":collection,"rkey":rkey,"author":did,"ts":ts,"prev":prev,"vis":visibility,"body":body});
        let signed = tangled::record::sign_json_record(self.identity.signing_key(), &unsigned)
            .map_err(err)?;
        let response = self.request(
            Method::Post,
            "/xrpc/tangled.repo.putRecord",
            vec![],
            Some(serde_json::to_vec(&json!({"record":signed.json})).map_err(err)?),
            "repo/write",
        )?;
        response.error_if_not_ok().map_err(err)?;
        let mut out = signed.json;
        out["cid"] = json!(signed.cid.to_string());
        out["uri"] = json!(format!("tangled://{did}/{collection}/{rkey}"));
        Ok(out)
    }

    pub fn remove(&mut self, collection: &str, rkey: &str) -> Result<Value> {
        let existing = self.get(collection, rkey)?;
        self.write(
            collection,
            rkey,
            Value::Null,
            existing["vis"].as_str().unwrap_or("cap"),
        )
    }

    pub fn put_blob(&mut self, bytes: &[u8], mime: &str) -> Result<String> {
        if bytes.len() > 32 * 1024 * 1024 {
            return Err("Personal attachment exceeds 32 MiB".into());
        }
        if mime.len() > 256 || mime.contains(['\r', '\n']) {
            return Err("Invalid MIME type".into());
        }
        let did = self.did();
        let response = self.request(
            Method::Post,
            "/xrpc/tangled.repo.putBlob",
            vec![("did".into(), did), ("mediaType".into(), mime.into())],
            Some(bytes.to_vec()),
            "blob/put",
        )?;
        response.error_if_not_ok().map_err(err)?;
        let body: Value = serde_json::from_slice(&response.body).map_err(err)?;
        body["cid"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "Missing blob CID".into())
    }

    pub fn get_blob(&mut self, cid: &str) -> Result<Vec<u8>> {
        let did = self.did();
        let response = self.request(
            Method::Get,
            "/xrpc/tangled.sync.getBlob",
            vec![("did".into(), did), ("cid".into(), cid.into())],
            None,
            "blob/get",
        )?;
        response.error_if_not_ok().map_err(err)?;
        if Cid::raw(&response.body).to_string() != cid {
            return Err("Attachment CID mismatch".into());
        }
        Ok(response.body)
    }

    pub fn export(&mut self) -> Result<Vec<u8>> {
        let did = self.did();
        let response = self.request(
            Method::Post,
            "/xrpc/tangled.sync.getRepoAuth",
            vec![],
            Some(serde_json::to_vec(&json!({"did":did,"blobs":true})).map_err(err)?),
            "repo/read",
        )?;
        response.error_if_not_ok().map_err(err)?;
        Ok(response.body)
    }

    pub fn import(&mut self, bytes: &[u8]) -> Result<()> {
        let car = tangled::car::decode(bytes).map_err(err)?;
        if car.roots.len() != 1 {
            return Err("Backup must have exactly one signed repository root".into());
        }
        let root = &car.roots[0];
        let root_bytes = car.get(root).ok_or("Missing root commit")?;
        let info = tangled::record::parse_commit(root_bytes).map_err(err)?;
        if info.did != self.did() {
            return Err("Backup belongs to a different identity; restore its owner identity into a new directory first".into());
        }
        if let Some(head) = self.engine.heads.get(&info.did) {
            if head.commit == root.to_string() {
                return Ok(());
            }
            let mut cursor = Some(info.clone());
            let mut seen = std::collections::HashSet::new();
            let mut extends = false;
            while let Some(commit) = cursor.take() {
                let Some(previous) = commit.prev else {
                    break;
                };
                if previous == head.commit {
                    extends = true;
                    break;
                }
                if !seen.insert(previous.clone()) {
                    return Err("Cyclic commit history".into());
                }
                let cid = Cid::parse(&previous).map_err(err)?;
                let b = car
                    .get(&cid)
                    .or_else(|| self.engine.blocks.get(&previous).map(Vec::as_slice))
                    .ok_or("Incomplete commit history")?;
                cursor = Some(tangled::record::parse_commit(b).map_err(err)?);
            }
            if !extends {
                return Err("Backup would roll back or fork the current repository".into());
            }
        }
        // Admission is transactional: the existing engine import mutates before every
        // possible error has been checked, so stage it entirely in memory first.
        let mut candidate = self.engine.clone();
        candidate.data_dir = None;
        let request = self.make_request(
            Method::Post,
            "/xrpc/tangled.repo.import",
            vec![("force".into(), "true".into())],
            Some(bytes.to_vec()),
            "repo/write",
        )?;
        let response = candidate.handle(&request).map_err(err)?;
        response.error_if_not_ok().map_err(err)?;
        candidate.data_dir = self.path.clone();
        candidate.persist().map_err(err)?;
        self.engine = candidate;
        Ok(())
    }

    fn request(
        &mut self,
        method: Method,
        path: &str,
        query: Vec<(String, String)>,
        body: Option<Vec<u8>>,
        command: &str,
    ) -> Result<KnotResponse> {
        let req = self.make_request(method, path, query, body, command)?;
        self.engine.now = now();
        self.engine.handle(&req).map_err(err)
    }

    fn make_request(
        &self,
        method: Method,
        path: &str,
        query: Vec<(String, String)>,
        body: Option<Vec<u8>>,
        command: &str,
    ) -> Result<KnotRequest> {
        let time = now();
        let did = self.did();
        let resource = format!("tangled://{did}");
        let mut cap = Capability {
            v: 1,
            iss: did.clone(),
            aud: did.clone(),
            cmd: command.into(),
            res: resource.clone(),
            nbf: time.saturating_sub(1),
            exp: time + 3600,
            pol: vec![],
            prf: vec![],
            meta: None,
            sig: None,
        };
        cap.sign(self.identity.signing_key()).map_err(err)?;
        let audience = self
            .engine
            .describe
            .as_ref()
            .map(|d| d.did.clone())
            .unwrap_or_else(|| did.clone());
        let nonce: [u8; 16] =
            blake3::hash(format!("{path}:{time}:{}", tangled::identity::tid_now()).as_bytes())
                .as_bytes()[..16]
                .try_into()
                .map_err(err)?;
        let mut invocation =
            Invocation::new(&cap, command, &resource, &audience, time, nonce).map_err(err)?;
        invocation.sign(self.identity.signing_key()).map_err(err)?;
        Ok(KnotRequest {
            method,
            path: path.into(),
            query,
            headers: vec![(
                "authorization".into(),
                cap_authorization_header(&invocation.encode_cbor().map_err(err)?),
            )],
            body,
            content_type: Some("application/json".into()),
        })
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn seed(value: &Value, key: &str) -> Result<[u8; 32]> {
    hex::decode(value[key].as_str().ok_or("Identity seed missing")?)
        .map_err(err)?
        .try_into()
        .map_err(|_| "Identity seed must be 32 bytes".into())
}
fn private_dir(path: &Path) -> Result<()> {
    private_permissions(path, 0o700)
}
fn private_permissions(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(err)?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
}
fn private_file(path: &Path, create_new: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if create_new {
        options.create_new(true);
    } else {
        options.create(true).truncate(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(err)
}
