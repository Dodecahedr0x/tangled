//! Protocol acceptance against persistent knot subprocesses over actual sockets.
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, ensure, Context, Result};
use futures_util::StreamExt;
use serde_json::{json, Value};
use tangled::{
    capability::{cap_authorization_header, circle_resource_uri, Capability, Invocation},
    car,
    cid::Cid,
    http::{GetRepoAuthRequest, KnotClient, PutRecordRequest, PutRecordResponse, SubscribeMessage},
    identity::Identity,
    record::sign_json_record,
};

use crate::cluster::Cluster;

pub const FEATURES: &[&str] = &[
    "PROTOCOL-DESCRIBE-STATS",
    "PROTOCOL-SIWS",
    "PROTOCOL-SIGNED-RECORDS",
    "PROTOCOL-VISIBILITY-CAPS",
    "PROTOCOL-CAP-EXPIRY-REVOCATION",
    "PROTOCOL-BLOBS",
    "PROTOCOL-CAR-SYNC",
    "PROTOCOL-CONCURRENT-HEADS",
    "PROTOCOL-RESTART",
    "PROTOCOL-WS",
    "PROTOCOL-OVERLAYS",
    "PROTOCOL-LOCATION-DEVICE",
    "PROTOCOL-ACTIVITYPUB",
];

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs()
}

fn unsigned(identity: &Identity, collection: &str, rkey: &str, vis: &str, body: Value) -> Value {
    json!({"v":1,"type":Cid::dag_cbor(collection.as_bytes()).to_string(),
        "collection":collection,"rkey":rkey,"author":identity.did_string(),
        "ts":now()*1000+SEQUENCE.fetch_add(1,Ordering::Relaxed),"prev":[],"vis":vis,"body":body})
}

async fn publish(
    client: &KnotClient,
    signer: &Identity,
    record: Value,
) -> Result<(String, PutRecordResponse)> {
    let label = format!("{}/{}", record["collection"], record["rkey"]);
    let signed = sign_json_record(signer.signing_key(), &record)?;
    let cid = signed.cid.to_string();
    let result = client
        .put_record(&PutRecordRequest {
            record: signed.json,
        })
        .await
        .with_context(|| format!("publishing {label}"))?;
    ensure!(
        result.cid.as_deref() == Some(cid.as_str()),
        "putRecord returned a different record CID"
    );
    ensure!(result.commit.is_some(), "putRecord omitted commit");
    Ok((cid, result))
}

fn expect_http<T: std::fmt::Debug>(
    result: tangled::Result<T>,
    status: u16,
    error: Option<&str>,
) -> Result<()> {
    match result {
        Err(tangled::Error::Http {
            status: actual,
            error: code,
        }) => {
            ensure!(
                actual == status,
                "expected HTTP {status}, got {actual}: {code}"
            );
            if let Some(expected) = error {
                ensure!(code == expected, "expected {expected}, got {code}");
            }
            Ok(())
        }
        other => bail!("expected HTTP {status}, got {other:?}"),
    }
}

fn capability(
    issuer: &Identity,
    audience: &Identity,
    cmd: &str,
    res: &str,
    expired: bool,
) -> Result<Capability> {
    let mut cap = Capability {
        v: 1,
        iss: issuer.did_string(),
        aud: audience.did_string(),
        cmd: cmd.into(),
        res: res.into(),
        nbf: now() - 300,
        exp: if expired { now() - 120 } else { now() + 3600 },
        pol: vec![],
        prf: vec![],
        meta: None,
        sig: None,
    };
    cap.sign(issuer.signing_key())?;
    Ok(cap)
}

