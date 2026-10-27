use tangled::http::{KnotRequest, Method};
use tangled::{car, record, Cid, Identity, KnotEngine};
use tangled_e2e::faults::{Fault, FaultScheduler};

fn identity() -> Identity {
    Identity::from_seeds([61; 32], [62; 32])
}
fn request(id: usize) -> KnotRequest {
    let owner = identity();
    let unsigned = serde_json::json!({"v":1,"type":Cid::dag_cbor(b"pub.tangled.post").to_string(),"collection":"pub.tangled.post","rkey":format!("record-{id}"),"author":owner.did_string(),"ts":1_700_000_000_000u64 + id as u64,"prev":[],"vis":"public","body":{"text":format!("value-{id}")}});
    let signed = record::sign_json_record(owner.signing_key(), &unsigned).unwrap();
    KnotRequest {
        method: Method::Post,
        path: "/xrpc/tangled.repo.putRecord".into(),
        query: vec![],
        headers: vec![],
        body: Some(serde_json::to_vec(&serde_json::json!({"record":signed.json})).unwrap()),
        content_type: Some("application/json".into()),
    }
}
fn engine() -> KnotEngine {
    let owner = identity();
    let mut engine = KnotEngine::hosting(&owner.did_string());
    engine.now = 1_700_000_000;
    engine.register_identity(&owner);
    engine
}
fn root(engine: &KnotEngine) -> String {
    engine.msts[&identity().did_string()]
        .root_cid_string_cids()
        .unwrap()
        .to_string()
}

#[test]
fn same_seed_replays_actual_protocol_and_healed_faults_converge() {
    let mut traces = Vec::new();
    for _ in 0..2 {
        let mut store = engine();
        let mut schedule = FaultScheduler::new(7, 16).unwrap();
        schedule.push(0, 0, request(0), Fault::Pass).unwrap();
        schedule.push(1, 0, request(1), Fault::Drop).unwrap();
        schedule.push(2, 0, request(2), Fault::Duplicate).unwrap();
        schedule
            .push(3, 1, request(1), Fault::PartitionUntil(3))
            .unwrap();
        let run = schedule.run(&mut store, &identity()).unwrap();
        assert_eq!(store.tips.len(), 3);
        assert!(run.deliveries <= 8);
        let mut reference = engine();
        for id in 0..3 {
            assert_eq!(reference.handle(&request(id)).unwrap().status, 200);
        }
        assert_eq!(root(&store), root(&reference));
        traces.push(run.trace);
    }
    assert_eq!(traces[0], traces[1]);
}

#[test]
fn varied_seeds_explore_orders_without_changing_signed_state() {
    let mut orders = std::collections::HashSet::new();
    let mut expected = None;
    for seed in 0..32 {
        let mut store = engine();
        let mut schedule = FaultScheduler::new(seed, 16).unwrap();
        for id in 0..8 {
            schedule
                .push(id, 0, request(id as usize), Fault::Pass)
                .unwrap();
        }
        let run = schedule.run(&mut store, &identity()).unwrap();
        orders.insert(run.trace.iter().map(|e| e.id).collect::<Vec<_>>());
        let actual = root(&store);
        if let Some(value) = &expected {
            assert_eq!(value, &actual);
        } else {
            expected = Some(actual);
        }
    }
    assert!(orders.len() > 1, "seed must affect delivery order");
}

#[test]
fn corruption_is_rejected_by_real_car_decoder_and_limits_are_checked() {
    let mut store = engine();
    store.handle(&request(0)).unwrap();
    let before = root(&store);
    let query = KnotRequest {
        method: Method::Get,
        path: "/xrpc/tangled.sync.getRepo".into(),
        query: vec![("did".into(), identity().did_string())],
        headers: vec![],
        body: None,
        content_type: None,
    };
    let mut schedule = FaultScheduler::new(9, 1).unwrap();
    schedule
        .push(0, 0, query.clone(), Fault::CorruptResponse)
        .unwrap();
    assert!(schedule.push(1, 0, query.clone(), Fault::Pass).is_err());
    let run = schedule.run(&mut store, &identity()).unwrap();
    assert!(car::decode(&run.responses[0].1.body).is_err());
    assert_eq!(root(&store), before);
    assert!(FaultScheduler::new(0, 0).is_err());
    let mut schedule = FaultScheduler::new(0, 1).unwrap();
    assert!(schedule.push(0, u64::MAX, query, Fault::Delay(1)).is_err());
}

