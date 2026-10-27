//! Hosted account and private archive acceptance through real HTTP sockets.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::{anyhow, ensure, Context, Result};
use reqwest::{header, Client, Response, StatusCode};
use serde_json::{json, Value};
use tangled::identity::Identity;
use tangled_import::ArchiveStore;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::{JoinHandle, JoinSet},
};

use crate::cluster::{Cluster, Node};

pub const FEATURES: &[&str] = &[
    "provider-auth",
    "provider-public-posts-custody",
    "provider-restart-durability",
    "provider-private-archive",
    "provider-scoped-ingestion",
    "provider-recovery",
    "provider-destination-bound-move",
    "provider-account-archive-move",
    "provider-interrupted-ack",
];

const EMAIL: &str = "archive-owner@example.test";
const PASSWORD: &str = "provider-original-password";
const NEXT_PASSWORD: &str = "provider-destination-password";
const RECOVERY_PASSWORD: &str = "separate archive recovery passphrase";
const PRIVATE_MARKER: &str = "provider-private-archive-fixture";
const LIVE_MARKER: &str = "provider-private-connector-fixture";

/// Provider startup is fixture setup and is excluded from workload deltas.
pub async fn setup(cluster: &mut Cluster) -> Result<()> {
    if cluster.providers.is_empty() {
        cluster
            .providers
            .push(Node::provider(cluster.root.path(), "provider-source").await?);
        cluster
            .providers
            .push(Node::provider(cluster.root.path(), "provider-destination").await?);
    }
    ensure!(cluster.providers.len() == 2, "expected two provider hosts");
    Ok(())
}

