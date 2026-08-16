//! Live SNS JSON-RPC against `solana-test-validator` (not the inject map, not mainnet).
//!
//! Unit tests in this file run without a validator. Tests marked `#[ignore]` require:
//!
//! ```text
//! scripts/sns-validator.sh --bg
//! SOLANA_RPC=http://127.0.0.1:8899 cargo test -p tangled --test sns_rpc -- --ignored
//! ```
//!
//! CI starts the same validator with the same fixtures under `fixtures/sns/`.

use std::path::PathBuf;

use tangled::handle::{
    live_sns, lookup_handle, parse_sns_account, resolve_handle, sns_name_account, HandleResolver,
    SNS_NAME_PROGRAM, SNS_ROOT_DOMAIN,
};
use tangled::identity::{decode_pubkey_base58, encode_pubkey_base58, Did};

const ALICE_OWNER: [u8; 32] = [9u8; 32];
const ALICE_TANGLED_PK: [u8; 32] = [1u8; 32];
const ALICE_HEAD: &str = "bafkheadci";
const BOB_OWNER: [u8; 32] = [7u8; 32];

fn alice_did() -> String {
    Did::mainnet(ALICE_TANGLED_PK).as_str()
}

fn bob_did() -> String {
    Did::mainnet(BOB_OWNER).as_str()
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/sns")
}

fn sns_account_bytes(owner: &[u8; 32], payload: &str) -> Vec<u8> {
    let parent = decode_pubkey_base58(SNS_ROOT_DOMAIN).expect("SNS root");
    let mut raw = vec![0u8; 96];
    raw[0..32].copy_from_slice(&parent);
    raw[32..64].copy_from_slice(owner);
    raw.extend_from_slice(payload.as_bytes());
    raw
}

fn cli_account_json(pubkey: &str, data: &[u8]) -> serde_json::Value {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(data);
    serde_json::json!({
        "pubkey": pubkey,
        "account": {
            "lamports": 1_000_000_000u64,
            "data": [b64, "base64"],
            "owner": SNS_NAME_PROGRAM,
            "executable": false,
            "rentEpoch": 0,
            "space": data.len() as u64,
        }
    })
}

fn alice_payload() -> String {
    serde_json::json!({
        "tangled": alice_did(),
        "head": ALICE_HEAD,
    })
    .to_string()
}

fn alice_cli_account() -> serde_json::Value {
    let pda = sns_name_account("alice").expect("alice pda");
    let data = sns_account_bytes(&ALICE_OWNER, &alice_payload());
    cli_account_json(&encode_pubkey_base58(&pda), &data)
}

fn bob_cli_account() -> serde_json::Value {
    let pda = sns_name_account("bob").expect("bob pda");
    let data = sns_account_bytes(&BOB_OWNER, "");
    cli_account_json(&encode_pubkey_base58(&pda), &data)
}

fn load_fixture(name: &str) -> serde_json::Value {
    let path = fixture_dir().join(name);
    let s = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing SNS fixture {} ({e}); regenerate with WRITE_SNS_FIXTURES=1 cargo test -p tangled --test sns_rpc write_sns_fixtures -- --exact",
            path.display()
        )
    });
    serde_json::from_str(&s).expect("fixture json")
}

/// Refuse mainnet-beta JSON-RPC unless `SNS_ALLOW_MAINNET=1`.
fn require_local_rpc() -> String {
    let url = std::env::var("SOLANA_RPC").expect(
        "SOLANA_RPC must be set (e.g. http://127.0.0.1:8899). Start scripts/sns-validator.sh",
    );
    if rpc_is_blocked_mainnet(&url) {
        panic!("refusing mainnet SOLANA_RPC={url}; set SNS_ALLOW_MAINNET=1 to override");
    }
    url
}

fn rpc_is_blocked_mainnet(url: &str) -> bool {
    if std::env::var("SNS_ALLOW_MAINNET").as_deref() == Ok("1") {
        return false;
    }
    let u = url.to_ascii_lowercase();
    u.contains("mainnet-beta") || u.contains("api.mainnet")
}

fn live_resolver(rpc: String) -> HandleResolver {
    HandleResolver {
        live: true,
        solana_rpc: Some(rpc),
        ..HandleResolver::default()
    }
}

