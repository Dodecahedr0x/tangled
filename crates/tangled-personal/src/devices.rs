//! Explicit encrypted file transfer between an owner's local installations.
use crate::store::replica::{BackupContents, MAX_CONTENT_BYTES};
use crate::{Result, Store};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
};
use tangled::{hpke, Identity};
use zeroize::Zeroizing;

const FORMAT: &str = "tangled.personal.recovery.v1";
const KDF: &str = "argon2id-v19-m19456-t2-p1";
const MAX_PACKET_BYTES: usize = MAX_CONTENT_BYTES * 2 + 4096;
#[derive(Serialize, Deserialize)]
struct Packet {
    format: String,
    owner: String,
    kdf: String,
    salt: String,
    enc: String,
    ct: String,
}

/// Passphrases arrive only in JSON input, never in output or command flags.
pub fn execute(store: &mut Store, input: Value) -> Result<Value> {
    let action = input["action"].as_str().ok_or("Missing devices action")?;
    match action {
        "status" => Ok(store.continuity_status()),
        "backup" => {
            let password = passphrase(&input)?;
            let path = field(&input, "output_path")?;
            let contents = store.backup_contents()?;
            let plaintext = Zeroizing::new(serde_json::to_vec(&contents).map_err(|_| "Cannot encode backup contents")?);
            if plaintext.len() > MAX_CONTENT_BYTES { return Err("Backup contents exceed 64 MiB".into()); }
            let mut salt = [0; 16];
            rand::thread_rng().fill_bytes(&mut salt);
            let key = key(password, &salt)?;
            let recipient = Identity::from_seeds([0;32], *key);
            let (enc, ct) = hpke::seal_record(&recipient.agreement_public_bytes(), &aad(&contents.owner)?, &plaintext).map_err(|_| "Cannot encrypt backup")?;
            let packet = Packet { format: FORMAT.into(), owner: contents.owner.clone(), kdf: KDF.into(), salt: hex::encode(salt), enc: hex::encode(enc), ct: hex::encode(ct) };
            let bytes = serde_json::to_vec(&packet).map_err(|_| "Cannot encode encrypted backup")?;
            if bytes.len() > MAX_PACKET_BYTES { return Err("Encrypted backup exceeds size limit".into()); }
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
            let mut file = options.open(path).map_err(|_| "Backup output must be a new writable file")?;
            file.write_all(&bytes).map_err(|_| "Cannot write encrypted backup")?;
            file.sync_all().map_err(|_| "Cannot persist encrypted backup")?;
            let mut status = store.continuity_status();
            status["output_path"] = json!(path);
            status["encrypted_bytes"] = json!(bytes.len());
            Ok(status)
        }
        "verify" | "restore" | "sync" => {
            let contents = open_packet(Path::new(field(&input,"input_path")?), passphrase(&input)?)?;
            match action {
                "verify" => Ok(Store::verify_contents(&contents)?.continuity_status()),
                "restore" => Store::restore_contents(&contents, Path::new(field(&input,"destination")?)),
                "sync" => store.sync_contents(&contents),
                _ => Err("Unsupported devices action".into()),
            }
        }
        _ => Err("Devices actions: status, backup, verify, restore, sync. Owner-key replicas cannot be individually revoked.".into()),
    }
}
fn field<'a>(input: &'a Value, name: &str) -> Result<&'a str> {
    let text = input[name]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Missing {name}"))?;
    if text.len() > 4096 {
        return Err(format!("{name} exceeds size limit"));
    }
    Ok(text)
}
fn passphrase(input: &Value) -> Result<&str> {
    let text = input["passphrase"]
        .as_str()
        .ok_or("Missing recovery passphrase")?;
    if !(12..=1024).contains(&text.len()) {
        return Err("Use a separate recovery passphrase of 12–1024 bytes".into());
    }
    Ok(text)
}
fn key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    if salt.len() != 16 {
        return Err("Invalid recovery salt length".into());
    }
    let params = Params::new(19456, 2, 1, Some(32)).map_err(|_| "Invalid recovery KDF")?;
    let mut key = Zeroizing::new([0; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut *key)
        .map_err(|_| "Cannot derive recovery key")?;
    Ok(key)
}
fn aad(owner: &str) -> Result<Vec<u8>> {
    serde_json::to_vec(&(FORMAT, KDF, owner)).map_err(|_| "Cannot encode recovery context".into())
}
fn open_packet(path: &Path, password: &str) -> Result<BackupContents> {
    let file = std::fs::File::open(path).map_err(|_| "Cannot open encrypted backup")?;
    if file
        .metadata()
        .map_err(|_| "Cannot inspect encrypted backup")?
        .len()
        > MAX_PACKET_BYTES as u64
    {
        return Err("Encrypted backup exceeds size limit".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_PACKET_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read encrypted backup")?;
    if bytes.len() > MAX_PACKET_BYTES {
        return Err("Encrypted backup exceeds size limit".into());
    }
    let packet: Packet =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid encrypted backup format")?;
    if packet.format != FORMAT
        || packet.kdf != KDF
        || packet.owner.len() > 256
        || packet.enc.len() != 64
        || packet.salt.len() != 32
        || packet.ct.len() > (MAX_CONTENT_BYTES + 16) * 2
    {
        return Err("Unsupported or oversized encrypted backup".into());
    }
    let salt = hex::decode(packet.salt).map_err(|_| "Invalid recovery salt")?;
    let key = key(password, &salt)?;
    let enc = hex::decode(packet.enc).map_err(|_| "Invalid recovery encapsulation")?;
    let ct = hex::decode(packet.ct).map_err(|_| "Invalid recovery ciphertext")?;
    let plaintext = Zeroizing::new(
        hpke::open_record(&key, &enc, &aad(&packet.owner)?, &ct)
            .map_err(|_| "Incorrect recovery passphrase or damaged backup")?,
    );
    if plaintext.len() > MAX_CONTENT_BYTES {
        return Err("Backup contents exceed 64 MiB".into());
    }
    let contents: BackupContents =
        serde_json::from_slice(&plaintext).map_err(|_| "Invalid backup contents")?;
    if contents.owner != packet.owner {
        return Err("Backup owner mismatch".into());
    }
    Ok(contents)
}
