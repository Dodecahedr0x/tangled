//! Knot engine: vis/cap/CAR/MST admission used by tests and the HTTP server.
//!
//! In-memory maps are the store. An optional `data_dir` snapshots records and
//! blobs to a filesystem layout. There is no second vis/cap implementation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use base64::Engine;

use crate::capability::{self, Capability, Invocation};
use crate::car::{self, Car};
use crate::cid::Cid;
use crate::error::{Error, Result};
use crate::http::{
    json_ok, query, ApiError, BacklinkHit, BacklinkQuery, CapVerifyResponse, GetBlocksRequest,
    GetBlocksResponse, GetHeadResponse, GetRecordResponse, GetRepoAuthRequest, HostedIdentity,
    IdentityResolveResponse, ImportResponse, KnotRequest, KnotResponse, ListRecordItem,
    ListRecordsResponse, Method, NodeCounts, NodeDescribe, NodeRates, NodeStats, NodeTopology,
    PutBlobResponse, PutRecordRequest, PutRecordResponse, SubscribeMessage, SubscribeOp,
};
use crate::identity::Did;
use crate::mst::Mst;
use crate::page;
use crate::record::{self, compact_allowed};
use crate::sync::{self, getblob_auth, BlobAuth, BlobRef};

/// One stored record version (MST tip is separate; prev-closure is kept).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StoredRecord {
    pub did: String,
    pub collection: String,
    pub rkey: String,
    pub vis: String,
    pub cid: String,
    pub record: serde_json::Value,
    pub circle: Option<String>,
    pub deleted: bool,
}

/// Knot admission + store. Tests use [`MockKnot`] (this type). The HTTP server
/// uses the same engine, optionally with a `data_dir`.
#[derive(Clone, Default)]
pub struct KnotEngine {
    pub describe: Option<NodeDescribe>,
    pub heads: HashMap<String, GetHeadResponse>,
    /// Legacy lookup used by older unit tests; `getRecord` prefers [`Self::stored`].
    pub records: HashMap<(String, String, String), GetRecordResponse>,
    pub blobs: HashMap<(String, String), Vec<u8>>,
    pub backlinks: HashMap<String, BacklinkQuery>,
    pub identities: HashMap<String, IdentityResolveResponse>,
    pub put_records: Vec<serde_json::Value>,
    /// Every accepted version, including superseded CRDT ops.
    pub stored: Vec<StoredRecord>,
    /// MST tip CID per `(did, collection, rkey)`.
    pub tips: HashMap<(String, String, String), String>,
    pub msts: HashMap<String, Mst>,
    pub rev: HashMap<String, u64>,
    /// Injected unix seconds (capability windows). Not wall clock.
    pub now: u64,
    /// CID → DAG-CBOR or raw block bytes (CAR export).
    pub blocks: HashMap<String, Vec<u8>>,
    /// Revoked capability CIDs (`sys.revocation`).
    pub revoked: HashMap<String, ()>,
    /// When false, `queryBacklinks` returns MethodNotImplemented.
    pub backlink_index: bool,
    /// When true, page compact writes must pass `PAGE_COMPACT_MIN_OPS` / interval.
    pub compact_admission: bool,
    /// Snapshot directory. `None` keeps the engine memory-only.
    pub data_dir: Option<PathBuf>,
    /// Public base URL written into `getHead.knot` (e.g. `http://127.0.0.1:3000`).
    pub knot_base: Option<String>,
    /// Public commit events drained by the HTTP layer after `handle`.
    pub pending_events: Vec<SubscribeMessage>,
    /// SIWS challenges keyed by nonce.
    pub challenges: HashMap<String, crate::session::ChallengeRecord>,
    /// SIWS sessions keyed by cookie token.
    pub sessions: HashMap<String, crate::session::SessionRecord>,
    /// Handle TXT / well-known lookup (injected in tests; `live` in production).
    pub handle_resolver: crate::handle::HandleResolver,
    /// Successful invocation counts keyed by leaf capability CID (`max_uses`).
    pub cap_uses: HashMap<String, u64>,
    /// Optional media type stored with a blob (`did`, `cid`).
    pub blob_types: HashMap<(String, String), String>,
    /// Subscribe event log (persisted next to the snapshot). ponytail: cap 256.
    pub event_log: Vec<SubscribeMessage>,
    /// Ed25519 seeds for hosted DIDs (commit signatures). Not persisted.
    pub signing_keys: HashMap<String, [u8; 32]>,
    /// Peer knot URLs learned from other knots' `describe.peers`. Not advertised.
    pub learned_peers: Vec<String>,
    /// CID-only private archive erasure fences; replayed from the durable fence file.
    pub private_archive_erasure_cids: std::collections::HashSet<String>,
}

/// Test and in-process alias — same engine the server runs.
pub type MockKnot = KnotEngine;

struct StoreOpts<'a> {
    prev: Option<String>,
    check_prev: bool,
    headers: &'a [(String, String)],
    skip_commit: bool,
}

impl KnotEngine {
    /// Knot hosting `did` with public/cap/sealed/group profiles.
    pub fn hosting(did: &str) -> Self {
        let mut s = Self {
            describe: Some(NodeDescribe {
                v: 1,
                did: did.to_string(),
                profiles: vec![
                    "knot.public".into(),
                    "knot.cap".into(),
                    "knot.sealed".into(),
                    "knot.group".into(),
                ],
                protocols: vec!["nostr".into()],
                max_blob: 134_217_728,
                custodial_sign: false,
                peers: vec![],
            }),
            now: 1_700_000_000,
            backlink_index: true,
            compact_admission: true,
            ..Default::default()
        };
        s.identities.insert(
            did.to_string(),
            IdentityResolveResponse {
                did: did.to_string(),
                id_cid: None,
                endpoints: vec![format!("inproc://{did}")],
                document: None,
            },
        );
        s
    }

    /// Register the identity key used to sign Commits for this DID.
    pub fn register_identity(&mut self, id: &crate::identity::Identity) {
        let (ed, _) = id.to_seeds();
        self.signing_keys.insert(id.did_string(), ed);
    }

