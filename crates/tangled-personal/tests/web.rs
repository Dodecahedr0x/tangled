use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};

struct Server {
    child: Child,
    base: String,
    token: String,
}
impl Server {
    fn start(data: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tangled-personal"))
            .args([
                "serve",
                "--data",
                data.to_str().unwrap(),
                "--listen",
                "127.0.0.1:0",
                "--no-open",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert!(
            line.starts_with("http://127.0.0.1:"),
            "server did not print startup URL: {line}"
        );
        let (base, token) = line.trim().split_once("/#session=").unwrap();
        assert_eq!(token.len(), 64);
        Self {
            child,
            base: base.into(),
            token: token.into(),
        }
    }
    async fn post(&self, client: &Client, path: &str, body: Value) -> reqwest::Response {
        client
            .post(format!("{}{path}", self.base))
            .header("X-Tangled-Session", &self.token)
            .json(&body)
            .send()
            .await
            .unwrap()
    }
    async fn session(&self, client: &Client) -> Value {
        client
            .get(format!("{}/api/v1/session", self.base))
            .header("X-Tangled-Session", &self.token)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[tokio::test]
async fn actual_listener_guards_public_assets_and_owner_data() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::start(dir.path());
    let client = Client::new();
    for (path, mime) in [
        ("/", "text/html"),
        ("/app.js", "text/javascript"),
        ("/ui.js", "text/javascript"),
        ("/routes.js", "text/javascript"),
        ("/today.js", "text/javascript"),
        ("/library.js", "text/javascript"),
        ("/reading.js", "text/javascript"),
        ("/manage.js", "text/javascript"),
        ("/app.css", "text/css"),
    ] {
        let response = client
            .get(format!("{}{path}", server.base))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with(mime));
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert!(!response.text().await.unwrap().contains(&server.token));
    }
    let manifest: Value = client
        .get(format!("{}/api/v1/manifest", server.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(manifest["apiVersion"], "v1");
    assert_eq!(
        manifest["app"]["recordLinkTemplate"],
        "/#library?item={uri}"
    );
    assert_eq!(
        manifest["app"]["uriEncoding"],
        "percent-encoded Tangled URI"
    );
    assert_eq!(manifest["app"]["defaultView"], "today");
    assert!(manifest["collections"].as_array().unwrap().len() >= 10);
    assert!(!manifest.to_string().contains(&server.token));
    assert!(!manifest
        .to_string()
        .contains(server.session(&client).await["did"].as_str().unwrap()));
    let session_url = format!("{}/api/v1/session", server.base);
    for path in [
        "/api/v1/session",
        "/api/v1/personal/records",
        "/api/v1/backups/export",
    ] {
        let response = client
            .post(format!("{}{path}", server.base))
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response.json::<Value>().await.unwrap()["error"].is_string());
    }
    assert_eq!(
        client.get(&session_url).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(&session_url)
            .header("X-Tangled-Session", "wrong")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    for origin in ["https://evil.example", "null", "http://localhost:1234"] {
        assert_eq!(
            client
                .get(&session_url)
                .header("X-Tangled-Session", &server.token)
                .header("Origin", origin)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        client
            .get(&session_url)
            .header("X-Tangled-Session", &server.token)
            .header("Origin", &server.base)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    for host in ["evil.example", "localhost", "127.0.0.1:1"] {
        assert_eq!(
            client
                .get(&session_url)
                .header("X-Tangled-Session", &server.token)
                .header("Host", host)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        client
            .get(&session_url)
            .header("X-Tangled-Session", &server.token)
            .header("Sec-Fetch-Site", "cross-site")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = client
        .request(reqwest::Method::OPTIONS, &session_url)
        .header("Origin", "https://evil.example")
        .send()
        .await
        .unwrap();
    assert!(!response
        .headers()
        .contains_key("access-control-allow-origin"));
    let response = client
        .post(format!("{}/api/v1/personal/notes", server.base))
        .header("X-Tangled-Session", &server.token)
        .header("Content-Type", "application/json")
        .body("{")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(response.json::<Value>().await.unwrap()["error"].is_string());
    let response = client
        .post(format!("{}/api/v1/personal/notes", server.base))
        .header("X-Tangled-Session", &server.token)
        .header(
            "Content-Length",
            (tangled_personal::web::MAX_REQUEST_BYTES + 1).to_string(),
        )
        .body("")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(response.json::<Value>().await.unwrap()["error"].is_string());
    assert_eq!(
        server
            .post(
                &client,
                "/api/v1/personal/devices",
                json!({"action":"backup","output_path":"/tmp/forbidden"})
            )
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn third_party_workflows_persist_and_recovery_uses_uploaded_packets() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::start(dir.path());
    let client = Client::new();
    let original = server.session(&client).await;
    let note: Value = server.post(&client, "/api/v1/personal/notes", json!({"action":"create","rkey":"one","body":{"title":"Saved from another client","text":"private material","extension":{"keep":true}}})).await.error_for_status().unwrap().json().await.unwrap();
    assert!(note["cid"].is_string());
    let result: Value = server
        .post(
            &client,
            "/api/v1/personal/memory",
            json!({"action":"search","query":"private material"}),
        )
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(result["total"], 1);
    assert!(
        tangled_personal::Store::open(dir.path()).is_err(),
        "server must retain exclusive Store lock"
    );
    let pass = "a separate long recovery passphrase";
    let exported: Value = server
        .post(
            &client,
            "/api/v1/backups/export",
            json!({"passphrase":pass}),
        )
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(exported["filename"].as_str().unwrap().ends_with(".json"));
    let backup = exported["backup"].clone();
    assert!(backup.is_object());
    assert!(!backup.to_string().contains("private material"));
    assert!(!exported.to_string().contains(pass));
    let verified: Value = server
        .post(
            &client,
            "/api/v1/backups/verify",
            json!({"passphrase":pass,"backup":backup}),
        )
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(verified["records"], 1);
    assert_eq!(
        server
            .post(
                &client,
                "/api/v1/backups/sync",
                json!({"passphrase":pass,"backup":backup})
            )
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(server.post(&client, "/api/v1/backups/restore", json!({"passphrase":"incorrect but long password","backup":backup,"label":"Recovered"})).await.status(), StatusCode::BAD_REQUEST);
    assert_eq!(server.session(&client).await["data"], original["data"]);
    assert_eq!(
        server
            .post(
                &client,
                "/api/v1/backups/restore",
                json!({"passphrase":pass,"backup":backup,"label":"../../escape"})
            )
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let restored: Value = server
        .post(
            &client,
            "/api/v1/backups/restore",
            json!({"passphrase":pass,"backup":backup,"label":"Recovered"}),
        )
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(restored["session"]["did"], original["did"]);
    assert_ne!(restored["session"]["data"], original["data"]);
    let restored_path = std::path::PathBuf::from(restored["session"]["data"].as_str().unwrap());
    assert!(restored_path.starts_with(dir.path().canonicalize().unwrap()));
    assert_eq!(
        server.session(&client).await["data"],
        restored["session"]["data"]
    );
    assert!(tangled_personal::Store::open(dir.path()).is_err());
    assert!(tangled_personal::Store::open(&restored_path).is_err());
    drop(server);
    let mut old = tangled_personal::Store::open(dir.path()).unwrap();
    assert_eq!(
        old.get("pub.tangled.note", "one").unwrap()["body"]["extension"]["keep"],
        true
    );
    drop(old);
    let restarted = Server::start(&restored_path);
    let fetched: Value = restarted
        .post(
            &client,
            "/api/v1/personal/records",
            json!({"action":"get","collection":"pub.tangled.note","rkey":"one"}),
        )
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(fetched["cid"], note["cid"]);
}

#[tokio::test]
async fn owner_agent_client_uses_signed_scoped_invocations_and_persistent_receipts() {
    use tangled::{
        capability::{Capability, Invocation},
        Identity,
    };
    let dir = tempfile::tempdir().unwrap();
    let server = Server::start(dir.path());
    let client = Client::new();
    let session = server.session(&client).await;
    let owner = session["did"].as_str().unwrap();
    let agent = Identity::generate();
    let created: Value = server
        .post(
            &client,
            "/api/v1/personal/notes",
            json!({"action":"create","rkey":"source","body":{"text":"orchid study"}}),
        )
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let grant: Value = server.post(&client, "/api/v1/personal/agents", json!({"action":"grant","audience":agent.did_string(),"collection":"pub.tangled.note","access":"read","ttl_seconds":3600})).await.error_for_status().unwrap().json().await.unwrap();
    let cap = Capability::from_json(&grant["capability"]).unwrap();
    let mut args = json!({"action":"search","collection":"pub.tangled.note","query":"orchid"});
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut invocation = Invocation::new(
        &cap,
        &cap.cmd,
        &format!("tangled://{owner}/pub.tangled.note"),
        owner,
        now,
        [7; 16],
    )
    .unwrap();
    invocation.args = args.clone();
    invocation.sign(agent.signing_key()).unwrap();
    args["invocation"] = json!(hex::encode(invocation.encode_cbor().unwrap()));
    let result: Value = server
        .post(&client, "/api/v1/personal/agents", args.clone())
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(result["records"][0]["cid"], created["cid"]);
    assert_eq!(result["receipt"]["body"]["actor"], agent.did_string());
    assert_eq!(
        server
            .post(&client, "/api/v1/personal/agents", args.clone())
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    args["collection"] = json!("pub.tangled.sourceItem");
    assert_eq!(
        server
            .post(&client, "/api/v1/personal/agents", args)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    drop(server);
    let restarted = Server::start(dir.path());
    let report: Value = restarted
        .post(
            &client,
            "/api/v1/personal/agents",
            json!({"action":"report"}),
        )
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(report["activities"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn chunked_body_limits_font_discovery_and_non_loopback_rejection() {
    use std::io::{Read, Write};
    let dir = tempfile::tempdir().unwrap();
    let server = Server::start(dir.path());
    let client = Client::new();
    let response = client
        .get(format!("{}/assets/fira-sans.woff2", server.base))
        .send()
        .await
        .unwrap();
    assert_eq!(response.headers()["content-type"], "font/woff2");
    assert!(response.bytes().await.unwrap().starts_with(b"wOF2"));
    let schema: Value = client
        .get(format!("{}/api/v1/schemas/pub.tangled.note", server.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(schema["title"], "pub.tangled.note");
    assert_eq!(
        client
            .get(format!(
                "{}/api/v1/session?session={}",
                server.base, server.token
            ))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .post(format!("{}/api/v1/personal/notes", server.base))
            .header("X-Tangled-Session", &server.token)
            .body("{}")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    let host = server.base.strip_prefix("http://").unwrap();
    let mut tcp = std::net::TcpStream::connect(host).unwrap();
    tcp.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    tcp.set_write_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    write!(tcp, "POST /api/v1/personal/notes HTTP/1.1\r\nHost: {host}\r\nX-Tangled-Session: {}\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n", server.token).unwrap();
    let chunk = vec![b' '; 1024 * 1024];
    for _ in 0..64 {
        tcp.write_all(b"100000\r\n").unwrap();
        tcp.write_all(&chunk).unwrap();
        tcp.write_all(b"\r\n").unwrap();
    }
    tcp.write_all(b"1\r\n \r\n0\r\n\r\n").unwrap();
    let mut response = String::new();
    tcp.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 413"), "{response}");
    let rejected = Command::new(env!("CARGO_BIN_EXE_tangled-personal"))
        .args([
            "serve",
            "--data",
            dir.path().join("outside").to_str().unwrap(),
            "--listen",
            "0.0.0.0:0",
            "--no-open",
        ])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8(rejected.stderr)
        .unwrap()
        .contains("loopback"));
    assert!(!dir.path().join("outside").exists());
}
