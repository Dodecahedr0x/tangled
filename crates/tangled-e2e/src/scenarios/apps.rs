//! Application-core acceptance over subprocess knots and real HTTP.
use anyhow::{ensure, Context, Result};
use tangled::http::{GetRepoAuthRequest, KnotClient, PutRecordRequest};

use crate::cluster::{Cluster, Node};

pub const REED_FEATURES: &[&str] = &[
    "REED-identity",
    "REED-text",
    "REED-reply-quote",
    "REED-edit-delete",
    "REED-follow",
    "REED-social",
    "REED-profile",
    "REED-media",
    "REED-search",
    "REED-offline-outbox",
    "REED-cross-knot",
    "REED-activitypub",
];
pub const LOOMED_FEATURES: &[&str] = &[
    "LOOMED-identity",
    "LOOMED-circle-wraps",
    "LOOMED-sealed-text",
    "LOOMED-sealed-image",
    "LOOMED-member-sync",
    "LOOMED-receipts",
    "LOOMED-search",
    "LOOMED-offline-outbox",
    "LOOMED-nonmember-deny",
];
pub const WEFT_FEATURES: &[&str] = &[
    "WEFT-counts",
    "WEFT-topology",
    "WEFT-lookup",
    "WEFT-handles",
    "WEFT-unreachable",
    "WEFT-private-exclusion",
    "WEFT-read-only",
];

fn workload(operations: usize, payload_bytes: usize) -> Result<String> {
    ensure!(
        operations > 0,
        "application workload needs at least one operation"
    );
    ensure!(
        (1..=32_000).contains(&payload_bytes),
        "application payload must be 1..=32000 bytes"
    );
    Ok("x".repeat(payload_bytes))
}

