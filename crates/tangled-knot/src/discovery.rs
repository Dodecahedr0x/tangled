//! Outbound discovery only. Operator seeds may use private destinations;
//! advertisements never acquire that privilege. Redirects are always disabled.
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tangled::{http::NodeDescribe, peers::peer_origin, Error, Result};

pub const MAX_DESCRIBE_BYTES: usize = 65_536;
fn rejected(message: &str) -> Error {
    Error::Transport(format!("peer discovery: {message}"))
}

/// Conservative public-unicast filter, including mapped/transition addresses.
pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || a == 0
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 192 && b == 0 && (c == 0 || c == 2))
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            s[0] & 0xe000 == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x200 || s[1] == 0xdb8))
                && s[0] != 0x2002
                && !(s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}

/// Explicit development exception. Advertising a loopback URL never sets this.
#[derive(Clone, Copy, Debug, Default)]
pub struct DiscoveryOptions {
    pub allow_learned_loopback: bool,
}

pub fn addresses_allowed(addresses: &[SocketAddr]) -> bool {
    addresses_allowed_with_options(addresses, DiscoveryOptions::default())
}

/// Validate the complete pinned DNS answer set, including the development case.
pub fn addresses_allowed_with_options(addresses: &[SocketAddr], options: DiscoveryOptions) -> bool {
    !addresses.is_empty()
        && addresses.len() <= 32
        && (addresses.iter().all(|a| public_ip(a.ip()))
            || (options.allow_learned_loopback && addresses.iter().all(|a| a.ip().is_loopback())))
}

/// Strict by default, regardless of the local knot's advertised origin.
pub async fn describe_peer(base: &str, configured: bool) -> Result<NodeDescribe> {
    describe_peer_with_options(base, configured, DiscoveryOptions::default()).await
}

pub async fn describe_peer_with_options(
    base: &str,
    configured: bool,
    options: DiscoveryOptions,
) -> Result<NodeDescribe> {
    let origin = peer_origin(base).ok_or_else(|| rejected("invalid origin"))?;
    let url = reqwest::Url::parse(&origin).map_err(|_| rejected("invalid URL"))?;
    let host = url
        .host_str()
        .ok_or_else(|| rejected("missing host"))?
        .trim_matches(['[', ']']);
    let port = url
        .port_or_known_default()
        .ok_or_else(|| rejected("missing port"))?;
    let addresses: Vec<_> = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        tokio::time::timeout(
            Duration::from_secs(1),
            tokio::net::lookup_host((host, port)),
        )
        .await
        .map_err(|_| rejected("DNS timeout"))?
        .map_err(|_| rejected("DNS resolution failed"))?
        .take(33)
        .collect()
    };
    if addresses.is_empty()
        || addresses.len() > 32
        || (!configured && !addresses_allowed_with_options(&addresses, options))
    {
        return Err(rejected("forbidden destination"));
    }
    // DNS is resolved once, checked in full, then pinned. No environment proxy
    // or second DNS lookup can bypass the destination check.
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(1))
        .timeout(Duration::from_secs(3))
        .resolve_to_addrs(host, &addresses)
        .build()
        .map_err(|e| rejected(&e.to_string()))?;
    let mut response = client
        .get(format!("{origin}/xrpc/tangled.node.describe"))
        .send()
        .await
        .map_err(|e| rejected(&e.to_string()))?;
    if !response.status().is_success() {
        return Err(rejected("non-success status or redirect"));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_DESCRIBE_BYTES as u64)
    {
        return Err(rejected("describe too large"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| rejected(&e.to_string()))?
    {
        if chunk.len() > MAX_DESCRIBE_BYTES - bytes.len() {
            return Err(rejected("describe too large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let mut d: NodeDescribe =
        serde_json::from_slice(&bytes).map_err(|_| rejected("invalid describe"))?;
    if d.v != 1 {
        return Err(rejected("unsupported version"));
    }
    d.peers.truncate(tangled::peers::MAX_ADVERTISEMENTS);
    Ok(d)
}