pub async fn run(
    cluster: &mut Cluster,
    operations: usize,
    payload_bytes: usize,
) -> Result<Vec<&'static str>> {
    ensure!(
        operations > 0 && payload_bytes > 0,
        "provider workload must be nonzero"
    );
    setup(cluster).await?;
    let client = cluster.client.clone();
    let source = cluster.providers[0].url.clone();
    let destination = cluster.providers[1].url.clone();

    status(
        client
            .post(format!("{source}/signup"))
            .form(&[
                ("email", "invalid"),
                ("name", "Invalid"),
                ("password", "short"),
            ])
            .send()
            .await?,
        StatusCode::BAD_REQUEST,
    )
    .await?;
    status(
        client.get(format!("{source}/api/imports")).send().await?,
        StatusCode::UNAUTHORIZED,
    )
    .await?;
    let denied = status(
        client
            .post(format!("{source}/posts"))
            .form(&[("text", "unauthorized-post")])
            .send()
            .await?,
        StatusCode::SEE_OTHER,
    )
    .await?;
    ensure!(
        denied
            .headers()
            .get(header::LOCATION)
            .and_then(|h| h.to_str().ok())
            == Some("/login"),
        "anonymous post did not redirect to login"
    );

    let initial_cookie = signup(&client, &source, EMAIL, "Provider Owner", PASSWORD).await?;
    status(
        client
            .post(format!("{source}/signup"))
            .form(&[
                ("email", EMAIL),
                ("name", "Duplicate"),
                ("password", PASSWORD),
            ])
            .send()
            .await?,
        StatusCode::BAD_REQUEST,
    )
    .await?;
    status(
        client
            .post(format!("{source}/login"))
            .form(&[("email", EMAIL), ("password", "wrong-password")])
            .send()
            .await?,
        StatusCode::UNAUTHORIZED,
    )
    .await?;
    let logout = status(
        client
            .post(format!("{source}/logout"))
            .header(header::COOKIE, &initial_cookie)
            .send()
            .await?,
        StatusCode::SEE_OTHER,
    )
    .await?;
    ensure!(
        logout
            .headers()
            .get(header::SET_COOKIE)
            .context("logout cookie")?
            .to_str()?
            .contains("Max-Age=0"),
        "logout did not expire cookie"
    );
    status(
        client
            .get(format!("{source}/api/imports"))
            .header(header::COOKIE, &initial_cookie)
            .send()
            .await?,
        StatusCode::UNAUTHORIZED,
    )
    .await?;
    let cookie = login(&client, &source, EMAIL, PASSWORD).await?;
    let stranger_cookie = signup(
        &client,
        &source,
        "stranger@example.test",
        "Stranger",
        "stranger-password",
    )
    .await?;
    let describe = response_json(
        client
            .get(format!("{source}/xrpc/tangled.node.describe"))
            .send()
            .await?,
    )
    .await?;
    ensure!(
        describe.to_string().contains("\"custodialSign\":true"),
        "provider must disclose custodial signing: {describe}"
    );
    let imports_page = status(
        client
            .get(format!("{source}/imports"))
            .header(header::COOKIE, &cookie)
            .send()
            .await?,
        StatusCode::OK,
    )
    .await?
    .text()
    .await?;
    ensure!(
        imports_page.contains("provider") && imports_page.contains("keys"),
        "archive page must disclose provider custody"
    );

    // The production posting limit is 2000 bytes. Every chunk is asserted before
    // the next write, so a bounded home feed cannot hide a dropped earlier post.
    let mut last_public = String::new();
    for operation in 0..operations {
        for (chunk, bytes) in vec![b'x'; payload_bytes].chunks(1900).enumerate() {
            last_public = format!(
                "provider-public-{operation}-{chunk}-{}",
                String::from_utf8_lossy(bytes)
            );
            post(&client, &source, &cookie, &last_public).await?;
            ensure!(
                home(&client, &source, None).await?.contains(&last_public),
                "guest did not observe public post {operation}/{chunk}"
            );
        }
    }

    let original = serde_json::to_vec(&json!({
        "personal_information":{"user_id":123},
        "chats":{"list":[{"id":7,"name":"Private fixture","type":"personal_chat",
            "messages":(0..operations).map(|index| json!({
                "id":index+1,"type":"message","date":"2024-01-02T03:04:05",
                "from":"Fixture Sender","from_id":"user5",
                "text":format!("{PRIVATE_MARKER}-{index}-{}", "p".repeat(payload_bytes))
            })).collect::<Vec<_>>()
        }]}
    }))?;
    let receipt = response_json(
        client
            .post(format!("{source}/imports/upload"))
            .header(header::COOKIE, &cookie)
            .multipart(
                reqwest::multipart::Form::new()
                    .text("source", "telegram")
                    .text("account", "123")
                    .part(
                        "file",
                        reqwest::multipart::Part::bytes(original.clone()).file_name("result.json"),
                    ),
            )
            .send()
            .await?,
    )
    .await?;
    ensure!(
        receipt["inserted"]
            .as_u64()
            .is_some_and(|count| count >= operations as u64),
        "upload did not insert requested objects: {receipt}"
    );
    let batch_id = field(&receipt, "id")?.to_owned();
    assert_archive_fixture(&client, &source, &cookie, operations, &original).await?;
    ensure!(
        archive(&client, &source, &cookie, "no-such-provider-message").await?["total"] == 0,
        "unmatched archive search returned objects"
    );
    exact_original(&client, &source, &cookie, &batch_id, &original).await?;
    assert_isolated(&client, &source, &stranger_cookie, &batch_id).await?;
    ensure!(
        !home(&client, &source, None).await?.contains(PRIVATE_MARKER),
        "private archive leaked into public feed"
    );
    status(
        client
            .post(format!("{source}/api/imports/upload"))
            .json(&json!({
                "source":"telegram","account":"123","filename":"result.json","data_base64":"e30="
            }))
            .send()
            .await?,
        StatusCode::UNAUTHORIZED,
    )
    .await?;

    let connection = connect(&client, &source, &cookie).await?;
    let connection_id = field(&connection["connection"], "id")?;
    let token = field(&connection, "token")?;
    let ingest_url = format!("{source}/api/imports/connections/{connection_id}/ingest");
    let live = json!({"source":"telegram","account":"123","objects":[{
        "id":"live-provider-1","kind":"message","conversation":"7",
        "sender":"Fixture Sender","timestamp":"2024-01-03T03:04:05",
        "text":LIVE_MARKER,"data":{"local_fixture":true}
    }],"files":[],"warnings":[]});
    status(
        client
            .post(&ingest_url)
            .bearer_auth("wrong-token")
            .json(&live)
            .send()
            .await?,
        StatusCode::UNAUTHORIZED,
    )
    .await?;
    let mut other_scope = live.clone();
    other_scope["account"] = "other-account".into();
    status(
        client
            .post(&ingest_url)
            .bearer_auth(token)
            .json(&other_scope)
            .send()
            .await?,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await?;
    other_scope["account"] = "123".into();
    other_scope["source"] = "twitter".into();
    status(
        client
            .post(&ingest_url)
            .bearer_auth(token)
            .json(&other_scope)
            .send()
            .await?,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await?;
    let ingested = response_json(
        client
            .post(&ingest_url)
            .bearer_auth(token)
            .json(&live)
            .send()
            .await?,
    )
    .await?;
    ensure!(
        ingested["inserted"] == 1,
        "live ingestion not materialized: {ingested}"
    );
    ensure!(
        archive(&client, &source, &cookie, LIVE_MARKER).await?["total"] == 1,
        "live message absent"
    );
    status(
        client
            .get(format!("{source}/api/imports"))
            .bearer_auth(token)
            .send()
            .await?,
        StatusCode::UNAUTHORIZED,
    )
    .await?;
    let forbidden_disconnect = client
        .post(format!(
            "{source}/api/imports/connections/{connection_id}/disconnect"
        ))
        .header(header::COOKIE, &stranger_cookie)
        .json(&json!({}))
        .send()
        .await?;
    ensure!(
        !forbidden_disconnect.status().is_success(),
        "another account disconnected owner's connector"
    );
    status(
        client
            .post(format!(
                "{source}/api/imports/connections/{connection_id}/disconnect"
            ))
            .header(header::COOKIE, &cookie)
            .json(&json!({}))
            .send()
            .await?,
        StatusCode::OK,
    )
    .await?;
    status(
        client
            .post(&ingest_url)
            .bearer_auth(token)
            .json(&live)
            .send()
            .await?,
        StatusCode::UNAUTHORIZED,
    )
    .await?;
    ensure!(
        archive(&client, &source, &cookie, LIVE_MARKER).await?["total"] == 1,
        "disconnect deleted retained activity"
    );

    // Leave a second, live token outstanding to test the migration fence.
    let move_connection = connect(&client, &source, &cookie).await?;
    let exported_recovery = recovery_packet(&client, &source, &cookie).await?;
    ensure!(
        tangled_import::recovery_open(&exported_recovery, "wrong recovery password").is_err(),
        "wrong recovery passphrase accepted"
    );
    let mut tampered = exported_recovery.clone();
    tampered["owner"] = Identity::generate().did_string().into();
    ensure!(
        tangled_import::recovery_open(&tampered, RECOVERY_PASSWORD).is_err(),
        "tampered owner accepted"
    );
    let recovered = tangled_import::recovery_open(&exported_recovery, RECOVERY_PASSWORD)
        .map_err(|e| anyhow!(e))?;
    let identity = recovered.identity().map_err(|e| anyhow!(e))?;
    ensure!(
        identity.did_string() == recovered.owner,
        "recovery identity differs from owner"
    );
    ensure!(
        home(&client, &source, Some(&cookie))
            .await?
            .contains(&recovered.owner),
        "recovery owner differs from hosted account"
    );
    let bundle_json = serde_json::to_string(&recovered)?;
    ensure!(
        !bundle_json.contains(token) && !bundle_json.contains(field(&move_connection, "token")?),
        "recovery exported connector credentials"
    );
    let recovered_dir = cluster.root.path().join("provider-offline-recovery");
    let mut restored = ArchiveStore::open(&recovered_dir).map_err(|e| anyhow!(e))?;
    restored
        .restore(&recovered.owner, &identity, &recovered.archive)
        .map_err(|e| anyhow!(e))?;
    restored
        .verify_archive(&recovered.owner, &identity)
        .map_err(|e| anyhow!(e))?;
    ensure!(
        restored
            .original(&recovered.owner, &identity, &batch_id)
            .map_err(|e| anyhow!(e))?
            .1
            == original,
        "offline recovery original bytes differ"
    );
    let restored_objects = restored
        .objects(&recovered.owner, &identity)
        .map_err(|e| anyhow!(e))?;
    assert_fixture_objects(
        &serde_json::to_value(&restored_objects)?,
        operations,
        &original,
    )?;
    ensure!(
        restored_objects
            .iter()
            .any(|item| item.object.text.as_deref() == Some(LIVE_MARKER)),
        "offline recovery lost connector activity"
    );
    drop(restored);

    cluster.providers[0].restart().await?;
    let cookie = login(&client, &source, EMAIL, PASSWORD).await?;
    ensure!(
        home(&client, &source, Some(&cookie))
            .await?
            .contains(&last_public),
        "source restart lost public history"
    );
    assert_archive_fixture(&client, &source, &cookie, operations, &original).await?;
    exact_original(&client, &source, &cookie, &batch_id, &original).await?;

    bound_handoff(cluster, &client, &source, &destination).await?;

    let code = start_move(&client, &source, &cookie).await?;
    let fault = AckFault::start(&source).await?;
    let move_form = [
        ("knot", fault.url.as_str()),
        ("code", &code),
        ("password", NEXT_PASSWORD),
    ];
    let interrupted = status(
        client
            .post(format!("{destination}/move"))
            .form(&move_form)
            .send()
            .await?,
        StatusCode::BAD_REQUEST,
    )
    .await?
    .text()
    .await?;
    ensure!(
        fault.rejected.load(Ordering::SeqCst),
        "fault proxy did not interrupt acknowledgement"
    );
    ensure!(
        interrupted.contains("archive is saved here"),
        "move did not retain destination archive after interrupted acknowledgement: {interrupted}"
    );
    status(
        client
            .get(format!("{source}/api/imports"))
            .header(header::COOKIE, &cookie)
            .send()
            .await?,
        StatusCode::UNAUTHORIZED,
    )
    .await?;
    let denied = status(
        client
            .post(format!("{source}/posts"))
            .header(header::COOKIE, &cookie)
            .form(&[("text", "source-must-not-sign-after-move")])
            .send()
            .await?,
        StatusCode::BAD_GATEWAY,
    )
    .await?
    .text()
    .await?;
    ensure!(
        denied.contains("This host can no longer sign for this account."),
        "prepared source did not explain signing fence"
    );
    let move_ingest = format!(
        "{source}/api/imports/connections/{}/ingest",
        field(&move_connection["connection"], "id")?
    );
    status(
        client
            .post(&move_ingest)
            .bearer_auth(field(&move_connection, "token")?)
            .json(&live)
            .send()
            .await?,
        StatusCode::UNAUTHORIZED,
    )
    .await?;

    // Persistence, rather than in-memory receipt state, must support resume.
    cluster.providers[0].restart().await?;
    cluster.providers[1].restart().await?;
    let wrong = status(
        client
            .post(format!("{destination}/move"))
            .form(&[
                ("knot", fault.url.as_str()),
                ("code", code.as_str()),
                ("password", "wrong-destination-password"),
            ])
            .send()
            .await?,
        StatusCode::BAD_REQUEST,
    )
    .await?
    .text()
    .await?;
    ensure!(
        wrong.contains("password you chose"),
        "wrong-password resume was not rejected by saved destination account: {wrong}"
    );
    let moved = status(
        client
            .post(format!("{destination}/move"))
            .form(&move_form)
            .send()
            .await?,
        StatusCode::SEE_OTHER,
    )
    .await?;
    let moved_cookie = session(&moved)?;
    fault.close().await;
    let moved_home = home(&client, &destination, Some(&moved_cookie)).await?;
    ensure!(
        moved_home.contains(&recovered.owner)
            && moved_home.contains("Provider Owner")
            && moved_home.contains(&last_public),
        "destination lost account or public history"
    );
    ensure!(
        !home(&client, &source, None)
            .await?
            .contains("source-must-not-sign-after-move"),
        "source signed after migration fence"
    );
    let moved_archive = archive(&client, &destination, &moved_cookie, PRIVATE_MARKER).await?;
    assert_archive_fixture(&client, &destination, &moved_cookie, operations, &original).await?;
    ensure!(
        moved_archive["connections"]
            .as_array()
            .context("destination connections")?
            .is_empty(),
        "source connector authorization moved to destination"
    );
    ensure!(
        archive(&client, &destination, &moved_cookie, LIVE_MARKER).await?["total"] == 1,
        "move lost live activity"
    );
    exact_original(&client, &destination, &moved_cookie, &batch_id, &original).await?;
    let destination_recovery = tangled_import::recovery_open(
        &recovery_packet(&client, &destination, &moved_cookie).await?,
        RECOVERY_PASSWORD,
    )
    .map_err(|e| anyhow!(e))?;
    ensure!(
        destination_recovery
            .identity()
            .map_err(|e| anyhow!(e))?
            .to_seeds()
            == identity.to_seeds(),
        "location move changed custody keys"
    );
    post(
        &client,
        &destination,
        &moved_cookie,
        "provider-new-host-signs",
    )
    .await?;
    ensure!(
        home(&client, &destination, None)
            .await?
            .contains("provider-new-host-signs"),
        "destination could not sign post"
    );
    let destination_stranger = signup(
        &client,
        &destination,
        "destination-stranger@example.test",
        "Destination Stranger",
        "stranger-password",
    )
    .await?;
    assert_isolated(&client, &destination, &destination_stranger, &batch_id).await?;
    cluster.providers[1].restart().await?;
    let persisted_cookie = login(&client, &destination, EMAIL, NEXT_PASSWORD).await?;
    exact_original(
        &client,
        &destination,
        &persisted_cookie,
        &batch_id,
        &original,
    )
    .await?;
    ensure!(
        home(&client, &destination, Some(&persisted_cookie))
            .await?
            .contains("provider-new-host-signs"),
        "destination restart lost signed post"
    );
    assert_archive_fixture(
        &client,
        &destination,
        &persisted_cookie,
        operations,
        &original,
    )
    .await?;
    Ok(FEATURES.to_vec())
}

async fn status(response: Response, expected: StatusCode) -> Result<Response> {
    let actual = response.status();
    if actual != expected {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "provider expected {expected}, received {actual}: {}",
            body.chars().take(1500).collect::<String>()
        );
    }
    Ok(response)
}