    /// Load a snapshot from `dir`, or create a new hosting knot and persist it.
    pub fn open(dir: impl AsRef<Path>, did: &str) -> Result<Self> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir).map_err(|e| Error::Transport(e.to_string()))?;
        let describe_path = dir.join("describe.json");
        let mut engine = if describe_path.exists() {
            load_engine(dir)?
        } else {
            let mut e = Self::hosting(did);
            e.data_dir = Some(dir.to_path_buf());
            e.persist()?;
            e
        };
        engine.data_dir = Some(dir.to_path_buf());
        if engine.describe.is_none() {
            engine.describe = Self::hosting(did).describe;
        }
        Ok(engine)
    }

    pub fn with_knot_base(mut self, url: impl Into<String>) -> Self {
        let url = url.into();
        self.knot_base = Some(url.clone());
        if let Some(did) = self.describe.as_ref().map(|d| d.did.clone()) {
            let id_cid = self.identities.get(&did).and_then(|i| i.id_cid.clone());
            self.identities.insert(
                did.clone(),
                IdentityResolveResponse {
                    did,
                    id_cid,
                    endpoints: vec![url],
                    document: None,
                },
            );
        }
        self
    }

    /// Advertise only these operator-marked public knot URLs on `node.describe`.
    pub fn set_public_peers(&mut self, peers: &[String]) {
        let mut out = Vec::new();
        crate::http::merge_peer_urls(&mut out, peers);
        if let Some(d) = self.describe.as_mut() {
            d.peers = out;
        }
    }

    /// Remember peer URLs from another knot's `describe`. Never copied into `describe.peers`.
    pub fn learn_peers(&mut self, urls: &[String]) {
        crate::http::merge_peer_urls(&mut self.learned_peers, urls);
        let _ = self.persist();
    }

    pub fn learned_peers(&self) -> &[String] {
        &self.learned_peers
    }

    /// Public-only aggregates. Cap/sealed, learned peers, and cap/session counters stay out.
    pub fn node_stats(&self) -> Result<NodeStats> {
        let d = self.describe.as_ref().ok_or_else(|| Error::Http {
            status: 404,
            error: "NotFound".into(),
        })?;
        const HOSTED_CAP: usize = 64;
        const RETAINED: u64 = 256;

        let tips: Vec<&StoredRecord> = self.live_public_tips().collect();
        let mut by_did: HashMap<String, u64> = HashMap::new();
        let mut records = 0u64;
        let mut circles = 0u64;
        let mut foreign = 0u64;
        let mut public_blobs = std::collections::HashSet::new();
        for rec in &tips {
            records += 1;
            *by_did.entry(rec.did.clone()).or_insert(0) += 1;
            if rec.collection == "sys.circle" {
                circles += 1;
            }
            if rec.collection == "sys.foreign" {
                foreign += 1;
            }
            let text = rec.record.to_string();
            for (did, cid) in self.blobs.keys() {
                if did == &rec.did && text.contains(cid) {
                    public_blobs.insert(cid.clone());
                }
            }
        }

        let mut hosted: Vec<HostedIdentity> = by_did
            .iter()
            .map(|(did, n)| HostedIdentity {
                did: did.clone(),
                handle: self
                    .also_known_as_for(did)
                    .into_iter()
                    .find(|h| !h.starts_with("did:")),
                rev: self.heads.get(did).map(|h| h.rev.clone()),
                records: *n,
            })
            .collect();
        hosted.sort_by(|a, b| a.did.cmp(&b.did));
        let truncated = hosted.len() > HOSTED_CAP;
        hosted.truncate(HOSTED_CAP);

        let identities = by_did.len() as u64;
        let repos = by_did
            .keys()
            .filter(|did| self.heads.contains_key(*did))
            .count() as u64;
        let commits = self.event_log.len() as u64;

        Ok(NodeStats {
            v: 1,
            did: d.did.clone(),
            as_of: self.now,
            counts: NodeCounts {
                identities,
                repos,
                records,
                blobs: public_blobs.len() as u64,
                peers: d.peers.len() as u64,
                circles,
                foreign,
                commits,
            },
            rates: NodeRates {
                public_commits: commits,
                retained: RETAINED,
            },
            topology: NodeTopology {
                peers: d.peers.clone(),
                hosted,
                truncated,
            },
        })
    }

    fn live_public_tips(&self) -> impl Iterator<Item = &StoredRecord> {
        self.tips.values().filter_map(|cid| {
            self.stored
                .iter()
                .find(|r| &r.cid == cid && r.vis == "public" && !r.deleted)
        })
    }

    pub fn take_events(&mut self) -> Vec<SubscribeMessage> {
        std::mem::take(&mut self.pending_events)
    }

    fn siws_domain(&self) -> String {
        self.knot_base
            .as_deref()
            .and_then(|u| {
                u.strip_prefix("https://")
                    .or_else(|| u.strip_prefix("http://"))
            })
            .map(|rest| {
                rest.split('/')
                    .next()
                    .unwrap_or(rest)
                    .split(':')
                    .next()
                    .unwrap_or(rest)
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "localhost".into())
    }

    fn session_did(&self, req: &KnotRequest) -> Option<String> {
        let token = crate::session::parse_session_cookie(&req.headers)?;
        let s = self.sessions.get(&token)?;
        if self.now > s.exp {
            return None;
        }
        Some(s.did.clone())
    }

    fn session_challenge(&mut self, req: &KnotRequest) -> Result<KnotResponse> {
        let did = query(&req.query, "did")?;
        Did::parse(&did).map_err(|_| Error::Http {
            status: 400,
            error: "InvalidRequest".into(),
        })?;
        let domain = self.siws_domain();
        let nonce = crate::session::new_nonce();
        let issued_at = self.now;
        let message = crate::session::siws_message(&domain, &did, &nonce, issued_at);
        self.challenges.insert(
            nonce.clone(),
            crate::session::ChallengeRecord {
                did: did.clone(),
                domain: domain.clone(),
                issued_at,
                exp: self
                    .now
                    .saturating_add(crate::session::CHALLENGE_TTL_SECONDS),
            },
        );
        json_ok(&crate::session::SiwsChallenge {
            domain,
            did,
            nonce,
            issued_at,
            message,
        })
    }

    fn session_verify(&mut self, req: &KnotRequest) -> Result<KnotResponse> {
        let body = req.body.as_deref().ok_or_else(|| Error::Http {
            status: 400,
            error: "InvalidRequest".into(),
        })?;
        let parsed: crate::session::SiwsVerifyRequest = serde_json::from_slice(body)
            .map_err(|e| Error::Transport(format!("session.verify json: {e}")))?;
        let ch = self
            .challenges
            .remove(&parsed.nonce)
            .ok_or_else(|| Error::Http {
                status: 400,
                error: "InvalidRequest".into(),
            })?;
        if self.now > ch.exp {
            return api_error(400, "InvalidRequest", "expired challenge");
        }
        if ch.did != parsed.did {
            return api_error(400, "InvalidRequest", "did mismatch");
        }
        let message =
            crate::session::siws_message(&ch.domain, &ch.did, &parsed.nonce, ch.issued_at);
        if crate::session::verify_siws_signature(&parsed.did, &message, &parsed.signature).is_err()
        {
            return api_error(400, "InvalidSignature", "SIWS signature");
        }
        let token = crate::session::new_session_token();
        let exp = self.now.saturating_add(crate::session::SESSION_TTL_SECONDS);
        self.sessions.insert(
            token.clone(),
            crate::session::SessionRecord {
                did: parsed.did.clone(),
                exp,
            },
        );
        let mut resp = json_ok(&crate::session::SiwsVerifyResponse {
            ok: true,
            did: parsed.did,
            exp,
        })?;
        resp.set_cookie = Some(crate::session::set_cookie_header_secure(
            &token,
            self.knot_base
                .as_deref()
                .is_some_and(|u| u.starts_with("https://")),
        ));
        Ok(resp)
    }

    fn ap_resolve(&self, req: &KnotRequest) -> Result<KnotResponse> {
        let resource = query(&req.query, "resource")?;
        let actor = if resource.starts_with("http://") || resource.starts_with("https://") {
            crate::foreign::canonical_foreign_id("activitypub", &resource)?
        } else {
            crate::foreign::activitypub_acct_actor(&resource)?
        };
        let id = crate::foreign::activitypub_actor_identity(&actor);
        let handle = resource
            .strip_prefix("acct:")
            .or_else(|| resource.strip_prefix('@'))
            .unwrap_or(&resource)
            .to_string();
        json_ok(&crate::http::ApResolveResponse {
            did: id.did_string(),
            actor,
            handle,
            display_name: None,
        })
    }

    fn identity_resolve(&self, req: &KnotRequest) -> Result<KnotResponse> {
        if let Some(handle) = req
            .query
            .iter()
            .find(|(k, _)| k == "handle")
            .map(|(_, v)| v.clone())
        {
            return self.resolve_handle_query(&handle);
        }
        let did = query(&req.query, "did")?;
        let mut id = self
            .identities
            .get(&did)
            .cloned()
            .ok_or_else(|| Error::Http {
                status: 404,
                error: "NotFound".into(),
            })?;
        id.id_cid = self.sys_id_cid(&did).or(id.id_cid);
        id.document = Some(self.did_document_for(&did));
        json_ok(&id)
    }

    fn resolve_handle_query(&self, handle: &str) -> Result<KnotResponse> {
        let proof = crate::handle::lookup_handle(handle, &self.handle_resolver).map_err(|e| {
            Error::Http {
                status: 404,
                error: format!("NotFound: {e}"),
            }
        })?;
        let also = self.also_known_as_for(&proof.did);
        crate::handle::resolve_handle(handle, &self.handle_resolver, &also).map_err(|e| {
            Error::Http {
                status: 404,
                error: format!("NotFound: {e}"),
            }
        })?;
        let endpoints = self
            .identities
            .get(&proof.did)
            .map(|i| i.endpoints.clone())
            .or_else(|| self.heads.get(&proof.did).map(|h| vec![h.knot.clone()]))
            .unwrap_or_else(|| vec![self.knot_uri(&proof.did)]);
        let id_cid = self
            .identities
            .get(&proof.did)
            .and_then(|i| i.id_cid.clone());
        json_ok(&IdentityResolveResponse {
            did: proof.did.clone(),
            id_cid: id_cid.or_else(|| self.sys_id_cid(&proof.did)),
            endpoints,
            document: Some(self.did_document_for(&proof.did)),
        })
    }

    /// Current `sys.id` `pubkey`, if one is stored. Absent means the DID key still signs.
    fn operational_pubkey(&self, did: &str) -> Option<[u8; 32]> {
        let cid = self
            .tips
            .get(&(did.to_string(), "sys.id".into(), "self".into()))?;
        let rec = self.stored.iter().rev().find(|r| {
            &r.cid == cid
                && r.did == did
                && r.collection == "sys.id"
                && r.rkey == "self"
                && !r.deleted
        })?;
        let body = rec.record.get("body")?;
        crate::identity::parse_sys_id_key32(body.get("pubkey")?)
    }

    fn sys_id_cid(&self, did: &str) -> Option<String> {
        self.tips
            .get(&(did.to_string(), "sys.id".into(), "self".into()))
            .cloned()
    }

    fn agreement_for(&self, did: &str) -> Option<[u8; 32]> {
        let rec = self
            .stored
            .iter()
            .rev()
            .find(|r| r.did == did && r.collection == "sys.id" && r.rkey == "self")?;
        let body = rec.record.get("body").unwrap_or(&rec.record);
        crate::identity::parse_sys_id_key32(body.get("agreement")?)
    }

    fn did_document_for(&self, did: &str) -> serde_json::Value {
        let Ok(parsed) = Did::parse(did) else {
            return serde_json::json!({ "id": did });
        };
        crate::identity::resolve_did_document(&parsed, self.agreement_for(did).as_ref())
    }

    fn also_known_as_for(&self, did: &str) -> Vec<String> {
        self.stored
            .iter()
            .rev()
            .find(|r| r.did == did && r.collection == "sys.id" && r.rkey == "self")
            .and_then(|r| r.record.get("body").or(Some(&r.record)))
            .and_then(|b| b.get("alsoKnownAs"))
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn import_authorized(&self, req: &KnotRequest, did: &str) -> Result<bool> {
        if self.session_did(req).as_deref() == Some(did) {
            return Ok(true);
        }
        let Some((cap, cap_json, inv)) = presented_invocation(req)? else {
            return Ok(false);
        };
        if self
            .verify_presented(&cap, &cap_json, inv.as_ref())
            .is_err()
        {
            return Ok(false);
        }
        let res = format!("tangled://{did}");
        Ok(capability::authorizes(&cap, "repo/write", &res, self.now).is_ok())
    }

    fn knot_uri(&self, did: &str) -> String {
        self.knot_base
            .clone()
            .unwrap_or_else(|| format!("inproc://{did}"))
    }

    pub fn persist(&self) -> Result<()> {
        let Some(dir) = &self.data_dir else {
            return Ok(());
        };
        persist_engine(self, dir)
    }

    /// Remove host-retained private import ciphertext, including historical revisions.
    /// Signed historical commits/MST nodes retain opaque CIDs, never the removed payload.
    /// The CID-only fence is replayed on restart before the engine becomes available.
    pub fn erase_private_archive(
        &mut self,
        owner: &str,
        record_cids: &[String],
        blob_cids: &[String],
    ) -> Result<()> {
        use std::collections::HashSet;
        let records: HashSet<_> = record_cids.iter().cloned().collect();
        for rec in self.stored.iter().filter(|r| records.contains(&r.cid)) {
            if rec.did != owner
                || rec.vis != "sealed"
                || ![
                    "app.tangled.import.object",
                    "app.tangled.import.file",
                    "app.tangled.import.batch",
                    "app.tangled.import.key",
                ]
                .contains(&rec.collection.as_str())
            {
                return Err(Error::Transport(
                    "Erasure may only remove the owner's private import records".into(),
                ));
            }
        }
        // Persist intent before removing any file. Retried/restarted erasure is idempotent.
        if let Some(dir) = &self.data_dir {
            let path = dir.join("private-archive-erasures.json");
            let mut fences: Vec<(String, Vec<String>, Vec<String>)> = if path.exists() {
                read_json(&path)?
            } else {
                Vec::new()
            };
            if let Some((_, old_records, old_blobs)) =
                fences.iter_mut().find(|(did, _, _)| did == owner)
            {
                old_records.extend(record_cids.iter().cloned());
                old_records.sort();
                old_records.dedup();
                old_blobs.extend(blob_cids.iter().cloned());
                old_blobs.sort();
                old_blobs.dedup();
            } else {
                fences.push((owner.into(), record_cids.to_vec(), blob_cids.to_vec()));
            }
            let temp = dir.join("private-archive-erasures.pending");
            let bytes = serde_json::to_vec(&fences).map_err(|e| Error::Transport(e.to_string()))?;
            use std::io::Write;
            let mut file = std::fs::File::create(&temp).map_err(fs_err)?;
            file.write_all(&bytes).map_err(fs_err)?;
            file.sync_all().map_err(fs_err)?;
            std::fs::rename(&temp, &path).map_err(fs_err)?;
            std::fs::File::open(dir)
                .and_then(|f| f.sync_all())
                .map_err(fs_err)?;
        }
        let removed = self
            .stored
            .iter()
            .filter(|r| r.did == owner && records.contains(&r.cid))
            .cloned()
            .collect::<Vec<_>>();
        self.stored
            .retain(|r| r.did != owner || !records.contains(&r.cid));
        self.tips
            .retain(|(did, _, _), cid| did != owner || !records.contains(cid));
        self.records
            .retain(|(did, _, _), r| did != owner || !records.contains(&record_cid(&r.record)));
        self.put_records
            .retain(|r| r["author"].as_str() != Some(owner) || !records.contains(&record_cid(r)));
        // Never remove shared chunks still referenced by any retained record or another owner.
        let blobs = blob_cids
            .iter()
            .filter(|cid| {
                !self
                    .stored
                    .iter()
                    .any(|r| r.did == owner && json_contains_str(&r.record, cid))
            })
            .cloned()
            .collect::<HashSet<_>>();
        self.private_archive_erasure_cids
            .extend(records.iter().cloned());
        self.private_archive_erasure_cids.extend(
            blobs
                .iter()
                .filter(|cid| {
                    !self
                        .blobs
                        .keys()
                        .any(|(did, other)| did != owner && other == *cid)
                        && !self
                            .stored
                            .iter()
                            .any(|r| r.did != owner && json_contains_str(&r.record, cid))
                })
                .cloned(),
        );
        self.blobs
            .retain(|(did, cid), _| did != owner || !blobs.contains(cid));
        self.blob_types
            .retain(|(did, cid), _| did != owner || !blobs.contains(cid));
        for cid in records.iter().chain(blobs.iter()) {
            if !self.blobs.keys().any(|(_, other)| other == cid)
                && (records.contains(cid)
                    || !self
                        .stored
                        .iter()
                        .any(|r| json_contains_str(&r.record, cid)))
            {
                self.blocks.remove(cid);
            }
        }
        self.msts.remove(owner);
        let mut mst = Mst::default();
        for ((did, collection, rkey), cid) in &self.tips {
            if did == owner {
                let key = sync::mst_key(collection, rkey);
                if let Ok(cid) = Cid::parse(cid) {
                    mst.insert(key.as_bytes(), cid).ok();
                }
            }
        }
        if let Ok(pairs) = mst.block_pairs_string_cids() {
            for (cid, bytes) in pairs {
                self.blocks.insert(cid.to_string(), bytes);
            }
        }
        // During a normal erase the owner key is present. Startup replay can retain the
        // previous signed head as opaque history until the owner's next signed write.
        if !removed.is_empty() && self.signing_keys.contains_key(owner) {
            let data = mst
                .root_cid_string_cids()
                .map_err(|e| Error::Transport(e.to_string()))?;
            let prev = self
                .heads
                .get(owner)
                .and_then(|h| Cid::parse(&h.commit).ok());
            let clock = self.rev.entry(owner.into()).or_insert(0);
            *clock += 1;
            let rev = crate::identity::tid_from_parts(*clock, 0);
            let unsigned = record::unsigned_commit(owner, &data, &rev, prev.as_ref());
            let sealed = record::seal_commit(&self.commit_signing_key(owner), &unsigned)
                .map_err(|e| Error::Transport(e.to_string()))?;
            self.blocks
                .insert(sealed.cid.to_string(), sealed.signed_cbor);
            self.heads.insert(
                owner.into(),
                GetHeadResponse {
                    did: owner.into(),
                    commit: sealed.cid.to_string(),
                    rev,
                    knot: self.knot_uri(owner),
                    data: data.to_string(),
                },
            );
        }
        self.msts.insert(owner.into(), mst);
        // Refresh surviving record proofs after removing the private MST leaves.
        let tips = self.tips.clone();
        for ((did, collection, rkey), cid) in tips {
            if did == owner {
                if let Some(rec) = self.stored.iter().find(|r| r.did == owner && r.cid == cid) {
                    self.records
                        .insert((did, collection, rkey), self.record_response(rec));
                }
            }
        }
        if let Some(dir) = &self.data_dir {
            remove_erased_repo_files(
                &dir.join("repos").join(safe_seg(owner)).join("collections"),
                &records,
            )?;
            // Also remove orphaned block files from an interrupted prior erasure.
            for cid in &blobs {
                remove_erased_file(&dir.join("blobs").join(safe_seg(owner)).join(safe_seg(cid)))?;
            }
            for cid in records.iter().chain(blobs.iter()) {
                if !self.blocks.contains_key(cid) {
                    remove_erased_file(&dir.join("blocks").join(safe_seg(cid)))?;
                }
            }
        }
        self.persist()
    }

    /// Spec 11: public collections need no cap on subscribe; others do.
    pub fn subscribe_authorized(
        &mut self,
        did: &str,
        collection: &str,
        req: &KnotRequest,
    ) -> Result<()> {
        let Some(rec) = self
            .stored
            .iter()
            .find(|r| r.did == did && r.collection == collection && r.vis != "public" && !r.deleted)
            .cloned()
        else {
            return Ok(());
        };
        match self.read_denied(&rec, req)? {
            None => Ok(()),
            Some(resp) => {
                let error = serde_json::from_slice::<ApiError>(&resp.body)
                    .map(|e| e.error)
                    .unwrap_or_else(|_| "CapabilityRequired".into());
                Err(Error::Http {
                    status: resp.status,
                    error,
                })
            }
        }
    }

    pub fn handle(&mut self, req: &KnotRequest) -> Result<KnotResponse> {
        let mut resp = self.dispatch(req)?;
        encode_cbor_if_requested(req, &mut resp)?;
        Ok(resp)
    }

    fn dispatch(&mut self, req: &KnotRequest) -> Result<KnotResponse> {
        let path = req.path.trim_end_matches('/');
        match (req.method, path) {
            (Method::Get, "/xrpc/tangled.node.describe") => {
                let body = self.describe.clone().ok_or_else(|| Error::Http {
                    status: 404,
                    error: "NotFound".into(),
                })?;
                json_ok(&body)
            }
            (Method::Get, "/xrpc/tangled.node.stats") => json_ok(&self.node_stats()?),
            (Method::Get, "/xrpc/tangled.sync.getHead") => {
                let did = query(&req.query, "did")?;
                let head = self.heads.get(&did).cloned().ok_or_else(|| Error::Http {
                    status: 404,
                    error: "NotFound".into(),
                })?;
                json_ok(&head)
            }
            (Method::Get, "/xrpc/tangled.sync.getRecord") => self.get_record(req),
            (Method::Get, "/xrpc/tangled.sync.listRecords") => self.list_records(req),
            (Method::Get, "/xrpc/tangled.sync.getBlob") => self.get_blob(req),
            (Method::Post, "/xrpc/tangled.sync.getBlocks") => self.get_blocks(req),
            (Method::Get, "/xrpc/tangled.sync.getRepo") => self.get_repo(req, false),
            (Method::Post, "/xrpc/tangled.sync.getRepoAuth") => self.get_repo(req, true),
            (Method::Get, "/xrpc/tangled.sync.queryBacklinks") => self.query_backlinks(req),
            (Method::Post, "/xrpc/tangled.sync.notify") => Ok(KnotResponse {
                status: 204,
                content_type: String::new(),
                body: vec![],
                set_cookie: None,
                headers: vec![],
            }),
            (Method::Post, "/xrpc/tangled.repo.putRecord") => self.put_record(req),
            (Method::Post, "/xrpc/tangled.repo.putBlob") => self.put_blob(req),
            (Method::Post, "/xrpc/tangled.repo.import") => self.import_repo(req),
            (Method::Get, "/xrpc/tangled.identity.resolve") => self.identity_resolve(req),
            (Method::Get, "/xrpc/tangled.ap.resolve") => self.ap_resolve(req),
            (Method::Get, "/xrpc/tangled.session.challenge") => self.session_challenge(req),
            (Method::Post, "/xrpc/tangled.session.verify") => self.session_verify(req),
            (Method::Post, "/xrpc/tangled.cap.verify") => self.cap_verify(req),
            (Method::Get, "/xrpc/tangled.loom.query") => self.loom_query(req),
            (Method::Get, "/xrpc/tangled.sync.subscribe")
            | (Method::Get, "/xrpc/tangled.loom.firehose") => Err(Error::Transport(
                "websocket upgrade is not implemented in MockKnot".into(),
            )),
            _ => Err(Error::Http {
                status: 404,
                error: "MethodNotImplemented".into(),
            }),
        }
    }

    fn get_record(&mut self, req: &KnotRequest) -> Result<KnotResponse> {
        let did = query(&req.query, "did")?;
        let collection = query(&req.query, "collection")?;
        let rkey = query(&req.query, "rkey")?;
        if let Some(cid) = self
            .tips
            .get(&(did.clone(), collection.clone(), rkey.clone()))
            .cloned()
        {
            let rec = self
                .stored
                .iter()
                .find(|r| r.did == did && r.cid == cid)
                .cloned()
                .ok_or_else(|| Error::Http {
                    status: 404,
                    error: "NotFound".into(),
                })?;
            if let Some(denied) = self.read_denied(&rec, req)? {
                return Ok(denied);
            }
            return json_ok(&self.record_response(&rec));
        }
        let rec = self
            .records
            .get(&(did, collection, rkey))
            .cloned()
            .ok_or_else(|| Error::Http {
                status: 404,
                error: "NotFound".into(),
            })?;
        let vis = rec
            .record
            .get("vis")
            .and_then(|v| v.as_str())
            .unwrap_or("public");
        if vis != "public" {
            let stub = StoredRecord {
                did: String::new(),
                collection: String::new(),
                rkey: String::new(),
                vis: vis.to_string(),
                cid: String::new(),
                record: rec.record.clone(),
                circle: rec
                    .record
                    .get("circle")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                deleted: rec.record.get("body").is_some_and(|b| b.is_null()),
            };
            if let Some(denied) = self.read_denied(&stub, req)? {
                return Ok(denied);
            }
        }
        json_ok(&rec)
    }

    fn list_records(&mut self, req: &KnotRequest) -> Result<KnotResponse> {
        let did = query(&req.query, "did")?;
        let collection = req
            .query
            .iter()
            .find(|(k, _)| k == "collection")
            .map(|(_, v)| v.as_str());
        let candidates: Vec<StoredRecord> = self
            .stored
            .iter()
            .filter(|rec| rec.did == did && collection.is_none_or(|c| rec.collection == c))
            .cloned()
            .collect();
        let mut items = Vec::new();
        for rec in &candidates {
            if self.read_denied(rec, req)?.is_some() {
                continue;
            }
            items.push(ListRecordItem {
                collection: rec.collection.clone(),
                rkey: rec.rkey.clone(),
                cid: rec.cid.clone(),
                record: rec.record.clone(),
            });
        }
        json_ok(&ListRecordsResponse {
            commit: self.heads.get(&did).map(|h| h.commit.clone()),
            records: items,
        })
    }

    fn get_blob(&mut self, req: &KnotRequest) -> Result<KnotResponse> {
        let did = query(&req.query, "did")?;
        let cid = query(&req.query, "cid")?;
        let bytes = self
            .blobs
            .get(&(did.clone(), cid.clone()))
            .cloned()
            .ok_or_else(|| Error::Http {
                status: 404,
                error: "NotFound".into(),
            })?;
        let refs: Vec<BlobRef> = self
            .stored
            .iter()
            .filter(|r| r.did == did)
            .map(|r| BlobRef {
                collection: r.collection.clone(),
                rkey: r.rkey.clone(),
                vis: r.vis.clone(),
                deleted: r.deleted,
                references_blob: json_contains_str(&r.record, &cid),
                circle: r.circle.clone(),
            })
            .collect();
        if getblob_auth(&refs) == BlobAuth::RequireCap {
            match self.blob_cap_ok(req, &did, &cid, &refs) {
                Ok(true) => {}
                Ok(false) => {
                    return api_error(
                        401,
                        "CapabilityRequired",
                        "blob is not referenced by a live public record",
                    );
                }
                Err(e) => return Err(e),
            }
        }
        let mut headers = vec![("Content-Length".into(), bytes.len().to_string())];
        if let Some(mt) = self.blob_types.get(&(did, cid)) {
            headers.push(("X-Tangled-Media-Type".into(), mt.clone()));
        }
        Ok(KnotResponse {
            status: 200,
            content_type: "application/octet-stream".into(),
            body: bytes,
            set_cookie: None,
            headers,
        })
    }

    fn put_record(&mut self, req: &KnotRequest) -> Result<KnotResponse> {
        let body = req.body.as_deref().ok_or_else(|| Error::Http {
            status: 400,
            error: "InvalidRequest".into(),
        })?;
        let parsed: PutRecordRequest = serde_json::from_slice(body)
            .map_err(|e| Error::Transport(format!("putRecord json: {e}")))?;
        let client_prev = serde_json::from_slice::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v.get("prev").and_then(|p| p.as_str()).map(str::to_string));
        self.put_records.push(parsed.record.clone());
        let record = parsed.record;
        if record
            .get("sig")
            .and_then(serde_json::Value::as_str)
            .is_none()
        {
            return api_error(
                400,
                "InvalidSignature",
                "putRecord requires a signed record",
            );
        }
        if let Err(e) = self.verify_put_signature(&record) {
            return api_error(400, "InvalidSignature", &e.to_string());
        }
        let collection = record
            .get("collection")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let author = record
            .get("author")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| self.describe.as_ref().map(|d| d.did.clone()))
            .unwrap_or_else(|| "did:sol:mock".into());
        if let Some(denied) = self.write_denied(&record, &author, &collection, req)? {
            return Ok(denied);
        }
        if record
            .get("body")
            .and_then(|b| b.get("compact"))
            .and_then(|c| c.as_u64())
            == Some(1)
            && !compact_allowed(&collection)
        {
            return api_error(
                400,
                "InvalidRequest",
                "Last-ts-wins collections MUST NOT compact",
            );
        }
        if collection == "pub.tangled.page" {
            if let Some(resp) = self.page_write_gate(&author, &collection, &record)? {
                return Ok(resp);
            }
        }
        self.store_record_opts(
            record,
            StoreOpts {
                prev: client_prev,
                check_prev: true,
                headers: &req.headers,
                skip_commit: false,
            },
        )
    }

    /// Replica ingest: store a verified record without write-cap / ConcurrentHeads gates.
    pub fn ingest_record(&mut self, record: serde_json::Value) -> Result<PutRecordResponse> {
        if let Err(e) = self.verify_put_signature(&record) {
            return Err(Error::Http {
                status: 400,
                error: format!("InvalidSignature: {e}"),
            });
        }
        match self.store_record_opts(
            record,
            StoreOpts {
                prev: None,
                check_prev: false,
                headers: &[],
                skip_commit: false,
            },
        )? {
            resp if resp.status == 200 => resp.json(),
            resp => Err(Error::Http {
                status: resp.status,
                error: String::from_utf8_lossy(&resp.body).into_owned(),
            }),
        }
    }

    fn store_record_opts(
        &mut self,
        record: serde_json::Value,
        opts: StoreOpts<'_>,
    ) -> Result<KnotResponse> {
        let collection = record
            .get("collection")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let rkey = record
            .get("rkey")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let vis = record
            .get("vis")
            .and_then(|v| v.as_str())
            .unwrap_or("public")
            .to_string();
        let author = record
            .get("author")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| self.describe.as_ref().map(|d| d.did.clone()))
            .unwrap_or_else(|| "did:sol:mock".into());
        let circle = record
            .get("circle")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let deleted = record.get("body").is_some_and(|b| b.is_null());
        if collection.is_empty() || rkey.is_empty() {
            return json_ok(&PutRecordResponse {
                cid: Some("bafyr4idput".into()),
                commit: Some("bafyr4idcommit".into()),
            });
        }
        if collection == "sys.foreign" {
            let proto = record
                .get("body")
                .and_then(|b| b.get("protocol"))
                .and_then(|p| p.as_str());
            let attested = record
                .get("body")
                .and_then(|b| b.get("attested"))
                .and_then(|a| a.as_bool())
                .unwrap_or(false);
            if proto == Some("activitypub") && !attested {
                return api_error(
                    400,
                    "InvalidRequest",
                    "ActivityPub imports MUST set attested: true",
                );
            }
            if proto == Some("activitypub") {
                let actor = record
                    .get("body")
                    .and_then(|b| b.get("foreign"))
                    .and_then(|f| f.as_str())
                    .or_else(|| {
                        record
                            .get("body")
                            .and_then(|b| b.get("foreign_id"))
                            .and_then(|f| f.as_str())
                    })
                    .unwrap_or("");
                if let Some(proof) = record.get("body").and_then(|b| b.get("foreignProof")) {
                    if crate::foreign::verify_ap_proof(&author, actor, proof).is_err() {
                        return api_error(
                            400,
                            "InvalidRequest",
                            "ActivityPub foreignProof MUST match did/actor and verify RFC 9421",
                        );
                    }
                } else if crate::httpsig::has_message_signature(opts.headers) {
                    if let Ok(parsed) = Did::parse(&author) {
                        let method = header_val(opts.headers, "x-tangled-method").unwrap_or("POST");
                        let path = header_val(opts.headers, "x-tangled-path").unwrap_or("/");
                        let body_bytes = record
                            .get("body")
                            .and_then(|b| b.get("bytes"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.as_bytes().to_vec())
                            .unwrap_or_default();
                        if crate::httpsig::verify_ed25519(
                            &parsed.pubkey,
                            method,
                            path,
                            opts.headers,
                            &body_bytes,
                        )
                        .is_err()
                        {
                            return api_error(
                                400,
                                "InvalidSignature",
                                "HTTP Message Signature verification failed",
                            );
                        }
                    }
                } else {
                    return api_error(
                        400,
                        "InvalidRequest",
                        "ActivityPub imports MUST include a verifiable HTTP Message Signature",
                    );
                }
            }
        }
        let mut unsigned = record.clone();
        if let Some(obj) = unsigned.as_object_mut() {
            obj.remove("sig");
        }
        if let Err(error) = record::validate_envelope(&unsigned) {
            return api_error(400, "InvalidRequest", &error.to_string());
        }
        let (cid, cbor) = record_cbor_and_cid(&record);
        if self.private_archive_erasure_cids.contains(&cid) {
            return api_error(
                409,
                "ArchiveErased",
                "This private archive record was erased on this host",
            );
        }
        self.blocks.insert(cid.clone(), cbor);
        let stored = StoredRecord {
            did: author.clone(),
            collection: collection.clone(),
            rkey: rkey.clone(),
            vis: vis.clone(),
            cid: cid.clone(),
            record: record.clone(),
            circle,
            deleted,
        };
        self.stored.push(stored.clone());
        self.tips.insert(
            (author.clone(), collection.clone(), rkey.clone()),
            cid.clone(),
        );
        if collection == "sys.revocation" {
            if let Some(rcid) = record
                .get("body")
                .and_then(|b| b.get("cid"))
                .and_then(|v| v.as_str())
            {
                self.revoked.insert(rcid.to_string(), ());
            }
        }
        let key = sync::mst_key(&collection, &rkey);
        if let Ok(parsed_cid) = Cid::parse(&cid) {
            self.msts
                .entry(author.clone())
                .or_default()
                .insert(key.as_bytes(), parsed_cid)
                .ok();
        }
        if collection == "sys.id" && rkey == "self" {
            if let Some(id) = self.identities.get_mut(&author) {
                id.id_cid = Some(cid.clone());
            }
        }
        if opts.skip_commit {
            let commit = self.heads.get(&author).map(|h| h.commit.clone());
            self.persist()?;
            return json_ok(&PutRecordResponse {
                cid: Some(cid),
                commit,
            });
        }
        let current_head = self.heads.get(&author).cloned();
        if opts.check_prev {
            if let Some(head) = &current_head {
                if let Some(p) = opts.prev.as_deref() {
                    if p != head.commit {
                        return api_error_head(
                            409,
                            "ConcurrentHeads",
                            "putRecord prev does not equal the current head commit CID",
                            &head.commit,
                        );
                    }
                }
            } else if opts.prev.is_some() {
                return api_error(400, "InvalidRequest", "genesis commit prev MUST be absent");
            }
        }
        let prev_cid = current_head
            .as_ref()
            .and_then(|h| Cid::parse(&h.commit).ok());
        let prev_rev = current_head.as_ref().map(|h| h.rev.clone());
        if let Some(p) = &prev_rev {
            if let Some(n) = crate::identity::tid_decode(p) {
                let micros = n >> 10;
                let e = self.rev.entry(author.clone()).or_insert(0);
                if *e <= micros {
                    *e = micros;
                }
            }
        }
        let rev = {
            let e = self.rev.entry(author.clone()).or_insert(0);
            *e += 1;
            crate::identity::tid_from_parts(*e, 0)
        };
        let data = self
            .msts
            .get(&author)
            .and_then(|m| m.root_cid_string_cids().ok())
            .unwrap_or_else(|| Cid::dag_cbor(cid.as_bytes()));
        let unsigned = record::unsigned_commit(&author, &data, &rev, prev_cid.as_ref());
        let sk = self.commit_signing_key(&author);
        let sealed = record::seal_commit(&sk, &unsigned).map_err(|e| Error::Http {
            status: 400,
            error: format!("InvalidSignature: {e}"),
        })?;
        self.blocks
            .insert(sealed.cid.to_string(), sealed.signed_cbor.clone());
        if let Some(Ok(pairs)) = self.msts.get(&author).map(|m| m.block_pairs_string_cids()) {
            for (nc, nb) in pairs {
                self.blocks.insert(nc.to_string(), nb);
            }
        }
        let commit = sealed.cid.to_string();
        self.heads.insert(
            author.clone(),
            GetHeadResponse {
                did: author.clone(),
                commit: commit.clone(),
                rev: rev.clone(),
                knot: self.knot_uri(&author),
                data: data.to_string(),
            },
        );
        if vis == "public" && !collection.is_empty() {
            let ev = SubscribeMessage {
                t: "commit".into(),
                did: author.clone(),
                commit: commit.clone(),
                ops: vec![SubscribeOp {
                    action: if deleted {
                        "delete".into()
                    } else {
                        "put".into()
                    },
                    collection: collection.clone(),
                    rkey: rkey.clone(),
                    cid: cid.clone(),
                }],
            };
            self.pending_events.push(ev.clone());
            self.event_log.push(ev);
            const EVENT_LOG_CAP: usize = 256;
            if self.event_log.len() > EVENT_LOG_CAP {
                let drop_n = self.event_log.len() - EVENT_LOG_CAP;
                self.event_log.drain(..drop_n);
            }
        }
        let proof = self.mst_proof_json(&author, &key);
        let resp = GetRecordResponse {
            record: record.clone(),
            proof,
            commit: serde_json::json!({ "did": author, "rev": rev.to_string() }),
        };
        self.records
            .insert((author.clone(), collection, rkey), resp);
        self.persist()?;
        json_ok(&PutRecordResponse {
            cid: Some(cid),
            commit: Some(commit),
        })
    }

    fn commit_signing_key(&self, did: &str) -> ed25519_dalek::SigningKey {
        if let Some(seed) = self.signing_keys.get(did) {
            return ed25519_dalek::SigningKey::from_bytes(seed);
        }
        // ponytail: unhosted/test DID — deterministic seed, not Ed25519→X25519.
        let seed = *blake3::hash(format!("tangled.commit\0{did}").as_bytes()).as_bytes();
        ed25519_dalek::SigningKey::from_bytes(&seed)
    }

    fn mst_proof_json(&self, did: &str, key: &str) -> Vec<serde_json::Value> {
        let Some(mst) = self.msts.get(did) else {
            return vec![];
        };
        let Ok(proof) = mst.inclusion_proof(key.as_bytes()) else {
            return vec![];
        };
        vec![serde_json::json!({
            "root": proof.root_cid.to_string(),
            "value": proof.value.to_string(),
        })]
    }

    fn page_write_gate(
        &self,
        author: &str,
        collection: &str,
        record: &serde_json::Value,
    ) -> Result<Option<KnotResponse>> {
        let rkey = record.get("rkey").and_then(|v| v.as_str()).unwrap_or("");
        let new_cid = record_cid(record);
        let prev: Vec<String> = record
            .get("prev")
            .and_then(|p| p.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(head) =
            self.tips
                .get(&(author.to_string(), collection.to_string(), rkey.to_string()))
        {
            if head != &new_cid {
                let covers_head = prev.contains(head)
                    || prev
                        .iter()
                        .any(|p| stored_prev_contains(self, author, p, head));
                let head_covers_new = stored_prev_contains(self, author, head, &new_cid);
                if !covers_head && !head_covers_new {
                    return Ok(Some(api_error_head(
                        409,
                        "ConcurrentHeads",
                        "putRecord would orphan the current MST value",
                        head,
                    )?));
                }
            }
        }
        let is_compact = record
            .get("body")
            .and_then(|b| b.get("compact"))
            .and_then(|c| c.as_u64())
            == Some(1);
        if is_compact && self.compact_admission {
            let same: Vec<&StoredRecord> = self
                .stored
                .iter()
                .filter(|r| r.did == author && r.collection == collection && r.rkey == rkey)
                .collect();
            let incremental = same
                .iter()
                .filter(|r| {
                    r.record
                        .get("body")
                        .and_then(|b| b.get("compact"))
                        .and_then(|c| c.as_u64())
                        != Some(1)
                        && !r.deleted
                })
                .count();
            let new_ts = record.get("ts").and_then(|t| t.as_u64()).unwrap_or(0);
            let prev_compact_ts = same
                .iter()
                .filter(|r| {
                    r.record
                        .get("body")
                        .and_then(|b| b.get("compact"))
                        .and_then(|c| c.as_u64())
                        == Some(1)
                })
                .filter_map(|r| r.record.get("ts").and_then(|t| t.as_u64()))
                .max();
            let interval = prev_compact_ts.map(|kts| new_ts.saturating_sub(kts));
            if page::compact_admission(incremental, interval).is_err() {
                let mut resp = api_error(
                    429,
                    "RateLimited",
                    "PAGE_COMPACT_MIN_OPS or PAGE_COMPACT_MIN_INTERVAL_MS",
                )?;
                resp.headers.push((
                    "Retry-After".into(),
                    (crate::constants::PAGE_COMPACT_MIN_INTERVAL_MS / 1000).to_string(),
                ));
                return Ok(Some(resp));
            }
        }
        Ok(None)
    }

    fn write_denied(
        &mut self,
        rec: &serde_json::Value,
        author: &str,
        collection: &str,
        req: &KnotRequest,
    ) -> Result<Option<KnotResponse>> {
        let hosted = self.describe.as_ref().is_some_and(|d| d.did == author);
        let has_signed_by = rec.get("signedBy").is_some();
        if hosted && !has_signed_by {
            return Ok(None);
        }
        if self.session_did(req).as_deref() == Some(author) {
            return Ok(None);
        }
        let Some((cap, cap_json, inv)) = presented_invocation(req)? else {
            return Ok(Some(api_error(
                401,
                "CapabilityRequired",
                "repo/write requires Authorization Cap or SIWS session",
            )?));
        };
        if let Err(msg) = self.verify_presented(&cap, &cap_json, inv.as_ref()) {
            return Ok(Some(api_error(403, "CapabilityInvalid", &msg)?));
        }
        let record_res = format!("tangled://{author}/{collection}");
        if capability::authorizes(&cap, "repo/write", &record_res, self.now).is_ok() {
            if let Err(msg) = self.consume_cap(&cap) {
                return Ok(Some(api_error(403, "CapabilityInvalid", &msg)?));
            }
            return Ok(None);
        }
        Ok(Some(api_error(
            403,
            "CapabilityInvalid",
            "stolen or insufficient capability cannot write",
        )?))
    }

    fn put_blob(&mut self, req: &KnotRequest) -> Result<KnotResponse> {
        let bytes = req.body.clone().unwrap_or_default();
        let max = self
            .describe
            .as_ref()
            .map(|d| d.max_blob as usize)
            .unwrap_or(134_217_728);
        if bytes.len() > max {
            return api_error(413, "BlobTooLarge", "blob exceeds maxBlob");
        }
        let cid = Cid::raw(&bytes).to_string();
        let did = req
            .query
            .iter()
            .find(|(k, _)| k == "did")
            .map(|(_, v)| v.clone())
            .or_else(|| self.session_did(req))
            .or_else(|| self.describe.as_ref().map(|d| d.did.clone()))
            .unwrap_or_else(|| "did:sol:mock".into());
        if self.private_archive_erasure_cids.contains(&cid) {
            return api_error(
                409,
                "ArchiveErased",
                "This private archive blob was erased on this host",
            );
        }
        let media_type = req
            .query
            .iter()
            .find(|(k, _)| k == "mediaType")
            .map(|(_, v)| v.clone())
            .or_else(|| {
                req.headers.iter().find_map(|(k, v)| {
                    if k.eq_ignore_ascii_case("x-tangled-media-type") {
                        Some(v.clone())
                    } else {
                        None
                    }
                })
            });
        if let Some(mt) = media_type {
            if !mt.is_empty() && mt != "application/octet-stream" {
                self.blob_types.insert((did.clone(), cid.clone()), mt);
            }
        }
        self.blobs.insert((did.clone(), cid.clone()), bytes.clone());
        self.blocks.insert(cid.clone(), bytes);
        self.persist()?;
        json_ok(&PutBlobResponse { cid })
    }

    fn cap_verify(&self, req: &KnotRequest) -> Result<KnotResponse> {
        let Some(body) = req.body.as_deref() else {
            return api_error(400, "InvalidRequest", "missing invocation");
        };
        if capability::looks_like_json(body) {
            return api_error(403, "CapabilityInvalid", "invocation MUST be DAG-CBOR");
        }
        let inv = match Invocation::decode_cbor(body) {
            Ok(i) => i,
            Err(e) => return api_error(403, "CapabilityInvalid", &e.to_string()),
        };
        let knot_did = self.describe.as_ref().map(|d| d.did.as_str()).unwrap_or("");
        match inv.verify(self.now, knot_did) {
            Ok(leaf) => {
                if self.revoked.contains_key(
                    &leaf
                        .cid()
                        .unwrap_or_else(|_| Cid::dag_cbor(b"x"))
                        .to_string(),
                ) {
                    return api_error(403, "CapabilityInvalid", "revoked");
                }
                json_ok(&CapVerifyResponse {
                    ok: true,
                    cmd: Some(leaf.cmd.clone()),
                    res: Some(leaf.res.clone()),
                })
            }
            Err(e) => api_error(403, "CapabilityInvalid", &e.to_string()),
        }
    }

    fn get_blocks(&mut self, req: &KnotRequest) -> Result<KnotResponse> {
        let body = req.body.as_deref().unwrap_or(b"{}");
        let parsed: GetBlocksRequest =
            serde_json::from_slice(body).unwrap_or(GetBlocksRequest { cids: vec![] });
        if parsed.cids.len() > 256 {
            return api_error(400, "InvalidRequest", "cids max 256");
        }
        let mut missing = Vec::new();
        let mut blocks = Vec::new();
        for cid_s in &parsed.cids {
            let bytes = self.blocks.get(cid_s).cloned().or_else(|| {
                self.blobs
                    .iter()
                    .find(|((_, cid), _)| cid == cid_s)
                    .map(|(_, bytes)| bytes.clone())
            });
            let Some(bytes) = bytes else {
                missing.push(cid_s.clone());
                continue;
            };
            let Ok(cid) = Cid::parse(cid_s) else { continue };
            let mut records: Vec<StoredRecord> = self
                .stored
                .iter()
                .filter(|record| record.cid == *cid_s)
                .cloned()
                .collect();
            // A partially imported/recovered record can exist in the block map
            // before its materialized index. Its envelope still determines auth.
            if records.is_empty() && cid.is_dag_cbor() {
                if let Some(json) = crate::dagcbor::decode(&bytes)
                    .ok()
                    .and_then(|v| cbor_record_to_json(&v))
                {
                    if let (Some(author), Some(collection), Some(rkey)) = (
                        json["author"].as_str(),
                        json["collection"].as_str(),
                        json["rkey"].as_str(),
                    ) {
                        records.push(StoredRecord {
                            did: author.into(),
                            collection: collection.into(),
                            rkey: rkey.into(),
                            vis: json["vis"].as_str().unwrap_or("public").into(),
                            cid: cid_s.clone(),
                            circle: json["circle"].as_str().map(str::to_owned),
                            deleted: json.get("body").is_some_and(|body| body.is_null()),
                            record: json,
                        });
                    }
                }
            }
            if !records.is_empty() {
                // Visibility is a property of the record envelope, not merely
                // a public record that happens to mention this CID.
                let mut denied = None;
                let mut allowed = false;
                for record in &records {
                    match self.read_denied(record, req)? {
                        None => {
                            allowed = true;
                            break;
                        }
                        response => denied = response,
                    }
                }
                if !allowed {
                    return Ok(denied.expect("denied record has a response"));
                }
            } else if cid.is_raw() || self.blobs.keys().any(|(_, blob)| blob == cid_s) {
                let mut owners = self
                    .blobs
                    .keys()
                    .filter(|(_, blob)| blob == cid_s)
                    .map(|(did, _)| did.clone())
                    .collect::<std::collections::BTreeSet<_>>();
                // CAR/recovery may retain a raw block only in the block map.
                owners.extend(
                    self.stored
                        .iter()
                        .filter(|record| json_contains_str(&record.record, cid_s))
                        .map(|record| record.did.clone()),
                );
                let mut allowed = false;
                for did in owners {
                    let refs = self
                        .stored
                        .iter()
                        .filter(|record| record.did == did)
                        .map(|record| BlobRef {
                            collection: record.collection.clone(),
                            rkey: record.rkey.clone(),
                            vis: record.vis.clone(),
                            deleted: record.deleted,
                            references_blob: json_contains_str(&record.record, cid_s),
                            circle: record.circle.clone(),
                        })
                        .collect::<Vec<_>>();
                    if getblob_auth(&refs) == BlobAuth::ServeWithoutCap
                        || self.blob_cap_ok(req, &did, cid_s, &refs)?
                    {
                        allowed = true;
                        break;
                    }
                }
                if !allowed {
                    let present = req
                        .headers
                        .iter()
                        .any(|(name, _)| name.eq_ignore_ascii_case("authorization"));
                    return api_error(
                        if present { 403 } else { 401 },
                        if present { "CapabilityInvalid" } else { "CapabilityRequired" },
                        "blob is not referenced by a live public record or covered by this capability",
                    );
                }
            }
            // Version 1 deliberately keeps commit/MST proof blocks public,
            // including sibling opaque keys/CIDs (spec 11, Get blocks).
            blocks.push((cid, bytes));
        }
        if !missing.is_empty() {
            return json_ok(&GetBlocksResponse { missing });
        }
        let roots = blocks
            .first()
            .map(|(c, _)| vec![c.clone()])
            .unwrap_or_default();
        let car = Car { roots, blocks };
        Ok(KnotResponse {
            status: 200,
            content_type: "application/vnd.ipld.car".into(),
            body: car::encode(&car)?,
            set_cookie: None,
            headers: vec![],
        })
    }

    fn get_repo(&mut self, req: &KnotRequest, auth: bool) -> Result<KnotResponse> {
        let did = if auth {
            let body = req.body.as_deref().unwrap_or(b"{}");
            serde_json::from_slice::<GetRepoAuthRequest>(body)
                .ok()
                .map(|b| b.did)
                .or_else(|| query(&req.query, "did").ok())
                .ok_or_else(|| Error::Http {
                    status: 400,
                    error: "InvalidRequest".into(),
                })?
        } else {
            query(&req.query, "did")?
        };
        let body_auth = if auth {
            req.body
                .as_deref()
                .and_then(|b| serde_json::from_slice::<GetRepoAuthRequest>(b).ok())
        } else {
            None
        };
        let since = req
            .query
            .iter()
            .find(|(k, _)| k == "since")
            .map(|(_, v)| v.clone())
            .or_else(|| body_auth.as_ref().and_then(|b| b.since.clone()));
        let blobs = req
            .query
            .iter()
            .find(|(k, _)| k == "blobs")
            .is_some_and(|(_, v)| v == "true")
            || body_auth.as_ref().and_then(|r| r.blobs).unwrap_or(false);
        let export_cap = if auth {
            match presented_invocation(req)? {
                Some((cap, json, inv)) => {
                    if let Err(msg) = self.verify_presented(&cap, &json, inv.as_ref()) {
                        return api_error(403, "CapabilityInvalid", &msg);
                    }
                    let scope = match capability::authorized_collections(&cap) {
                        Ok(scope) => scope,
                        Err(error) => {
                            return api_error(403, "CapabilityInvalid", &error.to_string())
                        }
                    };
                    match &scope {
                        capability::AuthorizedScope::Repo { did: owner, .. } if owner != &did => {
                            return api_error(
                                403,
                                "CapabilityInvalid",
                                "capability does not cover requested DID",
                            );
                        }
                        capability::AuthorizedScope::Circle { owner, .. } => {
                            if let Err(error) =
                                capability::verify_circle_read(&cap, owner, self.now)
                            {
                                return api_error(403, "CapabilityInvalid", &error.to_string());
                            }
                        }
                        _ => {}
                    }
                    if let Err(msg) = self.consume_cap(&cap) {
                        return api_error(403, "CapabilityInvalid", &msg);
                    }
                    Some((cap, scope))
                }
                None => {
                    return api_error(401, "CapabilityRequired", "getRepoAuth requires Cap");
                }
            }
        } else {
            None
        };
        let mut recs: Vec<&StoredRecord> = self
            .stored
            .iter()
            .filter(|r| r.did == did)
            .filter(|r| {
                if auth {
                    match &export_cap {
                        Some((cap, capability::AuthorizedScope::Circle { owner, rkey })) => {
                            r.circle.as_deref()
                                == Some(capability::circle_envelope(owner, rkey).as_str())
                                || (r.did == *owner
                                    && r.collection == "sys.circle"
                                    && r.rkey == *rkey
                                    && capability::authorizes(
                                        cap,
                                        "circle/read",
                                        &capability::circle_resource_uri(owner, rkey),
                                        self.now,
                                    )
                                    .is_ok())
                        }
                        Some((cap, capability::AuthorizedScope::Repo { .. })) => {
                            let resource =
                                format!("tangled://{}/{}/{}", r.did, r.collection, r.rkey);
                            capability::authorizes(cap, "repo/read", &resource, self.now).is_ok()
                        }
                        None => false,
                    }
                } else {
                    r.vis == "public"
                }
            })
            .collect();
        let since_is_commit = since.as_deref().is_some_and(|s| {
            self.blocks
                .get(s)
                .is_some_and(|b| record::parse_commit(b).is_ok())
        });
        if let Some(since_cid) = since.as_deref() {
            if !since_is_commit {
                if let Some(pos) = recs.iter().position(|r| r.cid == since_cid) {
                    recs = recs.split_off(pos + 1);
                }
            }
        }
        let mut blocks = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let push = |blocks: &mut Vec<(Cid, Vec<u8>)>,
                    seen: &mut std::collections::HashSet<String>,
                    cid_s: &str,
                    bytes: Vec<u8>| {
            if seen.insert(cid_s.to_string()) {
                if let Ok(cid) = Cid::parse(cid_s) {
                    blocks.push((cid, bytes));
                }
            }
        };
        let mut walk = self.heads.get(&did).map(|h| h.commit.clone());
        let mut commit_datas = Vec::new();
        while let Some(cid_s) = walk.take() {
            let at_since = since.as_deref() == Some(cid_s.as_str());
            if let Some(bytes) = self.blocks.get(&cid_s).cloned() {
                if let Ok(info) = record::parse_commit(&bytes) {
                    push(&mut blocks, &mut seen, &cid_s, bytes);
                    commit_datas.push(info.data.clone());
                    if at_since {
                        break;
                    }
                    walk = info.prev.filter(|p| p != &cid_s);
                    continue;
                }
                push(&mut blocks, &mut seen, &cid_s, bytes);
            }
            if at_since {
                break;
            }
            break;
        }
        for data in &commit_datas {
            collect_mst_closure(&self.blocks, data, &mut blocks, &mut seen);
        }
        if commit_datas.is_empty() {
            if let Some(mst) = self.msts.get(&did) {
                if let Ok(pairs) = mst.block_pairs_string_cids() {
                    for (c, b) in pairs {
                        push(&mut blocks, &mut seen, &c.to_string(), b);
                    }
                }
            }
        }
        for r in &recs {
            if let Some(bytes) = self.blocks.get(&r.cid) {
                push(&mut blocks, &mut seen, &r.cid, bytes.clone());
            }
        }
        if blobs {
            for ((blob_did, bcid), bytes) in &self.blobs {
                if blob_did != &did {
                    continue;
                }
                if recs.iter().any(|r| json_contains_str(&r.record, bcid)) {
                    if let Ok(cid) = Cid::parse(bcid) {
                        blocks.push((cid, bytes.clone()));
                    }
                }
            }
        }
        let root = self
            .heads
            .get(&did)
            .and_then(|h| Cid::parse(&h.commit).ok())
            .or_else(|| blocks.first().map(|(c, _)| c.clone()));
        let roots = root.into_iter().collect();
        let car = Car { roots, blocks };
        Ok(KnotResponse {
            status: 200,
            content_type: "application/vnd.ipld.car".into(),
            body: car::encode(&car)?,
            set_cookie: None,
            headers: vec![],
        })
    }

    fn import_repo(&mut self, req: &KnotRequest) -> Result<KnotResponse> {
        let force = req
            .query
            .iter()
            .any(|(k, v)| k == "force" && (v == "true" || v == "1"));
        let bytes = req.body.clone().unwrap_or_default();
        let car = car::decode(&bytes).map_err(|e| Error::Http {
            status: 400,
            error: format!("InvalidRequest: {e}"),
        })?;
        if let Err(e) = verify_imported_commits(&car, &self.blocks) {
            return api_error(400, "InvalidSignature", &e);
        }
        if car
            .blocks
            .iter()
            .any(|(cid, _)| self.private_archive_erasure_cids.contains(&cid.to_string()))
        {
            return api_error(
                409,
                "ArchiveErased",
                "Repository contains private archive blocks erased on this host",
            );
        }
        let mut imported_did: Option<String> = None;
        let mut last_commit = car.roots.first().map(|c| c.to_string()).unwrap_or_default();
        for (cid, data) in &car.blocks {
            self.blocks.insert(cid.to_string(), data.clone());
            if cid.is_raw() {
                let did = self
                    .describe
                    .as_ref()
                    .map(|d| d.did.clone())
                    .unwrap_or_else(|| "did:sol:imported".into());
                self.blobs.insert((did, cid.to_string()), data.clone());
                continue;
            }
            if let Ok(v) = crate::dagcbor::decode(data) {
                if let Some(json) = cbor_record_to_json(&v) {
                    if json.get("collection").is_some() && json.get("author").is_some() {
                        let author = json["author"].as_str().unwrap_or("").to_string();
                        if imported_did.is_none() {
                            imported_did = Some(author.clone());
                            if !self.import_authorized(req, &author)? {
                                return api_error(
                                    401,
                                    "CapabilityRequired",
                                    "import requires SIWS session or repo/write",
                                );
                            }
                        }
                        if !force
                            && self.describe.as_ref().is_some_and(|d| d.did == author)
                            && self.stored.iter().any(|r| r.did == author)
                        {
                            return api_error(
                                400,
                                "InvalidRequest",
                                "DID already hosted; pass force=true",
                            );
                        }
                        if self.verify_put_signature(&json).is_ok() {
                            let _ = self.store_record_opts(
                                json,
                                StoreOpts {
                                    prev: None,
                                    check_prev: false,
                                    headers: &[],
                                    skip_commit: true,
                                },
                            );
                        }
                    }
                }
            }
            last_commit = cid.to_string();
        }
        if let Some(root) = car.roots.first() {
            if let Some(bytes) = car.get(root) {
                if let Ok(info) = record::parse_commit(bytes) {
                    imported_did = Some(info.did.clone());
                    last_commit = root.to_string();
                    self.heads.insert(
                        info.did.clone(),
                        GetHeadResponse {
                            did: info.did.clone(),
                            commit: root.to_string(),
                            rev: info.rev,
                            knot: self.knot_uri(&info.did),
                            data: info.data,
                        },
                    );
                }
            }
        }
        if let Some(did) = imported_did.clone() {
            if self.describe.is_none() {
                self.describe = Some(NodeDescribe {
                    v: 1,
                    did: did.clone(),
                    profiles: vec!["knot.public".into()],
                    protocols: vec![],
                    max_blob: 134_217_728,
                    custodial_sign: false,
                    peers: vec![],
                });
            } else if let Some(d) = self.describe.as_mut() {
                d.did = did.clone();
            }
        }
        let resp = json_ok(&ImportResponse {
            did: imported_did.unwrap_or_else(|| {
                self.describe
                    .as_ref()
                    .map(|d| d.did.clone())
                    .unwrap_or_else(|| "did:sol:mock".into())
            }),
            commit: self
                .heads
                .values()
                .next()
                .map(|h| h.commit.clone())
                .unwrap_or(last_commit),
        })?;
        self.persist()?;
        Ok(resp)
    }

    fn query_backlinks(&self, req: &KnotRequest) -> Result<KnotResponse> {
        if !self.backlink_index {
            return api_error(404, "MethodNotImplemented", "no backlink index");
        }
        let cid = query(&req.query, "cid")?;
        let collection = req
            .query
            .iter()
            .find(|(k, _)| k == "collection")
            .map(|(_, v)| v.as_str());
        let limit = req
            .query
            .iter()
            .find(|(k, _)| k == "limit")
            .and_then(|(_, v)| v.parse::<usize>().ok())
            .unwrap_or(50)
            .min(500);
        if let Some(pre) = self.backlinks.get(&cid) {
            return json_ok(pre);
        }
        json_ok(&self.compute_backlinks(&cid, collection, limit))
    }

    fn loom_query(&self, req: &KnotRequest) -> Result<KnotResponse> {
        let q = query(&req.query, "q")?;
        let ty = req
            .query
            .iter()
            .find(|(k, _)| k == "type")
            .map(|(_, v)| v.as_str());
        if ty == Some("ref") {
            return json_ok(&self.compute_backlinks(&q, None, 50));
        }
        let mut hits = Vec::new();
        for rec in &self.stored {
            if rec.vis != "public" || rec.deleted {
                continue;
            }
            if let Some(t) = ty {
                if rec.collection != t {
                    continue;
                }
            }
            let text = serde_json::to_string(&rec.record).unwrap_or_default();
            if text.contains(&q) {
                hits.push(BacklinkHit {
                    did: rec.did.clone(),
                    collection: rec.collection.clone(),
                    rkey: rec.rkey.clone(),
                    cid: rec.cid.clone(),
                    commit: self
                        .heads
                        .get(&rec.did)
                        .map(|h| h.commit.clone())
                        .unwrap_or_default(),
                });
            }
        }
        let count = hits.len() as u64;
        json_ok(&BacklinkQuery {
            hits,
            count: Some(count),
            overlay: true,
            complete: false,
        })
    }

    fn compute_backlinks(
        &self,
        target: &str,
        collection: Option<&str>,
        limit: usize,
    ) -> BacklinkQuery {
        let mut hits = Vec::new();
        for rec in &self.stored {
            if rec.vis != "public" || rec.deleted {
                continue;
            }
            if collection.is_some_and(|c| rec.collection != c) {
                continue;
            }
            let env_ref = rec.record.get("ref").and_then(|v| v.as_str());
            let reply = rec
                .record
                .get("body")
                .and_then(|b| b.get("reply"))
                .and_then(|v| v.as_str());
            let matches = match (env_ref, reply) {
                (Some(r), _) => r == target,
                (None, Some(p)) => p == target,
                _ => false,
            };
            if !matches {
                continue;
            }
            hits.push(BacklinkHit {
                did: rec.did.clone(),
                collection: rec.collection.clone(),
                rkey: rec.rkey.clone(),
                cid: rec.cid.clone(),
                commit: self
                    .heads
                    .get(&rec.did)
                    .map(|h| h.commit.clone())
                    .unwrap_or_default(),
            });
        }
        hits.sort_by(|a, b| a.cid.cmp(&b.cid));
        hits.dedup_by(|a, b| a.cid == b.cid);
        let count = hits.len() as u64;
        hits.truncate(limit);
        BacklinkQuery {
            hits,
            count: Some(count),
            overlay: true,
            complete: false,
        }
    }

    fn consume_cap(&mut self, cap: &Capability) -> std::result::Result<(), String> {
        let cid = cap.cid().map_err(|e| e.to_string())?.to_string();
        let used = self.cap_uses.get(&cid).copied().unwrap_or(0);
        capability::evaluate_caveats(&cap.pol, &serde_json::json!({}), self.now, Some(used))
            .map_err(|e| e.to_string())?;
        *self.cap_uses.entry(cid).or_insert(0) += 1;
        self.persist().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Replay subscribe events after `since` (exclusive). Empty if `since` is unknown.
    pub fn events_after(
        &self,
        did: Option<&str>,
        since: &str,
        collection: Option<&str>,
    ) -> Vec<SubscribeMessage> {
        let start = self
            .event_log
            .iter()
            .rposition(|e| e.commit == since)
            .map(|p| p + 1)
            .unwrap_or(self.event_log.len());
        self.event_log[start..]
            .iter()
            .filter(|e| did.is_none_or(|d| e.did == d))
            .filter(|e| {
                collection
                    .is_none_or(|c| e.ops.is_empty() || e.ops.iter().any(|op| op.collection == c))
            })
            .cloned()
            .collect()
    }

    fn verify_presented(
        &self,
        cap: &Capability,
        cap_json: &serde_json::Value,
        inv: Option<&Invocation>,
    ) -> std::result::Result<(), String> {
        if let Ok(cid) = cap.cid() {
            if self.revoked.contains_key(&cid.to_string()) {
                return Err("capability revoked".into());
            }
        }
        if let Some(inv) = inv {
            let knot_did = self.describe.as_ref().map(|d| d.did.as_str()).unwrap_or("");
            inv.verify(self.now, knot_did).map_err(|e| e.to_string())?;
        } else {
            verify_cap_bytes(cap, cap_json)?;
        }
        Ok(())
    }

    fn record_response(&self, rec: &StoredRecord) -> GetRecordResponse {
        let commit = self
            .heads
            .get(&rec.did)
            .map(|h| serde_json::json!({ "did": h.did, "commit": h.commit, "rev": h.rev }))
            .unwrap_or(serde_json::Value::Null);
        let key = sync::mst_key(&rec.collection, &rec.rkey);
        GetRecordResponse {
            record: rec.record.clone(),
            proof: self.mst_proof_json(&rec.did, &key),
            commit,
        }
    }

    /// `None` = allowed. `Some(response)` = 401/403 body.
    fn read_denied(
        &mut self,
        rec: &StoredRecord,
        req: &KnotRequest,
    ) -> Result<Option<KnotResponse>> {
        if rec.vis == "public" {
            return Ok(None);
        }
        let Some((cap, cap_json, inv)) = presented_invocation(req)? else {
            return Ok(Some(api_error(
                401,
                "CapabilityRequired",
                "vis cap or sealed requires Authorization Cap",
            )?));
        };
        if let Err(msg) = self.verify_presented(&cap, &cap_json, inv.as_ref()) {
            return Ok(Some(api_error(403, "CapabilityInvalid", &msg)?));
        }
        let record_res = format!("tangled://{}/{}/{}", rec.did, rec.collection, rec.rkey);
        if capability::authorizes(&cap, "repo/read", &record_res, self.now).is_ok() {
            if let Err(msg) = self.consume_cap(&cap) {
                return Ok(Some(api_error(403, "CapabilityInvalid", &msg)?));
            }
            return Ok(None);
        }
        if cap.cmd == "circle/read" {
            if let Some(circle) = &rec.circle {
                if let Some((owner, rkey)) = circle.split_once('#') {
                    if capability::verify_circle_read(&cap, owner, self.now).is_ok() {
                        let cres = capability::circle_resource_uri(owner, rkey);
                        if capability::authorizes(&cap, "circle/read", &cres, self.now).is_ok() {
                            if let Err(msg) = self.consume_cap(&cap) {
                                return Ok(Some(api_error(403, "CapabilityInvalid", &msg)?));
                            }
                            return Ok(None);
                        }
                    }
                }
            }
        }
        Ok(Some(api_error(
            403,
            "CapabilityInvalid",
            "capability does not cover this record",
        )?))
    }

    fn blob_cap_ok(
        &mut self,
        req: &KnotRequest,
        did: &str,
        cid: &str,
        refs: &[BlobRef],
    ) -> Result<bool> {
        let Some((cap, cap_json, inv)) = presented_invocation(req)? else {
            return Ok(false);
        };
        if self
            .verify_presented(&cap, &cap_json, inv.as_ref())
            .is_err()
        {
            return Ok(false);
        }
        let blob_res = format!("tangled://{did}/blob/{cid}");
        if capability::authorizes(&cap, "blob/get", &blob_res, self.now).is_ok() {
            return Ok(self.consume_cap(&cap).is_ok());
        }
        for r in refs.iter().filter(|r| r.references_blob && !r.deleted) {
            let rec_res = format!("tangled://{did}/{}/{}", r.collection, r.rkey);
            if capability::authorizes(&cap, "repo/read", &rec_res, self.now).is_ok() {
                return Ok(self.consume_cap(&cap).is_ok());
            }
            if cap.cmd == "circle/read" {
                if let Some(circle) = &r.circle {
                    if let Some((owner, rk)) = circle.split_once('#') {
                        let cres = capability::circle_resource_uri(owner, rk);
                        if capability::verify_circle_read(&cap, owner, self.now).is_ok()
                            && capability::authorizes(&cap, "circle/read", &cres, self.now).is_ok()
                        {
                            return Ok(self.consume_cap(&cap).is_ok());
                        }
                    }
                }
            }
        }
        Ok(false)
    }
}

fn record_cid(record: &serde_json::Value) -> String {
    record_cbor_and_cid(record).0
}

pub(crate) fn record_cbor_and_cid(record: &serde_json::Value) -> (String, Vec<u8>) {
    let mut unsigned = record.clone();
    let sig_hex = unsigned
        .as_object_mut()
        .and_then(|o| o.remove("sig"))
        .and_then(|v| match v {
            serde_json::Value::String(s) => Some(s),
            _ => None,
        });
    if let Some(hex_s) = sig_hex {
        if let Ok(bytes) = hex::decode(&hex_s) {
            if let Ok(sig) = <[u8; 64]>::try_from(bytes.as_slice()) {
                if let Ok(cid) = record::signed_record_cid(&unsigned, &sig) {
                    if let Ok(cbor) = signed_cbor_bytes(&unsigned, &sig) {
                        return (cid.to_string(), cbor);
                    }
                    return (
                        cid.to_string(),
                        serde_json::to_vec(record).unwrap_or_default(),
                    );
                }
            }
        }
    }
    let raw = serde_json::to_vec(record).unwrap_or_default();
    (Cid::dag_cbor(&raw).to_string(), raw)
}

fn signed_cbor_bytes(unsigned: &serde_json::Value, sig: &[u8; 64]) -> Result<Vec<u8>> {
    let mut value = crate::dagcbor::from_json(unsigned)?;
    match &mut value {
        crate::dagcbor::Value::Map(m) => {
            m.insert("sig".into(), crate::dagcbor::Value::Bytes(sig.to_vec()));
        }
        _ => return Err(Error::Record("unsigned record must be a map".into())),
    }
    crate::dagcbor::encode(&value)
}

fn verify_put_signature(engine: &KnotEngine, record: &serde_json::Value) -> Result<()> {
    let mut unsigned = record.clone();
    let sig_hex = unsigned
        .as_object_mut()
        .and_then(|o| o.remove("sig"))
        .and_then(|v| v.as_str().map(str::to_string));
    let Some(hex_s) = sig_hex else {
        return Ok(());
    };
    let bytes = hex::decode(&hex_s).map_err(|_| Error::InvalidSignature)?;
    let sig: [u8; 64] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| Error::InvalidSignature)?;
    let mut pk = record::signer_pubkey(&unsigned)?;
    // ponytail: identity signatures follow sys.id pubkey once one exists, so a
    // host that kept the old seed cannot keep signing after a move.
    if unsigned.get("signedBy").is_none() {
        if let Some(author) = unsigned.get("author").and_then(|v| v.as_str()) {
            if let Some(op) = engine.operational_pubkey(author) {
                pk = op;
            }
        }
    }
    record::verify_json_record(&pk, &unsigned, &sig)
}