pub async fn reed(
    cluster: &mut Cluster,
    operations: usize,
    payload_bytes: usize,
) -> Result<Vec<&'static str>> {
    use reed_core::*;
    let payload = workload(operations, payload_bytes)?;
    let keys = ReedKeys::generate();
    let imported = ReedKeys::from_solana_json(&keys.export_json())?;
    ensure!(
        keys.did() == imported.did(),
        "Reed identity export/import changed DID"
    );
    ensure!(
        keys.identity().agreement_public_bytes() == imported.identity().agreement_public_bytes(),
        "Reed agreement key changed"
    );
    ensure!(
        ReedKeys::from_solana_json("[1,2,3]").is_err(),
        "bare keypair accepted"
    );
    let bob = ReedKeys::generate();
    let me = keys.did().as_str();
    let bob_did = bob.did().as_str();
    let client = cluster.nodes[0].siws(&imported.identity()).await?;
    let remote = cluster.nodes[1].siws(&bob.identity()).await?;
    let replica_dir = cluster.root.path().join("reed-replica");
    let mut replica = Replica::open(&replica_dir)?;
    let mut outbox = Outbox::new();
    let api = PostApi { keys: keys.clone() };
    let mut posts = Vec::new();
    for i in 0..operations {
        posts.push(api.compose_text(
            &mut replica,
            &mut outbox,
            &format!("reed-needle-{i} {payload}"),
            None,
            None,
        )?);
    }
    let original = replica
        .post(&posts[0])
        .context("local composed post absent")?;
    let reply = api.compose_text(&mut replica, &mut outbox, "reply", Some(&posts[0]), None)?;
    let quote = api.compose_text(&mut replica, &mut outbox, "quote", None, Some(&posts[0]))?;
    let edited = api.compose_text(&mut replica, &mut outbox, "draft", None, None)?;
    api.edit_text(&mut replica, &mut outbox, &edited, "edited over HTTP")?;
    // PostId is the content CID. A fresh pull sees the current record tip,
    // without the author's local aliases from earlier draft CIDs.
    let edited = replica
        .post(&edited)
        .context("local edited tip missing")?
        .id;
    let deleted = api.compose_text(&mut replica, &mut outbox, "delete me", None, None)?;
    api.delete(&mut replica, &mut outbox, &deleted)?;
    let deleted = replica
        .post(&deleted)
        .context("local tombstone tip missing")?
        .id;
    let image = b"\x89PNG\r\n\x1a\ncluster-image".to_vec();
    let video = b"\0\0\0\x18ftypisomcluster-video".to_vec();
    let media = api.compose_with_media(
        &mut replica,
        &mut outbox,
        "image and video",
        &[
            (
                image.clone(),
                "image/png".into(),
                Some("cluster image".into()),
            ),
            (video.clone(), "video/mp4".into(), None),
        ],
        None,
        None,
    )?;
    put_profile(
        &keys,
        &mut replica,
        &mut outbox,
        &Profile {
            did: me.clone(),
            handle: None,
            display_name: Some("Cluster Alice".into()),
            bio: Some("Reed cluster acceptance".into()),
            avatar_cid: None,
            source_url: None,
        },
    )?;
    follow(
        &keys,
        &mut replica,
        &mut outbox,
        &bob.did(),
        Some(&cluster.nodes[1].url),
    )?;
    ensure!(!outbox.is_empty(), "offline writes were not queued");
    let offline = KnotClient::http("http://127.0.0.1:0");
    // Use a record-only queue: Reed's blob queue currently removes a blob before
    // upload, so its retry semantics are outside this acceptance claim.
    let mut offline_outbox = Outbox::new();
    let queued = api.compose_text(
        &mut replica,
        &mut offline_outbox,
        "offline queued text",
        None,
        None,
    )?;
    ensure!(
        offline_outbox.drain(&offline).await.is_err(),
        "offline drain unexpectedly succeeded"
    );
    ensure!(
        !offline_outbox.is_empty(),
        "failed record upload lost pending write"
    );
    ensure!(
        Replica::open(&replica_dir)?.post(&queued).is_some(),
        "cached post did not survive reopen"
    );
    ensure!(
        outbox.drain(&client).await? >= operations + 10,
        "Reed workload was not uploaded"
    );
    ensure!(
        offline_outbox.drain(&client).await? == 1 && offline_outbox.is_empty(),
        "offline text was not retried"
    );
    let (bob_posts, _) = reed_core::seed::publish_posts_and_follows(
        &bob,
        &remote,
        &["remote knot timeline"],
        &[],
        None,
    )
    .await?;
    let mut sync = Sync::http(KnotUrl(cluster.nodes[0].url.clone()))?;
    sync.poll_once(&mut replica, &keys).await?;
    ensure!(
        replica.home(&me).iter().any(|i| i.post.id == bob_posts[0]),
        "followed remote author missing from home"
    );
    ensure!(replica.follows(&me, &bob_did), "follow missing");
    // Exercise reversible social state across actual publish/pull boundaries.
    like(&keys, &mut replica, &mut outbox, &posts[0])?;
    boost(&keys, &mut replica, &mut outbox, &posts[0])?;
    outbox.drain(&client).await?;
    let observer_dir = cluster.root.path().join("reed-observer");
    let mut observer = Replica::open(&observer_dir)?;
    sync.poll_once(&mut observer, &keys).await?;
    let item = observer
        .home(&me)
        .into_iter()
        .find(|i| i.post.id == posts[0])
        .context("published post missing")?;
    ensure!(
        item.post.liked_by_me
            && item.post.boosted_by_me
            && item.post.like_count == 1
            && item.post.boost_count == 1,
        "like/boost did not materialize"
    );
    unlike(&keys, &mut replica, &mut outbox, &posts[0])?;
    unboost(&keys, &mut replica, &mut outbox, &posts[0])?;
    mute(&keys, &mut replica, &mut outbox, &bob.did())?;
    outbox.drain(&client).await?;
    sync.poll_once(&mut observer, &keys).await?;
    ensure!(
        observer.home(&me).iter().all(|i| i.post.id != bob_posts[0]),
        "mute did not hide remote post"
    );
    unmute(&keys, &mut replica, &mut outbox, &bob.did())?;
    outbox.drain(&client).await?;
    sync.poll_once(&mut observer, &keys).await?;
    ensure!(
        observer.home(&me).iter().any(|i| i.post.id == bob_posts[0]),
        "unmute did not restore remote post"
    );
    block(&keys, &mut replica, &mut outbox, &bob.did())?;
    outbox.drain(&client).await?;
    sync.poll_once(&mut observer, &keys).await?;
    ensure!(
        observer.home(&me).iter().all(|i| i.post.id != bob_posts[0]),
        "block did not hide remote post"
    );
    unblock(&keys, &mut replica, &mut outbox, &bob.did())?;
    outbox.drain(&client).await?;
    sync.poll_once(&mut observer, &keys).await?;
    ensure!(
        observer.home(&me).iter().any(|i| i.post.id == bob_posts[0]),
        "unblock did not restore remote post"
    );
    let item = observer
        .home(&me)
        .into_iter()
        .find(|i| i.post.id == posts[0])
        .context("post missing after social undo")?;
    ensure!(
        !item.post.liked_by_me
            && !item.post.boosted_by_me
            && item.post.like_count == 0
            && item.post.boost_count == 0,
        "unlike/unboost did not materialize"
    );
    unfollow(&keys, &mut replica, &mut outbox, &bob.did())?;
    outbox.drain(&client).await?;
    sync.poll_once(&mut observer, &keys).await?;
    ensure!(
        !observer.follows(&me, &bob_did),
        "unfollow did not materialize"
    );
    ensure!(
        observer.home(&me).iter().all(|i| i.post.id != bob_posts[0]),
        "unfollowed author remained in home"
    );
    ensure!(
        observer.post(&posts[0]).context("text missing")?.text == original.text,
        "text changed across HTTP"
    );
    ensure!(
        observer
            .post(&reply)
            .context("reply missing")?
            .reply_to
            .as_ref()
            == Some(&posts[0]),
        "reply parent lost"
    );
    let quoted = observer.post(&quote).context("quote missing")?;
    ensure!(
        quoted.quote_of.as_ref() == Some(&posts[0]) && quoted.reply_to.is_none(),
        "quote/reply semantics mixed"
    );
    ensure!(
        observer.post(&edited).context("edited post missing")?.text == "edited over HTTP",
        "edit did not survive pull"
    );
    ensure!(
        observer
            .post(&deleted)
            .context("deleted post missing")?
            .deleted,
        "deletion not materialized"
    );
    ensure!(
        observer.discover(&me).iter().all(|i| i.post.id != deleted),
        "deleted post in discover"
    );
    ensure!(
        observer
            .profile(&me)
            .context("profile absent")?
            .display_name
            .as_deref()
            == Some("Cluster Alice"),
        "profile did not sync"
    );
    let media_post = observer.post(&media).context("media post absent")?;
    ensure!(
        media_post.media.len() == 2
            && media_post.media[0].mime == "image/png"
            && media_post.media[1].mime == "video/mp4",
        "image/video metadata lost"
    );
    for bytes in [&image, &video] {
        let cid = tangled::Cid::raw(bytes).to_string();
        ensure!(
            cluster.nodes[0]
                .client()
                .get_blob(&me, &cid)
                .await
                .context("Reed uploaded media HTTP retrieval")?
                == *bytes,
            "uploaded media bytes changed"
        );
        ensure!(
            observer.blob(&cid).as_deref() == Some(bytes.as_slice()),
            "media missing from cached replica"
        );
    }
    let hits = search(&observer, "REED-NEEDLE-");
    ensure!(
        hits.len() == operations && posts.iter().all(|id| hits.iter().any(|p| &p.id == id)),
        "local search lost workload posts"
    );
    let report = reed_core::seed::run_ap_mock(reed_core::seed::SeedOpts {
        knot: cluster.nodes[0].url.clone(),
        keys_path: cluster.root.path().join("reed-ap/keys.json"),
        create_local: true,
    })
    .await
    .context("local ActivityPub fixture")?;
    let ap_keys =
        reed_core::seed::load_local_keys(&cluster.root.path().join("reed-ap/keys.json"), false)?;
    let mut ap_replica = Replica::open(&cluster.root.path().join("reed-ap-replica"))?;
    sync.poll_once(&mut ap_replica, &ap_keys).await?;
    let federated: Vec<_> = ap_replica
        .home(&ap_keys.did().as_str())
        .into_iter()
        .filter(|i| i.post.attested)
        .collect();
    ensure!(
        federated.len() >= report.notes && federated.iter().all(|i| i.post.source_url.is_some()),
        "resolved AP notes missing attestation/source URL"
    );
    Ok(REED_FEATURES.to_vec())
}

