use tangled_import::store::uploads::UploadRequest;
use tangled_import::ArchiveStore;

fn request() -> UploadRequest {
    UploadRequest {
        source: "telegram".into(),
        account: "123".into(),
        filename: "result.json".into(),
        total: 11,
        sha256: None,
    }
}
#[test]
fn staged_upload_resumes_after_restart_and_replays_a_chunk_safely() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = ArchiveStore::open(dir.path()).unwrap();
    let started = store.create_upload("owner", request()).unwrap();
    assert_eq!(started.received, 0);
    store
        .append_upload("owner", &started.id, 0, b"hello ")
        .unwrap();
    drop(store);
    let mut store = ArchiveStore::open(dir.path()).unwrap();
    assert_eq!(
        store.upload_status("owner", &started.id).unwrap().received,
        6
    );
    store
        .append_upload("owner", &started.id, 0, b"hello ")
        .unwrap();
    assert!(store
        .append_upload("owner", &started.id, 0, b"other!")
        .is_err());
    assert!(store
        .append_upload("owner", &started.id, 8, b"gap")
        .is_err());
    assert!(store
        .append_upload("stranger", &started.id, 6, b"world")
        .is_err());
    store
        .append_upload("owner", &started.id, 6, b"world")
        .unwrap();
    let ready = store.upload_bytes("owner", &started.id).unwrap();
    assert_eq!(ready.1, b"hello world");
    assert_eq!(ready.0.status, "ready");
    let disk = std::fs::read(dir.path().join("imports.db")).unwrap();
    assert!(!disk.windows(11).any(|v| v == b"hello world"));
    assert!(!disk.windows(11).any(|v| v == b"result.json"));
}

#[test]
fn canceled_upload_cannot_be_resumed_and_pending_bytes_are_removed() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = ArchiveStore::open(dir.path()).unwrap();
    let started = store.create_upload("owner", request()).unwrap();
    store
        .append_upload("owner", &started.id, 0, b"secret")
        .unwrap();
    assert!(store.cancel_upload("other", &started.id).is_err());
    store.cancel_upload("owner", &started.id).unwrap();
    assert!(store.upload_bytes("owner", &started.id).is_err());
    assert!(store
        .append_upload("owner", &started.id, 6, b"world")
        .is_err());
    assert_eq!(
        store.upload_status("owner", &started.id).unwrap().status,
        "canceled"
    );
}

#[test]
fn wrong_file_digest_and_post_erasure_chunks_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = ArchiveStore::open(dir.path()).unwrap();
    let mut wanted = request();
    wanted.sha256 = Some("0".repeat(64));
    let started = store.create_upload("owner", wanted).unwrap();
    store
        .append_upload("owner", &started.id, 0, b"hello world")
        .unwrap();
    assert!(store
        .upload_bytes("owner", &started.id)
        .unwrap_err()
        .contains("digest"));
    let conn = rusqlite::Connection::open(dir.path().join("imports.db")).unwrap();
    conn.execute(
        "INSERT INTO archive_erasure_generations(owner,generation) VALUES('owner',1)",
        [],
    )
    .unwrap();
    assert!(store.upload_status("owner", &started.id).is_err());
    assert!(store
        .append_upload("owner", &started.id, 0, b"hello world")
        .is_err());
}