fn authorized(
    base: &str,
    signer: &Identity,
    cap: &Capability,
    audience: &str,
) -> Result<KnotClient> {
    let mut nonce = [0u8; 16];
    nonce[..8].copy_from_slice(&SEQUENCE.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    nonce[8..].copy_from_slice(&now().to_le_bytes());
    let mut invocation = Invocation::new(cap, &cap.cmd, &cap.res, audience, now(), nonce)?;
    invocation.sign(signer.signing_key())?;
    Ok(
        KnotClient::http(base)
            .with_cap_header(cap_authorization_header(&invocation.encode_cbor()?)),
    )
}

pub async fn run(
    cluster: &mut Cluster,
    operations: usize,
    payload_bytes: usize,
) -> Result<Vec<&'static str>> {
    ensure!(
        operations > 0 && payload_bytes > 0,
        "protocol workload sizes must be positive"
    );
    let mut passed = Vec::new();
    let did = cluster.nodes[0].identity.did_string();
    let public = cluster.nodes[0].client();
    let owner = cluster.nodes[0].siws(&cluster.nodes[0].identity).await?;
    let remote = cluster.nodes[1].siws(&cluster.nodes[0].identity).await?;
    let device = Identity::from_seeds([173; 32], [174; 32]);

    let describe = public.describe().await?;
    ensure!(
        describe.v == 1 && describe.did == did,
        "describe version/identity mismatch"
    );
    ensure!(
        describe.profiles.iter().any(|p| p == "knot.public"),
        "missing public profile"
    );
    for peer in &cluster.nodes[1..] {
        ensure!(
            describe.peers.contains(&peer.url),
            "configured peer absent from describe"
        );
    }
    let before = public.stats().await?;
    ensure!(
        before.v == 1 && before.did == did,
        "stats version/identity mismatch"
    );
    passed.push("PROTOCOL-DESCRIBE-STATS");

    let challenge = public.session_challenge(&device.did_string()).await?;
    ensure!(
        challenge.message.contains(&device.did_string()),
        "SIWS challenge not identity bound"
    );
    expect_http(
        public
            .session_verify(&tangled::session::SiwsVerifyRequest {
                did: device.did_string(),
                nonce: challenge.nonce,
                signature: "00".repeat(64),
            })
            .await,
        400,
        Some("InvalidSignature"),
    )?;
    let guest = cluster.nodes[0].siws(&device).await?;
    publish(
        &guest,
        &device,
        unsigned(
            &device,
            "pub.tangled.post",
            "protocol-siws",
            "public",
            json!({"text":"authenticated guest"}),
        ),
    )
    .await?;
    ensure!(
        public
            .get_record(&device.did_string(), "pub.tangled.post", "protocol-siws")
            .await?
            .record["body"]["text"]
            == "authenticated guest",
        "SIWS session write missing"
    );
    passed.push("PROTOCOL-SIWS");

    let mut seed = None;
    for index in 0..operations {
        let started = std::time::Instant::now();
        let rkey = format!("protocol-load-{index}");
        let out = publish(
            &owner,
            &cluster.nodes[0].identity,
            unsigned(
                &cluster.nodes[0].identity,
                "pub.tangled.post",
                &rkey,
                "public",
                json!({"text":"x".repeat(payload_bytes)}),
            ),
        )
        .await?;
        ensure!(
            public
                .get_record(&did, "pub.tangled.post", &rkey)
                .await?
                .record["body"]["text"]
                .as_str()
                .map(str::len)
                == Some(payload_bytes),
            "signed payload did not roundtrip"
        );
        cluster
            .operation_latencies_ms
            .push(started.elapsed().as_secs_f64() * 1000.0);
        seed = Some(out);
    }
    let seed = seed.context("nonempty protocol workload")?;
    let mut tampered = sign_json_record(
        cluster.nodes[0].identity.signing_key(),
        &unsigned(
            &cluster.nodes[0].identity,
            "pub.tangled.post",
            "protocol-bad-sig",
            "public",
            json!({"text":"signed"}),
        ),
    )?
    .json;
    tampered["body"]["text"] = json!("tampered");
    expect_http(
        owner
            .put_record(&PutRecordRequest { record: tampered })
            .await,
        400,
        None,
    )?;
    let invalid = unsigned(
        &cluster.nodes[0].identity,
        "pub.tangled.post",
        "protocol-bad-vis",
        "unknown",
        json!({"text":"bad visibility"}),
    );
    let invalid = sign_json_record(cluster.nodes[0].identity.signing_key(), &invalid)?.json;
    expect_http(
        owner
            .put_record(&PutRecordRequest { record: invalid })
            .await,
        400,
        None,
    )?;
    let victim = unsigned(
        &cluster.nodes[0].identity,
        "pub.tangled.post",
        "protocol-unsigned",
        "public",
        json!({"text":"missing signature"}),
    );
    expect_http(
        public
            .put_record(&PutRecordRequest { record: victim })
            .await,
        400,
        None,
    )?;
    let after = public.stats().await?;
    ensure!(
        after.counts.records >= before.counts.records + operations as u64,
        "stats omitted accepted writes"
    );
    passed.push("PROTOCOL-SIGNED-RECORDS");

    let cap_record = publish(
        &owner,
        &cluster.nodes[0].identity,
        unsigned(
            &cluster.nodes[0].identity,
            "pub.tangled.post",
            "protocol-cap",
            "cap",
            json!({"text":"private capability payload"}),
        ),
    )
    .await?
    .0;
    publish(
        &owner,
        &cluster.nodes[0].identity,
        unsigned(
            &cluster.nodes[0].identity,
            "sys.circle",
            "protocol-circle",
            "public",
            json!({"v":1,"epoch":0}),
        ),
    )
    .await?;
    let circle = format!("{did}#protocol-circle");
    let mut sealed = unsigned(
        &cluster.nodes[0].identity,
        "pub.tangled.chat",
        "protocol-sealed",
        "sealed",
        json!({"ciphertext":"00aabb"}),
    );
    sealed["circle"] = json!(circle);
    let sealed_record = publish(&owner, &cluster.nodes[0].identity, sealed).await?.0;
    expect_http(
        public
            .get_record(&did, "pub.tangled.post", "protocol-cap")
            .await,
        401,
        Some("CapabilityRequired"),
    )?;
    expect_http(
        public
            .get_record(&did, "pub.tangled.chat", "protocol-sealed")
            .await,
        401,
        None,
    )?;
    let repo_res = format!("tangled://{did}");
    let read = capability(
        &cluster.nodes[0].identity,
        &device,
        "repo/read",
        &repo_res,
        false,
    )?;
    let circle_res = circle_resource_uri(&did, "protocol-circle");
    let circle_cap = capability(
        &cluster.nodes[0].identity,
        &device,
        "circle/read",
        &circle_res,
        false,
    )?;
    ensure!(
        authorized(&cluster.nodes[0].url, &device, &read, &did)?
            .get_record(&did, "pub.tangled.post", "protocol-cap")
            .await?
            .record["body"]["text"]
            == "private capability payload",
        "cap read payload mismatch"
    );
    ensure!(
        authorized(&cluster.nodes[0].url, &device, &circle_cap, &did)?
            .get_record(&did, "pub.tangled.chat", "protocol-sealed")
            .await?
            .record["body"]["ciphertext"]
            == "00aabb",
        "sealed read payload mismatch"
    );
    let thief = Identity::from_seeds([175; 32], [176; 32]);
    expect_http(
        authorized(&cluster.nodes[0].url, &thief, &read, &did)?
            .get_record(&did, "pub.tangled.post", "protocol-cap")
            .await,
        403,
        None,
    )?;
    passed.push("PROTOCOL-VISIBILITY-CAPS");

    let expired = capability(
        &cluster.nodes[0].identity,
        &device,
        "repo/read",
        &repo_res,
        true,
    )?;
    expect_http(
        authorized(&cluster.nodes[0].url, &device, &expired, &did)?
            .get_record(&did, "pub.tangled.post", "protocol-cap")
            .await,
        403,
        None,
    )?;
    // A read capability is not a write authority, even when its audience signs the record.
    let mut forbidden = unsigned(
        &cluster.nodes[0].identity,
        "pub.tangled.post",
        "protocol-read-write",
        "public",
        json!({"text":"forbidden"}),
    );
    forbidden["signedBy"] = json!(hex::encode(device.pubkey_bytes()));
    forbidden["proof"] = json!([read.cid()?.to_string()]);
    let signed = sign_json_record(device.signing_key(), &forbidden)?.json;
    expect_http(
        authorized(&cluster.nodes[0].url, &device, &read, &did)?
            .put_record(&PutRecordRequest { record: signed })
            .await,
        403,
        None,
    )?;

    let blob_bytes = b"protocol public blob".to_vec();
    let blob = owner.put_blob(blob_bytes.clone()).await?;
    let public_blob_record = publish(
        &owner,
        &cluster.nodes[0].identity,
        unsigned(
            &cluster.nodes[0].identity,
            "pub.tangled.media",
            "protocol-public-blob",
            "public",
            json!({"blob":blob.cid,"alt":"fixture"}),
        ),
    )
    .await?
    .0;
    ensure!(
        public.get_blob(&did, &blob.cid).await? == blob_bytes,
        "public blob corrupted"
    );
    let private_bytes = b"protocol sealed blob".to_vec();
    let private_blob = owner.put_blob(private_bytes.clone()).await?;
    let mut media = unsigned(
        &cluster.nodes[0].identity,
        "pub.tangled.media",
        "protocol-private-blob",
        "sealed",
        json!({"blob":private_blob.cid}),
    );
    media["circle"] = json!(circle);
    publish(&owner, &cluster.nodes[0].identity, media).await?;
    expect_http(public.get_blob(&did, &private_blob.cid).await, 401, None)?;
    ensure!(
        authorized(&cluster.nodes[0].url, &device, &circle_cap, &did)?
            .get_blob(&did, &private_blob.cid)
            .await?
            == private_bytes,
        "authorized private blob corrupted"
    );
    passed.push("PROTOCOL-BLOBS");

    let full_bytes = public.get_repo(&did, None, Some(false)).await?;
    let full = car::decode(&full_bytes)?;
    ensure!(
        !full.blocks.is_empty() && !full.roots.is_empty(),
        "public CAR is empty"
    );
    ensure!(
        !full
            .blocks
            .iter()
            .any(|(cid, _)| cid.to_string() == cap_record || cid.to_string() == sealed_record),
        "public CAR exposed private records"
    );
    let imported = remote.import_repo(full_bytes, true).await?;
    ensure!(imported.did == did, "CAR import author mismatch");
    let remote_public = cluster.nodes[1].client();
    ensure!(
        remote_public
            .get_record(&did, "pub.tangled.post", "protocol-load-0")
            .await?
            .record["body"]["text"]
            .as_str()
            .map(str::len)
            == Some(payload_bytes),
        "public CAR import not materialized"
    );
    expect_http(
        remote_public
            .get_record(&did, "pub.tangled.post", "protocol-cap")
            .await,
        404,
        None,
    )?;
    // The current implementation's incremental-record cursor is a record CID.
    let cursor = public_blob_record;
    let commit_cursor = public.get_head(&did).await?.commit;
    let later = publish(
        &owner,
        &cluster.nodes[0].identity,
        unsigned(
            &cluster.nodes[0].identity,
            "pub.tangled.post",
            "protocol-incremental",
            "public",
            json!({"text":"incremental"}),
        ),
    )
    .await?;
    let diff_bytes = public.get_repo(&did, Some(&cursor), Some(false)).await?;
    let diff = car::decode(&diff_bytes)?;
    ensure!(
        diff.blocks
            .iter()
            .any(|(cid, _)| cid.to_string() == later.0),
        "incremental CAR omitted new record"
    );
    ensure!(
        !diff.blocks.iter().any(|(cid, _)| cid.to_string() == seed.0),
        "incremental CAR repeated old record"
    );
    let commit_delta_bytes = public
        .get_repo(&did, Some(&commit_cursor), Some(false))
        .await?;
    let commit_delta = car::decode(&commit_delta_bytes)?;
    ensure!(
        commit_delta
            .blocks
            .iter()
            .any(|(cid, _)| cid.to_string() == commit_cursor),
        "commit delta omitted since boundary"
    );
    ensure!(
        commit_delta
            .blocks
            .iter()
            .any(|(cid, _)| Some(cid.to_string()) == later.1.commit),
        "commit delta omitted new head"
    );
    ensure!(
        !commit_delta
            .blocks
            .iter()
            .any(|(cid, _)| Some(cid.to_string()) == seed.1.commit),
        "commit delta included older commit"
    );
    remote
        .import_repo(commit_delta_bytes, true)
        .await
        .context("importing normative commit-CID delta using local history")?;
    ensure!(
        remote_public
            .get_record(&did, "pub.tangled.post", "protocol-incremental")
            .await?
            .record["body"]["text"]
            == "incremental",
        "incremental import missing"
    );
    let mut other_circle = unsigned(
        &cluster.nodes[0].identity,
        "pub.tangled.chat",
        "protocol-other-circle",
        "sealed",
        json!({"ciphertext":"ccdd"}),
    );
    other_circle["circle"] = json!(format!("{did}#protocol-other"));
    let other_circle = publish(&owner, &cluster.nodes[0].identity, other_circle)
        .await?
        .0;
    let other_log = publish(
        &owner,
        &cluster.nodes[0].identity,
        unsigned(
            &cluster.nodes[0].identity,
            "sys.circle",
            "protocol-other",
            "cap",
            json!({"v":1,"epoch":0}),
        ),
    )
    .await?
    .0;
    let circle_bytes = authorized(&cluster.nodes[0].url, &device, &circle_cap, &did)?
        .get_repo_auth(&GetRepoAuthRequest {
            did: did.clone(),
            since: None,
            blobs: Some(false),
        })
        .await?;
    let circle_car = car::decode(&circle_bytes)?;
    ensure!(
        circle_car
            .blocks
            .iter()
            .any(|(cid, _)| cid.to_string() == sealed_record),
        "circle CAR omitted member payload"
    );
    ensure!(
        !circle_car.blocks.iter().any(|(cid, _)| [
            other_circle.as_str(),
            other_log.as_str(),
            cap_record.as_str()
        ]
        .contains(&cid.to_string().as_str())),
        "circle CAR leaked unrelated payload or circle log"
    );
    let auth_bytes = authorized(&cluster.nodes[0].url, &device, &read, &did)?
        .get_repo_auth(&GetRepoAuthRequest {
            did: did.clone(),
            since: None,
            blobs: Some(false),
        })
        .await?;
    let auth_car = car::decode(&auth_bytes)?;
    ensure!(
        auth_car
            .blocks
            .iter()
            .any(|(cid, _)| cid.to_string() == cap_record),
        "authorized CAR omitted permitted private record"
    );
    let private_dest = cluster.nodes[2].siws(&cluster.nodes[0].identity).await?;
    private_dest.import_repo(auth_bytes, true).await?;
    ensure!(
        authorized(&cluster.nodes[2].url, &device, &read, &did)?
            .get_record(&did, "pub.tangled.post", "protocol-cap")
            .await?
            .record["body"]["text"]
            == "private capability payload",
        "authorized import missing private payload"
    );
    passed.push("PROTOCOL-CAR-SYNC");

    overlays(&owner, &public, &cluster.nodes[0].identity, &later.0).await?;
    passed.push("PROTOCOL-OVERLAYS");
    concurrent(
        &owner,
        &public,
        &cluster.nodes[0].identity,
        &device,
        &cluster.nodes[0].url,
    )
    .await?;
    passed.push("PROTOCOL-CONCURRENT-HEADS");
    sockets(
        &owner,
        &public,
        &cluster.nodes[0].identity,
        &cluster.nodes[0].url,
    )
    .await?;
    passed.push("PROTOCOL-WS");

    publish(
        &owner,
        &cluster.nodes[0].identity,
        unsigned(
            &cluster.nodes[0].identity,
            "sys.revocation",
            "protocol-revoke-read",
            "public",
            json!({"cid":read.cid()?.to_string()}),
        ),
    )
    .await?;
    expect_http(
        authorized(&cluster.nodes[0].url, &device, &read, &did)?
            .get_record(&did, "pub.tangled.post", "protocol-cap")
            .await,
        403,
        Some("CapabilityInvalid"),
    )?;
    publish(
        &owner,
        &cluster.nodes[0].identity,
        unsigned(
            &cluster.nodes[0].identity,
            "sys.revocation",
            "protocol-revoke-circle",
            "public",
            json!({"cid":circle_cap.cid()?.to_string()}),
        ),
    )
    .await?;
    expect_http(
        authorized(&cluster.nodes[0].url, &device, &circle_cap, &did)?
            .get_record(&did, "pub.tangled.chat", "protocol-sealed")
            .await,
        403,
        None,
    )?;
    passed.push("PROTOCOL-CAP-EXPIRY-REVOCATION");

    migration(cluster, &device).await?;
    passed.push("PROTOCOL-LOCATION-DEVICE");
    let durable_head = public.get_head(&did).await?.commit;
    cluster.nodes[0].restart().await?;
    let restarted = cluster.nodes[0].client();
    ensure!(
        restarted.get_head(&did).await?.commit == durable_head,
        "restart changed durable head"
    );
    ensure!(
        restarted
            .get_record(&did, "pub.tangled.post", "protocol-incremental")
            .await?
            .record["body"]["text"]
            == "incremental",
        "restart lost record"
    );
    ensure!(
        restarted.get_blob(&did, &blob.cid).await? == blob_bytes,
        "restart lost blob"
    );
    expect_http(
        authorized(&cluster.nodes[0].url, &device, &read, &did)?
            .get_record(&did, "pub.tangled.post", "protocol-cap")
            .await,
        403,
        None,
    )?;
    passed.push("PROTOCOL-RESTART");
    activitypub(&restarted, &cluster.nodes[0].url, &cluster.client).await?;
    passed.push("PROTOCOL-ACTIVITYPUB");
    Ok(passed)
}

