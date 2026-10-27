use std::sync::Arc;
use std::time::Duration;

use axum::http::header;
use tangled_provider::{router, Provider};
use tempfile::tempdir;

fn spawn(dir: &std::path::Path) -> String {
    let dir = dir.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let provider = Provider::open(&dir, &url).unwrap();
            let app = router(Arc::clone(&provider));
            tx.send(url).unwrap();
            axum::serve(listener, app).await.unwrap();
        });
    });
    rx.recv().unwrap()
}

#[tokio::test]
async fn move_stops_the_old_host_signing() {
    let old_dir = tempdir().unwrap();
    let new_dir = tempdir().unwrap();
    let old = spawn(old_dir.path());
    let new = spawn(new_dir.path());
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    for url in [&old, &new] {
        let mut up = false;
        for _ in 0..50 {
            if client
                .get(format!("{url}/healthz"))
                .send()
                .await
                .is_ok_and(|r| r.status().is_success())
            {
                up = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(up, "provider did not accept connections at {url}");
    }

    let signup = client
        .post(format!("{old}/signup"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body("email=ada@example.com&name=Ada&password=correct-horse")
        .send()
        .await
        .unwrap();
    assert_eq!(signup.status(), reqwest::StatusCode::SEE_OTHER);
    let session = signup
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let posted = client
        .post(format!("{old}/posts"))
        .header(header::COOKIE, &session)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body("text=keep+this")
        .send()
        .await
        .unwrap();
    assert_eq!(posted.status(), reqwest::StatusCode::SEE_OTHER);

    let started = client
        .post(format!("{old}/migrate/start"))
        .header(header::COOKIE, &session)
        .send()
        .await
        .unwrap();
    assert_eq!(started.status(), reqwest::StatusCode::SEE_OTHER);
    let page = client
        .get(format!("{old}/move"))
        .header(header::COOKIE, &session)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let code = page
        .split("id=\"move-code\">")
        .nth(1)
        .unwrap()
        .split('<')
        .next()
        .unwrap();

    let moved = client
        .post(format!("{new}/move"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(format!("knot={old}&code={code}&password=new-password"))
        .send()
        .await
        .unwrap();
    assert_eq!(moved.status(), reqwest::StatusCode::SEE_OTHER);
    let next = moved
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let home = client
        .get(format!("{new}/"))
        .header(header::COOKIE, &next)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(home.contains("keep this"), "{home}");
    assert!(home.contains("Ada"), "{home}");

    let again = client
        .post(format!("{new}/posts"))
        .header(header::COOKIE, &next)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body("text=from+the+new+host")
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), reqwest::StatusCode::SEE_OTHER);

    let old_home = client
        .get(format!("{old}/"))
        .header(header::COOKIE, &session)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(old_home.contains("Create an account"), "{old_home}");
    assert!(!old_home.contains("from the new host"), "{old_home}");

    let conn = rusqlite::Connection::open(old_dir.path().join("accounts.db")).unwrap();
    let custodial: i64 = conn
        .query_row("SELECT custodial FROM accounts", [], |row| row.get(0))
        .unwrap();
    let seed: Vec<u8> = conn
        .query_row("SELECT ed_seed FROM accounts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(custodial, 0);
    assert_eq!(seed, vec![0u8; 32]);
}