#[test]
fn restart_uses_real_persisted_engine_and_keeps_precrash_data() {
    let dir = tempfile::tempdir().unwrap();
    let owner = identity();
    let mut store = KnotEngine::open(dir.path(), &owner.did_string()).unwrap();
    store.now = 1_700_000_000;
    store.register_identity(&owner);
    store.handle(&request(0)).unwrap();
    let mut schedule = FaultScheduler::new(10, 2).unwrap();
    schedule
        .push(0, 0, request(1), Fault::RestartBefore)
        .unwrap();
    schedule.run(&mut store, &owner).unwrap();
    assert_eq!(store.tips.len(), 2);
    let mut reference = engine();
    reference.handle(&request(0)).unwrap();
    reference.handle(&request(1)).unwrap();
    assert_eq!(root(&store), root(&reference));
}

#[test]
fn mixed_fault_campaign_records_reproducible_healing_evidence() {
    let mut rows = Vec::new();
    let mut orderings = std::collections::HashSet::new();
    let mut reference = engine();
    for id in 0..8 {
        assert_eq!(reference.handle(&request(id)).unwrap().status, 200);
    }
    let expected = root(&reference);
    for seed in 0..32 {
        let mut paired = Vec::new();
        for _ in 0..2 {
            let directory = tempfile::tempdir().unwrap();
            let owner = identity();
            let mut store = KnotEngine::open(directory.path(), &owner.did_string()).unwrap();
            store.now = 1_700_000_000;
            store.register_identity(&owner);
            let mut scheduler = FaultScheduler::new(seed, 16).unwrap();
            for id in 0..8 {
                let fault = match id % 5 {
                    0 => Fault::Pass,
                    1 => Fault::Drop,
                    2 => Fault::Duplicate,
                    3 => Fault::Delay(2),
                    _ => Fault::PartitionUntil(3),
                };
                scheduler
                    .push(id, 0, request(id as usize), fault.clone())
                    .unwrap();
                if fault == Fault::Drop {
                    scheduler
                        .push(100 + id, 4, request(id as usize), Fault::Pass)
                        .unwrap();
                }
            }
            let query = KnotRequest {
                method: Method::Get,
                path: "/xrpc/tangled.sync.getRepo".into(),
                query: vec![("did".into(), owner.did_string())],
                headers: vec![],
                body: None,
                content_type: None,
            };
            scheduler
                .push(200, 5, query.clone(), Fault::RestartBefore)
                .unwrap();
            scheduler
                .push(201, 6, query, Fault::CorruptResponse)
                .unwrap();
            let run = scheduler.run(&mut store, &owner).unwrap();
            assert!(run
                .responses
                .iter()
                .all(|(_, response)| response.status == 200));
            let good = &run
                .responses
                .iter()
                .find(|(id, _)| *id == 200)
                .unwrap()
                .1
                .body;
            let corrupt = &run
                .responses
                .iter()
                .find(|(id, _)| *id == 201)
                .unwrap()
                .1
                .body;
            assert!(car::decode(good).is_ok());
            assert!(car::decode(corrupt).is_err());
            assert_eq!(root(&store), expected);
            assert_eq!(store.tips.len(), 8);
            assert_eq!(run.restarts, 1);
            assert!(run.deliveries <= 24);
            orderings.insert(run.trace.iter().map(|e| e.id).collect::<Vec<_>>());
            paired.push(run);
        }
        assert_eq!(paired[0].trace, paired[1].trace);
        rows.push(
            serde_json::json!({"seed":seed,"repeat_equal":true,"final_root":expected,
            "records":8,"corrupt_car_rejected":true,"run":paired[0]}),
        );
    }
    assert!(orderings.len() >= 2);
    let metrics = serde_json::json!({"baseline_sha":"6baf075a88c178c75e266fe88f8d2bacedfe262c",
        "seeds":32,"repetitions_per_seed":2,"distinct_orders":orderings.len(),
        "healed_roots_equal":32,"paired_traces_equal":32,"corrupt_car_rejected":64,"rows":rows});
    if let Ok(path) = std::env::var("TANGLED_SIMULATION_METRICS") {
        std::fs::write(path, serde_json::to_vec_pretty(&metrics).unwrap()).unwrap();
    }
}

#[test]
fn scheduler_rejects_duplicate_ids_and_more_than_hard_event_limit() {
    assert!(FaultScheduler::new(1, 4097).is_err());
    let mut scheduler = FaultScheduler::new(1, 2).unwrap();
    scheduler.push(1, 0, request(0), Fault::Pass).unwrap();
    assert!(scheduler.push(1, 1, request(1), Fault::Pass).is_err());
    scheduler.push(2, 0, request(1), Fault::Pass).unwrap();
    assert!(scheduler.push(3, 0, request(2), Fault::Pass).is_err());
}