async fn overlays(
    owner: &KnotClient,
    public: &KnotClient,
    identity: &Identity,
    target: &str,
) -> Result<()> {
    let empty = public.query_backlinks(target, None, None).await?;
    ensure!(
        empty.hits.is_empty() && !empty.complete,
        "empty overlay incorrectly proves absence"
    );
    for (rkey, vis) in [
        ("protocol-overlay-public", "public"),
        ("protocol-overlay-private", "cap"),
    ] {
        let mut annotation = unsigned(
            identity,
            "pub.tangled.annotation",
            rkey,
            vis,
            json!({"reply":target,"text":rkey}),
        );
        annotation["ref"] = json!(target);
        publish(owner, identity, annotation).await?;
    }
    let query = public.query_backlinks(target, None, None).await?;
    ensure!(
        query.overlay && !query.complete,
        "overlay completeness contract changed"
    );
    ensure!(
        query
            .hits
            .iter()
            .any(|hit| hit.rkey == "protocol-overlay-public"),
        "public backlink missing"
    );
    ensure!(
        !query
            .hits
            .iter()
            .any(|hit| hit.rkey == "protocol-overlay-private"),
        "private backlink leaked"
    );
    let loom = public.loom_query(target, Some("ref")).await?;
    ensure!(
        loom.hits
            .iter()
            .any(|hit| hit.rkey == "protocol-overlay-public")
            && !loom
                .hits
                .iter()
                .any(|hit| hit.rkey == "protocol-overlay-private"),
        "loom overlay visibility mismatch"
    );
    Ok(())
}