async fn response_json(response: Response) -> Result<Value> {
    status(response, StatusCode::OK)
        .await?
        .json()
        .await
        .context("provider JSON response")
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("missing string field {key}"))
}

fn session(response: &Response) -> Result<String> {
    Ok(response
        .headers()
        .get(header::SET_COOKIE)
        .context("provider session cookie")?
        .to_str()?
        .split(';')
        .next()
        .context("empty provider session cookie")?
        .to_owned())
}

async fn signup(
    client: &Client,
    host: &str,
    email: &str,
    name: &str,
    password: &str,
) -> Result<String> {
    let response = status(
        client
            .post(format!("{host}/signup"))
            .form(&[("email", email), ("name", name), ("password", password)])
            .send()
            .await?,
        StatusCode::SEE_OTHER,
    )
    .await?;
    session(&response)
}

async fn login(client: &Client, host: &str, email: &str, password: &str) -> Result<String> {
    let response = status(
        client
            .post(format!("{host}/login"))
            .form(&[("email", email), ("password", password)])
            .send()
            .await?,
        StatusCode::SEE_OTHER,
    )
    .await?;
    session(&response)
}

async fn home(client: &Client, host: &str, cookie: Option<&str>) -> Result<String> {
    let mut request = client.get(format!("{host}/"));
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    Ok(status(request.send().await?, StatusCode::OK)
        .await?
        .text()
        .await?)
}

