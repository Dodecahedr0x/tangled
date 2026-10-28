//! Hosted Tangled provider. Listens on `0.0.0.0:$PORT` (Render sets `PORT`).

use std::env;
use std::net::SocketAddr;
use std::process;

use tangled_provider::{router, Provider};

fn main() {
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8787);
    let data = env::var("DATA_DIR").unwrap_or_else(|_| "./data-provider".into());
    let public_url = public_url(port);
    let provider = match Provider::open(&data, &public_url) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("open provider: {e}");
            process::exit(1);
        }
    };
    let app = router(provider);
    let addr: SocketAddr = ([0, 0, 0, 0], port).into();
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    if let Err(e) = runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        eprintln!("tangled-provider {public_url} data {data}");
        println!("tangled-provider listening {addr}");
        axum::serve(listener, app).await
    }) {
        eprintln!("serve: {e}");
        process::exit(1);
    }
}

fn public_url(port: u16) -> String {
    for key in ["PUBLIC_URL", "RENDER_EXTERNAL_URL"] {
        if let Ok(url) = env::var(key) {
            let url = url.trim().trim_end_matches('/').to_string();
            if !url.is_empty() {
                return url;
            }
        }
    }
    format!("http://127.0.0.1:{port}")
}
