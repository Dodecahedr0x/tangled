use axum::{response::Redirect, routing::get, Router};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tangled::KnotEngine;
use tangled_knot::{
    bind_ephemeral,
    discovery::{addresses_allowed, describe_peer, public_ip},
    gossip_configured_peers,
};

#[test]
fn forbidden_destinations_and_mixed_dns_fail_closed() {
    for ip in [
        "127.0.0.1",
        "10.0.0.1",
        "169.254.169.254",
        "0.0.0.0",
        "100.64.0.1",
        "192.0.2.1",
        "198.18.0.1",
        "224.0.0.1",
        "240.0.0.1",
        "::1",
        "::",
        "::ffff:127.0.0.1",
        "fd00::1",
        "fe80::1",
        "ff02::1",
        "2001:db8::1",
        "2002:7f00::1",
    ] {
        assert!(!public_ip(ip.parse().unwrap()), "{ip}");
    }
    assert!(public_ip("8.8.8.8".parse().unwrap()));
    assert!(public_ip("2606:4700:4700::1111".parse().unwrap()));
    assert!(!addresses_allowed(&[]));
    assert!(!addresses_allowed(&[
        "8.8.8.8:443".parse().unwrap(),
        "127.0.0.1:443".parse().unwrap()
    ]));
}

#[tokio::test]
async fn explicit_local_seed_works_but_learned_local_does_not() {
    let (addr, _) = bind_ephemeral(KnotEngine::hosting("did:example:seed"), false)
        .await
        .unwrap();
    let base = format!("http://{addr}");
    assert!(describe_peer(&base, false).await.is_err());
    assert!(describe_peer(&base, true).await.is_ok());
    for url in [
        "file:///etc/passwd",
        "https://u:p@example.com",
        "https://example.com/path",
        "https://example.com/?x=1",
        "https://example.com/#fragment",
    ] {
        assert!(describe_peer(url, false).await.is_err());
    }
}

#[tokio::test]
async fn redirects_are_never_followed_even_for_configured_seed() {
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route(
            "/xrpc/tangled.node.describe",
            get(|| async { Redirect::temporary("/secret") }),
        )
        .route(
            "/secret",
            get(move || {
                let h = h.clone();
                async move {
                    h.fetch_add(1, Ordering::SeqCst);
                    "secret"
                }
            }),
        );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    assert!(describe_peer(&format!("http://{addr}"), true)
        .await
        .is_err());
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    server.abort();
}

