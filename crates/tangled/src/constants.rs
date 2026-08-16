//! Normative constants from spec chapter 05 and 06. Implementations MUST use
//! these values; they are not interop-breaking settings.

/// MLS grow threshold (`CIRCLE_MLS_THRESHOLD`).
pub const CIRCLE_MLS_THRESHOLD: usize = 16;
/// Dual-stack epoch skew (`DUAL_STACK_MAX_SKEW_EPOCHS`).
pub const DUAL_STACK_MAX_SKEW_EPOCHS: u64 = 8;
/// Dual-stack time skew (`DUAL_STACK_MAX_SKEW_SECONDS`) — 72 hours.
pub const DUAL_STACK_MAX_SKEW_SECONDS: u64 = 259_200;
/// Agreement-rotation wrap window — 7 days.
pub const AGREEMENT_ROTATION_MAX_SKEW_SECONDS: u64 = 604_800;
/// Recovery-head serving delay — 72 hours.
pub const RECOVERY_DELAY_SECONDS: u64 = 259_200;
/// Maximum capability TTL — 365 days.
pub const CAPABILITY_MAX_TTL_SECONDS: u64 = 31_536_000;
/// Offline verifier remaining TTL unless a revocation MST is cached.
pub const OFFLINE_MAX_REMAINING: u64 = 7_200;
/// Clock-skew window for capability `nbf` / `exp` / `valid_until` (seconds).
pub const CAPABILITY_CLOCK_SKEW_SECONDS: u64 = 60;

/// Visible page text cap (Unicode scalars).
pub const PAGE_TEXT_MAX_SCALARS: usize = 1_048_576;
/// RGA atom cap including tombstones.
pub const PAGE_RGA_MAX_ATOMS: usize = 4_194_304;
/// Page title cap (Unicode scalars).
pub const PAGE_TITLE_MAX_SCALARS: usize = 512;
/// Knot admission: minimum incremental ops a compact SHOULD fold.
pub const PAGE_COMPACT_MIN_OPS: usize = 32;
/// Knot admission: minimum interval between compacts (ms).
pub const PAGE_COMPACT_MIN_INTERVAL_MS: u64 = 300_000;

/// HPKE info for 1:1 sealed records.
pub const HPKE_INFO_RECORD: &[u8] = b"tangled.hpke.record.v1";
/// HPKE info for epoch-secret wraps.
pub const HPKE_INFO_EPOCH: &[u8] = b"tangled.hpke.epoch.v1";
/// HKDF salt for epoch application AEAD key.
pub const EPOCH_AEAD_SALT: &[u8] = b"tangled.epoch.aead.v1";
/// HKDF salt for epoch nonce base.
pub const EPOCH_NONCE_SALT: &[u8] = b"tangled.epoch.nonce.v1";
/// HKDF info for sealed MST rkey encryption.
pub const SEALED_RKEY_INFO: &[u8] = b"tangled.rkey";
/// MLS application ID.
pub const MLS_APPLICATION_ID: &[u8] = b"tangled.circle.v1";
/// MLS exporter label (`tangled.epoch`).
pub const MLS_EXPORTER_LABEL: &[u8] = b"tangled.epoch";
/// MLS ciphersuite `0x0001`.
pub const MLS_CIPHERSUITE: u16 = 0x0001;
/// HPKE KEM `DHKEM(X25519, HKDF-SHA256)`.
pub const HPKE_KEM_ID: u16 = 0x0020;
/// HPKE KDF `HKDF-SHA256`.
pub const HPKE_KDF_ID: u16 = 0x0001;
/// HPKE AEAD `ChaCha20Poly1305`.
pub const HPKE_AEAD_ID: u16 = 0x0003;

/// rkey charset (spec 06).
pub const RKEY_PATTERN: &str = r"^[A-Za-z0-9._~:!-]{1,256}$";
