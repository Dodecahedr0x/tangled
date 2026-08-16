//! UCAN-style DAG-CBOR capabilities: monotonic attenuation, circle/read, invocation.

use std::collections::HashSet;

use ed25519_dalek::SigningKey;

use crate::cid::Cid;
use crate::constants::{CAPABILITY_CLOCK_SKEW_SECONDS, CAPABILITY_MAX_TTL_SECONDS};
use crate::dagcbor;
use crate::error::{Error, Result};
use crate::sign;

/// Known commands (unknown MUST be rejected).
pub const COMMANDS: &[&str] = &[
    "repo/read",
    "repo/write",
    "blob/get",
    "blob/put",
    "circle/invite",
    "circle/remove",
    "circle/read",
    "knot/replicate",
    "cap/issue",
    "*",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capability {
    pub v: u64,
    pub iss: String,
    pub aud: String,
    pub cmd: String,
    pub res: String,
    pub nbf: u64,
    pub exp: u64,
    pub pol: Vec<serde_json::Value>,
    pub prf: Vec<String>,
    pub meta: Option<serde_json::Value>,
    pub sig: Option<[u8; 64]>,
}

impl Capability {
    pub fn from_json(v: &serde_json::Value) -> Result<Self> {
        let o = v
            .as_object()
            .ok_or_else(|| Error::capability("capability must be a map"))?;
        let get_str = |k: &str| -> Result<String> {
            o.get(k)
                .and_then(|x| x.as_str())
                .map(str::to_string)
                .ok_or_else(|| Error::capability(format!("missing {k}")))
        };
        let get_u = |k: &str| -> Result<u64> {
            o.get(k)
                .and_then(|x| x.as_u64())
                .ok_or_else(|| Error::capability(format!("missing {k}")))
        };
        let pol = o
            .get("pol")
            .and_then(|x| x.as_array())
            .cloned()
            .unwrap_or_default();
        let prf = o
            .get("prf")
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let sig = o.get("sig").and_then(|x| match x {
            serde_json::Value::String(s) => crate::sign::sig_from_hex(s).ok(),
            _ => None,
        });
        Ok(Self {
            v: get_u("v")?,
            iss: get_str("iss")?,
            aud: get_str("aud")?,
            cmd: get_str("cmd")?,
            res: get_str("res")?,
            nbf: get_u("nbf")?,
            exp: get_u("exp")?,
            pol,
            prf,
            meta: o.get("meta").cloned(),
            sig,
        })
    }

    pub fn to_unsigned_json(&self) -> serde_json::Value {
        let mut m = serde_json::json!({
            "v": self.v,
            "iss": self.iss,
            "aud": self.aud,
            "cmd": self.cmd,
            "res": self.res,
            "nbf": self.nbf,
            "exp": self.exp,
            "pol": self.pol,
            "prf": self.prf,
        });
        if let Some(meta) = &self.meta {
            if let Some(obj) = m.as_object_mut() {
                obj.insert("meta".into(), meta.clone());
            }
        }
        m
    }

    pub fn sign(&mut self, sk: &SigningKey) -> Result<[u8; 64]> {
        validate_capability(self)?;
        let json = self.to_unsigned_json();
        let cbor = dagcbor::encode_json(&json)?;
        let sig = sign::sign_unsigned(sk, &cbor);
        self.sig = Some(sig);
        Ok(sig)
    }
}

pub fn validate_capability(cap: &Capability) -> Result<()> {
    if cap.v != 1 {
        return Err(Error::capability("v must be 1"));
    }
    if !COMMANDS.contains(&cap.cmd.as_str()) {
        return Err(Error::capability(format!("unknown command {}", cap.cmd)));
    }
    if cap.exp <= cap.nbf {
        return Err(Error::capability("exp MUST be greater than nbf"));
    }
    if cap.exp - cap.nbf > CAPABILITY_MAX_TTL_SECONDS {
        return Err(Error::capability("TTL exceeds CAPABILITY_MAX_TTL_SECONDS"));
    }
    parse_resource(&cap.res)?;
    for caveat in &cap.pol {
        let arr = caveat
            .as_array()
            .ok_or_else(|| Error::capability("caveat must be an array"))?;
        let name = arr
            .first()
            .and_then(|x| x.as_str())
            .ok_or_else(|| Error::capability("caveat name"))?;
        match name {
            "eq" | "lt" | "valid_until" | "max_uses" => {}
            other => {
                return Err(Error::capability(format!(
                    "unknown caveat {other} (fail closed)"
                )))
            }
        }
    }
    Ok(())
}

/// Resource URI kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resource {
    /// `tangled://<did>` plus optional `/collection` and `/rkey` or `/blob/<cid>`.
    Did { did: String, rest: Vec<String> },
    /// `tangled://circle/<owner-did>/<circle-rkey>`.
    Circle { owner: String, rkey: String },
}