fn car_records(bytes: &[u8]) -> Result<Vec<serde_json::Value>> {
    Ok(tangled::car::decode(bytes)?
        .blocks
        .into_iter()
        .filter_map(|(_, bytes)| {
            tangled::dagcbor::decode_json(&bytes)
                .ok()
                .filter(|v| v.get("collection").is_some())
        })
        .collect())
}

fn member_blob_client(
    node: &Node,
    owner: &tangled::identity::Identity,
    member: &tangled::identity::Identity,
    cid: &str,
) -> Result<KnotClient> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let res = format!("tangled://{}/blob/{cid}", owner.did_string());
    let mut cap = tangled::Capability {
        v: 1,
        iss: owner.did_string(),
        aud: member.did_string(),
        cmd: "blob/get".into(),
        res: res.clone(),
        nbf: now.saturating_sub(60),
        exp: now + 300,
        pol: vec![],
        prf: vec![],
        meta: None,
        sig: None,
    };
    cap.sign(owner.signing_key())?;
    let mut inv = tangled::capability::Invocation::new(
        &cap,
        "blob/get",
        &res,
        &node.identity.did_string(),
        now,
        [9; 16],
    )?;
    inv.sign(member.signing_key())?;
    Ok(node
        .client()
        .with_cap_header(tangled::capability::cap_authorization_header(
            &inv.encode_cbor()?,
        )))
}

