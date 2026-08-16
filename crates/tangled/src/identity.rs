//! `did:sol` identity: Ed25519 identity key + a **separate** X25519 agreement key.

use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use x25519_dalek::{PublicKey as XPublic, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{Error, Result};

const DID_PREFIX: &str = "did:sol:";
const DEVNET: &str = "did:sol:devnet:";
const TESTNET: &str = "did:sol:testnet:";

/// Solana cluster encoded in a DID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cluster {
    /// Mainnet-beta (no cluster prefix).
    Mainnet,
    Devnet,
    Testnet,
}

/// Parsed `did:sol` identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Did {
    pub cluster: Cluster,
    pub pubkey: [u8; 32],
}

impl Did {
    pub fn mainnet(pubkey: [u8; 32]) -> Self {
        Self {
            cluster: Cluster::Mainnet,
            pubkey,
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        let (cluster, rest) = if let Some(r) = s.strip_prefix(DEVNET) {
            (Cluster::Devnet, r)
        } else if let Some(r) = s.strip_prefix(TESTNET) {
            (Cluster::Testnet, r)
        } else if let Some(r) = s.strip_prefix(DID_PREFIX) {
            (Cluster::Mainnet, r)
        } else {
            return Err(Error::InvalidDid(format!("not a did:sol DID: {s}")));
        };
        let pubkey = decode_pubkey_base58(rest)?;
        Ok(Self { cluster, pubkey })
    }

    pub fn as_str(&self) -> String {
        let b58 = bs58::encode(self.pubkey).into_string();
        match self.cluster {
            Cluster::Mainnet => format!("{DID_PREFIX}{b58}"),
            Cluster::Devnet => format!("{DEVNET}{b58}"),
            Cluster::Testnet => format!("{TESTNET}{b58}"),
        }
    }

    pub fn pubkey_base58(&self) -> String {
        bs58::encode(self.pubkey).into_string()
    }
}

impl std::fmt::Display for Did {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.as_str())
    }
}

impl std::str::FromStr for Did {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

/// Decode Bitcoin-alphabet base58 to a 32-byte public key.
pub fn decode_pubkey_base58(s: &str) -> Result<[u8; 32]> {
    let bytes = bs58::decode(s)
        .into_vec()
        .map_err(|e| Error::InvalidDid(format!("base58: {e}")))?;
    bytes
        .try_into()
        .map_err(|_| Error::InvalidDid("public key must be 32 bytes".into()))
}

/// Encode a 32-byte public key as base58.
pub fn encode_pubkey_base58(pk: &[u8; 32]) -> String {
    bs58::encode(pk).into_string()
}

/// Format `did:sol:<base58>` (mainnet).
pub fn did_sol(pk: &[u8; 32]) -> String {
    format!("{DID_PREFIX}{}", encode_pubkey_base58(pk))
}

/// Identity keypair plus a distinct X25519 agreement key.
///
/// Implementations MUST NOT convert the Ed25519 identity key to X25519 for DH.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Identity {
    #[zeroize(skip)]
    signing: SigningKey,
    agreement: StaticSecret,
}

impl Identity {
    /// Generate a fresh identity (OsRng).
    pub fn generate() -> Self {
        let mut rng = OsRng;
        let signing = SigningKey::generate(&mut rng);
        let agreement = StaticSecret::random_from_rng(rng);
        Self { signing, agreement }
    }

    /// Construct from a 32-byte Ed25519 seed and a 32-byte X25519 scalar.
    pub fn from_seeds(ed25519_seed: [u8; 32], x25519_seed: [u8; 32]) -> Self {
        Self {
            signing: SigningKey::from_bytes(&ed25519_seed),
            agreement: StaticSecret::from(x25519_seed),
        }
    }

    pub fn signing_key(&self) -> &SigningKey {
        &self.signing
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing.verifying_key()
    }

    pub fn pubkey_bytes(&self) -> [u8; 32] {
        self.verifying_key().to_bytes()
    }

    pub fn agreement_secret(&self) -> &StaticSecret {
        &self.agreement
    }

    pub fn agreement_public(&self) -> XPublic {
        XPublic::from(&self.agreement)
    }

    pub fn agreement_public_bytes(&self) -> [u8; 32] {
        self.agreement_public().to_bytes()
    }

