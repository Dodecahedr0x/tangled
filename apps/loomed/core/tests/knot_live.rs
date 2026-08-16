//! Live HTTP integration: Loomed clients against a real `tangled-knot` process.
//!
//! Not run by default (`cargo test -p loomed-core`) so CI stays green when the
//! knot binary is not built. The knot is a subprocess on `127.0.0.1:0` with a
//! temp data dir; Drop kills it on success or panic.
//!
//! ```text
//! cargo build -p tangled-knot
//! cargo test -p loomed-core --test knot_live -- --ignored --nocapture
//! ```
//!
//! Override the binary with `TANGLED_KNOT=/path/to/tangled-knot`.
//! Writes use SIWS (`tangled_session` cookie). Sealed fetch uses `circle/read`
//! Cap + `getRepoAuth` inside `Sync::poll_once`.

use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use loomed_core::chat::Chat;
use loomed_core::identity::LoomedKeys;
use loomed_core::outbox::Outbox;
use loomed_core::replica::Replica;
use loomed_core::sync::Sync;
use loomed_core::types::KnotUrl;
use tangled::circle::CircleState;
use tangled::http::KnotClient;
use tangled::session::{sign_siws, SiwsVerifyRequest};

/// Starts `tangled-knot --listen 127.0.0.1:0 --data <temp>` and kills it on drop.
struct LiveKnot {
    child: Child,
    url: String,
    _data: tempfile::TempDir,
}

