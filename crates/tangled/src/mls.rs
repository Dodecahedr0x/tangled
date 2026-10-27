//! RFC 9420 MLS via OpenMLS. Ciphersuite `0x0001`.
//!
//! Credential identity is the 32-byte Solana public key. Exporter label
//! `tangled.epoch` with context `uint64be(N)` is `epoch_secret[N]`.
//!
//! Spec 10: the MLS leaf HPKE key is the member's Tangled X25519 `agreement`
//! key. OpenMLS 0.7 has no public setter for that key, so we inject it through
//! `OpenMlsCrypto::derive_hpke_keypair` on the leaf derivation (skipping the
//! KeyPackage init key, which RFC 9420 requires to differ). Welcome is sealed
//! to the init key; path secrets and HPKE wraps use agreement.

use std::sync::Mutex;

use openmls::prelude::tls_codec::{Deserialize, SecretVLBytes, Serialize};
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::{OpenMlsRustCrypto, RustCrypto};
use x25519_dalek::{PublicKey as XPublic, StaticSecret};

use crate::constants::{MLS_APPLICATION_ID, MLS_CIPHERSUITE, MLS_EXPORTER_LABEL};
use crate::error::{Error, Result};
use crate::identity::Identity;

const SUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

struct LeafInject {
    sk: [u8; 32],
    pk: [u8; 32],
    /// `Some(n)`: skip `n` `derive_hpke_keypair` calls, then inject once.
    pending: Option<u32>,
}

struct LeafHpkeCrypto {
    inner: RustCrypto,
    inject: Mutex<LeafInject>,
}

/// OpenMLS provider that can pin the next leaf HPKE key to Tangled agreement.
struct AgreementProvider {
    crypto: LeafHpkeCrypto,
    inner: OpenMlsRustCrypto,
}

impl AgreementProvider {
    fn new(agreement_sk: [u8; 32]) -> Self {
        let pk = XPublic::from(&StaticSecret::from(agreement_sk)).to_bytes();
        Self {
            crypto: LeafHpkeCrypto {
                inner: RustCrypto::default(),
                inject: Mutex::new(LeafInject {
                    sk: agreement_sk,
                    pk,
                    pending: None,
                }),
            },
            inner: OpenMlsRustCrypto::default(),
        }
    }

    fn arm_leaf(&self, skip: u32) {
        self.crypto
            .inject
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending = Some(skip);
    }
}

impl OpenMlsCrypto for LeafHpkeCrypto {
    fn supports(&self, ciphersuite: Ciphersuite) -> std::result::Result<(), CryptoError> {
        self.inner.supports(ciphersuite)
    }

    fn supported_ciphersuites(&self) -> Vec<Ciphersuite> {
        self.inner.supported_ciphersuites()
    }

    fn hkdf_extract(
        &self,
        hash_type: HashType,
        salt: &[u8],
        ikm: &[u8],
    ) -> std::result::Result<SecretVLBytes, CryptoError> {
        self.inner.hkdf_extract(hash_type, salt, ikm)
    }

    fn hmac(
        &self,
        hash_type: HashType,
        key: &[u8],
        message: &[u8],
    ) -> std::result::Result<SecretVLBytes, CryptoError> {
        self.inner.hmac(hash_type, key, message)
    }

    fn hkdf_expand(
        &self,
        hash_type: HashType,
        prk: &[u8],
        info: &[u8],
        okm_len: usize,
    ) -> std::result::Result<SecretVLBytes, CryptoError> {
        self.inner.hkdf_expand(hash_type, prk, info, okm_len)
    }

    fn hash(&self, hash_type: HashType, data: &[u8]) -> std::result::Result<Vec<u8>, CryptoError> {
        self.inner.hash(hash_type, data)
    }

    fn aead_encrypt(
        &self,
        alg: AeadType,
        key: &[u8],
        data: &[u8],
        nonce: &[u8],
        aad: &[u8],
    ) -> std::result::Result<Vec<u8>, CryptoError> {
        self.inner.aead_encrypt(alg, key, data, nonce, aad)
    }

    fn aead_decrypt(
        &self,
        alg: AeadType,
        key: &[u8],
        ct_tag: &[u8],
        nonce: &[u8],
        aad: &[u8],
    ) -> std::result::Result<Vec<u8>, CryptoError> {
        self.inner.aead_decrypt(alg, key, ct_tag, nonce, aad)
    }