async fn post(client: &Client, host: &str, cookie: &str, text: &str) -> Result<()> {
    status(
        client
            .post(format!("{host}/posts"))
            .header(header::COOKIE, cookie)
            .form(&[("text", text)])
            .send()
            .await?,
        StatusCode::SEE_OTHER,
    )
    .await?;
    Ok(())
}

async fn archive(client: &Client, host: &str, cookie: &str, query: &str) -> Result<Value> {
    response_json(
        client
            .get(format!("{host}/api/imports"))
            .header(header::COOKIE, cookie)
            .query(&[("q", query)])
            .send()
            .await?,
    )
    .await
}

async fn assert_archive_fixture(
    client: &Client,
    host: &str,
    cookie: &str,
    operations: usize,
    original: &[u8],
) -> Result<()> {
    let mut objects = Vec::new();
    loop {
        let page = response_json(
            client
                .get(format!("{host}/api/imports"))
                .header(header::COOKIE, cookie)
                .query(&[
                    ("q", PRIVATE_MARKER.to_owned()),
                    ("limit", "500".to_owned()),
                    ("offset", objects.len().to_string()),
                ])
                .send()
                .await?,
        )
        .await?;
        let next = page["objects"]
            .as_array()
            .context("archive search objects")?;
        let total = page["total"].as_u64().context("archive search total")? as usize;
        ensure!(
            !next.is_empty() || objects.len() == total,
            "archive pagination stopped before total"
        );
        objects.extend(next.iter().cloned());
        if objects.len() >= total {
            break;
        }
    }
    assert_fixture_objects(&Value::Array(objects), operations, original)
}