async fn concurrent(
    owner: &KnotClient,
    public: &KnotClient,
    identity: &Identity,
    device: &Identity,
    base: &str,
) -> Result<()> {
    let first = publish(
        owner,
        identity,
        unsigned(
            identity,
            "pub.tangled.page",
            "protocol-concurrent",
            "public",
            json!({"ops":[]}),
        ),
    )
    .await?;
    let res = format!("tangled://{}/pub.tangled.page", identity.did_string());
    let cap = capability(identity, device, "repo/write", &res, false)?;
    let mut a = unsigned(
        identity,
        "pub.tangled.page",
        "protocol-concurrent",
        "public",
        json!({"ops":[{"op":"ins","after":null,"run":"A"}]}),
    );
    a["signedBy"] = json!(hex::encode(device.pubkey_bytes()));
    a["proof"] = json!([cap.cid()?.to_string()]);
    a["prev"] = json!([first.0]);
    publish(
        &authorized(base, device, &cap, &identity.did_string())?,
        device,
        a.clone(),
    )
    .await?;
    a["body"] = json!({"ops":[{"op":"ins","after":null,"run":"B"}]});
    let signed = sign_json_record(device.signing_key(), &a)?.json;
    expect_http(
        authorized(base, device, &cap, &identity.did_string())?
            .put_record(&PutRecordRequest { record: signed })
            .await,
        409,
        Some("ConcurrentHeads"),
    )?;
    ensure!(
        public
            .get_record(
                &identity.did_string(),
                "pub.tangled.page",
                "protocol-concurrent"
            )
            .await?
            .record["body"]["ops"][0]["run"]
            == "A",
        "rejected concurrent head replaced accepted head"
    );
    Ok(())
}

