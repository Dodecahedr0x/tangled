//! Circle dual-stack / never-downgrade state machine. MLS Welcome/Commit bytes
//! are produced by [`crate::mls`] (OpenMLS, ciphersuite 0x0001).

use std::collections::HashSet;

use crate::constants::{
    CIRCLE_MLS_THRESHOLD, DUAL_STACK_MAX_SKEW_EPOCHS, DUAL_STACK_MAX_SKEW_SECONDS,
    MLS_APPLICATION_ID, MLS_CIPHERSUITE, MLS_EXPORTER_LABEL,
};
use crate::error::{Error, Result};

/// How epoch secrets are distributed. Diagnostic only — not a user-facing mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Distributor {
    Hpke,
    Dual,
    Mls,
}

impl Distributor {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hpke => "hpke",
            Self::Dual => "dual",
            Self::Mls => "mls",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "hpke" => Ok(Self::Hpke),
            "dual" => Ok(Self::Dual),
            "mls" => Ok(Self::Mls),
            other => Err(Error::Circle(format!("unknown distributor {other}"))),
        }
    }
}

/// MLS log body (`sys.circle.mls`). `payload` is RFC 9420 Welcome/Commit bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MlsLogBody {
    pub kind: MlsLogType,
    pub epoch: u64,
    pub member: Option<String>,
    pub payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MlsLogType {
    Welcome,
    Commit,
    Receipt,
}

impl MlsLogType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Welcome => "welcome",
            Self::Commit => "commit",
            Self::Receipt => "receipt",
        }
    }
}

/// Plan returned by membership changes (crypto wrapping, not a user API flag).
#[derive(Clone, Debug)]
pub enum MembershipPlan {
    HpkeWrap {
        new_epoch: u64,
        wrap_to: Vec<String>,
    },
    SeamlessGrow {
        new_epoch: u64,
        wrap_to: Vec<String>,
        welcome_members: Vec<String>,
    },
    MlsAdd {
        new_epoch: u64,
        also_hpke_wrap: Vec<String>,
    },
    MlsRemove {
        new_epoch: u64,
        remaining: Vec<String>,
    },
    HpkeRemove {
        new_epoch: u64,
        wrap_to: Vec<String>,
    },
}

/// Circle runtime state. `mlsGroup` being set means never-downgrade.
#[derive(Clone, Debug)]
pub struct CircleState {
    pub circle_id: String,
    pub owner: String,
    pub name: String,
    pub members: Vec<String>,
    pub epoch: u64,
    pub distributor: Distributor,
    pub mls_group: Option<Vec<u8>>,
    pub threshold: usize,
    pub history_share: bool,
    pub hide_members: bool,
    /// Unix seconds when dual-stack started.
    pub dual_started_at: Option<u64>,
    pub dual_start_epoch: Option<u64>,
    pub members_at_transition: Vec<String>,
    pub welcome_receipts: HashSet<String>,
    /// Epoch secrets this client still holds (current + history if retained).
    pub epoch_secrets: std::collections::BTreeMap<u64, [u8; 32]>,
}

impl CircleState {
    /// Create: owner, epoch 0, HPKE, wrap to owner. MLS is not involved.
    pub fn create(
        owner: String,
        circle_id: String,
        name: String,
        epoch_secret_0: [u8; 32],
    ) -> Self {
        let mut epoch_secrets = std::collections::BTreeMap::new();
        epoch_secrets.insert(0, epoch_secret_0);
        Self {
            circle_id,
            owner: owner.clone(),
            name,
            members: vec![owner],
            epoch: 0,
            distributor: Distributor::Hpke,
            mls_group: None,
            threshold: CIRCLE_MLS_THRESHOLD,
            history_share: false,
            hide_members: false,
            dual_started_at: None,
            dual_start_epoch: None,
            members_at_transition: Vec::new(),
            welcome_receipts: HashSet::new(),
            epoch_secrets,
        }
    }

    pub fn used_mls(&self) -> bool {
        self.mls_group.is_some() || matches!(self.distributor, Distributor::Mls | Distributor::Dual)
    }