impl Drop for LiveKnot {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl LiveKnot {
    async fn spawn() -> Self {
        let data = tempfile::tempdir().expect("temp data dir");
        let bin = knot_binary().unwrap_or_else(|| {
            panic!(
                "tangled-knot binary not found. Build it first:\n  \
                 cargo build -p tangled-knot\nthen:\n  \
                 cargo test -p loomed-core --test knot_live -- --ignored --nocapture\n\
                 Or set TANGLED_KNOT to the binary path."
            )
        });
        let mut child = Command::new(&bin)
            .args([
                "--listen",
                "127.0.0.1:0",
                "--data",
                data.path().to_str().expect("utf-8 data path"),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("spawn {}: {e}", bin.display()));

        let stderr = child.stderr.take().expect("piped stderr");
        let url = match listen_url_from_stderr(stderr, &mut child) {
            Ok(u) => u,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{e}");
            }
        };
        let knot = Self {
            child,
            url,
            _data: data,
        };
        wait_describe(&knot.url).await;
        knot
    }
}

fn knot_binary() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("TANGLED_KNOT") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    for profile in ["debug", "release"] {
        for name in ["tangled-knot", "tangled-knot.exe"] {
            let p = target.join(profile).join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn listen_url_from_stderr(
    stderr: std::process::ChildStderr,
    child: &mut Child,
) -> Result<String, String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                if let Some(url) = parse_listen_line(&line) {
                    return Ok(url);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Ok(Some(status)) = child.try_wait() {
                    return Err(format!("tangled-knot exited before listen: {status}"));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("tangled-knot stderr closed before listen line".into());
            }
        }
    }
    Err("timed out waiting for tangled-knot listen line (30s)".into())
}

fn parse_listen_line(line: &str) -> Option<String> {
    let rest = line.strip_prefix("listening on ")?;
    let addr = rest.split(" (advertised").next()?.trim();
    let _: SocketAddr = addr.parse().ok()?;
    Some(format!("http://{addr}"))
}

async fn wait_describe(url: &str) {
    let client = KnotClient::http(url);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if client.describe().await.is_ok() {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("knot describe timeout at {url} (30s)");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn siws_client(url: &str, keys: &LoomedKeys) -> KnotClient {
    let client = KnotClient::http(url);
    let did = keys.did().as_str();
    let ch = client
        .session_challenge(&did)
        .await
        .expect("session.challenge");
    let sig = sign_siws(keys.identity().signing_key(), &ch.message);
    let (_, token) = client
        .session_verify(&SiwsVerifyRequest {
            did,
            nonce: ch.nonce,
            signature: sig,
        })
        .await
        .expect("session.verify");
    KnotClient::http(url).with_session_cookie(token)
}

fn wrap_for_member(
    owner: &LoomedKeys,
    member: &LoomedKeys,
    circle: &str,
    state: &CircleState,
) -> serde_json::Value {
    let secret = *state
        .epoch_secrets
        .get(&state.epoch)
        .expect("epoch secret on owner replica");
    let (enc, ct) = tangled::hpke::wrap_epoch_secret(
        &member.identity().agreement_public_bytes(),
        circle,
        state.epoch,
        &secret,
    )
    .expect("wrap epoch secret");
    let rkey = format!(
        "{}-{}",
        state.epoch,
        tangled::identity::encode_pubkey_base58(&member.identity().pubkey_bytes())
    );
    let unsigned = serde_json::json!({
        "v": 1,
        "type": tangled::Cid::dag_cbor(b"sys.circle.wrap").to_string(),
        "collection": "sys.circle.wrap",
        "rkey": rkey,
        "author": owner.did().as_str(),
        "ts": unix_us(),
        "prev": [],
        "vis": "sealed",
        "circle": circle,
        "epoch": state.epoch,
        "body": {
            "alg": "hpke-record",
            "enc": hex::encode(enc),
            "ct": hex::encode(ct),
            "epoch": state.epoch,
        }
    });
    tangled::record::sign_json_record(owner.identity().signing_key(), &unsigned)
        .expect("sign wrap")
        .json
}

fn unix_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

fn replica_dir(prefix: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .expect("replica tempdir")
}

/// Two Loomed identities, one real knot: create a DM, send_text, poll_once, plaintext arrives.
#[tokio::test]
#[ignore = "starts a real tangled-knot process; cargo test -p loomed-core --test knot_live -- --ignored --nocapture"]
async fn live_knot_dm_plaintext_arrives() {
    let knot = LiveKnot::spawn().await;
    let alice = LoomedKeys::generate();
    let bob = LoomedKeys::generate();
    let alice_client = siws_client(&knot.url, &alice).await;

    let chat = Chat {
        keys: alice.clone(),
    };
    let (circle, sys) = chat
        .create_circle(std::slice::from_ref(&bob.did()))
        .expect("create_circle");

    let alice_dir = replica_dir("loomed-live-alice-");
    let mut alice_replica = Replica::open(alice_dir.path()).expect("alice replica");
    alice_replica
        .apply_record(&serde_json::to_vec(&sys).expect("sys json"))
        .expect("apply sys.circle");

    let mut outbox = Outbox::new();
    outbox.enqueue_record(sys).expect("enqueue sys.circle");
    let state = alice_replica
        .circle_state(&circle)
        .expect("alice circle_state");
    outbox
        .enqueue_record(wrap_for_member(&alice, &bob, &circle.0, &state))
        .expect("enqueue wrap");
    chat.send_text(
        &mut alice_replica,
        &mut outbox,
        &circle,
        "hello from the live knot",
    )
    .expect("send_text");
    let drained = outbox.drain(&alice_client).await.expect("drain");
    assert!(
        drained >= 3,
        "expected sys.circle + wrap + chat, drained {drained}"
    );

    let bob_dir = replica_dir("loomed-live-bob-");
    let mut bob_replica = Replica::open(bob_dir.path()).expect("bob replica");
    let mut sync = Sync::http(KnotUrl(knot.url.clone())).expect("Sync::http");
    sync.poll_once(&mut bob_replica, &bob)
        .await
        .expect("poll_once");

    let msgs = bob_replica.messages(&circle);
    assert!(
        msgs.iter()
            .any(|m| m.text.as_deref() == Some("hello from the live knot")),
        "Bob's replica should contain Alice's plaintext after poll_once, got {msgs:?}"
    );
}
