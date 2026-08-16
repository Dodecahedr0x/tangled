//! Seed store: Ed25519 + X25519, never derived from each other.
//!
//! Import JSON (`from_solana_json`):
//! - `{ "ed25519": "<64 hex>", "x25519": "<64 hex>" }` (what [`LoomedKeys::export_json`] writes)
//! - `{ "ed25519": [u8;32], "x25519": [u8;32] }`
//! - `{ "secretKey": [u8;64], "x25519": "<hex>" | [u8;32] }` — Solana keypair array (first 32 bytes are the Ed25519 seed) plus a separate X25519 seed
//!
//! A bare Solana `[u8;64]` keypair is rejected: it has no X25519 seed, and Loomed must not convert Ed25519→X25519.

use std::path::PathBuf;

use crate::error::{LoomedError, Result};
use tangled::{Did, Identity};

#[derive(Clone)]
pub struct LoomedKeys {
    ed25519: [u8; 32],
    x25519: [u8; 32],
}

impl LoomedKeys {
    pub fn generate() -> Self {
        let id = Identity::generate();
        let (ed25519, x25519) = id.to_seeds();
        Self { ed25519, x25519 }
    }

    pub fn from_seeds(ed25519: [u8; 32], x25519: [u8; 32]) -> Self {
        Self { ed25519, x25519 }
    }

    pub fn from_solana_json(json: &str) -> Result<Self> {
        let v: serde_json::Value =
            serde_json::from_str(json).map_err(|e| LoomedError::Identity(format!("json: {e}")))?;
        if v.is_array() {
            return Err(LoomedError::Identity(
                "bare Solana keypair JSON has no X25519 seed; use {\"ed25519\",\"x25519\"} hex or {\"secretKey\":[64],\"x25519\"}"
                    .into(),
            ));
        }
        let obj = v
            .as_object()
            .ok_or_else(|| LoomedError::Identity("expected object".into()))?;
        let ed = obj
            .get("ed25519")
            .or_else(|| obj.get("secretKey"))
            .ok_or_else(|| LoomedError::Identity("missing ed25519/secretKey".into()))?;
        let x = obj
            .get("x25519")
            .ok_or_else(|| LoomedError::Identity("missing x25519".into()))?;
        Ok(Self::from_seeds(parse_seed(ed)?, parse_seed(x)?))
    }

    pub fn identity(&self) -> Identity {
        Identity::from_seeds(self.ed25519, self.x25519)
    }

    pub fn did(&self) -> Did {
        self.identity().did()
    }

    pub fn export_json(&self) -> String {
        serde_json::json!({
            "ed25519": hex::encode(self.ed25519),
            "x25519": hex::encode(self.x25519),
        })
        .to_string()
    }
}

fn parse_seed(v: &serde_json::Value) -> Result<[u8; 32]> {
    match v {
        serde_json::Value::String(s) => {
            let bytes =
                hex::decode(s.trim()).map_err(|e| LoomedError::Identity(format!("hex: {e}")))?;
            <[u8; 32]>::try_from(bytes)
                .map_err(|_| LoomedError::Identity("seed must be 32 bytes".into()))
        }
        serde_json::Value::Array(a) => {
            let mut bytes = Vec::with_capacity(a.len());
            for x in a {
                let n = x
                    .as_u64()
                    .ok_or_else(|| LoomedError::Identity("seed byte not a number".into()))?;
                bytes.push(
                    u8::try_from(n)
                        .map_err(|_| LoomedError::Identity("seed byte out of range".into()))?,
                );
            }
            if bytes.len() == 64 {
                let mut out = [0u8; 32];
                out.copy_from_slice(&bytes[..32]);
                return Ok(out);
            }
            <[u8; 32]>::try_from(bytes)
                .map_err(|_| LoomedError::Identity("seed must be 32 bytes".into()))
        }
        _ => Err(LoomedError::Identity(
            "seed must be hex or byte array".into(),
        )),
    }
}

pub trait KeyStore: Send + Sync {
    fn save(&self, keys: &LoomedKeys) -> Result<()>;
    fn load(&self) -> Result<Option<LoomedKeys>>;
}

pub struct FileKeyStore {
    pub path: PathBuf,
}

impl KeyStore for FileKeyStore {
    fn save(&self, keys: &LoomedKeys) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = keys.export_json();
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&self.path)?;
            f.write_all(json.as_bytes())?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&self.path, json)?;
        }
        Ok(())
    }

    fn load(&self) -> Result<Option<LoomedKeys>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let s = std::fs::read_to_string(&self.path)?;
        Ok(Some(LoomedKeys::from_solana_json(&s)?))
    }
}