pub async fn loomed(
    cluster: &mut Cluster,
    operations: usize,
    payload_bytes: usize,
) -> Result<Vec<&'static str>> {
    use loomed_core::*;
    let payload = workload(operations, payload_bytes)?;
    let alice = LoomedKeys::generate();
    let bob = LoomedKeys::generate();
    let outsider = LoomedKeys::generate();
    let imported = LoomedKeys::from_solana_json(&alice.export_json())?;
    ensure!(
        alice.did() == imported.did()
            && alice.identity().agreement_public_bytes()
                == imported.identity().agreement_public_bytes(),
        "Loomed identity roundtrip changed keys"
    );
    ensure!(
        LoomedKeys::from_solana_json("[1,2,3]").is_err(),
        "bare keypair accepted"
    );
    let owner_client = cluster.nodes[0].siws(&imported.identity()).await?;
    let member_client = cluster.nodes[0].siws(&bob.identity()).await?;
    let outsider_client = cluster.nodes[0].siws(&outsider.identity()).await?;
    let owner_dir = cluster.root.path().join("loomed-owner");
    let member_dir = cluster.root.path().join("loomed-member");
    let mut owner = Replica::open(&owner_dir)?;
    let chat = Chat {
        keys: alice.clone(),
    };
    let (circle, sys) = chat.create_circle(std::slice::from_ref(&bob.did()))?;
    owner.apply_record(&serde_json::to_vec(&sys)?)?;
    let state = owner
        .circle_state(&circle)
        .context("circle state missing")?;
    ensure!(
        state.members.contains(&alice.did().as_str())
            && state.members.contains(&bob.did().as_str())
            && !state.members.contains(&outsider.did().as_str()),
        "circle membership differs"
    );
    let mut outbox = Outbox::new();
    outbox.enqueue_record(sys.clone())?;
    outbox.enqueue_record(loomed_core::seed::wrap_for_member(
        &alice, &bob, &circle.0, &state,
    )?)?;
    let mut ids = Vec::new();
    for i in 0..operations {
        ids.push(chat.send_text(
            &mut owner,
            &mut outbox,
            &circle,
            &format!("loomed-needle-{i} {payload}"),
        )?);
    }
    let offline = KnotClient::http("http://127.0.0.1:0");
    ensure!(
        outbox.drain(&offline).await.is_err() && !outbox.is_empty(),
        "offline Loomed queue lost records"
    );
    ensure!(
        Replica::open(&owner_dir)?.messages(&circle).len() == operations,
        "cached Loomed history lost on reopen"
    );
    ensure!(
        outbox.drain(&owner_client).await? == operations + 2 && outbox.is_empty(),
        "Loomed queued records not retried"
    );
    let image = b"\x89PNG\r\n\x1a\nloomed-image".to_vec();
    let blob = owner_client.put_blob(image.clone()).await?;
    let image_id = chat.send_image(&mut owner, &mut outbox, &circle, &blob.cid)?;
    outbox.drain(&owner_client).await?;
    let mut member = Replica::open(&member_dir)?;
    let mut sync = Sync::http(KnotUrl(cluster.nodes[0].url.clone()))?;
    sync.poll_once(&mut member, &bob).await?;
    let messages = member.messages(&circle);
    ensure!(
        ids.iter().enumerate().all(|(i, id)| {
            let expected = format!("loomed-needle-{i} {payload}");
            messages.iter().any(|message| {
                &message.id == id && message.text.as_deref() == Some(expected.as_str())
            })
        }),
        "member could not decrypt wrapped messages"
    );
    ensure!(
        messages
            .iter()
            .any(|m| m.id == image_id && m.blob_cid.as_deref() == Some(blob.cid.as_str())),
        "sealed image reference did not decrypt"
    );
    ensure!(
        member_blob_client(
            &cluster.nodes[0],
            &alice.identity(),
            &bob.identity(),
            &blob.cid
        )?
        .get_blob(&alice.did().as_str(), &blob.cid)
        .await
        .context("Loomed member authorized image retrieval")?
            == image,
        "image blob bytes changed"
    );
    ensure!(
        search(&member, "LOOMED-NEEDLE-").len() == operations,
        "decrypted local search lost messages"
    );
    ensure!(
        Replica::open(&member_dir)?.messages(&circle).len() == operations + 1,
        "member cached history lost"
    );
    // A separate three-person circle proves the same member-wrap path for
    // groups, with a fresh third replica instead of owner-side decryption.
    let carol = LoomedKeys::generate();
    let (group, group_sys) = chat.create_circle(&[bob.did(), carol.did()])?;
    owner.apply_record(&serde_json::to_vec(&group_sys)?)?;
    let group_state = owner.circle_state(&group).context("group state missing")?;
    let mut group_outbox = Outbox::new();
    group_outbox.enqueue_record(group_sys)?;
    for member_keys in [&bob, &carol] {
        group_outbox.enqueue_record(loomed_core::seed::wrap_for_member(
            &alice,
            member_keys,
            &group.0,
            &group_state,
        )?)?;
    }
    let group_message = chat.send_text(
        &mut owner,
        &mut group_outbox,
        &group,
        "three member group delivery",
    )?;
    ensure!(
        group_outbox.drain(&owner_client).await? == 4,
        "group setup not published"
    );
    let mut third_replica = Replica::open(&cluster.root.path().join("loomed-group-third"))?;
    sync.poll_once(&mut third_replica, &carol).await?;
    sync.poll_once(&mut member, &bob).await?;
    ensure!(
        third_replica
            .messages(&group)
            .iter()
            .any(|m| m.id == group_message
                && m.text.as_deref() == Some("three member group delivery")),
        "third group member cannot decrypt"
    );
    ensure!(
        member
            .messages(&group)
            .iter()
            .any(|m| m.id == group_message),
        "second group member cannot decrypt"
    );
    ensure!(
        third_replica.messages(&circle).is_empty(),
        "group-only member read separate DM"
    );
    let receipt = seal_receipt(
        &bob,
        &circle,
        &member
            .circle_state(&circle)
            .context("member circle missing")?,
        &ids[0],
    )?;
    let receipt_rkey = receipt["rkey"]
        .as_str()
        .context("receipt rkey absent")?
        .to_string();
    let mut receipts = Outbox::new();
    receipts.enqueue_record(receipt)?;
    ensure!(
        receipts.drain(&member_client).await? == 1,
        "receipt not published"
    );
    // Receipts live in their author's repo. Pull that repo explicitly with the
    // owner's production circle capability, then use Loomed's receipt parser.
    let cap_json = sys["body"]["grants"]
        .as_array()
        .context("grants absent")?
        .iter()
        .find(|g| g["aud"] == alice.did().as_str())
        .context("owner grant missing")?;
    let cap = tangled::Capability::from_json(cap_json)?;
    let res = cap.res.clone();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let mut inv = tangled::capability::Invocation::new(
        &cap,
        "circle/read",
        &res,
        &cluster.nodes[0].identity.did_string(),
        now,
        [7; 16],
    )?;
    inv.sign(alice.identity().signing_key())?;
    let authed =
        cluster.nodes[0]
            .client()
            .with_cap_header(tangled::capability::cap_authorization_header(
                &inv.encode_cbor()?,
            ));
    let records = car_records(
        &authed
            .get_repo_auth(&GetRepoAuthRequest {
                did: bob.did().as_str(),
                since: None,
                blobs: Some(false),
            })
            .await?,
    )?;
    let record = records
        .iter()
        .find(|r| r["collection"] == "pub.loomed.receipt" && r["rkey"] == receipt_rkey)
        .context("HTTP receipt missing")?;
    let opened = loomed_core::receipts::open_receipt(&alice, &state, record)?;
    owner.apply_record(&serde_json::to_vec(&opened)?)?;
    ensure!(
        owner
            .messages(&circle)
            .iter()
            .find(|m| m.id == ids[0])
            .context("receipt target missing")?
            .receipts
            .contains(&bob.did().as_str()),
        "receipt did not materialize"
    );
    let public = car_records(
        &cluster.nodes[0]
            .client()
            .get_repo(&alice.did().as_str(), None, Some(false))
            .await?,
    )?;
    ensure!(
        public.iter().all(|r| r["vis"] != "sealed"),
        "public CAR leaked sealed payload"
    );
    let mut outsider_replica = Replica::open(&cluster.root.path().join("loomed-outsider"))?;
    sync.poll_once(&mut outsider_replica, &outsider).await?;
    ensure!(
        outsider_replica.messages(&circle).is_empty(),
        "outsider decrypted circle messages"
    );
    let denied = outsider_client
        .get_repo_auth(&GetRepoAuthRequest {
            did: alice.did().as_str(),
            since: None,
            blobs: Some(false),
        })
        .await;
    ensure!(
        matches!(
            denied,
            Err(tangled::Error::Http {
                status: 401 | 403,
                ..
            })
        ),
        "outsider authorized repository request was not denied"
    );
    Ok(LOOMED_FEATURES.to_vec())
}

