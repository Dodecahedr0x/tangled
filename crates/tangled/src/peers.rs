//! Bounded local peer discovery state. Group quotas limit one advertisement
//! source or IP network; they do not establish independent ownership.
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

pub const MAX_PEERS: usize = 64;
pub const MAX_ATTEMPTS: usize = 8;
pub const MAX_ADVERTISEMENTS: usize = 128;
const SOURCE_QUOTA: usize = 8;
const NETWORK_QUOTA: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Peer {
    url: String,
    source: String,
    good: bool,
    failures: u8,
    #[serde(default)]
    contacted_only: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PeerBook {
    peers: Vec<Peer>,
    #[serde(default)]
    cursor: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerAttempt {
    pub url: String,
    /// Permission belongs to an exact operator-supplied origin, never its ads.
    pub configured: bool,
}

/// Origins only: paths, queries, credentials and fragments are not peer IDs.
pub fn peer_origin(raw: &str) -> Option<String> {
    if raw.len() > 2048 {
        return None;
    }
    let u = reqwest::Url::parse(raw.trim()).ok()?;
    if !matches!(u.scheme(), "http" | "https")
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.path() != "/"
        || u.query().is_some()
        || u.fragment().is_some()
    {
        return None;
    }
    Some(u.as_str().trim_end_matches('/').to_string())
}

pub fn network_group(raw: &str) -> Option<String> {
    let u = reqwest::Url::parse(raw).ok()?;
    let h = u.host_str()?.trim_start_matches('[').trim_end_matches(']');
    Some(match h.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            let a = ip.octets();
            format!("v4:{}.{}.{}", a[0], a[1], a[2])
        }
        Ok(IpAddr::V6(ip)) => {
            let a = ip.segments();
            format!("v6:{:x}:{:x}:{:x}", a[0], a[1], a[2])
        }
        Err(_) => format!(
            "dns:{}",
            h.trim_end_matches('.')
                .rsplit('.')
                .take(2)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(".")
        ),
    })
}

impl PeerBook {
    pub fn urls(&self) -> Vec<String> {
        self.peers.iter().map(|p| p.url.clone()).collect()
    }

    /// Preserve the existing advertised-learned accessor; successful seed
    /// contacts also live in the book but were not advertised to us.
    pub fn advertised_urls(&self) -> Vec<String> {
        self.peers
            .iter()
            .filter(|p| !p.contacted_only)
            .map(|p| p.url.clone())
            .collect()
    }

    /// At most 128 candidates are inspected per response, even for duplicate ads.
    pub fn learn(&mut self, source: &str, urls: &[String]) {
        let Some(source) = network_group(source) else {
            return;
        };
        for raw in urls.iter().take(MAX_ADVERTISEMENTS) {
            if let Some(url) = peer_origin(raw) {
                self.admit(url, source.clone(), false);
            }
        }
    }

    fn admit(&mut self, url: String, source: String, good: bool) {
        if let Some(p) = self.peers.iter_mut().find(|p| p.url == url) {
            if !good {
                p.contacted_only = false;
            }
            return;
        }
        let group = network_group(&url);
        // Evict only unverified entries, within whichever quota is exhausted.
        for constraint in 0..3 {
            let matches = |p: &&Peer| match constraint {
                0 => p.source == source,
                1 => network_group(&p.url) == group,
                _ => true,
            };
            let quota = match constraint {
                0 => SOURCE_QUOTA,
                1 => NETWORK_QUOTA,
                _ => MAX_PEERS,
            };
            if self.peers.iter().filter(matches).count() >= quota {
                let Some(i) = self.peers.iter().position(|p| !p.good && matches(&p)) else {
                    return;
                };
                self.peers.remove(i);
            }
        }
        self.peers.push(Peer {
            url,
            source,
            good,
            failures: 0,
            contacted_only: good,
        });
    }

    /// Success means a bounded valid describe reply, not trust in its claims.
    pub fn succeeded(&mut self, url: &str, source: &str) {
        let Some(url) = peer_origin(url) else {
            return;
        };
        if let Some(p) = self.peers.iter_mut().find(|p| p.url == url) {
            p.good = true;
            p.failures = 0;
        } else if let Some(source) = network_group(source) {
            self.admit(url, source, true);
        }
    }

    pub fn failed(&mut self, url: &str) {
        if let Some(p) = self.peers.iter_mut().find(|p| p.url == url) {
            p.failures = p.failures.saturating_add(1);
        }
        self.peers.retain(|p| p.failures < 3);
    }

    /// 4 seed slots, 2 known-good slots, 2 exploration slots. Cursors rotate
    /// independently of advertisement order and survive restart.
    pub fn select(&mut self, seeds: &[String]) -> Vec<PeerAttempt> {
        let mut out = Vec::new();
        let seeds: Vec<_> = seeds
            .iter()
            .filter_map(|s| peer_origin(s))
            .take(MAX_PEERS)
            .collect();
        let good: Vec<_> = self
            .peers
            .iter()
            .filter(|p| p.good)
            .map(|p| p.url.clone())
            .collect();
        let fresh: Vec<_> = self
            .peers
            .iter()
            .filter(|p| !p.good)
            .map(|p| p.url.clone())
            .collect();
        for (urls, slots, configured) in [(&seeds, 4, true), (&good, 2, false), (&fresh, 2, false)]
        {
            let mut added = 0;
            for n in 0..urls.len() {
                if added >= slots {
                    break;
                }
                let url = &urls[(self.cursor as usize % urls.len() + n) % urls.len()];
                if !out.iter().any(|p: &PeerAttempt| p.url == *url) {
                    out.push(PeerAttempt {
                        url: url.clone(),
                        configured: configured || seeds.contains(url),
                    });
                    added += 1;
                }
            }
        }
        self.cursor = self.cursor.wrapping_add(1);
        out
    }

    /// Reapply admission invariants to deserialized disk data.
    pub fn sanitize(self) -> Self {
        let mut clean = Self {
            peers: Vec::new(),
            cursor: self.cursor,
        };
        for p in self.peers.into_iter().take(MAX_PEERS) {
            if let Some(url) = peer_origin(&p.url) {
                clean.admit(url.clone(), p.source, p.good);
                if let Some(saved) = clean.peers.iter_mut().find(|saved| saved.url == url) {
                    saved.contacted_only = p.contacted_only;
                    saved.failures = p.failures;
                }
            }
        }
        clean
    }
}
