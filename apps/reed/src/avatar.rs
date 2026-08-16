//! Circular avatar with a deterministic default PFP. Never empty.

use dioxus::prelude::*;
use reed_core::Replica;

use crate::paths::replica_dir;

const PFP_PALETTE: [&str; 8] = [
    "#3d4a5c", "#4a3d38", "#3a4540", "#4a3d4e", "#3d454a", "#4a4238", "#38424a", "#453d3a",
];

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum AvatarSize {
    #[default]
    Feed,
    Small,
    Parent,
    Large,
}

impl AvatarSize {
    fn class(self) -> &'static str {
        match self {
            Self::Feed => "avatar",
            Self::Small => "avatar avatar-sm",
            Self::Parent => "avatar avatar-md",
            Self::Large => "avatar avatar-lg",
        }
    }
}

#[component]
pub fn Avatar(
    did: String,
    #[props(default)] name: Option<String>,
    #[props(default)] avatar_cid: Option<String>,
    #[props(default)] size: AvatarSize,
) -> Element {
    let letters = pfp_initials(&did, name.as_deref());
    let color = pfp_color(&did);
    let src = avatar_cid.as_deref().and_then(|c| blob_data_url(c, None));
    let mut broken = use_signal(|| false);
    let class = size.class();
    rsx! {
        span {
            class,
            style: "background:{color}",
            aria_hidden: "true",
            span { class: "avatar-initials", {letters} }
            if let Some(url) = src {
                if !broken() {
                    img {
                        class: "avatar-img",
                        src: url,
                        alt: "",
                        onerror: move |_| broken.set(true),
                    }
                }
            }
        }
    }
}

pub fn display_name(did: &str, name: Option<&str>, handle: Option<&str>) -> String {
    if let Some(n) = name.map(str::trim).filter(|s| !s.is_empty()) {
        return n.to_string();
    }
    if let Some(h) = handle.map(str::trim).filter(|s| !s.is_empty()) {
        return h.to_string();
    }
    crop_did(did)
}

pub fn handle_or_did(did: &str, handle: Option<&str>) -> String {
    if let Some(h) = handle.map(str::trim).filter(|s| !s.is_empty()) {
        if h.starts_with('@') {
            h.to_string()
        } else {
            format!("@{h}")
        }
    } else {
        crop_did(did)
    }
}

pub fn crop_did(did: &str) -> String {
    let tail = did.rsplit(':').next().unwrap_or(did);
    if tail.is_empty() {
        return did.to_string();
    }
    if tail.len() <= 16 {
        tail.to_string()
    } else {
        format!("{}…{}", &tail[..4], &tail[tail.len() - 4..])
    }
}

pub fn pfp_initials(did: &str, name: Option<&str>) -> String {
    if let Some(n) = name.map(str::trim).filter(|s| !s.is_empty()) {
        let parts: Vec<&str> = n.split_whitespace().collect();
        if parts.len() >= 2 {
            if let (Some(a), Some(b)) = (parts[0].chars().next(), parts[1].chars().next()) {
                return format!("{}{}", a.to_uppercase(), b.to_uppercase());
            }
        }
        let mut it = n.chars().filter(|c| c.is_alphanumeric());
        match (it.next(), it.next()) {
            (Some(a), Some(b)) => return format!("{}{}", a.to_uppercase(), b.to_uppercase()),
            (Some(a), None) => return a.to_uppercase().to_string(),
            _ => {}
        }
    }
    let tail = did.rsplit(':').next().unwrap_or(did);
    let mut it = tail.chars().filter(|c| c.is_alphanumeric());
    match (it.next(), it.next()) {
        (Some(a), Some(b)) => format!("{}{}", a.to_uppercase(), b.to_uppercase()),
        (Some(a), None) => a.to_uppercase().to_string(),
        _ => "?".into(),
    }
}

pub fn pfp_color(did: &str) -> &'static str {
    PFP_PALETTE[(fnv1a(did) as usize) % PFP_PALETTE.len()]
}

pub fn blob_data_url(cid: &str, mime: Option<&str>) -> Option<String> {
    let bytes = Replica::open(&replica_dir()).ok()?.blob(cid)?;
    if bytes.is_empty() {
        return None;
    }
    let sniffed = sniff_mime(&bytes);
    let mime = mime
        .filter(|m| m.starts_with("image/") || m.starts_with("video/"))
        .unwrap_or(sniffed);
    Some(data_url(&bytes, mime))
}

fn data_url(bytes: &[u8], mime: &str) -> String {
    format!("data:{mime};base64,{}", b64(bytes))
}

fn sniff_mime(bytes: &[u8]) -> &'static str {
    if bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff {
        return "image/jpeg";
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return "image/png";
    }
    if bytes.starts_with(b"GIF8") {
        return "image/gif";
    }
    if bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        return "image/webp";
    }
    if bytes.len() > 8 && &bytes[4..8] == b"ftyp" {
        return "video/mp4";
    }
    if bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        return "video/webm";
    }
    "application/octet-stream"
}

fn fnv1a(s: &str) -> u32 {
    let mut h = 2166136261u32;
    for b in s.as_bytes() {
        h ^= u32::from(*b);
        h = h.wrapping_mul(16777619);
    }
    h
}

fn b64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let a = u32::from(chunk[0]);
        let b = u32::from(chunk.get(1).copied().unwrap_or(0));
        let c = u32::from(chunk.get(2).copied().unwrap_or(0));
        let n = (a << 16) | (b << 8) | c;
        out.push(T[(n >> 18) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(T[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(T[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_from_did_and_name() {
        assert_eq!(pfp_initials("did:sol:alice", None), "AL");
        assert_eq!(pfp_initials("did:sol:alice", Some("Ada Lovelace")), "AL");
        assert_eq!(pfp_initials("did:sol:alice", Some("Ada")), "AD");
        assert_eq!(pfp_initials("did:sol:", None), "?");
    }

    #[test]
    fn color_is_stable() {
        assert_eq!(pfp_color("did:sol:alice"), pfp_color("did:sol:alice"));
        assert_ne!(pfp_color("did:sol:alice"), pfp_color("did:sol:bob"));
    }

    #[test]
    fn names_and_crop() {
        assert_eq!(display_name("did:sol:alice", Some("Ada"), None), "Ada");
        assert_eq!(handle_or_did("did:sol:alice", Some("ada")), "@ada");
        assert_eq!(crop_did("did:sol:alice"), "alice");
        assert_eq!(b64(b"hi"), "aGk=");
        assert_eq!(sniff_mime(b"\x89PNG"), "image/png");
    }
}