    fn signature_key_gen(
        &self,
        alg: SignatureScheme,
    ) -> std::result::Result<(Vec<u8>, Vec<u8>), CryptoError> {
        self.inner.signature_key_gen(alg)
    }

    fn verify_signature(
        &self,
        alg: SignatureScheme,
        data: &[u8],
        pk: &[u8],
        signature: &[u8],
    ) -> std::result::Result<(), CryptoError> {
        self.inner.verify_signature(alg, data, pk, signature)
    }

    fn sign(
        &self,
        alg: SignatureScheme,
        data: &[u8],
        key: &[u8],
    ) -> std::result::Result<Vec<u8>, CryptoError> {
        self.inner.sign(alg, data, key)
    }

    fn hpke_seal(
        &self,
        config: HpkeConfig,
        pk_r: &[u8],
        info: &[u8],
        aad: &[u8],
        ptxt: &[u8],
    ) -> std::result::Result<HpkeCiphertext, CryptoError> {
        self.inner.hpke_seal(config, pk_r, info, aad, ptxt)
    }

    fn hpke_open(
        &self,
        config: HpkeConfig,
        input: &HpkeCiphertext,
        sk_r: &[u8],
        info: &[u8],
        aad: &[u8],
    ) -> std::result::Result<Vec<u8>, CryptoError> {
        self.inner.hpke_open(config, input, sk_r, info, aad)
    }

    fn hpke_setup_sender_and_export(
        &self,
        config: HpkeConfig,
        pk_r: &[u8],
        info: &[u8],
        exporter_context: &[u8],
        exporter_length: usize,
    ) -> std::result::Result<(KemOutput, ExporterSecret), CryptoError> {
        self.inner.hpke_setup_sender_and_export(
            config,
            pk_r,
            info,
            exporter_context,
            exporter_length,
        )
    }

    fn hpke_setup_receiver_and_export(
        &self,
        config: HpkeConfig,
        enc: &[u8],
        sk_r: &[u8],
        info: &[u8],
        exporter_context: &[u8],
        exporter_length: usize,
    ) -> std::result::Result<ExporterSecret, CryptoError> {
        self.inner.hpke_setup_receiver_and_export(
            config,
            enc,
            sk_r,
            info,
            exporter_context,
            exporter_length,
        )
    }

    fn derive_hpke_keypair(
        &self,
        config: HpkeConfig,
        ikm: &[u8],
    ) -> std::result::Result<HpkeKeyPair, CryptoError> {
        let mut g = self.inject.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = g.pending {
            if n > 0 {
                g.pending = Some(n - 1);
                drop(g);
                return self.inner.derive_hpke_keypair(config, ikm);
            }
            g.pending = None;
            let pair = HpkeKeyPair {
                private: g.sk.to_vec().into(),
                public: g.pk.to_vec(),
            };
            return Ok(pair);
        }
        drop(g);
        self.inner.derive_hpke_keypair(config, ikm)
    }
}

impl OpenMlsProvider for AgreementProvider {
    type CryptoProvider = LeafHpkeCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = openmls_rust_crypto::MemoryStorage;

    fn storage(&self) -> &Self::StorageProvider {
        self.inner.storage()
    }

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        self.inner.rand()
    }
}

/// One MLS participant (owner or member).
pub struct MlsParty {
    pub pubkey: [u8; 32],
    agreement_sk: [u8; 32],
    agreement_pk: [u8; 32],
    provider: AgreementProvider,
    signer: SignatureKeyPair,
    credential_with_key: CredentialWithKey,
    key_package: KeyPackage,
}

/// Owner (or current committer) group state.
pub struct MlsSession {
    group: MlsGroup,
    provider: AgreementProvider,
    signer: SignatureKeyPair,
}

/// Welcome + Commit bytes plus the exporter secret for this epoch.
pub struct MlsAddOut {
    pub welcome: Vec<u8>,
    pub commit: Vec<u8>,
    pub exporter: [u8; 32],
    pub group_context: Vec<u8>,
}

fn mls_err(e: impl std::fmt::Debug) -> Error {
    Error::Circle(format!("mls: {e:?}"))
}

fn create_config() -> Result<MlsGroupCreateConfig> {
    debug_assert_eq!(SUITE as u16, MLS_CIPHERSUITE);
    Ok(MlsGroupCreateConfig::builder()
        .ciphersuite(SUITE)
        .use_ratchet_tree_extension(true)
        .with_group_context_extensions(Extensions::single(Extension::Unknown(
            0xf001,
            UnknownExtension(MLS_APPLICATION_ID.to_vec()),
        )))
        .map_err(mls_err)?
        .build())
}

