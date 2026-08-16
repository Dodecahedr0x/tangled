//! HPKE RFC 9180 base mode: DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305.
//!
//! Also epoch-application AEAD and sealed MST rkey wrapping (spec 05 / 07).

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::constants::{
    EPOCH_AEAD_SALT, EPOCH_NONCE_SALT, HPKE_AEAD_ID, HPKE_INFO_EPOCH, HPKE_INFO_RECORD,
    HPKE_KDF_ID, HPKE_KEM_ID, SEALED_RKEY_INFO,
};
use crate::error::{Error, Result};

const HPKE_VERSION: &[u8] = b"HPKE-v1";
const NSECRET: usize = 32;
const NK: usize = 32;
const NN: usize = 12;
const NH: usize = 32;

fn kem_suite_id() -> [u8; 5] {
    let mut s = [0u8; 5];
    s[..3].copy_from_slice(b"KEM");
    s[3..].copy_from_slice(&HPKE_KEM_ID.to_be_bytes());
    s
}

fn hpke_suite_id() -> [u8; 10] {
    let mut s = [0u8; 10];
    s[..4].copy_from_slice(b"HPKE");
    s[4..6].copy_from_slice(&HPKE_KEM_ID.to_be_bytes());
    s[6..8].copy_from_slice(&HPKE_KDF_ID.to_be_bytes());
    s[8..10].copy_from_slice(&HPKE_AEAD_ID.to_be_bytes());
    s
}

fn labeled_extract(salt: &[u8], label: &[u8], ikm: &[u8], suite_id: &[u8]) -> [u8; 32] {
    let mut labeled_ikm =
        Vec::with_capacity(HPKE_VERSION.len() + suite_id.len() + label.len() + ikm.len());
    labeled_ikm.extend_from_slice(HPKE_VERSION);
    labeled_ikm.extend_from_slice(suite_id);
    labeled_ikm.extend_from_slice(label);
    labeled_ikm.extend_from_slice(ikm);
    let (prk, _) = Hkdf::<Sha256>::extract(Some(salt), &labeled_ikm);
    let mut out = [0u8; 32];
    out.copy_from_slice(&prk);
    out
}

fn labeled_expand(
    prk: &[u8],
    label: &[u8],
    info: &[u8],
    l: usize,
    suite_id: &[u8],
) -> Result<Vec<u8>> {
    let hk = Hkdf::<Sha256>::from_prk(prk).map_err(|_| Error::Hpke("HKDF prk".into()))?;
    let mut labeled_info = Vec::new();
    labeled_info.extend_from_slice(&(l as u16).to_be_bytes());
    labeled_info.extend_from_slice(HPKE_VERSION);
    labeled_info.extend_from_slice(suite_id);
    labeled_info.extend_from_slice(label);
    labeled_info.extend_from_slice(info);
    let mut out = vec![0u8; l];
    hk.expand(&labeled_info, &mut out)
        .map_err(|_| Error::Hpke("HKDF expand".into()))?;
    Ok(out)
}

/// Raw X25519 DH (clamped). Used to check `hpke-sealed.json`.
pub fn x25519_shared(sk: &[u8; 32], pk: &[u8; 32]) -> [u8; 32] {
    let secret = StaticSecret::from(*sk);
    let public = PublicKey::from(*pk);
    secret.diffie_hellman(&public).to_bytes()
}

fn dhkem_encap_with(sk_e: &[u8; 32], pk_r: &[u8; 32]) -> Result<([u8; 32], [u8; 32])> {
    let sk = StaticSecret::from(*sk_e);
    let pk_e = PublicKey::from(&sk);
    let enc = pk_e.to_bytes();
    let dh = sk.diffie_hellman(&PublicKey::from(*pk_r)).to_bytes();
    let kem_context = {
        let mut c = Vec::with_capacity(64);
        c.extend_from_slice(&enc);
        c.extend_from_slice(pk_r);
        c
    };
    let eae_prk = labeled_extract(b"", b"eae_prk", &dh, &kem_suite_id());
    let shared = labeled_expand(
        &eae_prk,
        b"shared_secret",
        &kem_context,
        NSECRET,
        &kem_suite_id(),
    )?;
    let mut ss = [0u8; 32];
    ss.copy_from_slice(&shared);
    Ok((ss, enc))
}