#[tokio::test]
async fn response_bytes_and_crawl_attempts_are_bounded() {
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new().route(
        "/xrpc/tangled.node.describe",
        get(move || {
            let h = h.clone();
            async move {
                h.fetch_add(1, Ordering::SeqCst);
                "x".repeat(65537)
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let base = format!("http://{addr}");
    assert!(describe_peer(&base, true).await.is_err());
    let engine = std::sync::Mutex::new(KnotEngine::hosting("did:example:local"));
    let seeds = vec![base; 1000];
    let before = hits.load(Ordering::SeqCst);
    gossip_configured_peers(&engine, &seeds).await;
    assert!(hits.load(Ordering::SeqCst) - before <= 8);
    server.abort();
}

#[tokio::test]
async fn loopback_advertisement_never_enables_learned_loopback_by_default() {
    let hits = Arc::new(AtomicUsize::new(0));
    let observed = hits.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().route(
        "/xrpc/tangled.node.describe",
        get(move || {
            let observed = observed.clone();
            async move {
                observed.fetch_add(1, Ordering::SeqCst);
                axum::Json(KnotEngine::hosting("did:example:learned").describe.unwrap())
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let learned = format!("http://{address}");
    for advertised in [
        "http://localhost:3000",
        "http://127.0.0.1:3000",
        "http://[::1]:3000",
    ] {
        let mut engine = KnotEngine::hosting("did:example:local").with_knot_base(advertised);
        engine.learn_peers_from("https://seed.example", std::slice::from_ref(&learned));
        gossip_configured_peers(&std::sync::Mutex::new(engine), &[]).await;
    }
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "advertised origin must not grant development privileges"
    );
    server.abort();
}

#[test]
fn development_options_reject_lan_metadata_and_mixed_dns() {
    use tangled_knot::discovery::{addresses_allowed_with_options, DiscoveryOptions};
    let options = DiscoveryOptions {
        allow_learned_loopback: true,
    };
    for addresses in [
        vec!["10.0.0.1:80"],
        vec!["169.254.169.254:80"],
        vec!["[fd00::1]:80"],
        vec!["127.0.0.1:80", "10.0.0.1:80"],
        vec!["8.8.8.8:80", "169.254.169.254:80"],
        vec!["8.8.8.8:80", "127.0.0.1:80"],
    ] {
        let addresses: Vec<std::net::SocketAddr> =
            addresses.into_iter().map(|a| a.parse().unwrap()).collect();
        assert!(
            !addresses_allowed_with_options(&addresses, options),
            "{addresses:?}"
        );
    }
    let loopback = ["127.0.0.1:80".parse().unwrap(), "[::1]:80".parse().unwrap()];
    assert!(addresses_allowed_with_options(&loopback, options));
    assert!(!addresses_allowed(&loopback));
    assert!(!addresses_allowed_with_options(&[], options));
    assert!(!addresses_allowed_with_options(
        &vec![loopback[0]; 33],
        options
    ));
}

#[tokio::test]
async fn explicit_development_option_is_required_for_local_fixture_crawls() {
    use tangled_knot::{
        discovery::{describe_peer_with_options, DiscoveryOptions},
        gossip_configured_peers_with_options,
    };
    let (address, _) = bind_ephemeral(KnotEngine::hosting("did:example:local-fixture"), false)
        .await
        .unwrap();
    let learned = format!("http://{address}");
    let options = DiscoveryOptions {
        allow_learned_loopback: true,
    };
    assert!(describe_peer(&learned, false).await.is_err());
    assert!(describe_peer_with_options(&learned, false, options)
        .await
        .is_ok());
    for forbidden in [
        "http://10.0.0.1",
        "http://169.254.169.254",
        "http://[fd00::1]",
    ] {
        let error = describe_peer_with_options(forbidden, false, options)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("forbidden destination"),
            "{error}"
        );
    }
    let mut engine =
        KnotEngine::hosting("did:example:crawler").with_knot_base("https://public.example");
    engine.learn_peers_from("https://seed.example", std::slice::from_ref(&learned));
    let engine = std::sync::Mutex::new(engine);
    gossip_configured_peers_with_options(&engine, &[], options).await;
    let attempts = engine.lock().unwrap().select_peers(&[]);
    assert!(attempts.iter().any(|a| a.url == learned && !a.configured));
    // The strict helper must not retain permission from a previous opted-in round.
    for _ in 0..3 {
        gossip_configured_peers(&engine, &[]).await;
    }
    assert!(engine.lock().unwrap().select_peers(&[]).is_empty());
}

#[test]
fn development_flag_defaults_off_and_requires_explicit_configuration() {
    use tangled_knot::KnotCli;
    let default = KnotCli::parse(std::iter::empty::<&str>()).unwrap();
    assert!(!default.allow_learned_loopback);
    for advertised in [
        "http://localhost:3000",
        "http://127.0.0.1:3000",
        "http://[::1]:3000",
    ] {
        let cli = KnotCli::parse(["--public-url", advertised]).unwrap();
        assert!(!cli.allow_learned_loopback);
    }
    let opted_in = KnotCli::parse(["--allow-learned-loopback"]).unwrap();
    assert!(opted_in.allow_learned_loopback);
}

#[tokio::test]
async fn development_option_keeps_redirects_disabled_and_streams_bounded() {
    use tangled_knot::discovery::{describe_peer_with_options, DiscoveryOptions};
    let hits = Arc::new(AtomicUsize::new(0));
    let observed = hits.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route(
            "/xrpc/tangled.node.describe",
            get(|| async { Redirect::temporary("/secret") }),
        )
        .route(
            "/secret",
            get(move || {
                let observed = observed.clone();
                async move {
                    observed.fetch_add(1, Ordering::SeqCst);
                    "secret"
                }
            }),
        );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let options = DiscoveryOptions {
        allow_learned_loopback: true,
    };
    assert!(
        describe_peer_with_options(&format!("http://{address}"), false, options)
            .await
            .is_err()
    );
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    server.abort();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().route(
        "/xrpc/tangled.node.describe",
        get(|| async {
            axum::body::Body::from_stream(futures_util::stream::iter([
                Ok::<_, std::io::Error>(vec![b'x'; 32_768]),
                Ok(vec![b'x'; 32_769]),
            ]))
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let error = describe_peer_with_options(&format!("http://{address}"), false, options)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("describe too large"), "{error}");
    server.abort();
}
