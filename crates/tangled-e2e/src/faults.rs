//! Seeded delivery faults for real protocol requests in the e2e driver.
//! Virtual time controls request delivery, not OS scheduling or physical clocks.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use tangled::http::{KnotRequest, KnotResponse};
use tangled::{Identity, KnotEngine};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Fault {
    Pass,
    Drop,
    Duplicate,
    Delay(u64),
    PartitionUntil(u64),
    CorruptResponse,
    RestartBefore,
}

struct Scheduled {
    id: u64,
    tick: u64,
    priority: u64,
    request: KnotRequest,
    fault: Fault,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceEvent {
    pub id: u64,
    pub tick: u64,
    pub fault: Fault,
    pub delivery: usize,
    pub status: Option<u16>,
    pub body_digest: String,
    pub stored_records: usize,
}

#[derive(Debug, Serialize)]
pub struct FaultRun {
    pub trace: Vec<TraceEvent>,
    pub deliveries: usize,
    pub dropped: usize,
    pub restarts: usize,
    #[serde(skip)]
    pub responses: Vec<(u64, KnotResponse)>,
}

pub struct FaultScheduler {
    seed: u64,
    max_events: usize,
    events: Vec<Scheduled>,
}

impl FaultScheduler {
    pub fn new(seed: u64, max_events: usize) -> Result<Self> {
        ensure!(
            (1..=4096).contains(&max_events),
            "event limit must be 1..4096"
        );
        Ok(Self {
            seed,
            max_events,
            events: Vec::new(),
        })
    }

    pub fn push(&mut self, id: u64, tick: u64, request: KnotRequest, fault: Fault) -> Result<()> {
        ensure!(
            self.events.len() < self.max_events,
            "event budget exhausted"
        );
        ensure!(
            !self.events.iter().any(|event| event.id == id),
            "duplicate event ID"
        );
        let tick = match &fault {
            Fault::Delay(delay) => tick.checked_add(*delay).context("virtual time overflow")?,
            Fault::PartitionUntil(until) => tick.max(*until),
            _ => tick,
        };
        let mut hash = blake3::Hasher::new();
        hash.update(&self.seed.to_le_bytes());
        hash.update(&id.to_le_bytes());
        let digest = hash.finalize();
        let priority = u64::from_le_bytes(digest.as_bytes()[..8].try_into().expect("eight bytes"));
        self.events.push(Scheduled {
            id,
            tick,
            priority,
            request,
            fault,
        });
        Ok(())
    }

    /// The fixture wallet stays alive across an engine restart. This does not
    /// simulate wallet loss, process scheduling, torn writes or power failure.
    pub fn run(mut self, engine: &mut KnotEngine, owner: &Identity) -> Result<FaultRun> {
        self.events
            .sort_by_key(|event| (event.tick, event.priority, event.id));
        let mut run = FaultRun {
            trace: Vec::new(),
            deliveries: 0,
            dropped: 0,
            restarts: 0,
            responses: Vec::new(),
        };
        for event in self.events {
            if event.fault == Fault::Drop {
                run.dropped += 1;
                run.trace.push(TraceEvent {
                    id: event.id,
                    tick: event.tick,
                    fault: event.fault,
                    delivery: 0,
                    status: None,
                    body_digest: String::new(),
                    stored_records: engine.stored.len(),
                });
                continue;
            }
            if event.fault == Fault::RestartBefore {
                let directory = engine
                    .data_dir
                    .clone()
                    .context("restart requires a persistent engine")?;
                let now = engine.now;
                engine.persist()?;
                *engine = KnotEngine::open(directory, &owner.did_string())?;
                engine.now = now;
                engine.register_identity(owner);
                run.restarts += 1;
            }
            let count = if event.fault == Fault::Duplicate {
                2
            } else {
                1
            };
            for delivery in 0..count {
                let mut response = engine.handle(&event.request)?;
                if event.fault == Fault::CorruptResponse {
                    if let Some(first) = response.body.first_mut() {
                        *first ^= 0xff;
                    }
                }
                run.deliveries += 1;
                run.trace.push(TraceEvent {
                    id: event.id,
                    tick: event.tick,
                    fault: event.fault.clone(),
                    delivery,
                    status: Some(response.status),
                    body_digest: blake3::hash(&response.body).to_hex().to_string(),
                    stored_records: engine.stored.len(),
                });
                run.responses.push((event.id, response));
            }
        }
        Ok(run)
    }
}