fn dhkem_decap(sk_r: &[u8; 32], enc: &[u8; 32]) -> Result<[u8; 32]> {
    let sk = StaticSecret::from(*sk_r);
    let pk_r = PublicKey::from(&sk).to_bytes();
    let dh = sk.diffie_hellman(&PublicKey::from(*enc)).to_bytes();
    let kem_context = {
        let mut c = Vec::with_capacity(64);
        c.extend_from_slice(enc);
        c.extend_from_slice(&pk_r);
        c
    };
    let eae_prk = labeled_extract(b"", b"eae_prk", &dh, &kem_suite_id());
    let shared = labeled_expand(
        &eae_prk,
        b"shared_secret",
        &kem_context,
        NSECRET,
        &kem_suite_id(),
    )?;
    let mut ss = [0u8; 32];
    ss.copy_from_slice(&shared);
    Ok(ss)
}

fn key_schedule_base(shared_secret: &[u8; 32], info: &[u8]) -> Result<([u8; NK], [u8; NN])> {
    let suite = hpke_suite_id();
    let psk_id_hash = labeled_extract(b"", b"psk_id_hash", b"", &suite);
    let info_hash = labeled_extract(b"", b"info_hash", info, &suite);
    let mut key_schedule_context = Vec::with_capacity(1 + NH + NH);
    key_schedule_context.push(0x00); // mode base
    key_schedule_context.extend_from_slice(&psk_id_hash);
    key_schedule_context.extend_from_slice(&info_hash);
    let secret = labeled_extract(shared_secret, b"secret", b"", &suite);
    let key = labeled_expand(&secret, b"key", &key_schedule_context, NK, &suite)?;
    let base_nonce = labeled_expand(&secret, b"base_nonce", &key_schedule_context, NN, &suite)?;
    let mut k = [0u8; NK];
    let mut n = [0u8; NN];
    k.copy_from_slice(&key);
    n.copy_from_slice(&base_nonce);
    Ok((k, n))
}

fn aead_seal(key: &[u8; NK], nonce: &[u8; NN], aad: &[u8], pt: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .encrypt(Nonce::from_slice(nonce), Payload { msg: pt, aad })
        .map_err(|_| Error::Hpke("aead seal".into()))
}

fn aead_open(key: &[u8; NK], nonce: &[u8; NN], aad: &[u8], ct: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad })
        .map_err(|_| Error::Hpke("aead open".into()))
}