#[test]
fn sns_rpc_guard_rejects_mainnet() {
    assert!(rpc_is_blocked_mainnet(
        "https://api.mainnet-beta.solana.com"
    ));
    assert!(rpc_is_blocked_mainnet("https://api.mainnet.solana.com"));
    assert!(!rpc_is_blocked_mainnet("http://127.0.0.1:8899"));
    assert!(!rpc_is_blocked_mainnet("https://api.devnet.solana.com"));
}

#[test]
fn sns_rpc_not_used_unless_live() {
    let resolver = HandleResolver {
        live: false,
        solana_rpc: Some("http://127.0.0.1:8899".into()),
        ..HandleResolver::default()
    };
    let err = lookup_handle("alice.sol", &resolver).unwrap_err();
    assert!(
        err.to_string().contains("no SNS record"),
        "without live=true the inject miss must not hit RPC: {err}"
    );
}

#[test]
fn sns_fixtures_match_derivation() {
    let alice = load_fixture("alice.sol.json");
    let bob = load_fixture("bob.sol.json");
    assert_eq!(alice, alice_cli_account());
    assert_eq!(bob, bob_cli_account());

    let alice_b64 = alice["account"]["data"][0].as_str().unwrap();
    use base64::Engine;
    let alice_raw = base64::engine::general_purpose::STANDARD
        .decode(alice_b64)
        .unwrap();
    let rec = parse_sns_account(&alice_raw).unwrap();
    assert_eq!(rec.owner, ALICE_OWNER);
    assert_eq!(rec.tangled.as_deref(), Some(alice_did().as_str()));
    assert_eq!(rec.head.as_deref(), Some(ALICE_HEAD));

    let bob_b64 = bob["account"]["data"][0].as_str().unwrap();
    let bob_raw = base64::engine::general_purpose::STANDARD
        .decode(bob_b64)
        .unwrap();
    let rec = parse_sns_account(&bob_raw).unwrap();
    assert_eq!(rec.owner, BOB_OWNER);
    assert!(rec.tangled.is_none());
}

/// Regenerates `fixtures/sns/*.json`. Not run in CI (`WRITE_SNS_FIXTURES` unset).
#[test]
fn write_sns_fixtures() {
    if std::env::var("WRITE_SNS_FIXTURES").as_deref() != Ok("1") {
        return;
    }
    let dir = fixture_dir();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("alice.sol.json"),
        serde_json::to_string_pretty(&alice_cli_account()).unwrap() + "\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("bob.sol.json"),
        serde_json::to_string_pretty(&bob_cli_account()).unwrap() + "\n",
    )
    .unwrap();
}

#[test]
#[ignore]
fn live_sns_tangled_field_via_rpc() {
    let rpc = require_local_rpc();
    let rec = live_sns("alice.sol", &rpc).expect("getAccountInfo alice.sol");
    assert_eq!(rec.tangled.as_deref(), Some(alice_did().as_str()));
    assert_eq!(rec.head.as_deref(), Some(ALICE_HEAD));
    assert_eq!(rec.owner, ALICE_OWNER);

    let resolver = live_resolver(rpc);
    assert!(
        resolver.sns.is_empty(),
        "live test must not use the inject map"
    );
    let proof = lookup_handle("alice.sol", &resolver).unwrap();
    assert_eq!(proof.did, alice_did());
    assert_eq!(proof.head.as_deref(), Some(ALICE_HEAD));
}

#[test]
#[ignore]
fn live_sns_owner_fallback_via_rpc() {
    let rpc = require_local_rpc();
    let rec = live_sns("bob.sol", &rpc).expect("getAccountInfo bob.sol");
    assert!(rec.tangled.is_none());
    assert_eq!(rec.owner, BOB_OWNER);

    let proof = lookup_handle("bob.sol", &live_resolver(rpc)).unwrap();
    assert_eq!(proof.did, bob_did());
}

#[test]
#[ignore]
fn live_sns_unknown_name_fails() {
    let rpc = require_local_rpc();
    let err = live_sns("no-such-sns-name-xyz.sol", &rpc).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("not found") || msg.contains("SNS"),
        "unknown name must fail via RPC: {msg}"
    );
}

#[test]
#[ignore]
fn live_sns_one_sided_also_known_as_fails() {
    let rpc = require_local_rpc();
    let resolver = live_resolver(rpc);
    assert!(resolve_handle("alice.sol", &resolver, &[]).is_err());
    assert!(resolve_handle("alice.sol", &resolver, &["alice.sol".into()]).is_ok());
}