pub fn parse_resource(res: &str) -> Result<Resource> {
    let rest = res
        .strip_prefix("tangled://")
        .ok_or_else(|| Error::capability(format!("resource must be tangled:// ({res})")))?;
    if let Some(after) = rest.strip_prefix("circle/") {
        // owner DID contains colons; rkey is the last path segment.
        let (owner, rkey) = after
            .rsplit_once('/')
            .ok_or_else(|| Error::capability("circle resource needs owner and rkey"))?;
        if !owner.starts_with("did:") {
            return Err(Error::capability("circle owner must be a DID"));
        }
        return Ok(Resource::Circle {
            owner: owner.to_string(),
            rkey: rkey.to_string(),
        });
    }
    if !rest.starts_with("did:") {
        return Err(Error::capability(
            "expected tangled://did:… or tangled://circle/…",
        ));
    }
    // `did:sol:KEY` (and cluster forms) contain no `/`.
    let (did, tail) = rest
        .split_once('/')
        .map(|(d, t)| (d, Some(t)))
        .unwrap_or((rest, None));
    let rest_parts = tail
        .map(|t| {
            t.split('/')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Ok(Resource::Did {
        did: did.to_string(),
        rest: rest_parts,
    })
}

/// Child `res` MUST equal parent or be a path-prefix extension (`/` after parent).
pub fn resource_attenuates(parent: &str, child: &str) -> bool {
    if parent == child {
        return true;
    }
    child.starts_with(parent) && child.as_bytes().get(parent.len()) == Some(&b'/')
}

/// Command attenuation: parent `*` allows any known child; otherwise cmds must match.
pub fn command_attenuates(parent_cmd: &str, child_cmd: &str) -> bool {
    if !COMMANDS.contains(&child_cmd) {
        return false;
    }
    parent_cmd == "*" || parent_cmd == child_cmd
}

/// Time attenuation: child window inside parent.
pub fn time_attenuates(parent: &Capability, child: &Capability) -> bool {
    child.nbf >= parent.nbf && child.exp <= parent.exp
}

/// Full monotonic check parent → child.
pub fn attenuates(parent: &Capability, child: &Capability) -> Result<()> {
    validate_capability(parent)?;
    validate_capability(child)?;
    if !command_attenuates(&parent.cmd, &child.cmd) {
        return Err(Error::capability("command amplification"));
    }
    if !resource_attenuates(&parent.res, &child.res) {
        return Err(Error::capability("resource amplification"));
    }
    if !time_attenuates(parent, child) {
        return Err(Error::capability("time amplification"));
    }
    Ok(())
}

/// `nbf - SKEW <= now <= exp + SKEW` (spec 21.8).
pub fn in_time_window(nbf: u64, exp: u64, now: u64) -> bool {
    let skew = CAPABILITY_CLOCK_SKEW_SECONDS;
    now.saturating_add(skew) >= nbf && now <= exp.saturating_add(skew)
}

/// Does this capability authorize `cmd` on `res`?
pub fn authorizes(cap: &Capability, cmd: &str, res: &str, now: u64) -> Result<()> {
    validate_capability(cap)?;
    if !in_time_window(cap.nbf, cap.exp, now) {
        return Err(Error::capability("capability not in time window"));
    }
    if !command_attenuates(&cap.cmd, cmd) {
        return Err(Error::capability(format!(
            "{} does not authorize {cmd}",
            cap.cmd
        )));
    }
    if !resource_attenuates(&cap.res, res) && cap.res != res {
        // For exact collection reads, res may be a record under the collection.
        if !resource_attenuates(&cap.res, res) {
            return Err(Error::capability("resource not covered"));
        }
    }
    evaluate_caveats(&cap.pol, &serde_json::json!({}), now, None)
}

fn json_pointer<'a>(v: &'a serde_json::Value, ptr: &str) -> Option<&'a serde_json::Value> {
    if ptr.is_empty() || ptr == "/" {
        return Some(v);
    }
    let mut cur = v;
    for part in ptr.trim_start_matches('/').split('/') {
        cur = cur.get(part)?;
    }
    Some(cur)
}

