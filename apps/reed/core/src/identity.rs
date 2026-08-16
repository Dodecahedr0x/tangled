//! Seed store: Ed25519 + X25519, never derived from each other.

use std::path::PathBuf;

use crate::error::{ReedError, Result};
use tangled::{Did, Identity};

#[derive(Clone)]
pub struct ReedKeys {
    ed25519: [u8; 32],
    x25519: [u8; 32],
}

impl ReedKeys {
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
            serde_json::from_str(json).map_err(|e| ReedError::Identity(format!("json: {e}")))?;
        if v.is_array() {
            return Err(ReedError::Identity(
                "bare Solana keypair JSON has no X25519 seed; use {\"ed25519\",\"x25519\"} hex or {\"secretKey\":[64],\"x25519\"}"
                    .into(),
            ));
        }
        let obj = v
            .as_object()
            .ok_or_else(|| ReedError::Identity("expected object".into()))?;
        let ed = obj
            .get("ed25519")
            .or_else(|| obj.get("secretKey"))
            .ok_or_else(|| ReedError::Identity("missing ed25519/secretKey".into()))?;
        let x = obj
            .get("x25519")
            .ok_or_else(|| ReedError::Identity("missing x25519".into()))?;
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
                hex::decode(s.trim()).map_err(|e| ReedError::Identity(format!("hex: {e}")))?;
            <[u8; 32]>::try_from(bytes)
                .map_err(|_| ReedError::Identity("seed must be 32 bytes".into()))
        }
        serde_json::Value::Array(a) => {
            let mut bytes = Vec::with_capacity(a.len());
            for x in a {
                let n = x
                    .as_u64()
                    .ok_or_else(|| ReedError::Identity("seed byte not a number".into()))?;
                bytes.push(
                    u8::try_from(n)
                        .map_err(|_| ReedError::Identity("seed byte out of range".into()))?,
                );
            }
            if bytes.len() == 64 {
                let mut out = [0u8; 32];
                out.copy_from_slice(&bytes[..32]);
                return Ok(out);
            }
            <[u8; 32]>::try_from(bytes)
                .map_err(|_| ReedError::Identity("seed must be 32 bytes".into()))
        }
        _ => Err(ReedError::Identity("seed must be hex or byte array".into())),
    }
}

pub trait KeyStore: Send + Sync {
    fn save(&self, keys: &ReedKeys) -> Result<()>;
    fn load(&self) -> Result<Option<ReedKeys>>;
}

pub struct FileKeyStore {
    pub path: PathBuf,
}

impl KeyStore for FileKeyStore {
    fn save(&self, keys: &ReedKeys) -> Result<()> {
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

    fn load(&self) -> Result<Option<ReedKeys>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let s = std::fs::read_to_string(&self.path)?;
        Ok(Some(ReedKeys::from_solana_json(&s)?))
    }
}