fn vl_payload32(bytes: &[u8]) -> Result<[u8; 32]> {
    let n = *bytes
        .first()
        .ok_or_else(|| Error::Circle("empty HPKE key".into()))? as usize;
    if n != 32 || bytes.len() < 33 {
        return Err(Error::Circle("HPKE key is not 32 bytes".into()));
    }
    bytes[1..33]
        .try_into()
        .map_err(|_| Error::Circle("HPKE key is not 32 bytes".into()))
}

impl MlsParty {
    /// `pubkey` is the 32-byte Solana identity key (MLS basic credential).
    /// A fresh X25519 agreement key is generated and used as the leaf HPKE key.
    pub fn new(pubkey: [u8; 32]) -> Result<Self> {
        let agreement = StaticSecret::random_from_rng(rand::rngs::OsRng);
        Self::with_agreement(pubkey, agreement.to_bytes())
    }

    /// Leaf HPKE key is `agreement_sk`'s X25519 public key (spec 10).
    pub fn with_agreement(pubkey: [u8; 32], agreement_sk: [u8; 32]) -> Result<Self> {
        let agreement_pk = XPublic::from(&StaticSecret::from(agreement_sk)).to_bytes();
        let provider = AgreementProvider::new(agreement_sk);
        let credential = BasicCredential::new(pubkey.to_vec());
        let signer = SignatureKeyPair::new(SUITE.signature_algorithm()).map_err(mls_err)?;
        signer.store(provider.storage()).map_err(mls_err)?;
        let credential_with_key = CredentialWithKey {
            credential: credential.into(),
            signature_key: signer.to_public_vec().into(),
        };
        // KeyPackage::create derives init_key first, then the leaf encryption key.
        provider.arm_leaf(1);
        let bundle = KeyPackage::builder()
            .build(SUITE, &provider, &signer, credential_with_key.clone())
            .map_err(mls_err)?;
        Ok(Self {
            pubkey,
            agreement_sk,
            agreement_pk,
            provider,
            signer,
            credential_with_key,
            key_package: bundle.key_package().clone(),
        })
    }

    pub fn from_identity(id: &Identity) -> Result<Self> {
        Self::with_agreement(id.pubkey_bytes(), id.agreement_secret().to_bytes())
    }

    pub fn agreement_public(&self) -> [u8; 32] {
        self.agreement_pk
    }

    /// Leaf encryption public key from the KeyPackage (spec 10 = agreement).
    pub fn leaf_hpke_public(&self) -> Result<[u8; 32]> {
        let bytes = self
            .key_package
            .leaf_node()
            .encryption_key()
            .tls_serialize_detached()
            .map_err(mls_err)?;
        vl_payload32(&bytes)
    }

    /// RFC 9420 init key (Welcome target). MUST differ from the leaf HPKE key.
    pub fn init_hpke_public(&self) -> [u8; 32] {
        let s = self.key_package.hpke_init_key().as_slice();
        let mut out = [0u8; 32];
        out.copy_from_slice(&s[..32]);
        out
    }

    /// TLS-serialized KeyPackage for `sys.mls.keypackage` (spec 21.5).
    pub fn key_package_bytes(&self) -> Result<Vec<u8>> {
        self.key_package.tls_serialize_detached().map_err(mls_err)
    }
}

/// Body of a `sys.mls.keypackage` record.
pub fn keypackage_record_body(payload: &[u8], exp: u64) -> serde_json::Value {
    serde_json::json!({
        "type": "keypackage",
        "exp": exp,
        "payload": hex::encode(payload),
    })
}

impl MlsSession {
    pub fn create(owner: &MlsParty) -> Result<Self> {
        let provider = AgreementProvider::new(owner.agreement_sk);
        owner.signer.store(provider.storage()).map_err(mls_err)?;
        let config = create_config()?;
        // MlsGroup::new's first HPKE keypair is the creator leaf.
        provider.arm_leaf(0);
        let group = MlsGroup::new(
            &provider,
            &owner.signer,
            &config,
            owner.credential_with_key.clone(),
        )
        .map_err(mls_err)?;
        Ok(Self {
            group,
            provider,
            signer: owner.signer.clone(),
        })
    }

    pub fn add(&mut self, member: &MlsParty, circle_epoch: u64) -> Result<MlsAddOut> {
        self.add_key_package(&member.key_package_bytes()?, circle_epoch)
    }