fn assert_fixture_objects(objects: &Value, operations: usize, original: &[u8]) -> Result<()> {
    let objects = objects.as_array().context("archive fixture objects")?;
    let messages = objects
        .iter()
        .filter(|item| {
            item["object"]["kind"] == "message"
                && item["object"]["text"]
                    .as_str()
                    .is_some_and(|text| text.starts_with(PRIVATE_MARKER))
        })
        .collect::<Vec<_>>();
    ensure!(
        messages.len() == operations,
        "expected {operations} imported messages, observed {}",
        messages.len()
    );
    let export: Value = serde_json::from_slice(original)?;
    let expected = export["chats"]["list"][0]["messages"]
        .as_array()
        .context("fixture source messages")?;
    for message in expected {
        ensure!(
            messages
                .iter()
                .any(|item| item["object"]["text"] == message["text"]
                    && item["object"]["data"] == *message),
            "archive lost or changed source message {}",
            message["id"]
        );
    }
    ensure!(
        objects
            .iter()
            .any(|item| item["object"]["kind"] == "archive_metadata"
                && item["object"]["data"] == export),
        "archive did not preserve export metadata"
    );
    Ok(())
}

async fn exact_original(
    client: &Client,
    host: &str,
    cookie: &str,
    batch: &str,
    original: &[u8],
) -> Result<()> {
    let response = status(
        client
            .get(format!("{host}/api/imports/original/{batch}"))
            .header(header::COOKIE, cookie)
            .send()
            .await?,
        StatusCode::OK,
    )
    .await?;
    ensure!(
        response.bytes().await?.as_ref() == original,
        "provider original download bytes differ"
    );
    Ok(())
}