impl KnotEngine {
    fn verify_put_signature(&self, record: &serde_json::Value) -> Result<()> {
        verify_put_signature(self, record)
    }
}

fn presented_invocation(
    req: &KnotRequest,
) -> Result<Option<(Capability, serde_json::Value, Option<Invocation>)>> {
    let Some(raw) = req
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
        .map(|(_, v)| v.as_str())
    else {
        return Ok(None);
    };
    let Some(b64) = raw
        .strip_prefix("Cap ")
        .or_else(|| raw.strip_prefix("cap "))
    else {
        return Ok(None);
    };
    let bytes = Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        b64.trim(),
    )
    .or_else(|_| Engine::decode(&base64::engine::general_purpose::URL_SAFE, b64.trim()))
    .map_err(|e| Error::Http {
        status: 403,
        error: format!("CapabilityInvalid: {e}"),
    })?;
    if capability::looks_like_json(&bytes) {
        return Err(Error::Http {
            status: 403,
            error: "CapabilityInvalid".into(),
        });
    }
    let inv = Invocation::decode_cbor(&bytes).map_err(|e| Error::Http {
        status: 403,
        error: format!("CapabilityInvalid: {e}"),
    })?;
    let leaf = inv.leaf().map_err(|e| Error::Http {
        status: 403,
        error: format!("CapabilityInvalid: {e}"),
    })?;
    let cap_json = leaf.to_unsigned_json();
    let mut cap_json = cap_json;
    if let Some(sig) = leaf.sig {
        cap_json["sig"] = serde_json::Value::String(hex::encode(sig));
    }
    Ok(Some((leaf.clone(), cap_json, Some(inv))))
}