async fn sockets(
    owner: &KnotClient,
    public: &KnotClient,
    identity: &Identity,
    base: &str,
) -> Result<()> {
    let did = identity.did_string();
    let cursor = public.get_head(&did).await?.commit;
    let mut url = reqwest::Url::parse(base)?;
    url.set_scheme("ws")
        .map_err(|()| anyhow::anyhow!("invalid WS scheme"))?;
    url.set_path("/xrpc/tangled.sync.subscribe");
    url.query_pairs_mut()
        .append_pair("did", &did)
        .append_pair("since", &cursor);
    let (mut ws, _) = tokio_tungstenite::connect_async(url.as_str()).await?;
    let hello = recv_event(&mut ws).await?;
    ensure!(
        hello.t == "commit" && hello.did == did && hello.commit == cursor && hello.ops.is_empty(),
        "subscription initial head mismatch"
    );
    let latest = publish(
        owner,
        identity,
        unsigned(
            identity,
            "pub.tangled.post",
            "protocol-ws-live",
            "public",
            json!({"text":"live socket"}),
        ),
    )
    .await?;
    let event = recv_event(&mut ws).await?;
    ensure!(
        event.t == "commit"
            && event.did == did
            && event.commit == latest.1.commit.context("WS commit")?
            && event.ops.iter().any(|op| op.rkey == "protocol-ws-live"),
        "live subscription did not observe commit"
    );
    ws.close(None).await?;
    let (mut replay, _) = tokio_tungstenite::connect_async(url.as_str()).await?;
    let event = recv_event(&mut replay).await?;
    ensure!(
        event.ops.iter().any(|op| op.rkey == "protocol-ws-live"),
        "since replay omitted commit"
    );
    replay.close(None).await?;
    url.query_pairs_mut()
        .clear()
        .append_pair("did", &did)
        .append_pair("collection", "pub.tangled.chat");
    match tokio_tungstenite::connect_async(url.as_str()).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => ensure!(
            response.status().as_u16() == 401,
            "sealed subscription returned {}",
            response.status()
        ),
        other => bail!("sealed subscription should reject upgrade, got {other:?}"),
    }
    Ok(())
}