pub async fn weft(
    cluster: &mut Cluster,
    operations: usize,
    payload_bytes: usize,
) -> Result<Vec<&'static str>> {
    use weft_core::*;
    let payload = workload(operations, payload_bytes)?;
    // Establish known public deltas on every independently persisted knot.
    for (index, node) in cluster.nodes.iter().enumerate() {
        let keys = reed_core::ReedKeys::generate();
        let client = node.siws(&keys.identity()).await?;
        let before = node.client().stats().await?;
        let texts: Vec<_> = (0..operations)
            .map(|i| format!("weft-{index}-{i} {payload}"))
            .collect();
        let refs: Vec<_> = texts.iter().map(String::as_str).collect();
        reed_core::seed::publish_posts_and_follows(&keys, &client, &refs, &[], None).await?;
        let after = node.client().stats().await?;
        ensure!(
            after.counts.records == before.counts.records + operations as u64,
            "public record count delta differs"
        );
        ensure!(
            after.counts.identities == before.counts.identities + 1,
            "public identity count delta differs"
        );
    }
    let explorer = Explorer::http(KnotUrl(cluster.nodes[0].url.clone()))?;
    let before: Vec<_> = futures_util::future::try_join_all(
        cluster
            .nodes
            .iter()
            .map(|n| async move { n.client().stats().await }),
    )
    .await?;
    let network = explorer.assemble().await?;
    ensure!(
        network.knots.iter().filter(|n| n.reachable).count() == 3,
        "three advertised knots not reachable"
    );
    ensure!(
        network.totals.records == before.iter().map(|s| s.counts.records).sum::<u64>(),
        "network aggregate record count differs"
    );
    ensure!(
        network.totals.identities == before.iter().map(|s| s.counts.identities).sum::<u64>(),
        "network aggregate identity count differs"
    );
    for node in &cluster.nodes[1..] {
        ensure!(
            network.edges.iter().any(|e| e.from == cluster.nodes[0].url
                && e.to == node.url
                && matches!(e.kind, EdgeKind::Peer)),
            "advertised peer edge missing"
        );
        ensure!(
            matches!(explorer.lookup(&node.url).await.context("Weft peer knot URL lookup")?, LookupHit::Knot { url, .. } if url == node.url),
            "knot lookup mismatch"
        );
    }
    let hosted = before[0]
        .topology
        .hosted
        .first()
        .context("no public identity for lookup")?;
    ensure!(
        matches!(explorer.lookup(&hosted.did).await.context("Weft hosted DID lookup")?, LookupHit::Identity { did, records: Some(n), .. } if did == hosted.did && n == hosted.records),
        "DID lookup mismatch"
    );
    ensure!(explorer.lookup("").await.is_err(), "empty lookup accepted");
    for (node, expected) in cluster.nodes.iter().zip(&before) {
        ensure!(
            node.client().stats().await?.counts == expected.counts,
            "read-only explorer mutated persistent counters"
        );
    }
    let dead_url = "http://127.0.0.1:0".to_string();
    let isolated = Node::knot(
        cluster.root.path(),
        "weft-unreachable",
        std::slice::from_ref(&dead_url),
    )
    .await?;
    cluster.nodes.push(isolated);
    let isolated = cluster
        .nodes
        .last()
        .context("unreachable fixture node missing")?;
    let unreachable = Explorer::http(KnotUrl(isolated.url.clone()))?
        .assemble()
        .await?;
    ensure!(
        unreachable
            .knots
            .iter()
            .any(|n| n.url == dead_url && !n.reachable && n.stats.is_none()),
        "unreachable advertised peer was omitted"
    );
    // Publish public circle metadata, then sealed content. Only the first may
    // affect public record/circle aggregates or the hosted identity list.
    let private_keys = loomed_core::LoomedKeys::generate();
    let private_client = cluster.nodes[0].siws(&private_keys.identity()).await?;
    let chat = loomed_core::Chat {
        keys: private_keys.clone(),
    };
    let (circle, sys) = chat.create_circle(&[])?;
    let mut replica = loomed_core::Replica::open(&cluster.root.path().join("weft-private"))?;
    replica.apply_record(&serde_json::to_vec(&sys)?)?;
    private_client
        .put_record(&PutRecordRequest { record: sys })
        .await?;
    let public_before = explorer.local_stats().await?;
    let mut pending = loomed_core::Outbox::new();
    chat.send_text(&mut replica, &mut pending, &circle, "weft-private-secret")?;
    pending.drain(&private_client).await?;
    let public_after = explorer.local_stats().await?;
    ensure!(
        public_before.counts.records == public_after.counts.records
            && public_before.counts.circles == public_after.counts.circles
            && public_before.counts.identities == public_after.counts.identities
            && public_before.counts.blobs == public_after.counts.blobs,
        "sealed write changed public aggregates"
    );
    ensure!(
        serde_json::to_string(&public_after)?
            .find("weft-private-secret")
            .is_none(),
        "stats exposed private text"
    );
    weft_handle_fixture(cluster).await?;
    Ok(WEFT_FEATURES.to_vec())
}