fn stored_prev_contains(knot: &MockKnot, did: &str, start: &str, needle: &str) -> bool {
    let by_cid: HashMap<&str, &StoredRecord> = knot
        .stored
        .iter()
        .filter(|r| r.did == did)
        .map(|r| (r.cid.as_str(), r))
        .collect();
    let mut seen = std::collections::HashSet::new();
    fn walk(
        cid: &str,
        needle: &str,
        by_cid: &HashMap<&str, &StoredRecord>,
        seen: &mut std::collections::HashSet<String>,
    ) -> bool {
        if cid == needle {
            return true;
        }
        if !seen.insert(cid.to_string()) {
            return false;
        }
        let Some(r) = by_cid.get(cid) else {
            return false;
        };
        let prev = r
            .record
            .get("prev")
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();
        prev.iter()
            .filter_map(|x| x.as_str())
            .any(|p| walk(p, needle, by_cid, seen))
    }
    walk(start, needle, &by_cid, &mut seen)
}

fn cbor_record_to_json(v: &crate::dagcbor::Value) -> Option<serde_json::Value> {
    fn rec(v: &crate::dagcbor::Value) -> serde_json::Value {
        match v {
            crate::dagcbor::Value::Null => serde_json::Value::Null,
            crate::dagcbor::Value::Bool(b) => serde_json::Value::Bool(*b),
            crate::dagcbor::Value::Unsigned(n) => serde_json::json!(*n),
            crate::dagcbor::Value::Negative(n) => serde_json::json!(*n),
            crate::dagcbor::Value::Text(s) => serde_json::Value::String(s.clone()),
            crate::dagcbor::Value::Bytes(b) => serde_json::Value::String(hex::encode(b)),
            crate::dagcbor::Value::Array(a) => {
                serde_json::Value::Array(a.iter().map(rec).collect())
            }
            crate::dagcbor::Value::Map(m) => {
                let mut o = serde_json::Map::new();
                for (k, val) in m {
                    o.insert(k.clone(), rec(val));
                }
                serde_json::Value::Object(o)
            }
            crate::dagcbor::Value::Cid(c) => serde_json::Value::String(c.to_string()),
        }
    }
    let json = rec(v);
    json.get("collection")?;
    Some(json)
}