/// Spec 09 caveats: `eq`, `lt`, `valid_until`, `max_uses` (offline MAY skip uses).
pub fn evaluate_caveats(
    pol: &[serde_json::Value],
    args: &serde_json::Value,
    now: u64,
    uses: Option<u64>,
) -> Result<()> {
    for caveat in pol {
        let arr = caveat
            .as_array()
            .ok_or_else(|| Error::capability("caveat must be an array"))?;
        let name = arr
            .first()
            .and_then(|x| x.as_str())
            .ok_or_else(|| Error::capability("caveat name"))?;
        match name {
            "eq" => {
                let ptr = arr
                    .get(1)
                    .and_then(|x| x.as_str())
                    .ok_or_else(|| Error::capability("eq pointer"))?;
                let want = arr.get(2).ok_or_else(|| Error::capability("eq value"))?;
                let Some(got) = json_pointer(args, ptr) else {
                    if args.as_object().is_some_and(|o| o.is_empty()) {
                        continue;
                    }
                    return Err(Error::capability("eq missing arg"));
                };
                if got != want {
                    return Err(Error::capability("eq caveat failed"));
                }
            }
            "lt" => {
                let ptr = arr
                    .get(1)
                    .and_then(|x| x.as_str())
                    .ok_or_else(|| Error::capability("lt pointer"))?;
                let want = arr
                    .get(2)
                    .and_then(|x| x.as_f64())
                    .ok_or_else(|| Error::capability("lt number"))?;
                let Some(got) = json_pointer(args, ptr).and_then(|v| v.as_f64()) else {
                    if args.as_object().is_some_and(|o| o.is_empty()) {
                        continue;
                    }
                    return Err(Error::capability("lt missing arg"));
                };
                if got >= want {
                    return Err(Error::capability("lt caveat failed"));
                }
            }
            "valid_until" => {
                let until = arr
                    .get(1)
                    .and_then(|x| x.as_u64())
                    .ok_or_else(|| Error::capability("valid_until seconds"))?;
                if now > until.saturating_add(CAPABILITY_CLOCK_SKEW_SECONDS) {
                    return Err(Error::capability("valid_until exceeded"));
                }
            }
            "max_uses" => {
                let n = arr
                    .get(1)
                    .and_then(|x| x.as_u64())
                    .ok_or_else(|| Error::capability("max_uses n"))?;
                if let Some(u) = uses {
                    if u >= n {
                        return Err(Error::capability("max_uses exceeded"));
                    }
                }
            }
            other => {
                return Err(Error::capability(format!(
                    "unknown caveat {other} (fail closed)"
                )))
            }
        }
    }
    Ok(())
}

