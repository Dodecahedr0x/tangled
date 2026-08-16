//! Solana off-chain message envelope and Ed25519 sign/verify (spec 05).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use crate::error::{Error, Result};

/// Header: `0xff || "solana offchain"` (16 bytes).
const HEADER: &[u8] = b"\xffsolana offchain";
const VERSION: u8 = 0x00;
const FORMAT: u8 = 0x00; // restricted ASCII
const BODY_LEN: u16 = 64;

/// Build the off-chain message for a 32-byte BLAKE3 digest.
///
/// `body` is lowercase hex of the digest (64 ASCII bytes).
pub fn offchain_message(digest32: &[u8; 32]) -> [u8; 84] {
    let mut msg = [0u8; 84];
    msg[..16].copy_from_slice(HEADER);
    msg[16] = VERSION;
    msg[17] = FORMAT;
    msg[18] = (BODY_LEN & 0xff) as u8;
    msg[19] = (BODY_LEN >> 8) as u8;
    let hex = hex::encode(digest32);
    debug_assert_eq!(hex.len(), 64);
    msg[20..].copy_from_slice(hex.as_bytes());
    msg
}

/// BLAKE3-256 of unsigned DAG-CBOR, then the off-chain envelope.
pub fn envelope_for_unsigned(unsigned_cbor: &[u8]) -> ([u8; 32], [u8; 84]) {
    let digest = *blake3::hash(unsigned_cbor).as_bytes();
    let msg = offchain_message(&digest);
    (digest, msg)
}

/// Sign unsigned DAG-CBOR with the Solana off-chain envelope.
pub fn sign_unsigned(sk: &SigningKey, unsigned_cbor: &[u8]) -> [u8; 64] {
    let (_digest, msg) = envelope_for_unsigned(unsigned_cbor);
    sk.sign(&msg).to_bytes()
}

/// Verify an off-chain envelope signature over unsigned DAG-CBOR.
pub fn verify_unsigned(pk: &VerifyingKey, unsigned_cbor: &[u8], sig: &[u8; 64]) -> Result<()> {
    let (_digest, msg) = envelope_for_unsigned(unsigned_cbor);
    let signature = Signature::from_bytes(sig);
    pk.verify(&msg, &signature)
        .map_err(|_| Error::InvalidSignature)
}

/// Verify using a raw 32-byte public key.
pub fn verify_unsigned_raw(pk: &[u8; 32], unsigned_cbor: &[u8], sig: &[u8; 64]) -> Result<()> {
    let vk = VerifyingKey::from_bytes(pk).map_err(|_| Error::InvalidSignature)?;
    verify_unsigned(&vk, unsigned_cbor, sig)
}

/// Sign arbitrary envelope bytes (already constructed).
pub fn sign_message(sk: &SigningKey, message: &[u8]) -> [u8; 64] {
    sk.sign(message).to_bytes()
}

/// Parse a 64-byte signature from hex.
pub fn sig_from_hex(s: &str) -> Result<[u8; 64]> {
    let bytes = hex::decode(s.trim()).map_err(|e| Error::crypto(format!("sig hex: {e}")))?;
    bytes
        .try_into()
        .map_err(|_| Error::crypto("signature must be 64 bytes"))
}

/// Parse a 32-byte key from hex.
pub fn key32_from_hex(s: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(s.trim()).map_err(|e| Error::crypto(format!("key hex: {e}")))?;
    bytes
        .try_into()
        .map_err(|_| Error::crypto("key must be 32 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_layout() {
        let d = [0u8; 32];
        let msg = offchain_message(&d);
        assert_eq!(&msg[..16], HEADER);
        assert_eq!(msg[16], 0);
        assert_eq!(msg[17], 0);
        assert_eq!(u16::from_le_bytes([msg[18], msg[19]]), 64);
        assert_eq!(&msg[20..], hex::encode(d).as_bytes());
    }
}