fn verify_cap_bytes(
    cap: &Capability,
    cap_json: &serde_json::Value,
) -> std::result::Result<(), String> {
    let Some(sig_hex) = cap_json.get("sig").and_then(|s| s.as_str()) else {
        return Ok(());
    };
    let pk = Did::parse(&cap.iss).map_err(|e| e.to_string())?.pubkey;
    let bytes = hex::decode(sig_hex).map_err(|e| e.to_string())?;
    let sig: [u8; 64] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| "capability sig must be 64 bytes".to_string())?;
    capability::verify_cap_sig(&pk, &cap.to_unsigned_json(), &sig).map_err(|e| e.to_string())
}

fn json_contains_str(v: &serde_json::Value, needle: &str) -> bool {
    match v {
        serde_json::Value::String(s) => s == needle,
        serde_json::Value::Array(a) => a.iter().any(|x| json_contains_str(x, needle)),
        serde_json::Value::Object(m) => m.values().any(|x| json_contains_str(x, needle)),
        _ => false,
    }
}

fn header_val<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn collect_mst_closure(
    store: &HashMap<String, Vec<u8>>,
    root: &str,
    blocks: &mut Vec<(Cid, Vec<u8>)>,
    seen: &mut std::collections::HashSet<String>,
) {
    let mut stack = vec![root.to_string()];
    while let Some(cid_s) = stack.pop() {
        if !seen.insert(cid_s.clone()) {
            continue;
        }
        let Some(bytes) = store.get(&cid_s) else {
            continue;
        };
        if let Ok(cid) = Cid::parse(&cid_s) {
            blocks.push((cid, bytes.clone()));
        }
        for link in mst_node_links(bytes) {
            if !seen.contains(&link) {
                stack.push(link);
            }
        }
    }
}

