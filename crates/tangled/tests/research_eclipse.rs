use tangled::{
    peers::{PeerBook, MAX_ATTEMPTS, MAX_PEERS},
    KnotEngine,
};

#[test]
fn poison_is_bounded_and_confirmed_honest_survives_restart() {
    let honest = "https://8.8.8.8";
    let mut book = PeerBook::default();
    book.succeeded(honest, "https://1.1.1.1");
    for n in 0..1000 {
        book.learn(
            "https://9.9.9.9",
            &[format!("https://11.{}.{}.1", n / 256, n % 256)],
        );
    }
    let mut book: PeerBook = serde_json::from_slice(&serde_json::to_vec(&book).unwrap()).unwrap();
    assert!(book.urls().contains(&honest.to_string()));
    assert!(book.urls().len() <= 9);
    assert!(book.urls().len() <= MAX_PEERS);
    let attempts = book.select(&["https://1.1.1.1".into()]);
    assert!(attempts.len() <= MAX_ATTEMPTS);
    assert!(attempts.iter().any(|p| p.url == honest));
    assert!(attempts.iter().any(|p| p.configured));
}

#[test]
fn many_sources_cannot_fill_one_network_and_exploration_rotates() {
    let mut book = PeerBook::default();
    for n in 0..100 {
        book.learn(
            &format!("https://{}.1.1.1", 20 + n),
            &[format!("https://11.1.1.{}", n + 1)],
        );
    }
    assert!(book.urls().len() <= 4);
    let a = book.select(&[]);
    let b = book.select(&[]);
    assert_ne!(a, b);
}

#[test]
fn engine_legacy_learning_is_bounded() {
    let mut engine = KnotEngine::hosting("did:example:research");
    engine.learn_peers(
        &(0..10000)
            .map(|n| format!("https://11.{}.{}.1", n / 256, n % 256))
            .collect::<Vec<_>>(),
    );
    assert!(engine.learned_peers().len() <= 8);
}

#[test]
fn failures_and_cursor_survive_restart_and_late_honest_can_enter() {
    let mut book = PeerBook::default();
    for n in 0..1000 {
        book.learn(
            "https://9.9.9.9",
            &[format!("https://11.{}.{}.1", n / 256, n % 256)],
        );
    }
    book.learn("https://9.9.9.9", &["https://8.8.8.8".into()]);
    assert!(book.urls().contains(&"https://8.8.8.8".into()));
    book.succeeded("https://8.8.8.8", "https://9.9.9.9");
    for _ in 0..3 {
        book.failed("https://8.8.8.8");
        book = serde_json::from_slice::<PeerBook>(&serde_json::to_vec(&book).unwrap())
            .unwrap()
            .sanitize();
    }
    assert!(!book.urls().contains(&"https://8.8.8.8".into()));
}

#[test]
fn many_source_churn_has_global_bound_and_network_quotas() {
    let mut book = PeerBook::default();
    for n in 0..2000 {
        book.learn(
            &format!("https://20.{}.{}.1", n / 256, n % 256),
            &[format!("https://21.{}.{}.1", n / 256, n % 256)],
        );
        assert!(book.urls().len() <= MAX_PEERS);
    }
    let attempts = book.select(
        &(0..100)
            .map(|n| format!("https://22.{}.1.1", n))
            .collect::<Vec<_>>(),
    );
    assert!(attempts.len() <= MAX_ATTEMPTS);
    assert!(attempts.iter().any(|p| p.configured));
    assert!(attempts.iter().any(|p| !p.configured));
}

#[test]
fn engine_persists_confirmed_peers_and_migrates_legacy_config() {
    let dir = std::env::temp_dir().join(format!("tangled-peer-test-{}", std::process::id()));
    let mut e = KnotEngine::open(&dir, "did:example:migration").unwrap();
    e.learn_peers_from("https://1.1.1.1", &["https://8.8.8.8".into()]);
    e.peer_succeeded("https://8.8.8.8");
    drop(e);
    let mut e = KnotEngine::open(&dir, "did:example:migration").unwrap();
    assert!(e
        .select_peers(&[])
        .iter()
        .any(|a| a.url == "https://8.8.8.8"));
    std::fs::remove_file(dir.join("peers-state.json")).unwrap();
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("config.json")).unwrap()).unwrap();
    config["learned_peers"] =
        serde_json::json!(["https://8.8.8.8", "https://8.8.8.8", "file:///secret"]);
    std::fs::write(
        dir.join("config.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    let e = KnotEngine::open(&dir, "did:example:migration").unwrap();
    assert_eq!(e.learned_peers(), &["https://8.8.8.8"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn corrupt_and_oversized_optional_peer_cache_does_not_block_repo_startup() {
    let dir = std::env::temp_dir().join(format!("tangled-peer-corrupt-{}", std::process::id()));
    drop(KnotEngine::open(&dir, "did:example:corrupt").unwrap());
    for bad in [b"{".to_vec(), vec![b' '; 1_048_576]] {
        std::fs::write(dir.join("peers-state.json"), bad).unwrap();
        let mut e = KnotEngine::open(&dir, "did:example:corrupt").unwrap();
        assert!(e.learned_peers().is_empty());
        assert!(e
            .select_peers(&["https://8.8.8.8".into()])
            .iter()
            .any(|a| a.configured));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn configured_origin_permission_follows_seed_when_selected_as_good() {
    let mut book = PeerBook::default();
    let seeds: Vec<_> = (0..16)
        .map(|n| format!("https://{}.1.1.1", n + 30))
        .collect();
    for url in &seeds {
        book.succeeded(url, url);
    }
    let attempts = book.select(&seeds);
    assert!(attempts.len() <= MAX_ATTEMPTS);
    assert!(attempts.iter().all(|a| a.configured));
}
