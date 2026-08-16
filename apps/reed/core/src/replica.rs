use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{ReedError, Result};
use crate::types::{
    MediaRef, Post, PostId, Profile, SocialEvent, SocialKind, TimelineItem, TimelineReason,
};
use crate::wire::{
    record_cid, BLOCK, BOOST, DEFAULT_MAX_BLOB, FOLLOW, FOREIGN, LIKE, MUTE, POST, PROFILE, SCHEMA,
};

#[derive(Serialize, Deserialize, Clone)]
struct StoredRecord {
    cid: String,
    collection: String,
    rkey: String,
    author: String,
    ts: u64,
    vis: String,
    env_ref: Option<String>,
    body: serde_json::Value,
    deleted: bool,
}

#[derive(Serialize, Deserialize)]
struct Data {
    records: Vec<StoredRecord>,
    handles: HashMap<String, String>,
    aliases: HashMap<String, String>,
    max_blob: u64,
    schemas: HashSet<String>,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            handles: HashMap::new(),
            aliases: HashMap::new(),
            max_blob: DEFAULT_MAX_BLOB,
            schemas: HashSet::new(),
        }
    }
}

pub struct Replica {
    dir: PathBuf,
    data: Data,
}

impl Replica {
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        std::fs::create_dir_all(dir.join("blobs"))?;
        let path = dir.join("replica.json");
        let data = if path.exists() {
            let s = std::fs::read_to_string(&path)?;
            serde_json::from_str(&s).map_err(|e| ReedError::Codec(e.to_string()))?
        } else {
            Data::default()
        };
        Ok(Self {
            dir: dir.to_path_buf(),
            data,
        })
    }

    pub fn max_blob(&self) -> u64 {
        self.data.max_blob
    }

    pub fn set_max_blob(&mut self, n: u64) {
        self.data.max_blob = n;
        let _ = self.persist();
    }

    pub fn store_blob(&mut self, cid: &str, bytes: &[u8]) -> Result<()> {
        let path = self.dir.join("blobs").join(cid);
        std::fs::write(path, bytes)?;
        Ok(())
    }

    pub fn blob(&self, cid: &str) -> Option<Vec<u8>> {
        std::fs::read(self.dir.join("blobs").join(cid)).ok()
    }

    pub fn has_schema(&self, name: &str) -> bool {
        self.data.schemas.contains(name)
    }

    pub fn home(&self, me: &str) -> Vec<TimelineItem> {
        let follows = self.follow_subjects(me);
        let mut authors = follows;
        authors.insert(me.to_string());
        let hide = self.hidden_authors(me);
        let mut items = Vec::new();
        for rec in self.live(POST) {
            if hide.contains(&rec.author) {
                continue;
            }
            if !authors.contains(&rec.author) {
                continue;
            }
            if let Some(post) = self.hydrate(&rec.cid, Some(me)) {
                if post.deleted {
                    continue;
                }
                items.push(TimelineItem {
                    post,
                    reason: TimelineReason::Authored,
                });
            }
        }
        for rec in self.live(BOOST) {
            if hide.contains(&rec.author) || !authors.contains(&rec.author) {
                continue;
            }
            let Some(target) = rec.body.get("target").and_then(|t| t.as_str()) else {
                continue;
            };
            let Some(post) = self.hydrate(target, Some(me)) else {
                continue;
            };
            if post.deleted || hide.contains(&post.author) {
                continue;
            }
            items.push(TimelineItem {
                post,
                reason: TimelineReason::Boost {
                    by: rec.author.clone(),
                },
            });
        }
        items.sort_by_key(|b| std::cmp::Reverse(b.post.ts_us));
        items
    }

    pub fn discover(&self, me: &str) -> Vec<TimelineItem> {
        let hide = self.hidden_authors(me);
        let mut items = Vec::new();
        for rec in self.live(POST) {
            if hide.contains(&rec.author) {
                continue;
            }
            if let Some(post) = self.hydrate(&rec.cid, Some(me)) {
                if post.deleted {
                    continue;
                }
                items.push(TimelineItem {
                    post,
                    reason: TimelineReason::Authored,
                });
            }
        }
        items.sort_by_key(|b| std::cmp::Reverse(b.post.ts_us));
        items
    }

    pub fn profile_posts(&self, did: &str) -> Vec<Post> {
        self.profile_posts_view(did, None)
    }

    pub fn profile_posts_view(&self, did: &str, viewer: Option<&str>) -> Vec<Post> {
        let mut posts: Vec<Post> = self
            .live(POST)
            .into_iter()
            .filter(|r| r.author == did)
            .filter_map(|r| self.hydrate(&r.cid, viewer))
            .filter(|p| !p.deleted)
            .collect();
        posts.sort_by_key(|b| std::cmp::Reverse(b.ts_us));
        posts
    }

    pub fn follows(&self, me: &str, subject: &str) -> bool {
        self.follow_subjects(me).contains(subject)
    }

    pub fn reply_count(&self, id: &PostId) -> u32 {
        self.live(POST)
            .into_iter()
            .filter(|r| {
                !r.deleted && r.body.get("reply").and_then(|x| x.as_str()) == Some(id.0.as_str())
            })
            .count() as u32
    }

    pub fn thread(&self, id: &PostId) -> Vec<Post> {
        self.thread_view(id, None)
    }

    pub fn thread_view(&self, id: &PostId, viewer: Option<&str>) -> Vec<Post> {
        let root = self.root_of(id);
        let mut posts: Vec<Post> = self
            .live(POST)
            .into_iter()
            .filter_map(|r| self.hydrate(&r.cid, viewer))
            .filter(|p| !p.deleted)
            .filter(|p| self.root_of(&p.id) == root)
            .collect();
        posts.sort_by(|a, b| {
            if a.id == root {
                std::cmp::Ordering::Less
            } else if b.id == root {
                std::cmp::Ordering::Greater
            } else {
                a.ts_us.cmp(&b.ts_us)
            }
        });
        posts
    }

    pub fn post(&self, id: &PostId) -> Option<Post> {
        self.hydrate(&id.0, None)
    }

    pub fn profile(&self, did: &str) -> Option<Profile> {
        let rec = self
            .live(PROFILE)
            .into_iter()
            .find(|r| r.author == did && r.rkey == "self");
        let foreign = self.ap_foreign_for_author(did);
        if rec.is_none() && foreign.is_none() {
            return None;
        }
        Some(Profile {
            did: did.to_string(),
            handle: rec
                .and_then(|r| body_acct(&r.body, "handle"))
                .or_else(|| self.data.handles.get(did).cloned())
                .or_else(|| foreign.and_then(|r| foreign_acct(&r.body))),
            display_name: rec
                .and_then(|r| r.body.get("displayName"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            bio: rec
                .and_then(|r| r.body.get("bio"))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            avatar_cid: rec
                .and_then(|r| r.body.get("avatar"))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            source_url: rec
                .and_then(|r| http_url_field(&r.body, "url"))
                .or_else(|| foreign.and_then(|r| http_url_field(&r.body, "foreign"))),
        })
    }

    pub fn inbound_social(&self, me: &str) -> Vec<SocialEvent> {
        let mut out = Vec::new();
        for rec in self.live(FOLLOW) {
            if rec.author == me {
                continue;
            }
            if rec.body.get("subject").and_then(|s| s.as_str()) != Some(me) {
                continue;
            }
            out.push(SocialEvent {
                id: rec.cid.clone(),
                actor: rec.author.clone(),
                kind: SocialKind::Follow,
                target: None,
            });
        }
        for (collection, kind) in [(LIKE, SocialKind::Like), (BOOST, SocialKind::Boost)] {
            for rec in self.live(collection) {
                if rec.author == me {
                    continue;
                }
                let Some(target) = rec.body.get("target").and_then(|t| t.as_str()) else {
                    continue;
                };
                let Some(post) = self.hydrate(target, None) else {
                    continue;
                };
                if post.author != me || post.deleted {
                    continue;
                }
                out.push(SocialEvent {
                    id: rec.cid.clone(),
                    actor: rec.author.clone(),
                    kind,
                    target: Some(PostId(target.to_string())),
                });
            }
        }
        out
    }

    pub fn apply_record(&mut self, bytes: &[u8]) -> Result<()> {
        let v: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| ReedError::Codec(e.to_string()))?;
        let collection = v
            .get("collection")
            .and_then(|c| c.as_str())
            .ok_or_else(|| ReedError::Codec("missing collection".into()))?
            .to_string();
        let author = v
            .get("author")
            .and_then(|a| a.as_str())
            .ok_or_else(|| ReedError::Codec("missing author".into()))?
            .to_string();
        let rkey = v
            .get("rkey")
            .and_then(|r| r.as_str())
            .ok_or_else(|| ReedError::Codec("missing rkey".into()))?
            .to_string();
        let ts = v.get("ts").and_then(|t| t.as_u64()).unwrap_or(0);
        let vis = v
            .get("vis")
            .and_then(|x| x.as_str())
            .unwrap_or("public")
            .to_string();
        let env_ref = v.get("ref").and_then(|r| r.as_str()).map(str::to_string);
        let body = v.get("body").cloned().unwrap_or(serde_json::Value::Null);
        let deleted = body
            .get("deleted")
            .and_then(|d| d.as_bool())
            .unwrap_or(false);
        let cid = record_cid(&v).unwrap_or_else(|_| tangled::Cid::dag_cbor(bytes).to_string());
        if collection == SCHEMA {
            if let Some(name) = body.get("name").and_then(|n| n.as_str()) {
                self.data.schemas.insert(name.to_string());
            }
        }
        if collection == PROFILE {
            if let Some(h) = body.get("handle").and_then(|v| v.as_str()) {
                remember_handle(&mut self.data.handles, &author, h);
            }
        }
        if collection == FOREIGN {
            if let Some(h) = body.get("acct").and_then(|v| v.as_str()) {
                remember_handle(&mut self.data.handles, &author, h);
            }
        }
        self.upsert(StoredRecord {
            cid,
            collection,
            rkey,
            author,
            ts,
            vis,
            env_ref,
            body,
            deleted,
        });
        self.persist()?;
        Ok(())
    }

    pub(crate) fn all_posts(&self) -> Vec<Post> {
        self.live(POST)
            .into_iter()
            .filter_map(|r| self.hydrate(&r.cid, None))
            .collect()
    }

    pub fn liked_by(&self, me: &str, id: &PostId) -> bool {
        self.has_target(LIKE, &id.0, me)
    }

    pub fn boosted_by(&self, me: &str, id: &PostId) -> bool {
        self.has_target(BOOST, &id.0, me)
    }

    pub(crate) fn follow_subjects(&self, me: &str) -> HashSet<String> {
        self.live(FOLLOW)
            .into_iter()
            .filter(|r| r.author == me)
            .filter_map(|r| {
                r.body
                    .get("subject")
                    .and_then(|s| s.as_str())
                    .map(str::to_string)
            })
            .collect()
    }

    pub(crate) fn follow_knots(&self, me: &str) -> Vec<(String, Option<String>)> {
        self.live(FOLLOW)
            .into_iter()
            .filter(|r| r.author == me)
            .filter_map(|r| {
                let subject = r.body.get("subject").and_then(|s| s.as_str())?.to_string();
                let knot = r
                    .body
                    .get("knot")
                    .and_then(|k| k.as_str())
                    .map(str::to_string);
                Some((subject, knot))
            })
            .collect()
    }

    pub(crate) fn post_body(&self, id: &PostId) -> Option<serde_json::Value> {
        let cid = self.resolve_cid(&id.0);
        self.data
            .records
            .iter()
            .find(|r| r.collection == POST && r.cid == cid)
            .map(|r| r.body.clone())
    }

    fn hidden_authors(&self, me: &str) -> HashSet<String> {
        let mut hide = HashSet::new();
        for rec in self.live(MUTE).into_iter().chain(self.live(BLOCK)) {
            if rec.author != me {
                continue;
            }
            if let Some(s) = rec.body.get("subject").and_then(|s| s.as_str()) {
                hide.insert(s.to_string());
            }
        }
        for rec in self.live(BLOCK) {
            if rec.body.get("subject").and_then(|s| s.as_str()) == Some(me) {
                hide.insert(rec.author.clone());
            }
        }
        hide
    }

    fn live(&self, collection: &str) -> Vec<&StoredRecord> {
        self.data
            .records
            .iter()
            .filter(|r| r.collection == collection && !r.deleted)
            .collect()
    }

    fn resolve_cid(&self, cid: &str) -> String {
        let mut cur = cid.to_string();
        let mut seen = HashSet::new();
        while let Some(next) = self.data.aliases.get(&cur) {
            if !seen.insert(cur.clone()) {
                break;
            }
            cur = next.clone();
        }
        cur
    }

    fn hydrate(&self, cid: &str, viewer: Option<&str>) -> Option<Post> {
        let cid = self.resolve_cid(cid);
        let rec = self
            .data
            .records
            .iter()
            .find(|r| r.collection == POST && r.cid == cid)?;
        let text = rec
            .body
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let langs = rec
            .body
            .get("langs")
            .and_then(|l| l.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let reply_to = rec
            .body
            .get("reply")
            .and_then(|r| r.as_str())
            .map(|s| PostId(s.to_string()));
        let quote_of = rec.env_ref.as_ref().map(|s| PostId(s.clone()));
        let media = rec
            .body
            .get("media")
            .and_then(|m| m.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str())
                    .map(|cid| self.media_ref(cid))
                    .collect()
            })
            .unwrap_or_default();
        let like_count = self.count_target(LIKE, &cid);
        let boost_count = self.count_target(BOOST, &cid);
        let liked_by_me = viewer.is_some_and(|me| self.has_target(LIKE, &cid, me));
        let boosted_by_me = viewer.is_some_and(|me| self.has_target(BOOST, &cid, me));
        Some(Post {
            id: PostId(rec.cid.clone()),
            rkey: rec.rkey.clone(),
            author: rec.author.clone(),
            handle: self
                .data
                .handles
                .get(&rec.author)
                .cloned()
                .or_else(|| self.profile_handle(&rec.author))
                .or_else(|| {
                    self.ap_mapped_foreign(&rec.cid, &rec.rkey)
                        .and_then(|r| foreign_acct(&r.body))
                }),
            text,
            langs,
            reply_to,
            quote_of,
            media,
            ts_us: rec.ts,
            like_count,
            boost_count,
            liked_by_me,
            boosted_by_me,
            deleted: rec.deleted,
            attested: self.is_attested(&rec.cid, &rec.rkey),
            source_url: self
                .ap_mapped_foreign(&rec.cid, &rec.rkey)
                .and_then(|r| post_source_url(&r.body)),
        })
    }

    fn profile_handle(&self, did: &str) -> Option<String> {
        let rec = self
            .live(PROFILE)
            .into_iter()
            .find(|r| r.author == did && r.rkey == "self")?;
        body_acct(&rec.body, "handle")
    }

    fn ap_mapped_foreign(&self, post_cid: &str, post_rkey: &str) -> Option<&StoredRecord> {
        self.data.records.iter().find(|r| {
            r.collection == FOREIGN
                && r.body.get("protocol").and_then(|p| p.as_str()) == Some("activitypub")
                && r.body.get("mapped").and_then(|m| m.as_str()) == Some(post_cid)
                && r.rkey == post_rkey
        })
    }

    fn ap_foreign_for_author(&self, did: &str) -> Option<&StoredRecord> {
        self.live(FOREIGN).into_iter().find(|r| {
            r.author == did
                && r.body.get("protocol").and_then(|p| p.as_str()) == Some("activitypub")
        })
    }

    fn is_attested(&self, post_cid: &str, post_rkey: &str) -> bool {
        self.ap_mapped_foreign(post_cid, post_rkey)
            .is_some_and(|r| r.body.get("attested").and_then(|a| a.as_bool()) == Some(true))
    }

    fn media_ref(&self, cid: &str) -> MediaRef {
        if let Some(rec) = self.data.records.iter().find(|r| r.cid == cid) {
            return MediaRef {
                cid: rec
                    .body
                    .get("blob")
                    .and_then(|b| b.as_str())
                    .unwrap_or(cid)
                    .to_string(),
                mime: rec
                    .body
                    .get("mime")
                    .and_then(|m| m.as_str())
                    .unwrap_or("application/octet-stream")
                    .to_string(),
                alt: rec
                    .body
                    .get("alt")
                    .and_then(|a| a.as_str())
                    .map(str::to_string),
            };
        }
        MediaRef {
            cid: cid.to_string(),
            mime: "application/octet-stream".into(),
            alt: None,
        }
    }

    fn count_target(&self, collection: &str, target: &str) -> u32 {
        self.live(collection)
            .into_iter()
            .filter(|r| r.body.get("target").and_then(|t| t.as_str()) == Some(target))
            .count() as u32
    }

    fn has_target(&self, collection: &str, target: &str, author: &str) -> bool {
        self.live(collection).into_iter().any(|r| {
            r.author == author && r.body.get("target").and_then(|t| t.as_str()) == Some(target)
        })
    }

    fn root_of(&self, id: &PostId) -> PostId {
        let mut cur = id.clone();
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(cur.0.clone()) {
                break;
            }
            let Some(rec) = self
                .data
                .records
                .iter()
                .find(|r| r.collection == POST && r.cid == cur.0)
            else {
                break;
            };
            match rec.body.get("reply").and_then(|r| r.as_str()) {
                Some(parent) => cur = PostId(parent.to_string()),
                None => break,
            }
        }
        cur
    }

    fn upsert(&mut self, rec: StoredRecord) {
        if let Some(i) = self.data.records.iter().position(|r| {
            r.author == rec.author && r.collection == rec.collection && r.rkey == rec.rkey
        }) {
            if rec.ts >= self.data.records[i].ts {
                let old = self.data.records[i].cid.clone();
                if old != rec.cid {
                    self.data.aliases.insert(old, rec.cid.clone());
                }
                self.data.records[i] = rec;
            }
        } else {
            self.data.records.push(rec);
        }
    }

    fn persist(&self) -> Result<()> {
        let path = self.dir.join("replica.json");
        let s = serde_json::to_string(&self.data).map_err(|e| ReedError::Codec(e.to_string()))?;
        std::fs::write(path, s)?;
        Ok(())
    }
}