async fn recv_event(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Result<SubscribeMessage> {
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(message) = ws.next().await {
            let message = message?;
            if message.is_text() {
                return Ok(serde_json::from_str(message.to_text()?)?);
            }
        }
        bail!("subscription closed before commit")
    })
    .await
    .context("subscription deadline")?
}

async fn migration(cluster: &Cluster, device: &Identity) -> Result<()> {
    let identity = &cluster.nodes[0].identity;
    let did = identity.did_string();
    let destination = cluster.nodes[1].siws(identity).await?;
    let previous = cluster.nodes[1]
        .client()
        .get_record(&did, "sys.id", "self")
        .await;
    let mut location = unsigned(
        identity,
        "sys.id",
        "self",
        "public",
        json!({"v":1,"pubkey":hex::encode(identity.pubkey_bytes()),"agreement":hex::encode(identity.agreement_public_bytes()),"rotation":[hex::encode(identity.pubkey_bytes())],"recovery":[],"endpoints":[{"type":"knot","url":cluster.nodes[1].url}],"alsoKnownAs":[],"bindings":[]}),
    );
    if let Ok(record) = previous {
        let mut unsigned_previous = record.record;
        unsigned_previous
            .as_object_mut()
            .context("sys.id record object")?
            .remove("sig");
        location["prev"] = json!([
            sign_json_record(identity.signing_key(), &unsigned_previous)?
                .cid
                .to_string()
        ]);
    }
    publish(&destination, identity, location).await?;
    let moved = cluster.nodes[1]
        .client()
        .get_record(&did, "sys.id", "self")
        .await?;
    ensure!(
        moved.record["body"]["endpoints"][0]["url"] == cluster.nodes[1].url,
        "location update did not materialize at destination"
    );
    ensure!(
        cluster.nodes[1]
            .client()
            .get_record(&did, "pub.tangled.post", "protocol-load-0")
            .await?
            .record["author"]
            == did,
        "migration lost imported history"
    );
    let owner = cluster.nodes[0].siws(identity).await?;
    let public = cluster.nodes[0].client();
    let res = format!("tangled://{did}");
    let cap = capability(identity, device, "repo/write", &res, false)?;
    for (rkey, revoked) in [
        ("protocol-device-before", false),
        ("protocol-device-after", true),
    ] {
        if revoked {
            publish(
                &owner,
                identity,
                unsigned(
                    identity,
                    "sys.revocation",
                    "protocol-device-revoke",
                    "public",
                    json!({"cid":cap.cid()?.to_string(),"reason":"lost device"}),
                ),
            )
            .await?;
        }
        let mut record = unsigned(
            identity,
            "pub.tangled.post",
            rkey,
            "public",
            json!({"text":rkey}),
        );
        record["signedBy"] = json!(hex::encode(device.pubkey_bytes()));
        record["proof"] = json!([cap.cid()?.to_string()]);
        let signed = sign_json_record(device.signing_key(), &record)?.json;
        let result = authorized(&cluster.nodes[0].url, device, &cap, &did)?
            .put_record(&PutRecordRequest { record: signed })
            .await;
        if revoked {
            expect_http(result, 403, None)?;
        } else {
            ensure!(result?.cid.is_some(), "device write missing CID");
        }
    }
    expect_http(
        public
            .get_record(&did, "pub.tangled.post", "protocol-device-after")
            .await,
        404,
        None,
    )?;
    Ok(())
}