/// Own the fixture task so every error path closes its listener.
struct RpcFixture {
    url: String,
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for RpcFixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl RpcFixture {
    async fn start(identity: &tangled::identity::Identity) -> Result<Self> {
        use base64::Engine;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let expected =
            tangled::identity::encode_pubkey_base58(&tangled::handle::sns_name_account("cluster")?);
        let mut account = vec![0; 96];
        account[32..64].copy_from_slice(&identity.pubkey_bytes());
        let encoded = base64::engine::general_purpose::STANDARD.encode(account);
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let task_calls = calls.clone();
        let task = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let result = tokio::time::timeout(std::time::Duration::from_secs(3), async {
                    let mut request = Vec::new();
                    let (header_len, body_len) = loop {
                        let mut chunk = [0u8; 4096];
                        let n = socket.read(&mut chunk).await?;
                        if n == 0 { return Ok::<(), std::io::Error>(()); }
                        request.extend_from_slice(&chunk[..n]);
                        if request.len() > 64 * 1024 { return Ok(()); }
                        if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                            let headers = String::from_utf8_lossy(&request[..end]);
                            let len = headers.lines().find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().ok()).flatten()
                            }).unwrap_or(0);
                            break (end + 4, len);
                        }
                    };
                    while request.len() < header_len + body_len {
                        let mut chunk = [0u8; 4096];
                        let n = socket.read(&mut chunk).await?;
                        if n == 0 { return Ok(()); }
                        request.extend_from_slice(&chunk[..n]);
                        if request.len() > 64 * 1024 { return Ok(()); }
                    }
                    let request: serde_json::Value = serde_json::from_slice(&request[header_len..header_len + body_len]).unwrap_or_default();
                    let valid = request["method"] == "getAccountInfo" && request["params"][0] == expected;
                    if valid { task_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
                    let value = if valid { serde_json::json!({"data":[encoded,"base64"]}) } else { serde_json::Value::Null };
                    let body = serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":{"value":value}}).to_string();
                    let header = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                    socket.write_all(header.as_bytes()).await?;
                    socket.write_all(body.as_bytes()).await?;
                    socket.shutdown().await
                }).await;
                if result.is_err() {
                    continue;
                }
            }
        });
        Ok(Self { url, calls, task })
    }
}