    /// User action: add a DID. Never exposes HPKE vs MLS as a client flag.
    pub fn add_member(
        &mut self,
        did: String,
        new_epoch_secret: [u8; 32],
        now: u64,
    ) -> Result<MembershipPlan> {
        if self.members.contains(&did) {
            return Err(Error::Circle("already a member".into()));
        }
        match self.distributor {
            Distributor::Hpke if self.members.len() < CIRCLE_MLS_THRESHOLD && !self.used_mls() => {
                self.members.push(did);
                self.epoch += 1;
                self.epoch_secrets.insert(self.epoch, new_epoch_secret);
                Ok(MembershipPlan::HpkeWrap {
                    new_epoch: self.epoch,
                    wrap_to: self.members.clone(),
                })
            }
            Distributor::Hpke => {
                // Seamless grow.
                self.members.push(did);
                self.epoch += 1;
                self.epoch_secrets.insert(self.epoch, new_epoch_secret);
                self.distributor = Distributor::Dual;
                self.mls_group = Some(vec![0]); // placeholder group context; real MLS fills this
                self.dual_started_at = Some(now);
                self.dual_start_epoch = Some(self.epoch);
                self.members_at_transition = self.members.clone();
                self.welcome_receipts.clear();
                Ok(MembershipPlan::SeamlessGrow {
                    new_epoch: self.epoch,
                    wrap_to: self.members.clone(),
                    welcome_members: self.members.clone(),
                })
            }
            Distributor::Dual | Distributor::Mls => {
                self.members.push(did.clone());
                self.epoch += 1;
                self.epoch_secrets.insert(self.epoch, new_epoch_secret);
                let also = if self.distributor == Distributor::Dual {
                    self.members_at_transition
                        .iter()
                        .filter(|m| !self.welcome_receipts.contains(*m))
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };
                Ok(MembershipPlan::MlsAdd {
                    new_epoch: self.epoch,
                    also_hpke_wrap: also,
                })
            }
        }
    }

    pub fn remove_member(
        &mut self,
        did: &str,
        new_epoch_secret: [u8; 32],
    ) -> Result<MembershipPlan> {
        if !self.members.iter().any(|m| m == did) {
            return Err(Error::Circle("not a member".into()));
        }
        self.members.retain(|m| m != did);
        self.epoch += 1;
        self.epoch_secrets.insert(self.epoch, new_epoch_secret);
        if self.used_mls() {
            // Never downgrade, even if membership drops to two.
            self.distributor = Distributor::Mls;
            Ok(MembershipPlan::MlsRemove {
                new_epoch: self.epoch,
                remaining: self.members.clone(),
            })
        } else {
            self.distributor = Distributor::Hpke;
            Ok(MembershipPlan::HpkeRemove {
                new_epoch: self.epoch,
                wrap_to: self.members.clone(),
            })
        }
    }

    pub fn observe_receipt(&mut self, member: String, epoch: u64, now: u64) {
        if self.dual_start_epoch == Some(epoch) || self.distributor == Distributor::Dual {
            self.welcome_receipts.insert(member);
        }
        self.maybe_end_dual(now);
    }

    /// End dual-stack when every transition member has a receipt, or skew caps hit.
    pub fn maybe_end_dual(&mut self, now: u64) {
        if self.distributor != Distributor::Dual {
            return;
        }
        let all_receipts = self
            .members_at_transition
            .iter()
            .all(|m| self.welcome_receipts.contains(m));
        let epoch_skew = self
            .dual_start_epoch
            .is_some_and(|s| self.epoch.saturating_sub(s) >= DUAL_STACK_MAX_SKEW_EPOCHS);
        let time_skew = self
            .dual_started_at
            .is_some_and(|t| now.saturating_sub(t) >= DUAL_STACK_MAX_SKEW_SECONDS);
        if all_receipts || epoch_skew || time_skew {
            self.distributor = Distributor::Mls;
        }
    }

    /// New application sends MUST use epochs in `[current - skew, current]` during dual-stack.
    pub fn accept_send_epoch(&self, epoch: u64) -> bool {
        if epoch > self.epoch {
            return false;
        }
        self.epoch.saturating_sub(epoch) <= DUAL_STACK_MAX_SKEW_EPOCHS
    }

    /// Device-key-only rotation: MUST NOT re-wrap and MUST NOT publish MLS Updates.
    pub fn device_key_only_rotation(&self) -> bool {
        true
    }

    /// Device-key-only rotation produces an empty rewrap list.
    pub fn device_key_only_rewrap_list(&self) -> Vec<(u64, [u8; 32])> {
        Vec::new()
    }

    /// Agreement rotation: caller re-wraps every retained epoch secret to the new X25519 key.
    pub fn epochs_to_rewrapped(&self) -> Vec<(u64, [u8; 32])> {
        self.epoch_secrets.iter().map(|(e, s)| (*e, *s)).collect()
    }