fn remember_handle(handles: &mut HashMap<String, String>, did: &str, raw: &str) {
    let s = raw
        .strip_prefix("acct:")
        .unwrap_or(raw)
        .trim()
        .trim_start_matches('@');
    if !s.is_empty() && s.contains('@') {
        handles.insert(did.to_string(), s.to_string());
    }
}

fn body_acct(body: &serde_json::Value, key: &str) -> Option<String> {
    let raw = body.get(key).and_then(|v| v.as_str())?;
    let s = raw
        .strip_prefix("acct:")
        .unwrap_or(raw)
        .trim()
        .trim_start_matches('@');
    (!s.is_empty() && s.contains('@')).then(|| s.to_string())
}

fn foreign_acct(body: &serde_json::Value) -> Option<String> {
    body_acct(body, "acct").or_else(|| {
        body.get("foreign")
            .and_then(|f| f.as_str())
            .and_then(|iri| tangled::foreign::activitypub_actor_acct(iri, None))
    })
}

fn http_url(s: &str) -> Option<String> {
    (s.starts_with("https://") || s.starts_with("http://")).then(|| s.to_string())
}

fn http_url_field(body: &serde_json::Value, key: &str) -> Option<String> {
    body.get(key).and_then(|v| v.as_str()).and_then(http_url)
}

fn post_source_url(body: &serde_json::Value) -> Option<String> {
    http_url_field(body, "foreign_id").or_else(|| http_url_field(body, "foreign"))
}