    pub fn did(&self) -> Did {
        Did::mainnet(self.pubkey_bytes())
    }

    pub fn did_string(&self) -> String {
        self.did().as_str()
    }

    /// Ed25519 seed and X25519 scalar for durable storage.
    pub fn to_seeds(&self) -> ([u8; 32], [u8; 32]) {
        (self.signing.to_bytes(), self.agreement.to_bytes())
    }
}

/// Generative `did:sol` document (spec 04). Sole verification method is the key.
/// MUST NOT invent `keyAgreement` (that would require deriving X25519 from Ed25519).
pub fn generative_did_document(did: &Did) -> serde_json::Value {
    let id = did.as_str();
    let vm = format!("{id}#key");
    serde_json::json!({
        "id": id,
        "verificationMethod": [{
            "id": vm,
            "type": "Ed25519VerificationKey2020",
            "controller": id,
            "publicKeyBase58": encode_pubkey_base58(&did.pubkey),
        }],
        "authentication": [vm.clone()],
        "capabilityInvocation": [vm],
    })
}

/// Operational DID document: generative methods plus `keyAgreement` from `sys/id`.
///
/// `agreement` is the distinct X25519 public key. Encoding is
/// `X25519KeyAgreementKey2020` + `publicKeyBase58` (same style as the Ed25519 method).
pub fn operational_did_document(did: &Did, agreement: &[u8; 32]) -> serde_json::Value {
    let mut doc = generative_did_document(did);
    let id = did.as_str();
    let agree_id = format!("{id}#agreement");
    if let Some(vms) = doc
        .get_mut("verificationMethod")
        .and_then(|v| v.as_array_mut())
    {
        vms.push(serde_json::json!({
            "id": agree_id,
            "type": "X25519KeyAgreementKey2020",
            "controller": id,
            "publicKeyBase58": encode_pubkey_base58(agreement),
        }));
    }
    doc["keyAgreement"] = serde_json::json!([agree_id]);
    doc
}

/// Generative document when `agreement` is absent; operational when `sys/id` supplied it.
pub fn resolve_did_document(did: &Did, agreement: Option<&[u8; 32]>) -> serde_json::Value {
    match agreement {
        Some(ag) => operational_did_document(did, ag),
        None => generative_did_document(did),
    }
}

/// Parse a 32-byte key from `sys/id` JSON (`hex` or base58).
pub fn parse_sys_id_key32(v: &serde_json::Value) -> Option<[u8; 32]> {
    let s = v.as_str()?;
    if let Ok(b) = hex::decode(s) {
        if let Ok(pk) = <[u8; 32]>::try_from(b.as_slice()) {
            return Some(pk);
        }
    }
    decode_pubkey_base58(s).ok()
}

/// TID: 13-char Crockford base32 of `(unix_micros << 10) | entropy10`.
pub fn tid_from_parts(unix_micros: u64, entropy10: u16) -> String {
    let n = (unix_micros << 10) | u64::from(entropy10 & 0x3ff);
    crockford13(n)
}

/// Generate a TID from wall clock + random entropy.
pub fn tid_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);
    let entropy = rand::random::<u16>() & 0x3ff;
    tid_from_parts(micros, entropy)
}

const CROCKFORD: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

fn crockford13(mut n: u64) -> String {
    let mut buf = [b'0'; 13];
    for i in (0..13).rev() {
        buf[i] = CROCKFORD[(n & 31) as usize];
        n >>= 5;
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Decode a 13-char Crockford TID to the integer `(unix_micros << 10) | entropy10`.
pub fn tid_decode(s: &str) -> Option<u64> {
    if s.len() != 13 {
        return None;
    }
    let mut n = 0u64;
    for c in s.bytes() {
        let v = CROCKFORD.iter().position(|&x| x == c)? as u64;
        n = n.checked_shl(5)? | v;
    }
    Some(n)
}

/// Validate `rkey` against spec 06 charset.
pub fn valid_rkey(rkey: &str) -> bool {
    if rkey.is_empty() || rkey.len() > 256 {
        return false;
    }
    rkey.chars()
        .all(|c| matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '.' | '_' | '~' | ':' | '!' | '-'))
}

/// Page slug rules (spec 06): no leading/trailing `.`, no `..`.
pub fn valid_page_slug(rkey: &str) -> bool {
    if !valid_rkey(rkey) {
        return false;
    }
    if rkey.starts_with('.') || rkey.ends_with('.') || rkey.contains("..") {
        return false;
    }
    rkey.split('.').all(|seg| {
        !seg.is_empty()
            && seg.chars().all(
                |c| matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '~' | ':' | '!' | '-'),
            )
    })
}