async fn weft_handle_fixture(cluster: &mut Cluster) -> Result<()> {
    let identity = tangled::identity::Identity::generate();
    let fixture = RpcFixture::start(&identity).await?;
    let node =
        Node::knot_with_solana_rpc(cluster.root.path(), "weft-handles", &[], &fixture.url).await?;
    cluster.nodes.push(node);
    let node = cluster.nodes.last().context("SNS fixture node missing")?;
    let explorer = weft_core::Explorer::http(weft_core::KnotUrl(node.url.clone()))?;
    // The RPC proves forward ownership; without a signed reverse claim this
    // must fail even though the fixture recognizes the name.
    ensure!(
        explorer.lookup("cluster.sol").await.is_err(),
        "one-sided SNS handle accepted"
    );
    let unsigned = serde_json::json!({
        "v": 1, "type": tangled::Cid::dag_cbor(b"sys.id").to_string(),
        "collection": "sys.id", "rkey": "self", "author": identity.did_string(),
        "ts": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_micros() as u64,
        "prev": [], "vis": "public",
        "body": {
            "v":1, "pubkey":hex::encode(identity.pubkey_bytes()),
            "agreement":hex::encode(identity.agreement_public_bytes()),
            "alsoKnownAs":["cluster.sol"],
        },
    });
    let record = tangled::record::sign_json_record(identity.signing_key(), &unsigned)?.json;
    node.siws(&identity)
        .await?
        .put_record(&PutRecordRequest { record })
        .await?;
    ensure!(
        matches!(explorer.lookup("cluster.sol").await.context("Weft signed reverse-claim SNS handle lookup")?, weft_core::LookupHit::Identity { did, handle: Some(handle), .. } if did == identity.did_string() && handle == "cluster.sol"),
        "local SNS handle resolved incorrectly"
    );
    ensure!(
        fixture.calls.load(std::sync::atomic::Ordering::Relaxed) >= 3,
        "SNS fixture did not receive real resolver requests"
    );
    Ok(())
}