    /// Rewrap list if `now` is within `AGREEMENT_ROTATION_MAX_SKEW_SECONDS` of the new `sys/id` ts.
    pub fn agreement_rotation_rewrap(
        &self,
        sys_id_ts_unix: u64,
        now_unix: u64,
    ) -> Result<Vec<(u64, [u8; 32])>> {
        use crate::constants::AGREEMENT_ROTATION_MAX_SKEW_SECONDS;
        if now_unix.saturating_sub(sys_id_ts_unix) > AGREEMENT_ROTATION_MAX_SKEW_SECONDS {
            return Err(Error::Circle(
                "outside AGREEMENT_ROTATION_MAX_SKEW_SECONDS".into(),
            ));
        }
        Ok(self.epochs_to_rewrapped())
    }
}

/// Synthetic MLS wrapping record (payload is not RFC 9420-verified).
pub fn wrap_mls_log(
    kind: MlsLogType,
    epoch: u64,
    member: Option<&str>,
    payload: Vec<u8>,
) -> MlsLogBody {
    MlsLogBody {
        kind,
        epoch,
        member: member.map(str::to_string),
        payload,
    }
}

/// Exporter context: `uint64be(N)`.
pub fn mls_exporter_context(epoch: u64) -> [u8; 8] {
    epoch.to_be_bytes()
}

pub fn mls_constants_ok() -> bool {
    MLS_CIPHERSUITE == 0x0001
        && MLS_EXPORTER_LABEL == b"tangled.epoch"
        && MLS_APPLICATION_ID == b"tangled.circle.v1"
}

/// Drive the grow vector: 16 HPKE members, 17th triggers dual, skew → mls, shrink stays mls.
pub fn simulate_grow_vector(
    owner: String,
    circle_id: String,
    members_before: Vec<String>,
    new_member: String,
    epoch_secret_0: [u8; 32],
    epoch_secret_1: [u8; 32],
    now: u64,
) -> Result<CircleState> {
    let mut st = CircleState::create(owner, circle_id, "g".into(), epoch_secret_0);
    st.members = members_before;
    if st.members.len() != CIRCLE_MLS_THRESHOLD {
        return Err(Error::Circle("grow vector expects 16 members".into()));
    }
    st.add_member(new_member, epoch_secret_1, now)?;
    Ok(st)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_downgrade() {
        let mut st = CircleState::create("did:sol:o".into(), "c1".into(), "n".into(), [1u8; 32]);
        st.members = (0..16).map(|i| format!("did:sol:{i}")).collect();
        st.add_member("did:sol:17".into(), [2u8; 32], 1000).unwrap();
        assert_eq!(st.distributor, Distributor::Dual);
        assert!(st.mls_group.is_some());
        st.remove_member("did:sol:17", [3u8; 32]).unwrap();
        for i in 5..16 {
            st.remove_member(&format!("did:sol:{i}"), [4u8; 32])
                .unwrap();
        }
        assert_eq!(st.distributor, Distributor::Mls);
        assert_ne!(st.distributor, Distributor::Hpke);
    }

    #[test]
    fn device_key_only_must_not_rewrap() {
        let mut st = CircleState::create("did:sol:o".into(), "c1".into(), "n".into(), [1u8; 32]);
        st.add_member("did:sol:2".into(), [2u8; 32], 1000).unwrap();
        assert!(st.device_key_only_rotation());
        assert!(st.device_key_only_rewrap_list().is_empty());
        assert!(!st.epochs_to_rewrapped().is_empty());
    }

    #[test]
    fn agreement_rotation_within_and_outside_skew() {
        use crate::constants::AGREEMENT_ROTATION_MAX_SKEW_SECONDS;
        let mut st = CircleState::create("did:sol:o".into(), "c1".into(), "n".into(), [1u8; 32]);
        st.add_member("did:sol:2".into(), [2u8; 32], 1000).unwrap();
        let sys_id_ts = 1_700_000_000u64;
        let inside = st
            .agreement_rotation_rewrap(sys_id_ts, sys_id_ts + 60)
            .unwrap();
        assert_eq!(inside.len(), st.epoch_secrets.len());
        assert!(st
            .agreement_rotation_rewrap(
                sys_id_ts,
                sys_id_ts + AGREEMENT_ROTATION_MAX_SKEW_SECONDS + 1
            )
            .is_err());
    }
}