fn mst_node_links(bytes: &[u8]) -> Vec<String> {
    let Ok(v) = crate::dagcbor::decode_json(bytes) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    push_link(v.get("l"), &mut out);
    if let Some(ents) = v.get("e").and_then(|e| e.as_array()) {
        for ent in ents {
            // Record values are emitted separately after visibility/scope filtering.
            push_link(ent.get("t"), &mut out);
        }
    }
    out
}

fn push_link(v: Option<&serde_json::Value>, out: &mut Vec<String>) {
    let Some(v) = v else {
        return;
    };
    if v.is_null() {
        return;
    }
    if let Some(s) = v.as_str() {
        out.push(s.to_string());
        return;
    }
    if let Some(s) = v.get("/").and_then(|x| x.as_str()) {
        out.push(s.to_string());
    }
}

fn verify_imported_commits(
    car: &Car,
    stored: &HashMap<String, Vec<u8>>,
) -> std::result::Result<(), String> {
    let Some(root) = car.roots.first() else {
        return Ok(());
    };
    let mut cid = root.clone();
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(cid.to_string()) {
            return Err("commit prev cycle".into());
        }
        let bytes = car
            .get(&cid)
            .or_else(|| {
                (cid != *root)
                    .then(|| stored.get(&cid.to_string()).map(Vec::as_slice))
                    .flatten()
            })
            .ok_or_else(|| format!("missing commit block {cid}"))?;
        let info = match record::parse_commit(bytes) {
            Ok(i) => i,
            Err(_) if cid == *root => return Ok(()), // Legacy CAR without commit objects.
            Err(_) => return Err("invalid predecessor commit block".into()),
        };
        if let Ok(did) = Did::parse(&info.did) {
            record::verify_commit(&did.pubkey, &info.unsigned, &info.sig)
                .map_err(|_| "commit signature".to_string())?;
        }
        match info.prev {
            None => return Ok(()),
            Some(p) => {
                cid = Cid::parse(&p).map_err(|e| e.to_string())?;
            }
        }
    }
}

fn encode_cbor_if_requested(req: &KnotRequest, resp: &mut KnotResponse) -> Result<()> {
    if !resp.content_type.starts_with("application/json") {
        return Ok(());
    }
    let accept = req
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("accept"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    if !wants_cbor(accept) {
        return Ok(());
    }
    let v: serde_json::Value = serde_json::from_slice(&resp.body)
        .map_err(|e| Error::Transport(format!("cbor json: {e}")))?;
    resp.body = crate::dagcbor::encode_json(&v)?;
    resp.content_type = "application/cbor".into();
    Ok(())
}

fn wants_cbor(accept: &str) -> bool {
    let a = accept.to_ascii_lowercase();
    match (a.find("application/cbor"), a.find("application/json")) {
        (Some(c), Some(j)) => c < j,
        (Some(_), None) => true,
        _ => false,
    }
}

fn api_error(status: u16, error: &str, message: &str) -> Result<KnotResponse> {
    api_error_head(status, error, message, "")
}

fn api_error_head(status: u16, error: &str, message: &str, head: &str) -> Result<KnotResponse> {
    Ok(KnotResponse {
        status,
        content_type: "application/json".into(),
        body: serde_json::to_vec(&ApiError {
            error: error.into(),
            message: message.into(),
            retry: false,
            head: if head.is_empty() {
                None
            } else {
                Some(head.into())
            },
        })
        .map_err(|e| Error::Transport(e.to_string()))?,
        set_cookie: None,
        headers: vec![],
    })
}

fn fs_err(e: std::io::Error) -> Error {
    Error::Http {
        status: 503,
        error: format!("Unavailable: {e}"),
    }
}

fn safe_seg(s: &str) -> String {
    s.replace(['/', '\\', ':'], "_")
}

fn write_json(path: impl AsRef<Path>, v: &impl serde::Serialize) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(fs_err)?;
    }
    let bytes = serde_json::to_vec_pretty(v).map_err(|e| Error::Transport(e.to_string()))?;
    std::fs::write(path, bytes).map_err(fs_err)
}

