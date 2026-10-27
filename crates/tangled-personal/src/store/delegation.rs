//! Owner-side cryptographic boundary for the external personal agent workspace.
use super::{err, now, Store};
use crate::Result;
use serde_json::{json, Value};
use tangled::capability::{Capability, Invocation};
use tangled::identity::Did;

impl Store {
    pub(crate) fn issue_agent_capability(
        &self,
        audience: &str,
        collection: &str,
        command: &str,
        ttl: u64,
    ) -> Result<Value> {
        Did::parse(audience).map_err(err)?;
        if audience == self.did() {
            return Err("Use a separate agent identity as audience".into());
        }
        if !(1..=86400).contains(&ttl) {
            return Err("ttl_seconds must be between 1 and 86400".into());
        }
        let time = now();
        let mut cap = Capability {
            v: 1,
            iss: self.did(),
            aud: audience.into(),
            cmd: command.into(),
            res: format!("tangled://{}/{collection}", self.did()),
            nbf: time,
            exp: time + ttl,
            pol: vec![],
            prf: vec![],
            meta: Some(json!({"purpose":"personal-agent","nonce":tangled::identity::tid_now()})),
            sig: None,
        };
        let sig = cap.sign(self.identity.signing_key()).map_err(err)?;
        let mut value = cap.to_unsigned_json();
        value["sig"] = json!(hex::encode(sig));
        Ok(json!({"capability":value,"cid":cap.cid().map_err(err)?.to_string()}))
    }

    pub(crate) fn verify_agent_invocation(
        &self,
        bytes: &[u8],
        args: &Value,
        collection: &str,
        command: &str,
    ) -> Result<Invocation> {
        let invocation = Invocation::decode_cbor(bytes).map_err(err)?;
        if invocation.v != 1 {
            return Err("Unsupported agent invocation version".into());
        }
        let time = now();
        let cap = invocation.verify(time, &self.did()).map_err(err)?;
        // Personal grants are direct owner certificates. Deliberately disallow
        // proof chains until this workspace supports delegation-of-delegation.
        if invocation.prf.len() != 1 || cap.iss != self.did() || !cap.prf.is_empty() {
            return Err("Expected a direct owner-issued agent capability".into());
        }
        let resource = format!("tangled://{}/{collection}", self.did());
        if cap.cmd != command
            || invocation.cmd != command
            || cap.res != resource
            || invocation.res != resource
        {
            return Err("Agent command or collection is outside the grant".into());
        }
        // Enforce the advertised TTL exactly, without the protocol's clock skew
        // allowance; both parties are using this local owner's clock.
        if time < cap.nbf || time >= cap.exp {
            return Err("Agent grant has expired or is not active yet".into());
        }
        if invocation.ts > time.saturating_add(60) || time.saturating_sub(invocation.ts) > 300 {
            return Err("Agent invocation timestamp is outside the accepted window".into());
        }
        let cid = cap.cid().map_err(err)?.to_string();
        if invocation.cap != cid || self.engine.revoked.contains_key(&cid) {
            return Err("Agent capability CID mismatch or revoked grant".into());
        }
        if invocation.args != *args {
            return Err("Invocation arguments differ from the requested operation".into());
        }
        Ok(invocation)
    }
}
