//! Knot server CLI: listen address, data directory, identity.

use std::env;
use std::process;

use tangled::KnotEngine;
use tangled_knot::{
    collect_public_peers, discovery::DiscoveryOptions, gossip_configured_peers_with_options,
    load_or_create_identity, KnotCli,
};

fn print_help() {
    eprintln!(
        "tangled-knot — Tangled knot HTTP server\n\
         \n\
         Usage:\n\
           tangled-knot [--listen ADDR] [--data DIR] [--public-url URL] [--peer URL]... [--allow-learned-loopback]\n\
         \n\
         Flags:\n\
           --listen ADDR      bind address (default 127.0.0.1:3000; :0 for ephemeral)\n\
           --data DIR         data directory (default ./data)\n\
           --public-url URL   advertised knot URL in getHead / identity.resolve\n\
           --peer URL         public peer knot URL advertised in node.describe (repeatable)\n\
           --allow-learned-loopback  development only: permit learned loopback peers (default off)\n\
           --help             this message\n\
         \n\
         Also: TANGLED_PEERS (comma or space) and {{data}}/peers.txt (one URL per line).\n\
         Learned peers from other knots are stored locally and never re-advertised.\n"
    );
}

#[tokio::main]
async fn main() {
    let cli = match KnotCli::parse(env::args().skip(1)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            print_help();
            process::exit(2);
        }
    };
    if cli.help {
        print_help();
        return;
    }

    if let Err(e) = std::fs::create_dir_all(&cli.data) {
        eprintln!("create data dir: {e}");
        process::exit(1);
    }
    let identity = match load_or_create_identity(&cli.data) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("identity: {e}");
            process::exit(1);
        }
    };
    let did = identity.did_string();
    let mut engine = match KnotEngine::open(&cli.data, &did) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("open engine: {e}");
            process::exit(1);
        }
    };
    engine.now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let listener = match tokio::net::TcpListener::bind(&cli.listen).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("bind {}: {e}", cli.listen);
            process::exit(1);
        }
    };
    let addr = match listener.local_addr() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("local_addr: {e}");
            process::exit(1);
        }
    };
    let advertised = cli
        .public_url
        .clone()
        .unwrap_or_else(|| format!("http://{addr}"));
    engine.register_identity(&identity);
    engine = engine.with_knot_base(advertised.clone());
    engine.handle_resolver.live = true;
    if let Ok(rpc) = env::var("SOLANA_RPC") {
        engine.handle_resolver.solana_rpc = Some(rpc);
    }
    let public_peers = collect_public_peers(
        &cli.peers,
        &cli.data,
        env::var("TANGLED_PEERS").ok().as_deref(),
    );
    engine.set_public_peers(&public_peers);
    if let Err(e) = engine.persist() {
        eprintln!("persist: {e}");
        process::exit(1);
    }

    eprintln!("tangled-knot did={did}");
    eprintln!("listening on {addr} (advertised {advertised})");
    eprintln!("data {}", cli.data.display());
    if !public_peers.is_empty() {
        eprintln!("public peers {}", public_peers.join(" "));
    }

    let discovery_options = DiscoveryOptions {
        allow_learned_loopback: cli.allow_learned_loopback,
    };
    let state = tangled_knot::AppState::new(engine, true);
    tokio::spawn({
        let engine = state.engine.clone();
        let seeds = public_peers.clone();
        async move {
            loop {
                gossip_configured_peers_with_options(&engine, &seeds, discovery_options).await;
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        }
    });
    if let Err(e) = axum::serve(listener, tangled_knot::router(state)).await {
        eprintln!("serve: {e}");
        process::exit(1);
    }
}
