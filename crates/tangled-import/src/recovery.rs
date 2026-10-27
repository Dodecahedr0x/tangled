//! A recovery file is independent of the original provider's custody keys.
use crate::ArchiveBackup;
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tangled::{hpke, identity::Identity};

#[derive(Serialize, Deserialize)]
pub struct RecoveryBundle {
    pub version: u32,
    pub owner: String,
    pub ed25519: String,
    pub x25519: String,
    pub archive: ArchiveBackup,
}
impl RecoveryBundle {
    pub fn identity(&self) -> Result<Identity, String> {
        let ed = hex::decode(&self.ed25519)
            .map_err(|_| "Invalid recovery signing key")?
            .try_into()
            .map_err(|_| "Invalid recovery signing key length")?;
        let x = hex::decode(&self.x25519)
            .map_err(|_| "Invalid recovery agreement key")?
            .try_into()
            .map_err(|_| "Invalid recovery agreement key length")?;
        if self.version != 1 || self.owner != self.archive.owner {
            return Err("Invalid recovery owner/version".into());
        }
        Ok(Identity::from_seeds(ed, x))
    }
}
fn key(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    if password.len() < 12 || password.len() > 1024 || salt.len() != 16 {
        return Err("Use a separate recovery passphrase of 12–1024 bytes".into());
    }
    let params = Params::new(19_456, 2, 1, Some(32)).map_err(|e| e.to_string())?;
    let mut key = [0; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| e.to_string())?;
    Ok(key)
}
fn aad(owner: &str) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&("tangled.import.recovery.v1", owner)).map_err(|e| e.to_string())
}
pub fn recovery_export(
    owner: &str,
    id: &Identity,
    archive: &ArchiveBackup,
    password: &str,
) -> Result<Value, String> {
    if archive.owner != owner {
        return Err("Recovery account mismatch".into());
    }
    let mut salt = [0; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let derived = key(password, &salt)?;
    let recipient = Identity::from_seeds([0; 32], derived);
    let (ed, x) = id.to_seeds();
    let bundle = RecoveryBundle {
        version: 1,
        owner: owner.into(),
        ed25519: hex::encode(ed),
        x25519: hex::encode(x),
        archive: archive.clone(),
    };
    let plaintext = serde_json::to_vec(&bundle).map_err(|e| e.to_string())?;
    let (enc, ct) = hpke::seal_record(
        &recipient.agreement_public_bytes(),
        &aad(owner)?,
        &plaintext,
    )
    .map_err(|e| e.to_string())?;
    Ok(
        json!({"v":1,"owner":owner,"kdf":"argon2id-v19-m19456-t2-p1","salt":hex::encode(salt),"enc":hex::encode(enc),"ct":hex::encode(ct)}),
    )
}
pub fn recovery_open(packet: &Value, password: &str) -> Result<RecoveryBundle, String> {
    if packet["v"] != 1 || packet["kdf"] != "argon2id-v19-m19456-t2-p1" {
        return Err("Unsupported recovery format".into());
    }
    let owner = packet["owner"].as_str().ok_or("Missing recovery owner")?;
    let salt = hex::decode(packet["salt"].as_str().ok_or("Missing recovery salt")?)
        .map_err(|_| "Invalid recovery salt")?;
    let derived = key(password, &salt)?;
    let enc = hex::decode(
        packet["enc"]
            .as_str()
            .ok_or("Missing recovery encapsulation")?,
    )
    .map_err(|_| "Invalid recovery encapsulation")?;
    let ct = hex::decode(packet["ct"].as_str().ok_or("Missing recovery ciphertext")?)
        .map_err(|_| "Invalid recovery ciphertext")?;
    let plaintext = hpke::open_record(&derived, &enc, &aad(owner)?, &ct)
        .map_err(|_| "Incorrect recovery passphrase or damaged backup")?;
    let bundle: RecoveryBundle =
        serde_json::from_slice(&plaintext).map_err(|_| "Invalid recovery contents")?;
    if bundle.owner != owner {
        return Err("Recovery owner mismatch".into());
    }
    bundle.identity()?;
    Ok(bundle)
}
