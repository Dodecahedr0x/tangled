use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tangled::http::{KnotClient, NodeStats};
use tangled::MockKnot;

use crate::error::{Result, WeftError};
use crate::types::{EdgeKind, KnotNode, KnotUrl, LookupHit, NetworkTotals, NetworkView, TopoEdge};

const PEER_FETCH_CAP: usize = 8;

pub struct Explorer {
    pub knot: KnotUrl,
    pub client: KnotClient,
    remotes: HashMap<String, Arc<Mutex<MockKnot>>>,
}

impl Explorer {
    pub fn http(knot: KnotUrl) -> Result<Self> {
        if knot.0.trim().is_empty() {
            return Err(WeftError::KnotUnreachable);
        }
        Ok(Self {
            client: KnotClient::http(knot.0.clone()),
            knot,
            remotes: HashMap::new(),
        })
    }

    pub fn mock(engine: Arc<Mutex<MockKnot>>) -> Self {
        Self {
            knot: KnotUrl("mock://".into()),
            client: KnotClient::mock_shared(engine),
            remotes: HashMap::new(),
        }
    }

    pub fn with_remote(mut self, url: impl Into<String>, engine: Arc<Mutex<MockKnot>>) -> Self {
        self.remotes.insert(url.into(), engine);
        self
    }

    pub async fn local_stats(&self) -> Result<NodeStats> {
        self.client
            .stats()
            .await
            .map_err(|_| WeftError::KnotUnreachable)
    }

    pub async fn assemble(&self) -> Result<NetworkView> {
        let stats = self.local_stats().await?;
        let home = trim_base(&self.knot.0);
        let mut knots = vec![KnotNode {
            url: home.clone(),
            did: stats.did.clone(),
            reachable: true,
            stats: Some(stats.clone()),
        }];
        let mut edges = Vec::new();
        let mut totals = add_stats(
            NetworkTotals {
                knots: 1,
                ..Default::default()
            },
            &stats,
        );

        for hosted in &stats.topology.hosted {
            edges.push(TopoEdge {
                from: home.clone(),
                to: hosted.did.clone(),
                kind: EdgeKind::Hosted,
            });
        }

        for (i, peer) in stats.topology.peers.iter().enumerate() {
            let url = trim_base(peer);
            edges.push(TopoEdge {
                from: home.clone(),
                to: url.clone(),
                kind: EdgeKind::Peer,
            });
            if i >= PEER_FETCH_CAP {
                if !knots.iter().any(|n| n.url == url) {
                    knots.push(KnotNode {
                        url,
                        did: String::new(),
                        reachable: false,
                        stats: None,
                    });
                    totals.knots += 1;
                }
                continue;
            }
            match self.client_for(&url).stats().await {
                Ok(remote) => {
                    totals = add_stats(totals, &remote);
                    totals.knots += 1;
                    for p in &remote.topology.peers {
                        let to = trim_base(p);
                        edges.push(TopoEdge {
                            from: url.clone(),
                            to: to.clone(),
                            kind: EdgeKind::Peer,
                        });
                        if !knots.iter().any(|n| n.url == to) && to != home {
                            knots.push(KnotNode {
                                url: to,
                                did: String::new(),
                                reachable: false,
                                stats: None,
                            });
                            totals.knots += 1;
                        }
                    }
                    for hosted in &remote.topology.hosted {
                        edges.push(TopoEdge {
                            from: url.clone(),
                            to: hosted.did.clone(),
                            kind: EdgeKind::Hosted,
                        });
                    }
                    knots.push(KnotNode {
                        url,
                        did: remote.did.clone(),
                        reachable: true,
                        stats: Some(remote),
                    });
                }
                Err(_) => {
                    totals.knots += 1;
                    knots.push(KnotNode {
                        url,
                        did: String::new(),
                        reachable: false,
                        stats: None,
                    });
                }
            }
        }

        Ok(NetworkView {
            home,
            knots,
            edges,
            totals,
        })
    }

    pub async fn lookup(&self, q: &str) -> Result<LookupHit> {
        let q = q.trim();
        if q.is_empty() {
            return Err(WeftError::NotFound);
        }
        if q.starts_with("http://") || q.starts_with("https://") {
            let url = trim_base(q);
            let stats = self
                .client_for(&url)
                .stats()
                .await
                .map_err(|_| WeftError::NotFound)?;
            return Ok(LookupHit::Knot { url, stats });
        }
        if q.starts_with("did:") {
            return self.lookup_did(q).await;
        }
        let resolved = self
            .client
            .identity_resolve(Some(q), None)
            .await
            .map_err(|_| WeftError::NotFound)?;
        self.lookup_did(&resolved.did).await
    }

    async fn lookup_did(&self, did: &str) -> Result<LookupHit> {
        let stats = self.client.stats().await.ok();
        let hosted = stats
            .as_ref()
            .and_then(|s| s.topology.hosted.iter().find(|h| h.did == did).cloned());
        let head = self.client.get_head(did).await.ok();
        let resolved = self.client.identity_resolve(None, Some(did)).await.ok();
        if hosted.is_none() && head.is_none() && resolved.is_none() {
            return Err(WeftError::NotFound);
        }
        Ok(LookupHit::Identity {
            did: did.to_string(),
            handle: hosted.as_ref().and_then(|h| h.handle.clone()),
            knot: head
                .as_ref()
                .map(|h| h.knot.clone())
                .or_else(|| resolved.and_then(|r| r.endpoints.into_iter().next())),
            rev: hosted
                .as_ref()
                .and_then(|h| h.rev.clone())
                .or_else(|| head.map(|h| h.rev)),
            records: hosted.map(|h| h.records),
        })
    }

    fn client_for(&self, url: &str) -> KnotClient {
        let key = trim_base(url);
        if let Some(engine) = self.remotes.get(&key) {
            return KnotClient::mock_shared(engine.clone());
        }
        KnotClient::http(key)
    }
}

fn trim_base(url: &str) -> String {
    let s = url.trim();
    if s.ends_with("://") {
        return s.to_string();
    }
    s.trim_end_matches('/').to_string()
}

fn add_stats(mut t: NetworkTotals, s: &NodeStats) -> NetworkTotals {
    t.identities += s.counts.identities;
    t.records += s.counts.records;
    t.blobs += s.counts.blobs;
    t.circles += s.counts.circles;
    t.foreign += s.counts.foreign;
    t.commits += s.counts.commits;
    t
}