fn read_json<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = std::fs::read(path).map_err(fs_err)?;
    serde_json::from_slice(&bytes).map_err(|e| Error::Transport(e.to_string()))
}

fn remove_erased_file(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(fs_err(e)),
    }
}

fn remove_erased_repo_files(
    path: &Path,
    records: &std::collections::HashSet<String>,
) -> Result<()> {
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(fs_err(e)),
    };
    for entry in entries {
        let entry = entry.map_err(fs_err)?;
        let kind = entry.file_type().map_err(fs_err)?;
        if kind.is_dir() {
            remove_erased_repo_files(&entry.path(), records)?;
        } else if kind.is_file() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let erased = name
                .strip_suffix(".json")
                .is_some_and(|cid| records.contains(cid))
                || (name == "tip"
                    && records.contains(
                        std::fs::read_to_string(entry.path())
                            .map_err(fs_err)?
                            .trim(),
                    ));
            if erased {
                remove_erased_file(&entry.path())?;
            }
        }
    }
    Ok(())
}

fn persist_engine(engine: &KnotEngine, dir: &Path) -> Result<()> {
    write_json(dir.join("describe.json"), &engine.describe)?;
    write_json(
        dir.join("config.json"),
        &serde_json::json!({
            "backlink_index": engine.backlink_index,
            "compact_admission": engine.compact_admission,
            "now": engine.now,
            "knot_base": engine.knot_base,
            "learned_peers": engine.learned_peers,
        }),
    )?;
    write_json(dir.join("identities.json"), &engine.identities)?;
    write_json(dir.join("heads.json"), &engine.heads)?;
    write_json(
        dir.join("erased-cids.json"),
        &engine
            .private_archive_erasure_cids
            .iter()
            .collect::<std::collections::BTreeSet<_>>(),
    )?;
    write_json(dir.join("rev.json"), &engine.rev)?;
    write_json(dir.join("cap-uses.json"), &engine.cap_uses)?;
    write_json(dir.join("subscribe.json"), &engine.event_log)?;
    write_json(
        dir.join("blob-types.json"),
        &engine
            .blob_types
            .iter()
            .map(|((did, cid), mt)| serde_json::json!({"did": did, "cid": cid, "mediaType": mt}))
            .collect::<Vec<_>>(),
    )?;
    write_json(
        dir.join("revoked.json"),
        &engine.revoked.keys().cloned().collect::<Vec<String>>(),
    )?;
    for rec in &engine.stored {
        let path = dir
            .join("repos")
            .join(safe_seg(&rec.did))
            .join("collections")
            .join(safe_seg(&rec.collection))
            .join(safe_seg(&rec.rkey))
            .join(format!("{}.json", safe_seg(&rec.cid)));
        write_json(&path, rec)?;
        std::fs::write(
            dir.join("repos").join(safe_seg(&rec.did)).join("did.txt"),
            rec.did.as_bytes(),
        )
        .map_err(fs_err)?;
    }
    for ((did, coll, rkey), cid) in &engine.tips {
        let path = dir
            .join("repos")
            .join(safe_seg(did))
            .join("collections")
            .join(safe_seg(coll))
            .join(safe_seg(rkey))
            .join("tip");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(fs_err)?;
        }
        std::fs::write(&path, cid.as_bytes()).map_err(fs_err)?;
    }
    for ((did, cid), bytes) in &engine.blobs {
        let d = dir.join("blobs").join(safe_seg(did));
        std::fs::create_dir_all(&d).map_err(fs_err)?;
        std::fs::write(d.join("did.txt"), did.as_bytes()).map_err(fs_err)?;
        std::fs::write(d.join(safe_seg(cid)), bytes).map_err(fs_err)?;
    }
    let block_dir = dir.join("blocks");
    std::fs::create_dir_all(&block_dir).map_err(fs_err)?;
    for (cid, bytes) in &engine.blocks {
        std::fs::write(block_dir.join(safe_seg(cid)), bytes).map_err(fs_err)?;
    }
    Ok(())
}

fn load_engine(dir: &Path) -> Result<KnotEngine> {
    let mut e = KnotEngine {
        describe: if dir.join("describe.json").exists() {
            read_json(&dir.join("describe.json"))?
        } else {
            None
        },
        backlink_index: true,
        compact_admission: true,
        ..Default::default()
    };
    if dir.join("config.json").exists() {
        let cfg: serde_json::Value = read_json(&dir.join("config.json"))?;
        e.backlink_index = cfg
            .get("backlink_index")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        e.compact_admission = cfg
            .get("compact_admission")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if let Some(n) = cfg.get("now").and_then(|v| v.as_u64()) {
            e.now = n;
        }
        e.knot_base = cfg
            .get("knot_base")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        if let Some(peers) = cfg.get("learned_peers").and_then(|v| v.as_array()) {
            crate::http::merge_peer_urls(
                &mut e.learned_peers,
                peers.iter().filter_map(|v| v.as_str()),
            );
        }
    }
    if dir.join("identities.json").exists() {
        e.identities = read_json(&dir.join("identities.json"))?;
    }
    if dir.join("heads.json").exists() {
        e.heads = read_json(&dir.join("heads.json"))?;
    }
    if dir.join("rev.json").exists() {
        e.rev = read_json(&dir.join("rev.json"))?;
    }
    if dir.join("revoked.json").exists() {
        let keys: Vec<String> = read_json(&dir.join("revoked.json"))?;
        for k in keys {
            e.revoked.insert(k, ());
        }
    }
    if dir.join("erased-cids.json").exists() {
        e.private_archive_erasure_cids = read_json(&dir.join("erased-cids.json"))?;
    }
    if dir.join("cap-uses.json").exists() {
        e.cap_uses = read_json(&dir.join("cap-uses.json"))?;
    }
    if dir.join("subscribe.json").exists() {
        e.event_log = read_json(&dir.join("subscribe.json"))?;
    }
    if dir.join("blob-types.json").exists() {
        let rows: Vec<serde_json::Value> = read_json(&dir.join("blob-types.json"))?;
        for row in rows {
            if let (Some(did), Some(cid), Some(mt)) = (
                row.get("did").and_then(|v| v.as_str()),
                row.get("cid").and_then(|v| v.as_str()),
                row.get("mediaType").and_then(|v| v.as_str()),
            ) {
                e.blob_types
                    .insert((did.to_string(), cid.to_string()), mt.to_string());
            }
        }
    }
    let repos = dir.join("repos");
    if repos.is_dir() {
        load_repos(&mut e, &repos)?;
    }
    let blobs = dir.join("blobs");
    if blobs.is_dir() {
        load_blobs(&mut e, &blobs)?;
    }
    let blocks = dir.join("blocks");
    if blocks.is_dir() {
        for ent in std::fs::read_dir(&blocks).map_err(fs_err)? {
            let ent = ent.map_err(fs_err)?;
            if !ent.file_type().map_err(fs_err)?.is_file() {
                continue;
            }
            let cid = ent.file_name().to_string_lossy().into_owned();
            let bytes = std::fs::read(ent.path()).map_err(fs_err)?;
            e.blocks.insert(cid, bytes);
        }
    }
    rebuild_derived(&mut e);
    let erasures = dir.join("private-archive-erasures.json");
    if erasures.exists() {
        let fences: Vec<(String, Vec<String>, Vec<String>)> = read_json(&erasures)?;
        e.data_dir = Some(dir.to_path_buf());
        for (owner, records, blobs) in fences {
            e.erase_private_archive(&owner, &records, &blobs)?;
        }
    }
    Ok(e)
}

fn load_repos(e: &mut KnotEngine, repos: &Path) -> Result<()> {
    for repo in std::fs::read_dir(repos).map_err(fs_err)? {
        let repo = repo.map_err(fs_err)?;
        if !repo.file_type().map_err(fs_err)?.is_dir() {
            continue;
        }
        let collections = repo.path().join("collections");
        if !collections.is_dir() {
            continue;
        }
        for coll in std::fs::read_dir(&collections).map_err(fs_err)? {
            let coll = coll.map_err(fs_err)?;
            if !coll.file_type().map_err(fs_err)?.is_dir() {
                continue;
            }
            for rkey in std::fs::read_dir(coll.path()).map_err(fs_err)? {
                let rkey = rkey.map_err(fs_err)?;
                if !rkey.file_type().map_err(fs_err)?.is_dir() {
                    continue;
                }
                for file in std::fs::read_dir(rkey.path()).map_err(fs_err)? {
                    let file = file.map_err(fs_err)?;
                    let name = file.file_name();
                    let name = name.to_string_lossy();
                    if name == "tip" {
                        let cid = std::fs::read_to_string(file.path()).map_err(fs_err)?;
                        let rec_path = rkey.path().join(format!("{cid}.json"));
                        if rec_path.exists() {
                            let rec: StoredRecord = read_json(&rec_path)?;
                            e.tips.insert(
                                (rec.did.clone(), rec.collection.clone(), rec.rkey.clone()),
                                cid.trim().to_string(),
                            );
                        }
                        continue;
                    }
                    if name.ends_with(".json") {
                        let rec: StoredRecord = read_json(&file.path())?;
                        e.stored.push(rec);
                    }
                }
            }
        }
    }
    Ok(())
}

fn load_blobs(e: &mut KnotEngine, blobs: &Path) -> Result<()> {
    for did_dir in std::fs::read_dir(blobs).map_err(fs_err)? {
        let did_dir = did_dir.map_err(fs_err)?;
        if !did_dir.file_type().map_err(fs_err)?.is_dir() {
            continue;
        }
        let did = std::fs::read_to_string(did_dir.path().join("did.txt"))
            .map_err(fs_err)?
            .trim()
            .to_string();
        for file in std::fs::read_dir(did_dir.path()).map_err(fs_err)? {
            let file = file.map_err(fs_err)?;
            if file.file_name() == "did.txt" {
                continue;
            }
            if !file.file_type().map_err(fs_err)?.is_file() {
                continue;
            }
            let cid = file.file_name().to_string_lossy().into_owned();
            let bytes = std::fs::read(file.path()).map_err(fs_err)?;
            e.blobs.insert((did.clone(), cid), bytes);
        }
    }
    Ok(())
}

fn rebuild_derived(e: &mut KnotEngine) {
    let tips = e.tips.clone();
    for ((did, coll, rkey), cid) in &tips {
        let key = crate::sync::mst_key(coll, rkey);
        if let Ok(parsed) = Cid::parse(cid) {
            e.msts
                .entry(did.clone())
                .or_default()
                .insert(key.as_bytes(), parsed)
                .ok();
        }
    }
    let stored = e.stored.clone();
    for ((did, coll, rkey), cid) in &tips {
        if let Some(rec) = stored.iter().find(|r| r.did == *did && r.cid == *cid) {
            let resp = e.record_response(rec);
            e.records
                .insert((did.clone(), coll.clone(), rkey.clone()), resp);
        }
    }
}

#[cfg(test)]
mod persist_tests {
    use super::*;