async fn assert_isolated(
    client: &Client,
    host: &str,
    stranger_cookie: &str,
    batch: &str,
) -> Result<()> {
    let other = archive(client, host, stranger_cookie, "").await?;
    ensure!(
        other["objects"]
            .as_array()
            .context("stranger objects")?
            .is_empty(),
        "private objects leaked to another account"
    );
    ensure!(
        other["receipts"]
            .as_array()
            .context("stranger receipts")?
            .is_empty(),
        "import receipts leaked to another account"
    );
    let denied = client
        .get(format!("{host}/api/imports/original/{batch}"))
        .header(header::COOKIE, stranger_cookie)
        .send()
        .await?;
    ensure!(
        !denied.status().is_success(),
        "another account downloaded private original"
    );
    status(
        client
            .get(format!("{host}/api/imports/original/{batch}"))
            .send()
            .await?,
        StatusCode::UNAUTHORIZED,
    )
    .await?;
    Ok(())
}

async fn connect(client: &Client, host: &str, cookie: &str) -> Result<Value> {
    response_json(
        client
            .post(format!("{host}/api/imports/connections"))
            .header(header::COOKIE, cookie)
            .json(&json!({"source":"telegram","account":"123"}))
            .send()
            .await?,
    )
    .await
}

async fn recovery_packet(client: &Client, host: &str, cookie: &str) -> Result<Value> {
    response_json(
        client
            .post(format!("{host}/api/imports/export"))
            .header(header::COOKIE, cookie)
            .json(&json!({"passphrase":RECOVERY_PASSWORD}))
            .send()
            .await?,
    )
    .await
}