/// Display slug `a.b.c` as `a/b/c`.
pub fn slug_display(rkey: &str) -> String {
    rkey.replace('.', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn did_roundtrip() {
        let id = Identity::generate();
        let s = id.did_string();
        let parsed = Did::parse(&s).unwrap();
        assert_eq!(parsed.pubkey, id.pubkey_bytes());
        assert_eq!(parsed.cluster, Cluster::Mainnet);
    }

    #[test]
    fn agreement_is_not_ed25519() {
        let id = Identity::generate();
        assert_ne!(id.pubkey_bytes(), id.agreement_public_bytes());
    }

    #[test]
    fn rkey_and_slug() {
        assert!(valid_rkey("Home"));
        assert!(valid_page_slug("rfc.draft"));
        assert!(!valid_page_slug(".hidden"));
        assert_eq!(slug_display("rfc.draft"), "rfc/draft");
    }

    #[test]
    fn cluster_dids_roundtrip() {
        let id = Identity::from_seeds([7u8; 32], [8u8; 32]);
        let main = id.did_string();
        assert!(main.starts_with("did:sol:"));
        assert!(!main.starts_with("did:sol:devnet:"));
        let pk = id.pubkey_bytes();
        let b58 = encode_pubkey_base58(&pk);
        let dev = Did::parse(&format!("did:sol:devnet:{b58}")).unwrap();
        assert_eq!(dev.cluster, Cluster::Devnet);
        assert_eq!(dev.as_str(), format!("did:sol:devnet:{b58}"));
        let test = Did::parse(&format!("did:sol:testnet:{b58}")).unwrap();
        assert_eq!(test.cluster, Cluster::Testnet);
        assert!(Did::parse("did:web:example.com").is_err());
        assert!(Did::parse("did:sol:not-valid-base58!!!").is_err());
        let doc = generative_did_document(&dev);
        assert_eq!(doc["id"], dev.as_str());
        assert_eq!(doc["authentication"][0], format!("{}#key", dev.as_str()));
        assert_eq!(
            doc["capabilityInvocation"][0],
            format!("{}#key", dev.as_str())
        );
        assert_eq!(doc["verificationMethod"].as_array().unwrap().len(), 1);
        assert!(doc.get("keyAgreement").is_none());
    }

    #[test]
    fn generative_has_no_key_agreement() {
        let id = Identity::from_seeds([3u8; 32], [4u8; 32]);
        let doc = generative_did_document(&id.did());
        assert!(doc.get("keyAgreement").is_none());
        assert_eq!(doc["verificationMethod"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn operational_includes_sys_id_agreement() {
        let id = Identity::from_seeds([3u8; 32], [4u8; 32]);
        let doc = resolve_did_document(&id.did(), Some(&id.agreement_public_bytes()));
        assert_eq!(
            doc["keyAgreement"][0],
            format!("{}#agreement", id.did_string())
        );
        let vm = doc["verificationMethod"].as_array().unwrap();
        assert_eq!(vm.len(), 2);
        assert_eq!(vm[1]["type"], "X25519KeyAgreementKey2020");
        assert_eq!(
            vm[1]["publicKeyBase58"],
            encode_pubkey_base58(&id.agreement_public_bytes())
        );
        assert_ne!(
            vm[1]["publicKeyBase58"],
            encode_pubkey_base58(&id.pubkey_bytes())
        );
        let gen = resolve_did_document(&id.did(), None);
        assert!(gen.get("keyAgreement").is_none());
    }

    #[test]
    fn tid_crockford_length() {
        let t = tid_from_parts(1_700_000_000_000_000, 42);
        assert_eq!(t.len(), 13);
        assert!(t
            .chars()
            .all(|c| "0123456789abcdefghjkmnpqrstvwxyz".contains(c)));
        assert_eq!(tid_decode(&t), Some((1_700_000_000_000_000u64 << 10) | 42));
    }
}
