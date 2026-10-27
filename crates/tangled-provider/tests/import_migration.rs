use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::{header, Client, StatusCode};
use std::{path::Path, sync::Arc, time::Duration};
use tangled_provider::{router, Provider};

fn spawn(dir: &Path) -> String {
    spawn_with_failed_ack(dir, false)
}

fn spawn_with_failed_ack(dir: &Path, fail_first_ack: bool) -> String {
    let dir = dir.to_owned();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let url = format!("http://{}", listener.local_addr().unwrap());
                let provider = Provider::open(dir, &url).unwrap();
                let reject = Arc::new(std::sync::atomic::AtomicBool::new(fail_first_ack));
                let app = router(provider).layer(axum::middleware::from_fn(
                    move |request: axum::extract::Request, next: axum::middleware::Next| {
                        let reject = Arc::clone(&reject);
                        async move {
                            if request.uri().path().ends_with("/ack")
                                && reject.swap(false, std::sync::atomic::Ordering::SeqCst)
                            {
                                return axum::response::IntoResponse::into_response(
                                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                                );
                            }
                            next.run(request).await
                        }
                    },
                ));
                tx.send(url).unwrap();
                axum::serve(listener, app).await.unwrap();
            });
    });
    rx.recv().unwrap()
}
fn session(response: &reqwest::Response) -> String {
    response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .into()
}