    #[test]
    fn persist_roundtrip_record_and_blob() {
        let dir = std::env::temp_dir().join(format!(
            "tangled-engine-persist-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let did = "did:sol:persisttest";
        let mut e = KnotEngine::hosting(did);
        e.data_dir = Some(dir.clone());
        e.ingest_record(serde_json::json!({
            "v": 1,
            "collection": "pub.tangled.post",
            "rkey": "p1",
            "author": did,
            "vis": "public",
            "body": { "text": "snap" },
        }))
        .unwrap();
        e.blobs
            .insert((did.into(), "bafkblob".into()), b"raw".to_vec());
        e.persist().unwrap();

        let loaded = KnotEngine::open(&dir, did).unwrap();
        assert_eq!(loaded.stored.len(), 1);
        assert_eq!(loaded.stored[0].record["body"]["text"], "snap");
        assert_eq!(
            loaded.blobs.get(&(did.into(), "bafkblob".into())).unwrap(),
            b"raw"
        );
        assert!(loaded.heads.contains_key(did));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_repo_auth_honors_body_since() {
        let did = "did:sol:sincetest";
        let mut e = KnotEngine::hosting(did);
        e.ingest_record(serde_json::json!({
            "v": 1, "collection": "pub.tangled.post", "rkey": "a",
            "author": did, "vis": "public", "body": { "text": "one" },
        }))
        .unwrap();
        let first = e.stored[0].cid.clone();
        e.ingest_record(serde_json::json!({
            "v": 1, "collection": "pub.tangled.post", "rkey": "b",
            "author": did, "vis": "public", "body": { "text": "two" },
        }))
        .unwrap();
        let body = serde_json::to_vec(&crate::http::GetRepoAuthRequest {
            did: did.into(),
            since: Some(first.clone()),
            blobs: None,
        })
        .unwrap();
        let resp = e
            .handle(&KnotRequest {
                method: Method::Post,
                path: "/xrpc/tangled.sync.getRepoAuth".into(),
                query: vec![],
                headers: vec![],
                body: Some(body),
                content_type: Some("application/json".into()),
            })
            .unwrap();
        assert_eq!(resp.status, 401);
        let qresp = e
            .handle(&KnotRequest {
                method: Method::Get,
                path: "/xrpc/tangled.sync.getRepo".into(),
                query: vec![("did".into(), did.into()), ("since".into(), first)],
                headers: vec![],
                body: None,
                content_type: None,
            })
            .unwrap();
        assert_eq!(qresp.status, 200);
    }

    #[test]
    fn blob_get_sends_length_and_media_type() {
        let did = "did:sol:blobtype";
        let mut e = KnotEngine::hosting(did);
        let bytes = b"png-bytes".to_vec();
        let resp = e
            .handle(&KnotRequest {
                method: Method::Post,
                path: "/xrpc/tangled.repo.putBlob".into(),
                query: vec![("mediaType".into(), "image/png".into())],
                headers: vec![],
                body: Some(bytes.clone()),
                content_type: Some("application/octet-stream".into()),
            })
            .unwrap();
        let put: crate::http::PutBlobResponse = resp.json().unwrap();
        e.ingest_record(serde_json::json!({
            "v": 1, "collection": "pub.tangled.post", "rkey": "pic",
            "author": did, "vis": "public",
            "body": { "text": "pic", "blob": put.cid },
        }))
        .unwrap();
        let got = e
            .handle(&KnotRequest {
                method: Method::Get,
                path: "/xrpc/tangled.sync.getBlob".into(),
                query: vec![("did".into(), did.into()), ("cid".into(), put.cid)],
                headers: vec![],
                body: None,
                content_type: None,
            })
            .unwrap();
        assert_eq!(
            got.header("Content-Length"),
            Some(bytes.len().to_string().as_str())
        );
        assert_eq!(got.header("X-Tangled-Media-Type"), Some("image/png"));
        assert_eq!(got.body, bytes);
    }

    #[test]
    fn generative_then_operational_did_document() {
        let id = crate::identity::Identity::from_seeds([21u8; 32], [22u8; 32]);
        let did = id.did_string();
        let mut e = KnotEngine::hosting(&did);
        e.register_identity(&id);
        let gen = e
            .handle(&KnotRequest {
                method: Method::Get,
                path: "/xrpc/tangled.identity.resolve".into(),
                query: vec![("did".into(), did.clone())],
                headers: vec![],
                body: None,
                content_type: None,
            })
            .unwrap();
        let g: crate::http::IdentityResolveResponse = gen.json().unwrap();
        let doc = g.document.expect("document");
        assert!(doc.get("keyAgreement").is_none());
        e.ingest_record(serde_json::json!({
            "v": 1, "collection": "sys.id", "rkey": "self",
            "author": did, "vis": "public",
            "body": {
                "v": 1,
                "pubkey": hex::encode(id.pubkey_bytes()),
                "agreement": hex::encode(id.agreement_public_bytes()),
                "rotation": [hex::encode(id.pubkey_bytes())],
                "recovery": [],
                "endpoints": [{ "type": "knot", "url": "inproc://x" }],
                "alsoKnownAs": [],
                "bindings": []
            }
        }))
        .unwrap();
        let op = e
            .handle(&KnotRequest {
                method: Method::Get,
                path: "/xrpc/tangled.identity.resolve".into(),
                query: vec![("did".into(), did)],
                headers: vec![],
                body: None,
                content_type: None,
            })
            .unwrap();
        let r: crate::http::IdentityResolveResponse = op.json().unwrap();
        let doc = r.document.expect("operational");
        assert_eq!(
            doc["keyAgreement"][0],
            format!("{}#agreement", id.did_string())
        );
        assert_eq!(
            doc["verificationMethod"][1]["publicKeyBase58"],
            crate::identity::encode_pubkey_base58(&id.agreement_public_bytes())
        );
        assert!(r.id_cid.is_some());
    }

    #[test]
    fn signed_commit_prev_chain_and_mismatch() {
        let id = crate::identity::Identity::from_seeds([23u8; 32], [24u8; 32]);
        let did = id.did_string();
        let mut e = KnotEngine::hosting(&did);
        e.register_identity(&id);
        e.ingest_record(serde_json::json!({
            "v": 1, "collection": "pub.tangled.post", "rkey": "a",
            "author": did, "vis": "public", "body": { "text": "one" },
        }))
        .unwrap();
        let head1 = e.heads.get(&did).cloned().unwrap();
        assert!(!head1.data.is_empty());
        let c1 = record::parse_commit(e.blocks.get(&head1.commit).unwrap()).unwrap();
        assert!(c1.prev.is_none());
        assert_eq!(c1.data, head1.data);
        record::verify_commit(&id.pubkey_bytes(), &c1.unsigned, &c1.sig).unwrap();
        e.ingest_record(serde_json::json!({
            "v": 1, "collection": "pub.tangled.post", "rkey": "b",
            "author": did, "vis": "public", "body": { "text": "two" },
        }))
        .unwrap();
        let head2 = e.heads.get(&did).cloned().unwrap();
        assert_ne!(head2.commit, head1.commit);
        let c2 = record::parse_commit(e.blocks.get(&head2.commit).unwrap()).unwrap();
        assert_eq!(c2.prev.as_deref(), Some(head1.commit.as_str()));
        let bad = e
            .handle(&KnotRequest {
                method: Method::Post,
                path: "/xrpc/tangled.repo.putRecord".into(),
                query: vec![],
                headers: vec![],
                body: Some(
                    serde_json::to_vec(&serde_json::json!({
                        "prev": "bafyr4idnotthehead0000000000000000000000000000000000000000",
                        "record": record::sign_json_record(id.signing_key(), &serde_json::json!({
                            "v": 1, "collection": "pub.tangled.post", "rkey": "c",
                            "author": did, "vis": "public", "body": { "text": "nope" },
                        })).unwrap().json
                    }))
                    .unwrap(),
                ),
                content_type: Some("application/json".into()),
            })
            .unwrap();
        assert_eq!(bad.status, 409, "{}", String::from_utf8_lossy(&bad.body));
        let error: serde_json::Value = serde_json::from_slice(&bad.body).unwrap();
        assert_eq!(error["error"], "ConcurrentHeads");
        assert_eq!(error["head"], head2.commit);
        let mut cache = crate::sync::HeadCache::default();
        cache.observe(&did, &head2.commit, &head2.rev).unwrap();
        assert!(cache.observe(&did, &head1.commit, &head1.rev).is_err());
        let repo = e
            .handle(&KnotRequest {
                method: Method::Get,
                path: "/xrpc/tangled.sync.getRepo".into(),
                query: vec![("did".into(), did)],
                headers: vec![],
                body: None,
                content_type: None,
            })
            .unwrap();
        let car = crate::car::decode(&repo.body).unwrap();
        assert_eq!(car.roots[0].to_string(), head2.commit);
        assert!(car
            .get(&crate::cid::Cid::parse(&head2.commit).unwrap())
            .is_some());
        assert!(car
            .get(&crate::cid::Cid::parse(&head1.commit).unwrap())
            .is_some());
    }

    #[test]
    fn imported_commit_delta_requires_valid_local_predecessors_and_car_root() {
        let identity = crate::identity::Identity::from_seeds([231; 32], [232; 32]);
        let data = Cid::dag_cbor(b"commit-delta-data");
        let first = record::seal_commit(
            identity.signing_key(),
            &record::unsigned_commit(&identity.did_string(), &data, "one", None),
        )
        .unwrap();
        let second = record::seal_commit(
            identity.signing_key(),
            &record::unsigned_commit(&identity.did_string(), &data, "two", Some(&first.cid)),
        )
        .unwrap();
        let delta = Car {
            roots: vec![second.cid.clone()],
            blocks: vec![(second.cid.clone(), second.signed_cbor.clone())],
        };
        assert!(
            verify_imported_commits(&delta, &HashMap::new()).is_err(),
            "missing local predecessor must fail"
        );
        let mut stored = HashMap::from([(first.cid.to_string(), first.signed_cbor.clone())]);
        assert!(verify_imported_commits(&delta, &stored).is_ok());
        let mut tampered = crate::dagcbor::decode(&first.signed_cbor).unwrap();
        if let crate::dagcbor::Value::Map(map) = &mut tampered {
            map.insert("sig".into(), crate::dagcbor::Value::Bytes(vec![0; 64]));
        }
        stored.insert(
            first.cid.to_string(),
            crate::dagcbor::encode(&tampered).unwrap(),
        );
        assert!(
            verify_imported_commits(&delta, &stored).is_err(),
            "tampered local predecessor signature must fail"
        );
        stored.insert(first.cid.to_string(), first.signed_cbor);
        stored.insert(second.cid.to_string(), second.signed_cbor);
        let missing_root = Car {
            roots: vec![second.cid],
            blocks: vec![],
        };
        assert!(
            verify_imported_commits(&missing_root, &stored).is_err(),
            "current CAR must provide its own root block"
        );
    }

    #[test]
    fn ap_httpsig_mutated_body_rejected() {
        let id = crate::identity::Identity::from_seeds([25u8; 32], [26u8; 32]);
        let did = id.did_string();
        let mut e = KnotEngine::hosting(&did);
        e.register_identity(&id);
        let body = br#"{"type":"Create"}"#;
        let (input, sig, digest) = crate::httpsig::sign_ed25519(
            id.signing_key(),
            &format!("{did}#key"),
            "POST",
            "/inbox",
            body,
        );
        let signed = |mut value: serde_json::Value| {
            value["type"] = serde_json::json!(Cid::dag_cbor(b"sys.foreign").to_string());
            value["ts"] = serde_json::json!(1_700_000_000_000u64);
            value["prev"] = serde_json::json!([]);
            record::sign_json_record(id.signing_key(), &value)
                .unwrap()
                .json
        };
        let rec = serde_json::json!({
            "v": 1, "collection": "sys.foreign", "rkey": "ap1",
            "author": did, "vis": "public",
            "body": {
                "protocol": "activitypub",
                "foreign": "https://ex.example/users/a",
                "attested": true,
                "foreignProof": {
                    "did": did,
                    "actor": "https://ex.example/users/a",
                    "signatureInput": input,
                    "signature": sig,
                    "contentDigest": digest,
                    "method": "POST",
                    "path": "/inbox",
                    "body": "tamper"
                }
            }
        });
        let resp = e
            .handle(&KnotRequest {
                method: Method::Post,
                path: "/xrpc/tangled.repo.putRecord".into(),
                query: vec![],
                headers: vec![],
                body: Some(
                    serde_json::to_vec(&serde_json::json!({ "record": signed(rec) })).unwrap(),
                ),
                content_type: Some("application/json".into()),
            })
            .unwrap();
        assert_eq!(resp.status, 400);
        let error: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(error["error"], "InvalidRequest");
        assert!(
            error["message"].as_str().unwrap().contains("foreignProof"),
            "{error}"
        );
        assert!(e.stored.iter().all(|record| record.rkey != "ap1"));
        let unattested = serde_json::json!({
            "v": 1, "collection": "sys.foreign", "rkey": "ap2",
            "author": did, "vis": "public",
            "body": { "protocol": "activitypub", "foreign": "https://ex.example/n/1", "attested": false }
        });
        let resp = e
            .handle(&KnotRequest {
                method: Method::Post,
                path: "/xrpc/tangled.repo.putRecord".into(),
                query: vec![],
                headers: vec![],
                body: Some(
                    serde_json::to_vec(&serde_json::json!({ "record": signed(unattested) }))
                        .unwrap(),
                ),
                content_type: Some("application/json".into()),
            })
            .unwrap();
        assert_eq!(resp.status, 400);
        let error: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(error["error"], "InvalidRequest");
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .contains("attested: true"),
            "{error}"
        );
        assert!(e.stored.iter().all(|record| record.rkey != "ap2"));
        let attested_only = serde_json::json!({
            "v": 1, "collection": "sys.foreign", "rkey": "ap3",
            "author": did, "vis": "public",
            "body": {
                "protocol": "activitypub",
                "foreign": "https://ex.example/users/a",
                "attested": true,
                "foreignProof": { "did": did, "actor": "https://ex.example/users/a" }
            }
        });
        let resp = e
            .handle(&KnotRequest {
                method: Method::Post,
                path: "/xrpc/tangled.repo.putRecord".into(),
                query: vec![],
                headers: vec![],
                body: Some(
                    serde_json::to_vec(&serde_json::json!({ "record": signed(attested_only) }))
                        .unwrap(),
                ),
                content_type: Some("application/json".into()),
            })
            .unwrap();
        assert_eq!(resp.status, 400);
        let error: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(error["error"], "InvalidRequest");
        assert!(
            error["message"].as_str().unwrap().contains("foreignProof"),
            "{error}"
        );
        assert!(e.stored.iter().all(|record| record.rkey != "ap3"));
        let proof =
            crate::foreign::activitypub_foreign_proof(&id, "https://ex.example/users/a", body);
        let ok = serde_json::json!({
            "v": 1, "collection": "sys.foreign", "rkey": "ap4",
            "author": did, "vis": "public",
            "body": {
                "protocol": "activitypub",
                "foreign": "https://ex.example/users/a",
                "attested": true,
                "foreignProof": proof
            }
        });
        let resp = e
            .handle(&KnotRequest {
                method: Method::Post,
                path: "/xrpc/tangled.repo.putRecord".into(),
                query: vec![],
                headers: vec![],
                body: Some(
                    serde_json::to_vec(&serde_json::json!({ "record": signed(ok) })).unwrap(),
                ),
                content_type: Some("application/json".into()),
            })
            .unwrap();
        assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    }

    #[test]
    fn subscribe_since_survives_restart() {
        let dir = std::env::temp_dir().join(format!(
            "tangled-sub-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let id = crate::identity::Identity::from_seeds([41u8; 32], [42u8; 32]);
        let did = id.did_string();
        let mut e = KnotEngine::open(&dir, &did).unwrap();
        e.register_identity(&id);
        e.ingest_record(serde_json::json!({
            "v": 1, "collection": "pub.tangled.post", "rkey": "a",
            "author": did, "vis": "public", "body": { "text": "one" },
        }))
        .unwrap();
        let first = e.heads.get(&did).unwrap().commit.clone();
        e.ingest_record(serde_json::json!({
            "v": 1, "collection": "pub.tangled.post", "rkey": "b",
            "author": did, "vis": "public", "body": { "text": "two" },
        }))
        .unwrap();
        drop(e);
        let loaded = KnotEngine::open(&dir, &did).unwrap();
        let evs = loaded.events_after(Some(&did), &first, None);
        assert!(evs.iter().any(|ev| ev.ops.iter().any(|op| op.rkey == "b")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_identity_key_cannot_sign_after_sys_id_rotation() {
        let old = crate::identity::Identity::from_seeds([7u8; 32], [8u8; 32]);
        let new = crate::identity::Identity::from_seeds([9u8; 32], [10u8; 32]);
        let did = old.did_string();
        let mut e = KnotEngine::hosting(&did);
        e.register_identity(&old);
        let post = |id: &crate::identity::Identity, rkey: &str| {
            let unsigned = serde_json::json!({
                "v": 1, "collection": "pub.tangled.post", "rkey": rkey,
                "author": did.clone(), "ts": 1, "prev": [], "vis": "public",
                "body": { "text": rkey },
            });
            record::sign_json_record(id.signing_key(), &unsigned)
                .unwrap()
                .json
        };
        e.ingest_record(post(&old, "before")).unwrap();
        let rotate = serde_json::json!({
            "v": 1, "collection": "sys.id", "rkey": "self",
            "author": did, "ts": 2, "prev": [], "vis": "public",
            "body": {
                "v": 1,
                "pubkey": hex::encode(new.pubkey_bytes()),
                "agreement": hex::encode(new.agreement_public_bytes()),
                "rotation": [hex::encode(new.pubkey_bytes())],
                "recovery": [],
                "endpoints": [{ "type": "knot", "url": "https://next.example" }],
                "alsoKnownAs": [],
                "bindings": [],
            },
        });
        let signed = record::sign_json_record(old.signing_key(), &rotate).unwrap();
        e.ingest_record(signed.json).unwrap();
        let err = e.ingest_record(post(&old, "after")).unwrap_err();
        assert!(err.to_string().contains("invalid signature"), "{err}");
        e.signing_keys.insert(did.clone(), new.to_seeds().0);
        e.ingest_record(post(&new, "next")).unwrap();
    }
}