    /// Add from a published KeyPackage (offline member; spec 21.5).
    pub fn add_key_package(&mut self, key_package: &[u8], circle_epoch: u64) -> Result<MlsAddOut> {
        let kp_in = KeyPackageIn::tls_deserialize(&mut &*key_package).map_err(mls_err)?;
        let kp = kp_in
            .validate(self.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(mls_err)?;
        let (commit, welcome, _info) = self
            .group
            .add_members(&self.provider, &self.signer, std::slice::from_ref(&kp))
            .map_err(mls_err)?;
        self.group
            .merge_pending_commit(&self.provider)
            .map_err(mls_err)?;
        let exporter = self.exporter(circle_epoch)?;
        Ok(MlsAddOut {
            welcome: tls_serialize(&welcome)?,
            commit: tls_serialize(&commit)?,
            exporter,
            group_context: self.group.group_id().as_slice().to_vec(),
        })
    }

    pub fn exporter(&self, epoch: u64) -> Result<[u8; 32]> {
        let ctx = epoch.to_be_bytes();
        let v = self
            .group
            .export_secret(
                self.provider.crypto(),
                std::str::from_utf8(MLS_EXPORTER_LABEL).unwrap(),
                &ctx,
                32,
            )
            .map_err(mls_err)?;
        v.try_into()
            .map_err(|_| Error::Circle("mls exporter length".into()))
    }

    /// Leaf HPKE public key for the member whose MLS credential is `pubkey`.
    pub fn member_leaf_hpke(&self, pubkey: &[u8; 32]) -> Option<[u8; 32]> {
        self.group.members().find_map(|m| {
            if m.credential.serialized_content() == pubkey {
                m.encryption_key.as_slice().try_into().ok()
            } else {
                None
            }
        })
    }
}

/// Process a Welcome (ratchet tree in the extension). Consumes the party so
/// the KeyPackage secrets stay on the same OpenMLS provider.
pub fn join_welcome(member: MlsParty, welcome_bytes: &[u8]) -> Result<MlsSession> {
    let msg = MlsMessageIn::tls_deserialize(&mut &*welcome_bytes).map_err(mls_err)?;
    let welcome = match msg.extract() {
        MlsMessageBodyIn::Welcome(w) => w,
        _ => return Err(Error::Circle("not a Welcome".into())),
    };
    let join = MlsGroupJoinConfig::builder()
        .use_ratchet_tree_extension(true)
        .build();
    let staged =
        StagedWelcome::new_from_welcome(&member.provider, &join, welcome, None).map_err(mls_err)?;
    let group = staged.into_group(&member.provider).map_err(mls_err)?;
    Ok(MlsSession {
        group,
        provider: member.provider,
        signer: member.signer,
    })
}

fn tls_serialize(msg: &MlsMessageOut) -> Result<Vec<u8>> {
    msg.tls_serialize_detached().map_err(mls_err)
}

pub fn ciphersuite_is_spec() -> bool {
    (SUITE as u16) == MLS_CIPHERSUITE && MLS_APPLICATION_ID == b"tangled.circle.v1"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circle::{CircleState, Distributor};
    use crate::constants::CIRCLE_MLS_THRESHOLD;
    use crate::hpke;

    #[test]
    fn create_add_welcome_exporter() {
        let owner_pk = [1u8; 32];
        let member_pk = [2u8; 32];
        let owner = MlsParty::new(owner_pk).unwrap();
        let member = MlsParty::new(member_pk).unwrap();
        let mut sess = MlsSession::create(&owner).unwrap();
        assert_eq!(owner.pubkey, owner_pk);
        let out = sess.add(&member, 1).unwrap();
        assert!(!out.welcome.is_empty());
        assert!(!out.commit.is_empty());
        let joined = join_welcome(member, &out.welcome).unwrap();
        assert_eq!(joined.exporter(1).unwrap(), out.exporter);
        assert_eq!(sess.exporter(1).unwrap(), out.exporter);
        assert!(ciphersuite_is_spec());
    }

    #[test]
    fn add_from_published_keypackage_bytes() {
        let owner = MlsParty::new([1u8; 32]).unwrap();
        let member = MlsParty::new([2u8; 32]).unwrap();
        let bytes = member.key_package_bytes().unwrap();
        let body = keypackage_record_body(&bytes, 1_700_000_000 + 86_400);
        assert_eq!(body["type"], "keypackage");
        let mut sess = MlsSession::create(&owner).unwrap();
        let out = sess.add_key_package(&bytes, 1).unwrap();
        let joined = join_welcome(member, &out.welcome).unwrap();
        assert_eq!(joined.exporter(1).unwrap(), out.exporter);
    }

    #[test]
    fn leaf_hpke_is_agreement_and_hpke_wrap_target() {
        let id = Identity::generate();
        let party = MlsParty::from_identity(&id).unwrap();
        let leaf = party.leaf_hpke_public().unwrap();
        let agree = id.agreement_public_bytes();
        assert_eq!(leaf, agree);
        assert_eq!(party.agreement_public(), agree);
        assert_ne!(party.init_hpke_public(), agree);

        let secret = [9u8; 32];
        let (enc, ct) = hpke::wrap_epoch_secret(&agree, "did:sol:c#r", 1, &secret).unwrap();
        let opened = hpke::unwrap_epoch_secret(
            &id.agreement_secret().to_bytes(),
            &enc,
            "did:sol:c#r",
            1,
            &ct,
        )
        .unwrap();
        assert_eq!(opened, secret);

        let owner = MlsParty::from_identity(&Identity::generate()).unwrap();
        let mut sess = MlsSession::create(&owner).unwrap();
        let out = sess.add(&party, 3).unwrap();
        assert_eq!(sess.member_leaf_hpke(&id.pubkey_bytes()).unwrap(), agree);
        let joined = join_welcome(party, &out.welcome).unwrap();
        assert_eq!(joined.member_leaf_hpke(&id.pubkey_bytes()).unwrap(), agree);
        assert_eq!(joined.exporter(3).unwrap(), out.exporter);
    }

    #[test]
    fn grow_past_threshold_welcome_and_never_downgrade() {
        let owner_id = [0u8; 32];
        let owner = MlsParty::new(owner_id).unwrap();
        let mut sess = MlsSession::create(&owner).unwrap();
        let mut st =
            CircleState::create("did:sol:owner".into(), "c1".into(), "g".into(), [9u8; 32]);
        st.members = (0..CIRCLE_MLS_THRESHOLD)
            .map(|i| format!("did:sol:{i}"))
            .collect();

        let new_pk = [17u8; 32];
        let newbie = MlsParty::new(new_pk).unwrap();
        let out = sess.add(&newbie, 16).unwrap();
        let plan = st
            .add_member("did:sol:17".into(), out.exporter, 1_000)
            .unwrap();
        assert!(matches!(
            plan,
            crate::circle::MembershipPlan::SeamlessGrow { .. }
        ));
        assert_eq!(st.distributor, Distributor::Dual);
        let joined = join_welcome(newbie, &out.welcome).unwrap();
        assert_eq!(joined.exporter(16).unwrap(), out.exporter);
        st.remove_member("did:sol:17", [3u8; 32]).unwrap();
        assert_eq!(st.distributor, Distributor::Mls);
        assert_ne!(st.distributor, Distributor::Hpke);
    }

    #[test]
    fn wrapping_vector_is_not_live_rfc9420_bytes() {
        let v: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/spec/vectors/mls-welcome-commit.json"
        ))
        .unwrap();
        let welcome = hex::decode(v["welcome_record"]["payload_hex"].as_str().unwrap()).unwrap();
        assert_eq!(welcome.len(), 32, "wrapping-only stand-in, not TLS Welcome");
        assert_eq!(v["ciphersuite"], "0x0001");
        assert_eq!(v["exporter_label"], "tangled.epoch");
        let owner = MlsParty::new([1u8; 32]).unwrap();
        let member = MlsParty::new([2u8; 32]).unwrap();
        let mut sess = MlsSession::create(&owner).unwrap();
        let out = sess.add(&member, 1).unwrap();
        assert!(out.welcome.len() > 32);
        assert_ne!(out.welcome, welcome);
    }

    #[test]
    fn dual_stack_window_accepts_skew() {
        let mut st = CircleState::create("did:sol:o".into(), "c".into(), "n".into(), [1u8; 32]);
        st.members = (0..16).map(|i| format!("did:sol:{i}")).collect();
        st.add_member("did:sol:17".into(), [2u8; 32], 1000).unwrap();
        assert!(st.accept_send_epoch(st.epoch));
        assert!(st.accept_send_epoch(st.epoch.saturating_sub(1)));
        assert!(!st.accept_send_epoch(st.epoch + 1));
    }
}