async fn start_move(client: &Client, host: &str, cookie: &str) -> Result<String> {
    status(
        client
            .post(format!("{host}/migrate/start"))
            .header(header::COOKIE, cookie)
            .send()
            .await?,
        StatusCode::SEE_OTHER,
    )
    .await?;
    let page = status(
        client
            .get(format!("{host}/move"))
            .header(header::COOKIE, cookie)
            .send()
            .await?,
        StatusCode::OK,
    )
    .await?
    .text()
    .await?;
    Ok(page
        .split("id=\"move-code\">")
        .nth(1)
        .and_then(|s| s.split('<').next())
        .context("move invitation code")?
        .to_owned())
}

/// Separate account: protocol binding checks must not consume the real move's
/// temporary destination key or otherwise bypass the destination's persistence.
async fn bound_handoff(
    cluster: &mut Cluster,
    client: &Client,
    source: &str,
    destination: &str,
) -> Result<()> {
    let cookie = signup(
        client,
        source,
        "binding@example.test",
        "Binding Fixture",
        PASSWORD,
    )
    .await?;
    let code = start_move(client, source, &cookie).await?;
    let info = response_json(
        client
            .get(format!("{source}/migrate/{code}"))
            .send()
            .await?,
    )
    .await?;
    let recipient = Identity::generate();
    let request = json!({"protocol":2,"knot":destination,
        "pubkey":hex::encode(recipient.pubkey_bytes()),
        "agreement":hex::encode(recipient.agreement_public_bytes())});
    let prepared = response_json(
        client
            .post(format!("{source}/migrate/{code}"))
            .json(&request)
            .send()
            .await?,
    )
    .await?;
    ensure!(
        prepared.get("ed_seed").is_none() && prepared["archive"].get("backup").is_none(),
        "handoff leaked plaintext custody/archive"
    );
    let mut other = request.clone();
    other["agreement"] = hex::encode(Identity::generate().agreement_public_bytes()).into();
    status(
        client
            .post(format!("{source}/migrate/{code}"))
            .json(&other)
            .send()
            .await?,
        StatusCode::CONFLICT,
    )
    .await?;
    other = request.clone();
    other["knot"] = source.to_string().into();
    status(
        client
            .post(format!("{source}/migrate/{code}"))
            .json(&other)
            .send()
            .await?,
        StatusCode::CONFLICT,
    )
    .await?;
    status(
        client
            .post(format!("{source}/migrate/{code}/ack"))
            .json(&json!({"acknowledgement":"wrong"}))
            .send()
            .await?,
        StatusCode::FORBIDDEN,
    )
    .await?;
    cluster.providers[0].restart().await?;
    let replay = response_json(
        client
            .post(format!("{source}/migrate/{code}"))
            .json(&request)
            .send()
            .await?,
    )
    .await?;
    ensure!(
        replay == prepared,
        "source restart changed sealed destination-bound handoff"
    );
    let binding = serde_json::to_vec(&json!([
        "tangled.provider.archive-move.v2",
        info["did"],
        code,
        request["knot"],
        request["pubkey"],
        request["agreement"]
    ]))?;
    let enc = hex::decode(field(&prepared["archive"], "enc")?)?;
    let ciphertext = hex::decode(field(&prepared["archive"], "ciphertext")?)?;
    ensure!(
        tangled::hpke::open_record(
            &Identity::generate().to_seeds().1,
            &enc,
            &binding,
            &ciphertext
        )
        .is_err(),
        "unrelated destination decrypted handoff"
    );
    ensure!(
        tangled::hpke::open_record(
            &recipient.to_seeds().1,
            &enc,
            b"wrong destination binding",
            &ciphertext
        )
        .is_err(),
        "changed destination binding decrypted handoff"
    );
    let plaintext =
        tangled::hpke::open_record(&recipient.to_seeds().1, &enc, &binding, &ciphertext)
            .map_err(|e| anyhow!(e))?;
    let handoff: Value = serde_json::from_slice(&plaintext)?;
    ensure!(
        handoff["did"] == info["did"] && handoff["backup"]["owner"] == info["did"],
        "handoff owner mismatch"
    );
    let acknowledgement = json!({"acknowledgement":handoff["acknowledgement"]});
    for _ in 0..2 {
        status(
            client
                .post(format!("{source}/migrate/{code}/ack"))
                .json(&acknowledgement)
                .send()
                .await?,
            StatusCode::NO_CONTENT,
        )
        .await?;
    }
    Ok(())
}