/// HPKE single-shot base-mode seal. Returns `(enc, ciphertext)`.
pub fn seal(
    pk_r: &[u8; 32],
    info: &[u8],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<(Vec<u8>, Vec<u8>)> {
    let sk_e = StaticSecret::random_from_rng(rand::rngs::OsRng);
    let sk_bytes = sk_e.to_bytes();
    let (ss, enc) = dhkem_encap_with(&sk_bytes, pk_r)?;
    let (key, nonce) = key_schedule_base(&ss, info)?;
    let ct = aead_seal(&key, &nonce, aad, plaintext)?;
    Ok((enc.to_vec(), ct))
}

/// HPKE single-shot base-mode seal with a caller-supplied ephemeral X25519 seed.
pub fn seal_with_ephemeral(
    sk_e: &[u8; 32],
    pk_r: &[u8; 32],
    info: &[u8],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<(Vec<u8>, Vec<u8>)> {
    let (ss, enc) = dhkem_encap_with(sk_e, pk_r)?;
    let (key, nonce) = key_schedule_base(&ss, info)?;
    let ct = aead_seal(&key, &nonce, aad, plaintext)?;
    Ok((enc.to_vec(), ct))
}

/// HPKE single-shot base-mode open.
pub fn open(
    sk_r: &[u8; 32],
    enc: &[u8],
    info: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    let enc: [u8; 32] = enc
        .try_into()
        .map_err(|_| Error::Hpke("enc must be 32 bytes".into()))?;
    let ss = dhkem_decap(sk_r, &enc)?;
    let (key, nonce) = key_schedule_base(&ss, info)?;
    aead_open(&key, &nonce, aad, ciphertext)
}

/// 1:1 sealed-record wrap (`info = tangled.hpke.record.v1`).
pub fn seal_record(pk_r: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    seal(pk_r, HPKE_INFO_RECORD, aad, plaintext)
}

/// Open a 1:1 sealed record.
pub fn open_record(sk_r: &[u8; 32], enc: &[u8], aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    open(sk_r, enc, HPKE_INFO_RECORD, aad, ciphertext)
}

/// Epoch-secret wrap AAD: `circle_did || uint64be(N)`.
pub fn epoch_wrap_aad(circle_did: &str, epoch: u64) -> Vec<u8> {
    let mut aad = circle_did.as_bytes().to_vec();
    aad.extend_from_slice(&epoch.to_be_bytes());
    aad
}

/// HPKE-wrap `epoch_secret[N]` to a member agreement key.
pub fn wrap_epoch_secret(
    pk_r: &[u8; 32],
    circle_did: &str,
    epoch: u64,
    epoch_secret: &[u8; 32],
) -> Result<(Vec<u8>, Vec<u8>)> {
    let aad = epoch_wrap_aad(circle_did, epoch);
    seal(pk_r, HPKE_INFO_EPOCH, &aad, epoch_secret)
}

/// Open an epoch-secret wrap.
pub fn unwrap_epoch_secret(
    sk_r: &[u8; 32],
    enc: &[u8],
    circle_did: &str,
    epoch: u64,
    ciphertext: &[u8],
) -> Result<[u8; 32]> {
    let aad = epoch_wrap_aad(circle_did, epoch);
    let pt = open(sk_r, enc, HPKE_INFO_EPOCH, &aad, ciphertext)?;
    pt.try_into()
        .map_err(|_| Error::Hpke("epoch secret must be 32 bytes".into()))
}

fn hkdf_extract_expand(salt: &[u8], ikm: &[u8], info: &[u8], l: usize) -> Result<Vec<u8>> {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut out = vec![0u8; l];
    hk.expand(info, &mut out)
        .map_err(|_| Error::Hpke("epoch HKDF expand".into()))?;
    Ok(out)
}

/// Derive epoch application AEAD key (32) and nonce base (12).
pub fn derive_epoch_aead(
    epoch_secret: &[u8; 32],
    circle_did: &str,
) -> Result<([u8; 32], [u8; 12])> {
    let key_v = hkdf_extract_expand(EPOCH_AEAD_SALT, epoch_secret, circle_did.as_bytes(), 32)?;
    let nonce_v = hkdf_extract_expand(EPOCH_NONCE_SALT, epoch_secret, circle_did.as_bytes(), 12)?;
    let mut key = [0u8; 32];
    let mut nonce = [0u8; 12];
    key.copy_from_slice(&key_v);
    nonce.copy_from_slice(&nonce_v);
    Ok((key, nonce))
}

/// Record AEAD nonce = `nonce_base XOR first 12 bytes of BLAKE3(unsigned without body)`.
pub fn record_nonce(nonce_base: &[u8; 12], unsigned_without_body: &[u8]) -> [u8; 12] {
    let digest = blake3::hash(unsigned_without_body);
    let mut n = [0u8; 12];
    for i in 0..12 {
        n[i] = nonce_base[i] ^ digest.as_bytes()[i];
    }
    n
}

/// Encrypt a circle application body under `epoch_secret[N]`.
pub fn epoch_aead_seal(
    epoch_secret: &[u8; 32],
    circle_did: &str,
    unsigned_without_body: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let (key, nonce_base) = derive_epoch_aead(epoch_secret, circle_did)?;
    let nonce = record_nonce(&nonce_base, unsigned_without_body);
    aead_seal(&key, &nonce, b"", plaintext)
}

/// Decrypt a circle application body.
pub fn epoch_aead_open(
    epoch_secret: &[u8; 32],
    circle_did: &str,
    unsigned_without_body: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    let (key, nonce_base) = derive_epoch_aead(epoch_secret, circle_did)?;
    let nonce = record_nonce(&nonce_base, unsigned_without_body);
    aead_open(&key, &nonce, b"", ciphertext)
}

/// Encrypted MST rkey: hex of ChaCha20-Poly1305 output truncated to 32 bytes.
pub fn encrypt_rkey(epoch_secret: &[u8; 32], rkey: &str) -> Result<String> {
    let key_v = hkdf_extract_expand(&[], epoch_secret, SEALED_RKEY_INFO, 32)?;
    let mut key = [0u8; 32];
    key.copy_from_slice(&key_v);
    let nonce = [0u8; 12];
    let ct = aead_seal(&key, &nonce, b"", rkey.as_bytes())?;
    let take = ct.len().min(32);
    Ok(hex::encode(&ct[..take]))
}

/// Birational map Ed25519 public key → X25519 (libsodium `crypto_sign_ed25519_pk_to_curve25519`).
pub fn ed25519_pk_to_x25519(ed_pk: &[u8; 32]) -> Result<[u8; 32]> {
    use curve25519_dalek::edwards::CompressedEdwardsY;
    let point = CompressedEdwardsY(*ed_pk)
        .decompress()
        .ok_or_else(|| Error::crypto("invalid Ed25519 public key"))?;
    Ok(point.to_montgomery().to_bytes())
}

/// ECDH that MUST reject using the identity Ed25519 key as the X25519 peer.
pub fn x25519_shared_checked(
    sk: &[u8; 32],
    pk: &[u8; 32],
    identity_ed25519: &[u8; 32],
) -> Result<[u8; 32]> {
    let forbidden = ed25519_pk_to_x25519(identity_ed25519)?;
    if pk == &forbidden {
        return Err(Error::crypto(
            "MUST NOT perform ECDH with the Ed25519 identity key",
        ));
    }
    Ok(x25519_shared(sk, pk))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_record() {
        let sk_r = StaticSecret::random_from_rng(rand::rngs::OsRng);
        let pk_r = PublicKey::from(&sk_r).to_bytes();
        let pt = b"secret body";
        let aad = b"unsigned-without-body";
        let (enc, ct) = seal_record(&pk_r, aad, pt).unwrap();
        let out = open_record(&sk_r.to_bytes(), &enc, aad, &ct).unwrap();
        assert_eq!(out, pt);
    }

    #[test]
    fn epoch_aead_roundtrip() {
        let secret = [7u8; 32];
        let did = "did:sol:abc#circ";
        let unsigned = b"no-body";
        let pt = b"hello before grow";
        let ct = epoch_aead_seal(&secret, did, unsigned, pt).unwrap();
        let out = epoch_aead_open(&secret, did, unsigned, &ct).unwrap();
        assert_eq!(out, pt);
    }

    #[test]
    fn reject_ecdh_of_identity_key() {
        use crate::identity::Identity;
        let id = Identity::generate();
        let converted = ed25519_pk_to_x25519(&id.pubkey_bytes()).unwrap();
        assert_ne!(converted, id.agreement_public_bytes());
        let err = x25519_shared_checked(
            &id.agreement_secret().to_bytes(),
            &converted,
            &id.pubkey_bytes(),
        );
        assert!(err.is_err());
        let ok = x25519_shared_checked(
            &id.agreement_secret().to_bytes(),
            &id.agreement_public_bytes(),
            &id.pubkey_bytes(),
        );
        assert!(ok.is_ok());
    }

    #[test]
    fn sealed_rkey_is_hex_and_stable() {
        let secret = [9u8; 32];
        let a = encrypt_rkey(&secret, "chat1").unwrap();
        let b = encrypt_rkey(&secret, "chat1").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len() % 2, 0);
        assert!(a.len() <= 64);
        assert_ne!(a, encrypt_rkey(&secret, "chat2").unwrap());
    }
}