async fn start(client: &Client, host: &str, cookie: &str) -> String {
    let response = client
        .post(format!("{host}/migrate/start"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let html = client
        .get(format!("{host}/move"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    html.split("id=\"move-code\">")
        .nth(1)
        .unwrap()
        .split('<')
        .next()
        .unwrap()
        .into()
}
#[tokio::test]
async fn private_archive_and_original_survive_two_location_moves() {
    let dirs = [
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
    ];
    let hosts = dirs.iter().map(|dir| spawn(dir.path())).collect::<Vec<_>>();
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap();
    let response = client
        .post(format!("{}/signup", hosts[0]))
        .form(&[
            ("email", "private@example.com"),
            ("name", "Private"),
            ("password", "first-password"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let mut cookie = session(&response);
    let original = br#"{"personal_information":{"user_id":123},"chats":{"list":[{"id":7,"name":"Secret","type":"personal_chat","messages":[{"id":42,"type":"message","date":"2024-01-02T03:04:05","from":"Ada","from_id":"user5","text":"private migration message"}]}]}}"#;
    let response = client.post(format!("{}/api/imports/upload",hosts[0])).header(header::COOKIE,&cookie).json(&serde_json::json!({"source":"telegram","account":"123","filename":"result.json","data_base64":STANDARD.encode(original)})).send().await.unwrap();
    assert!(
        response.status().is_success(),
        "upload failed: {}",
        response.text().await.unwrap()
    );
    let receipt: serde_json::Value = response.json().await.unwrap();
    let batch = receipt["id"].as_str().unwrap();
    let connected: serde_json::Value = client
        .post(format!("{}/api/imports/connections", hosts[0]))
        .header(header::COOKIE, &cookie)
        .json(&serde_json::json!({"source":"telegram","account":"123"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let original_seeds: (Vec<u8>, Vec<u8>) =
        rusqlite::Connection::open(dirs[0].path().join("accounts.db"))
            .unwrap()
            .query_row("SELECT ed_seed,x_seed FROM accounts", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
    for index in 0..2 {
        let code = start(&client, &hosts[index], &cookie).await;
        let response = client
            .post(format!("{}/move", hosts[index + 1]))
            .form(&[
                ("knot", hosts[index].as_str()),
                ("code", code.as_str()),
                ("password", "next-password"),
            ])
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::SEE_OTHER,
            "{}",
            response.text().await.unwrap()
        );
        cookie = session(&response);
        let browse = client
            .get(format!("{}/api/imports", hosts[index + 1]))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert!(browse.status().is_success());
        let state: serde_json::Value = browse.json().await.unwrap();
        assert!(state["objects"]
            .to_string()
            .contains("private migration message"));
        assert_eq!(state["connections"].as_array().unwrap().len(), 0);
        if index == 0 {
            let denied = client.post(format!("{}/api/imports/connections/{}/ingest",hosts[0],connected["connection"]["id"].as_str().unwrap()))
                .bearer_auth(connected["token"].as_str().unwrap())
                .json(&serde_json::json!({"source":"telegram","account":"123","objects":[],"files":[],"warnings":[]}))
                .send().await.unwrap();
            assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
        }
        let download = client
            .get(format!("{}/api/imports/original/{batch}", hosts[index + 1]))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert!(download.status().is_success());
        assert_eq!(download.bytes().await.unwrap().as_ref(), original);
        let conn = rusqlite::Connection::open(dirs[index].path().join("accounts.db")).unwrap();
        let retired: (i64, Vec<u8>, Vec<u8>) = conn
            .query_row("SELECT custodial,ed_seed,x_seed FROM accounts", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(retired, (0, vec![0; 32], vec![0; 32]));
        let destination =
            rusqlite::Connection::open(dirs[index + 1].path().join("accounts.db")).unwrap();
        let preserved: (Vec<u8>, Vec<u8>) = destination
            .query_row("SELECT ed_seed,x_seed FROM accounts", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(preserved, original_seeds);
    }
    let stranger = client
        .post(format!("{}/signup", hosts[2]))
        .form(&[
            ("email", "stranger@example.com"),
            ("name", "Stranger"),
            ("password", "stranger-password"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(stranger.status(), StatusCode::SEE_OTHER);
    let stranger_cookie = session(&stranger);
    let denied = client
        .get(format!("{}/api/imports/original/{batch}", hosts[2]))
        .header(header::COOKIE, &stranger_cookie)
        .send()
        .await
        .unwrap();
    assert!(!denied.status().is_success());
    let stranger_state: serde_json::Value = client
        .get(format!("{}/api/imports", hosts[2]))
        .header(header::COOKIE, &stranger_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(stranger_state["objects"].as_array().unwrap().is_empty());
    assert!(!client
        .get(format!("{}/api/imports/original/{batch}", hosts[2]))
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
}

#[tokio::test]
async fn encrypted_move_is_bound_resumable_and_requires_destination_acknowledgement() {
    let directory = tempfile::tempdir().unwrap();
    let source = spawn(directory.path());
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let signup = client
        .post(format!("{source}/signup"))
        .form(&[
            ("email", "prepare@example.com"),
            ("name", "Prepare"),
            ("password", "first-password"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::SEE_OTHER);
    let cookie = session(&signup);
    let code = start(&client, &source, &cookie).await;
    let info: serde_json::Value = client
        .get(format!("{source}/migrate/{code}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let destination = tangled::identity::Identity::generate();
    let request = serde_json::json!({
        "protocol":2, "knot":"https://destination.example",
        "pubkey":hex::encode(destination.pubkey_bytes()),
        "agreement":hex::encode(destination.agreement_public_bytes())
    });
    let prepared = client
        .post(format!("{source}/migrate/{code}"))
        .json(&request)
        .send()
        .await
        .unwrap();
    assert!(
        prepared.status().is_success(),
        "{}",
        prepared.text().await.unwrap()
    );
    let prepared: serde_json::Value = prepared.json().await.unwrap();
    assert!(prepared.get("ed_seed").is_none());
    assert!(prepared["archive"].get("backup").is_none());
    let conn = rusqlite::Connection::open(directory.path().join("accounts.db")).unwrap();
    let retained: (i64, Vec<u8>) = conn
        .query_row("SELECT custodial,ed_seed FROM accounts", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(retained.0, 0, "Prepared source must stop signing.");
    assert_ne!(
        retained.1,
        vec![0; 32],
        "Source must retain recovery custody until destination persistence."
    );
    let denied = client
        .post(format!("{source}/migrate/{code}/ack"))
        .json(&serde_json::json!({"acknowledgement":"wrong"}))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let mut other = request.clone();
    other["agreement"] =
        hex::encode(tangled::identity::Identity::generate().agreement_public_bytes()).into();
    assert_eq!(
        client
            .post(format!("{source}/migrate/{code}"))
            .json(&other)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    // Reopening the same source storage reproduces the already sealed handoff,
    // without relying on in-memory source custody or changing destination keys.
    let reopened = spawn(directory.path());
    let replay: serde_json::Value = client
        .post(format!("{reopened}/migrate/{code}"))
        .json(&request)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(replay, prepared);
    let binding = serde_json::to_vec(&serde_json::json!([
        "tangled.provider.archive-move.v2",
        info["did"],
        code,
        request["knot"],
        request["pubkey"],
        request["agreement"]
    ]))
    .unwrap();
    let plaintext = tangled::hpke::open_record(
        &destination.to_seeds().1,
        &hex::decode(prepared["archive"]["enc"].as_str().unwrap()).unwrap(),
        &binding,
        &hex::decode(prepared["archive"]["ciphertext"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    let handoff: serde_json::Value = serde_json::from_slice(&plaintext).unwrap();
    assert_eq!(
        hex::decode(handoff["ed_seed"].as_str().unwrap()).unwrap(),
        retained.1
    );
    assert_eq!(handoff["did"], info["did"]);
    let acknowledgement = serde_json::json!({"acknowledgement":handoff["acknowledgement"]});
    let acknowledged = client
        .post(format!("{reopened}/migrate/{code}/ack"))
        .json(&acknowledgement)
        .send()
        .await
        .unwrap();
    assert_eq!(acknowledged.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        client
            .post(format!("{reopened}/migrate/{code}/ack"))
            .json(&acknowledgement)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    let wiped: Vec<u8> = conn
        .query_row("SELECT ed_seed FROM accounts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(wiped, vec![0; 32]);
}

#[tokio::test]
async fn interrupted_ack_resumes_only_after_destination_password_verification() {
    let source_dir = tempfile::tempdir().unwrap();
    let destination_dir = tempfile::tempdir().unwrap();
    let source = spawn_with_failed_ack(source_dir.path(), true);
    let destination = spawn(destination_dir.path());
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap();
    let signup = client
        .post(format!("{source}/signup"))
        .form(&[
            ("email", "resume@example.com"),
            ("name", "Resume"),
            ("password", "original-password"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::SEE_OTHER);
    let code = start(&client, &source, &session(&signup)).await;
    let request = [
        ("knot", source.as_str()),
        ("code", code.as_str()),
        ("password", "destination-password"),
    ];
    let interrupted = client
        .post(format!("{destination}/move"))
        .form(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(interrupted.status(), StatusCode::BAD_REQUEST);
    assert!(interrupted
        .text()
        .await
        .unwrap()
        .contains("archive is saved here"));
    let source_db = rusqlite::Connection::open(source_dir.path().join("accounts.db")).unwrap();
    let retained: Vec<u8> = source_db
        .query_row("SELECT ed_seed FROM accounts", [], |row| row.get(0))
        .unwrap();
    assert_ne!(retained, vec![0; 32]);
    let destination_db =
        rusqlite::Connection::open(destination_dir.path().join("accounts.db")).unwrap();
    let persisted: Vec<u8> = destination_db
        .query_row("SELECT ed_seed FROM accounts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(persisted, retained);
    let wrong = client
        .post(format!("{destination}/move"))
        .form(&[
            ("knot", source.as_str()),
            ("code", code.as_str()),
            ("password", "wrong-password"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        source_db
            .query_row("SELECT ed_seed FROM accounts", [], |row| row
                .get::<_, Vec<u8>>(0))
            .unwrap(),
        retained
    );
    let resumed = client
        .post(format!("{destination}/move"))
        .form(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(
        resumed.status(),
        StatusCode::SEE_OTHER,
        "{}",
        resumed.text().await.unwrap()
    );
    assert_eq!(
        source_db
            .query_row("SELECT ed_seed FROM accounts", [], |row| row
                .get::<_, Vec<u8>>(0))
            .unwrap(),
        vec![0; 32]
    );
    assert_eq!(
        destination_db
            .query_row("SELECT count(*) FROM archive_move_receipts", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        0
    );
}