/// Fault injection is test-only, owns all forwarding tasks, and forwards normal
/// requests into the source's counted boundary. No server behavior is changed.
struct AckFault {
    url: String,
    rejected: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

impl AckFault {
    async fn close(mut self) {
        self.task.abort();
        let _ = (&mut self.task).await;
    }

    async fn start(source: &str) -> Result<Self> {
        let target = reqwest::Url::parse(source)?;
        let target = format!(
            "{}:{}",
            target.host_str().context("source host")?,
            target.port_or_known_default().context("source port")?
        );
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let rejected = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&rejected);
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { break };
                        let target = target.clone();
                        let flag = Arc::clone(&flag);
                        connections.spawn(async move { let _ = forward_or_reject(stream, &target, &flag).await; });
                    }
                    _ = connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        Ok(Self {
            url,
            rejected,
            task,
        })
    }
}

impl Drop for AckFault {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn forward_or_reject(
    mut incoming: TcpStream,
    target: &str,
    rejected: &AtomicBool,
) -> Result<()> {
    let mut prefix = Vec::new();
    loop {
        let mut bytes = [0; 4096];
        let count = incoming.read(&mut bytes).await?;
        ensure!(count != 0, "fault proxy received incomplete request");
        prefix.extend_from_slice(&bytes[..count]);
        if prefix.windows(4).any(|s| s == b"\r\n\r\n") {
            break;
        }
        ensure!(
            prefix.len() < 65536,
            "fault proxy request headers too large"
        );
    }
    let line = prefix
        .split(|b| *b == b'\n')
        .next()
        .context("HTTP request line")?;
    let line = String::from_utf8_lossy(line);
    let path = line.split_whitespace().nth(1).unwrap_or("");
    if line.starts_with("POST /migrate/")
        && path.ends_with("/ack")
        && !rejected.swap(true, Ordering::SeqCst)
    {
        incoming.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await?;
        incoming.shutdown().await?;
        return Ok(());
    }
    // Inspect every request on a fresh connection, including the later ack.
    // Without this, HTTP keep-alive could carry the ack through the already
    // forwarded move-info connection without passing the fault matcher.
    let header_end = prefix
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .context("HTTP header boundary")?;
    prefix.splice(
        header_end + 2..header_end + 2,
        b"Connection: close\r\n".iter().copied(),
    );
    let mut upstream = TcpStream::connect(target).await?;
    upstream.write_all(&prefix).await?;
    tokio::io::copy_bidirectional(&mut incoming, &mut upstream).await?;
    Ok(())
}