/// circle/read: root iss must be the circle owner; res must be a circle URI.
pub fn verify_circle_read(cap: &Capability, owner: &str, now: u64) -> Result<()> {
    validate_capability(cap)?;
    if cap.cmd != "circle/read" {
        return Err(Error::capability("cmd is not circle/read"));
    }
    match parse_resource(&cap.res)? {
        Resource::Circle { owner: o, .. } => {
            if o != owner {
                return Err(Error::capability("circle resource owner mismatch"));
            }
        }
        Resource::Did { .. } => {
            return Err(Error::capability(
                "circle/read res MUST be tangled://circle/<owner>/<rkey>",
            ));
        }
    }
    if cap.iss != owner {
        return Err(Error::capability(
            "root iss is not the circle owner and is not a cap/issue delegate",
        ));
    }
    if !in_time_window(cap.nbf, cap.exp, now) {
        return Err(Error::capability("capability not in time window"));
    }
    Ok(())
}

/// What `getRepoAuth` may include for this capability (no sibling leak).
pub fn authorized_collections(cap: &Capability) -> Result<AuthorizedScope> {
    validate_capability(cap)?;
    match cap.cmd.as_str() {
        "circle/read" => match parse_resource(&cap.res)? {
            Resource::Circle { owner, rkey } => Ok(AuthorizedScope::Circle { owner, rkey }),
            _ => Err(Error::capability("circle/read needs circle URI")),
        },
        "repo/read" | "*" => match parse_resource(&cap.res)? {
            Resource::Did { did, rest } => Ok(AuthorizedScope::Repo { did, rest }),
            Resource::Circle { .. } => Err(Error::capability("repo/read is not a circle URI")),
        },
        other => Err(Error::capability(format!("{other} does not fetch repos"))),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthorizedScope {
    Repo { did: String, rest: Vec<String> },
    Circle { owner: String, rkey: String },
}

impl AuthorizedScope {
    /// Sibling collections MUST NOT leak from a collection-scoped repo/read.
    pub fn allows_collection(&self, collection: &str) -> bool {
        match self {
            Self::Repo { rest, .. } => {
                if rest.is_empty() {
                    return true;
                }
                rest.first().is_some_and(|c| c == collection)
            }
            Self::Circle { .. } => {
                collection == "sys.circle"
                    || collection == "sys.circle.wrap"
                    || collection == "sys.circle.mls"
            }
        }
    }

    pub fn allows_circle_read(&self) -> bool {
        matches!(self, Self::Circle { .. })
    }
}

/// Envelope `circle` value: `did:sol:<owner>#<rkey>`.
pub fn circle_envelope(owner: &str, rkey: &str) -> String {
    format!("{owner}#{rkey}")
}

pub fn circle_resource_uri(owner: &str, rkey: &str) -> String {
    format!("tangled://circle/{owner}/{rkey}")
}

/// Verify capability signature over unsigned JSON.
pub fn verify_cap_sig(pk: &[u8; 32], unsigned: &serde_json::Value, sig: &[u8; 64]) -> Result<()> {
    let cbor = dagcbor::encode_json(unsigned)?;
    sign::verify_unsigned_raw(pk, &cbor, sig)
}

impl Capability {
    /// DAG-CBOR map including `sig` as 64 bytes when present.
    pub fn to_cbor_value(&self) -> Result<dagcbor::Value> {
        let mut m = match dagcbor::from_json(&self.to_unsigned_json())? {
            dagcbor::Value::Map(m) => m,
            _ => return Err(Error::capability("capability is not a map")),
        };
        if let Some(sig) = self.sig {
            m.insert("sig".into(), dagcbor::Value::Bytes(sig.to_vec()));
        }
        Ok(dagcbor::Value::Map(m))
    }

    /// CID of the complete signed capability (DAG-CBOR including `sig`).
    pub fn cid(&self) -> Result<Cid> {
        let bytes = dagcbor::encode(&self.to_cbor_value()?)?;
        Ok(Cid::dag_cbor(&bytes))
    }

    pub fn from_cbor_value(v: &dagcbor::Value) -> Result<Self> {
        let dagcbor::Value::Map(m) = v else {
            return Err(Error::capability("capability must be a map"));
        };
        let get_str = |k: &str| -> Result<String> {
            match m.get(k) {
                Some(dagcbor::Value::Text(s)) => Ok(s.clone()),
                _ => Err(Error::capability(format!("missing {k}"))),
            }
        };
        let get_u = |k: &str| -> Result<u64> {
            match m.get(k) {
                Some(dagcbor::Value::Unsigned(n)) => Ok(*n),
                _ => Err(Error::capability(format!("missing {k}"))),
            }
        };
        let pol = match m.get("pol") {
            Some(dagcbor::Value::Array(a)) => {
                a.iter().map(cbor_to_json).collect::<Result<Vec<_>>>()?
            }
            _ => Vec::new(),
        };
        let prf = match m.get("prf") {
            Some(dagcbor::Value::Array(a)) => a
                .iter()
                .filter_map(|x| match x {
                    dagcbor::Value::Text(s) => Some(s.clone()),
                    dagcbor::Value::Cid(c) => Some(c.to_string()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let sig = match m.get("sig") {
            Some(dagcbor::Value::Bytes(b)) if b.len() == 64 => {
                let mut s = [0u8; 64];
                s.copy_from_slice(b);
                Some(s)
            }
            _ => None,
        };
        let meta = m.get("meta").map(cbor_to_json).transpose()?;
        Ok(Self {
            v: get_u("v")?,
            iss: get_str("iss")?,
            aud: get_str("aud")?,
            cmd: get_str("cmd")?,
            res: get_str("res")?,
            nbf: get_u("nbf")?,
            exp: get_u("exp")?,
            pol,
            prf,
            meta,
            sig,
        })
    }
}

/// Invocation (HTTP `Authorization: Cap <base64url(DAG-CBOR)>`).
#[derive(Clone, Debug)]
pub struct Invocation {
    pub v: u64,
    pub cap: String,
    pub prf: Vec<Capability>,
    pub cmd: String,
    pub res: String,
    pub args: serde_json::Value,
    pub ts: u64,
    pub aud: String,
    pub nonce: Vec<u8>,
    pub sig: Option<[u8; 64]>,
}

impl Invocation {
    pub fn new(
        leaf: &Capability,
        cmd: &str,
        res: &str,
        aud: &str,
        ts: u64,
        nonce: [u8; 16],
    ) -> Result<Self> {
        let cap = leaf.cid()?.to_string();
        Ok(Self {
            v: 1,
            cap,
            prf: vec![leaf.clone()],
            cmd: cmd.to_string(),
            res: res.to_string(),
            args: serde_json::json!({}),
            ts,
            aud: aud.to_string(),
            nonce: nonce.to_vec(),
            sig: None,
        })
    }

    pub fn to_unsigned_value(&self) -> Result<dagcbor::Value> {
        use std::collections::BTreeMap;
        let mut m = BTreeMap::new();
        m.insert("v".into(), dagcbor::Value::Unsigned(self.v));
        m.insert("cap".into(), dagcbor::Value::Cid(Cid::parse(&self.cap)?));
        let mut prf = Vec::new();
        for cap in &self.prf {
            prf.push(cap.to_cbor_value()?);
        }
        m.insert("prf".into(), dagcbor::Value::Array(prf));
        m.insert("cmd".into(), dagcbor::Value::Text(self.cmd.clone()));
        m.insert("res".into(), dagcbor::Value::Text(self.res.clone()));
        m.insert("args".into(), dagcbor::from_json(&self.args)?);
        m.insert("ts".into(), dagcbor::Value::Unsigned(self.ts));
        m.insert("aud".into(), dagcbor::Value::Text(self.aud.clone()));
        m.insert("nonce".into(), dagcbor::Value::Bytes(self.nonce.clone()));
        Ok(dagcbor::Value::Map(m))
    }

    pub fn encode_cbor(&self) -> Result<Vec<u8>> {
        let mut v = self.to_unsigned_value()?;
        if let Some(sig) = self.sig {
            if let dagcbor::Value::Map(m) = &mut v {
                m.insert("sig".into(), dagcbor::Value::Bytes(sig.to_vec()));
            }
        }
        dagcbor::encode(&v)
    }

    pub fn sign(&mut self, sk: &SigningKey) -> Result<[u8; 64]> {
        let cbor = dagcbor::encode(&self.to_unsigned_value()?)?;
        let sig = sign::sign_unsigned(sk, &cbor);
        self.sig = Some(sig);
        Ok(sig)
    }

    /// Decode DAG-CBOR. JSON objects (`{...}`) MUST be rejected.
    pub fn decode_cbor(bytes: &[u8]) -> Result<Self> {
        if bytes.first() == Some(&b'{') {
            return Err(Error::capability("invocation MUST be DAG-CBOR, not JSON"));
        }
        let v = dagcbor::decode(bytes)?;
        Self::from_cbor_value(&v)
    }

    pub fn from_cbor_value(v: &dagcbor::Value) -> Result<Self> {
        let dagcbor::Value::Map(m) = v else {
            return Err(Error::capability("invocation must be a map"));
        };
        let get_str = |k: &str| -> Result<String> {
            match m.get(k) {
                Some(dagcbor::Value::Text(s)) => Ok(s.clone()),
                _ => Err(Error::capability(format!("invocation missing {k}"))),
            }
        };
        let get_u = |k: &str| -> Result<u64> {
            match m.get(k) {
                Some(dagcbor::Value::Unsigned(n)) => Ok(*n),
                _ => Err(Error::capability(format!("invocation missing {k}"))),
            }
        };
        let cap = match m.get("cap") {
            Some(dagcbor::Value::Cid(c)) => c.to_string(),
            Some(dagcbor::Value::Text(s)) => s.clone(),
            _ => return Err(Error::capability("invocation missing cap")),
        };
        let prf = match m.get("prf") {
            Some(dagcbor::Value::Array(a)) if !a.is_empty() => a
                .iter()
                .map(Capability::from_cbor_value)
                .collect::<Result<Vec<_>>>()?,
            _ => {
                return Err(Error::capability(
                    "invocation prf must be a non-empty array",
                ))
            }
        };
        let nonce = match m.get("nonce") {
            Some(dagcbor::Value::Bytes(b)) if b.len() == 16 => b.clone(),
            _ => return Err(Error::capability("nonce MUST be 16 bytes")),
        };
        let sig = match m.get("sig") {
            Some(dagcbor::Value::Bytes(b)) if b.len() == 64 => {
                let mut s = [0u8; 64];
                s.copy_from_slice(b);
                Some(s)
            }
            _ => None,
        };
        let args = match m.get("args") {
            Some(v) => cbor_to_json(v)?,
            None => serde_json::json!({}),
        };
        Ok(Self {
            v: get_u("v")?,
            cap,
            prf,
            cmd: get_str("cmd")?,
            res: get_str("res")?,
            args,
            ts: get_u("ts")?,
            aud: get_str("aud")?,
            nonce,
            sig,
        })
    }

    pub fn leaf(&self) -> Result<&Capability> {
        self.prf
            .last()
            .ok_or_else(|| Error::capability("empty proof chain"))
    }

    /// Verify signatures, audience, time, and monotonic attenuation.
    pub fn verify(&self, now: u64, knot_did: &str) -> Result<&Capability> {
        let leaf = self.leaf()?;
        validate_capability(leaf)?;
        let sig = self
            .sig
            .as_ref()
            .ok_or_else(|| Error::capability("invocation missing sig"))?;
        let leaf_pk = crate::identity::Did::parse(&leaf.aud)?.pubkey;
        let unsigned = dagcbor::encode(&self.to_unsigned_value()?)?;
        sign::verify_unsigned_raw(&leaf_pk, &unsigned, sig)?;
        evaluate_caveats(&leaf.pol, &self.args, now, None)?;
        if self.aud != knot_did {
            return Err(Error::capability("invocation aud is not this knot"));
        }
        if self.cmd != leaf.cmd && leaf.cmd != "*" {
            return Err(Error::capability("invocation cmd does not match leaf"));
        }
        if !resource_attenuates(&leaf.res, &self.res) && leaf.res != self.res {
            return Err(Error::capability("invocation res not covered by leaf"));
        }
        for cap in &self.prf {
            if !in_time_window(cap.nbf, cap.exp, now) {
                return Err(Error::capability("capability not in time window"));
            }
            let Some(csig) = cap.sig else {
                return Err(Error::capability("capability missing sig"));
            };
            let pk = crate::identity::Did::parse(&cap.iss)?.pubkey;
            verify_cap_sig(&pk, &cap.to_unsigned_json(), &csig)?;
        }
        for pair in self.prf.windows(2) {
            if pair[0].aud != pair[1].iss {
                return Err(Error::capability("broken capability chain"));
            }
            attenuates(&pair[0], &pair[1])?;
        }
        Ok(leaf)
    }
}

fn cbor_to_json(v: &dagcbor::Value) -> Result<serde_json::Value> {
    Ok(match v {
        dagcbor::Value::Null => serde_json::Value::Null,
        dagcbor::Value::Bool(b) => serde_json::Value::Bool(*b),
        dagcbor::Value::Unsigned(n) => serde_json::json!(n),
        dagcbor::Value::Negative(n) => serde_json::json!(n),
        dagcbor::Value::Text(s) => serde_json::Value::String(s.clone()),
        dagcbor::Value::Bytes(b) => serde_json::Value::String(hex::encode(b)),
        dagcbor::Value::Array(a) => {
            serde_json::Value::Array(a.iter().map(cbor_to_json).collect::<Result<Vec<_>>>()?)
        }
        dagcbor::Value::Map(m) => {
            let mut obj = serde_json::Map::new();
            for (k, val) in m {
                obj.insert(k.clone(), cbor_to_json(val)?);
            }
            serde_json::Value::Object(obj)
        }
        dagcbor::Value::Cid(c) => serde_json::Value::String(c.to_string()),
    })
}

pub fn cap_authorization_header(invocation_cbor: &[u8]) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(invocation_cbor);
    format!("Cap {b64}")
}

/// True when `bytes` look like a JSON object rather than DAG-CBOR.
pub fn looks_like_json(bytes: &[u8]) -> bool {
    matches!(bytes.first(), Some(b'{') | Some(b'['))
}

/// Revocation set: a chain is invalid if it contains any revoked CID.
pub fn chain_revoked(chain_cids: &[Cid], revoked: &HashSet<String>) -> bool {
    chain_cids
        .iter()
        .any(|c| revoked.contains(&c.to_string_multibase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attenuation_negative() {
        let parent =
            "tangled://did:sol:2sxknyqubWpqPSrk7Hncw1NyyySXQAoXa4HebbZV9qKs/pub.tangled.chat";
        let child = "tangled://did:sol:2sxknyqubWpqPSrk7Hncw1NyyySXQAoXa4HebbZV9qKs";
        assert!(!resource_attenuates(parent, child));
        assert!(resource_attenuates(child, parent));
    }

    #[test]
    fn circle_not_did_prefix() {
        let did = "tangled://did:sol:X";
        let circ = "tangled://circle/did:sol:X/abc";
        assert!(!resource_attenuates(did, circ));
        assert!(!resource_attenuates(circ, did));
    }

    #[test]
    fn unknown_command_rejected() {
        let cap = Capability {
            v: 1,
            iss: "did:sol:x".into(),
            aud: "did:sol:y".into(),
            cmd: "repo/explode".into(),
            res: "tangled://did:sol:x".into(),
            nbf: 1,
            exp: 2,
            pol: vec![],
            prf: vec![],
            meta: None,
            sig: None,
        };
        assert!(validate_capability(&cap).is_err());
    }

    #[test]
    fn unknown_caveat_fail_closed() {
        let cap = Capability {
            v: 1,
            iss: "did:sol:x".into(),
            aud: "did:sol:y".into(),
            cmd: "repo/read".into(),
            res: "tangled://did:sol:x".into(),
            nbf: 1,
            exp: 2,
            pol: vec![serde_json::json!(["not_a_caveat", 1])],
            prf: vec![],
            meta: None,
            sig: None,
        };
        assert!(validate_capability(&cap).is_err());
    }

    #[test]
    fn invocation_dagcbor_not_json() {
        let json = br#"{"v":1,"cmd":"repo/read"}"#;
        assert!(looks_like_json(json));
        assert!(Invocation::decode_cbor(json).is_err());
    }

    #[test]
    fn expired_capability_rejected() {
        let cap = Capability {
            v: 1,
            iss: "did:sol:x".into(),
            aud: "did:sol:y".into(),
            cmd: "repo/read".into(),
            res: "tangled://did:sol:x".into(),
            nbf: 1000,
            exp: 2000,
            pol: vec![],
            prf: vec![],
            meta: None,
            sig: None,
        };
        assert!(authorizes(&cap, "repo/read", "tangled://did:sol:x", 3000).is_err());
        assert!(authorizes(&cap, "repo/read", "tangled://did:sol:x", 1500).is_ok());
    }

    #[test]
    fn caveats_eq_lt_valid_until_max_uses() {
        let pol = vec![
            serde_json::json!(["eq", "/did", "did:sol:x"]),
            serde_json::json!(["lt", "/n", 10]),
            serde_json::json!(["valid_until", 5000]),
            serde_json::json!(["max_uses", 2]),
        ];
        let args = serde_json::json!({"did": "did:sol:x", "n": 3});
        evaluate_caveats(&pol, &args, 4000, Some(0)).unwrap();
        assert!(
            evaluate_caveats(&pol, &serde_json::json!({"did": "no", "n": 3}), 4000, None).is_err()
        );
        assert!(evaluate_caveats(
            &pol,
            &serde_json::json!({"did": "did:sol:x", "n": 10}),
            4000,
            None
        )
        .is_err());
        assert!(evaluate_caveats(&pol, &args, 5061, None).is_err());
        assert!(evaluate_caveats(&pol, &args, 4000, Some(2)).is_err());
    }

    #[test]
    fn clock_skew_window_on_nbf_exp_and_valid_until() {
        let cap = Capability {
            v: 1,
            iss: "did:sol:x".into(),
            aud: "did:sol:y".into(),
            cmd: "repo/read".into(),
            res: "tangled://did:sol:x".into(),
            nbf: 1000,
            exp: 2000,
            pol: vec![],
            prf: vec![],
            meta: None,
            sig: None,
        };
        assert!(authorizes(&cap, "repo/read", "tangled://did:sol:x", 980).is_ok());
        assert!(authorizes(&cap, "repo/read", "tangled://did:sol:x", 900).is_err());
        assert!(authorizes(&cap, "repo/read", "tangled://did:sol:x", 2050).is_ok());
        assert!(authorizes(&cap, "repo/read", "tangled://did:sol:x", 2070).is_err());
        let pol = vec![serde_json::json!(["valid_until", 5000])];
        evaluate_caveats(&pol, &serde_json::json!({}), 5050, None).unwrap();
        assert!(evaluate_caveats(&pol, &serde_json::json!({}), 5070, None).is_err());
    }
}