async fn activitypub(client: &KnotClient, base: &str, http: &reqwest::Client) -> Result<()> {
    let peer = tangled_ap::demo::MockPeer::bind("protocol-fixture").await?;
    let note = peer.publish_note("protocol local ActivityPub outbox");
    let resolved = client.ap_resolve(&peer.acct()).await?;
    let did = tangled::foreign::activitypub_actor_identity(peer.actor_id()).did_string();
    ensure!(
        resolved.did == did && resolved.actor == peer.actor_id(),
        "AP resolve actor mismatch"
    );
    let rkey = tangled::foreign::import_rkey("activitypub", &note)?;
    let foreign = client.get_record(&did, "sys.foreign", &rkey).await?;
    ensure!(
        foreign.record["body"]["attested"] == true
            && foreign.record["body"]["protocol"] == "activitypub",
        "AP imported foreign record lacks gateway attestation"
    );
    ensure!(
        client
            .get_record(&did, "pub.tangled.post", &rkey)
            .await?
            .record["body"]["text"]
            == "protocol local ActivityPub outbox",
        "AP outbox text missing"
    );
    let delivered = peer
        .post_create_note(&format!("{base}/inbox"), "protocol local signed inbox")
        .await?;
    ensure!(
        delivered.status == 202,
        "signed AP inbox rejected: {} {}",
        delivered.status,
        delivered.body
    );
    let rkey = tangled::foreign::import_rkey("activitypub", &delivered.note_id)?;
    ensure!(
        client
            .get_record(&did, "pub.tangled.post", &rkey)
            .await?
            .record["body"]["text"]
            == "protocol local signed inbox",
        "signed AP inbox did not materialize"
    );
    let rejected = http.post(format!("{base}/inbox")).header("content-type","application/activity+json").json(&json!({"type":"Create","actor":peer.actor_id(),"object":{"type":"Note","id":"urn:protocol:unsigned","attributedTo":peer.actor_id(),"content":"unsigned"}})).send().await?;
    ensure!(
        rejected.status().is_client_error(),
        "unsigned AP inbox was accepted: {}",
        rejected.status()
    );
    Ok(())
}
