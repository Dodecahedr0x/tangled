//! Outbound URL allowlist for actor fetch and inbox delivery.

use std::net::{IpAddr, ToSocketAddrs};

use reqwest::Url;

/// Local advertised knot (`http://127.0.0.1` / `localhost`) may fetch loopback actors.
pub fn advertised_allows_loopback(url: &str) -> bool {
    Url::parse(url).is_ok_and(|u| is_loopback_url(&u))
}

pub fn delivery_url_allowed(url: &str, allow_loopback: bool) -> bool {
    let Ok(u) = Url::parse(url) else {
        return false;
    };
    match u.scheme() {
        "https" => {}
        "http" => {
            if !is_loopback_url(&u) || !allow_loopback {
                return false;
            }
        }
        _ => return false,
    }
    !host_is_blocked(&u, allow_loopback)
}

pub fn inbox_host_matches_actor(inbox: &str, actor_id: &str) -> bool {
    let (Ok(i), Ok(a)) = (Url::parse(inbox), Url::parse(actor_id)) else {
        return false;
    };
    match (i.host_str(), a.host_str()) {
        (Some(ih), Some(ah)) => ih.eq_ignore_ascii_case(ah),
        _ => false,
    }
}

fn host_literal(u: &Url) -> Option<&str> {
    let h = u.host_str()?;
    Some(
        h.strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or(h),
    )
}

fn is_loopback_url(u: &Url) -> bool {
    let Some(h) = host_literal(u) else {
        return false;
    };
    if h.eq_ignore_ascii_case("localhost") || h.ends_with(".localhost") {
        return true;
    }
    h.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

fn host_is_blocked(u: &Url, allow_loopback: bool) -> bool {
    let Some(h) = host_literal(u) else {
        return true;
    };
    if let Ok(ip) = h.parse::<IpAddr>() {
        return !ip_allowed(ip, allow_loopback);
    }
    if h.eq_ignore_ascii_case("localhost") || h.ends_with(".localhost") {
        return !allow_loopback;
    }
    let port = u.port_or_known_default().unwrap_or(443);
    if let Ok(addrs) = (h, port).to_socket_addrs() {
        for addr in addrs {
            if !ip_allowed(addr.ip(), allow_loopback) {
                return true;
            }
        }
    }
    false
}

fn ip_allowed(ip: IpAddr, allow_loopback: bool) -> bool {
    if ip.is_loopback() {
        return allow_loopback;
    }
    !is_blocked_ip(ip)
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            v.is_private()
                || v.is_link_local()
                || v.is_unspecified()
                || v.is_broadcast()
                || v.octets()[0] == 0
        }
        IpAddr::V6(v) => {
            v.is_unique_local()
                || v.is_unicast_link_local()
                || v.is_unspecified()
                || v.is_multicast()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_public_host_allowed() {
        assert!(delivery_url_allowed(
            "https://mastodon.social/users/a/inbox",
            false
        ));
    }

    #[test]
    fn http_public_rejected() {
        assert!(!delivery_url_allowed("http://ex.example/inbox", false));
        assert!(!delivery_url_allowed("http://ex.example/inbox", true));
    }

    #[test]
    fn metadata_and_private_ips_rejected() {
        assert!(!delivery_url_allowed("http://169.254.169.254/latest", true));
        assert!(!delivery_url_allowed("https://169.254.169.254/", false));
        assert!(!delivery_url_allowed("http://10.0.0.1/inbox", true));
        assert!(!delivery_url_allowed("https://192.168.1.1/inbox", true));
        assert!(!delivery_url_allowed("https://[fd00::1]/inbox", false));
    }

    #[test]
    fn loopback_http_only_with_flag() {
        assert!(delivery_url_allowed("http://127.0.0.1:1234/inbox", true));
        assert!(delivery_url_allowed("http://localhost/inbox", true));
        assert!(!delivery_url_allowed("http://127.0.0.1:1234/inbox", false));
        assert!(!delivery_url_allowed("http://localhost/inbox", false));
    }

    #[test]
    fn advertised_loopback_http_enables_demo_peer_fetch() {
        assert!(advertised_allows_loopback("http://127.0.0.1:3000"));
        assert!(advertised_allows_loopback("http://localhost:3000"));
        assert!(!advertised_allows_loopback("https://knot.example"));
        assert!(!advertised_allows_loopback("http://0.0.0.0:3000"));
    }

    #[test]
    fn inbox_host_pinned_to_actor() {
        assert!(inbox_host_matches_actor(
            "https://ex.example/inbox",
            "https://ex.example/users/a"
        ));
        assert!(!inbox_host_matches_actor(
            "https://evil.com/inbox",
            "https://ex.example/users/a"
        ));
        assert!(!inbox_host_matches_actor(
            "http://169.254.169.254/",
            "https://ex.example/users/a"
        ));
    }

    #[test]
    fn ftp_and_garbage_rejected() {
        assert!(!delivery_url_allowed("ftp://ex.example/inbox", false));
        assert!(!delivery_url_allowed("not a url", false));
    }
}
