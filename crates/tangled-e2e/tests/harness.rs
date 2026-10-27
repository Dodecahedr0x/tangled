use tangled_e2e::metrics::{percentile, rss_bytes, state_bytes, verify_coverage};

#[test]
fn state_growth_counts_nested_files_without_following_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("repo")).unwrap();
    std::fs::write(dir.path().join("repo/record"), b"12345").unwrap();
    let before = state_bytes(dir.path()).unwrap();
    assert_eq!(before, 5);
    std::fs::write(dir.path().join("blob"), b"1234567").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(dir.path(), dir.path().join("cycle")).unwrap();
    assert_eq!(state_bytes(dir.path()).unwrap() - before, 7);
    assert!(state_bytes(&dir.path().join("absent")).is_err());
}

#[test]
fn percentile_uses_nearest_rank_and_rejects_empty_or_invalid_samples() {
    assert_eq!(percentile(&[30.0, 10.0, 20.0, 40.0], 0.5).unwrap(), 20.0);
    assert_eq!(percentile(&[-5.0, -1.0], 0.5).unwrap(), -5.0);
    assert_eq!(percentile(&[30.0, 10.0, 20.0, 40.0], 0.95).unwrap(), 40.0);
    assert!(percentile(&[], 0.5).is_err());
    assert!(percentile(&[f64::NAN], 0.5).is_err());
    assert!(percentile(&[1.0], 1.1).is_err());
}

#[test]
fn missing_and_duplicate_feature_results_cannot_pass_coverage() {
    assert!(verify_coverage(&["a", "b"], &["a"]).is_err());
    assert!(verify_coverage(&["a"], &["a", "a"]).is_err());
    assert!(verify_coverage(&["a"], &["a", "unknown"]).is_err());
    verify_coverage(&["a", "b"], &["b", "a"]).unwrap();
}

#[test]
fn rss_reports_live_process_bytes_and_errors_for_missing_process() {
    assert!(rss_bytes(std::process::id()).unwrap() > 0);
    assert!(rss_bytes(u32::MAX).is_err());
}

#[tokio::test]
async fn proxy_counts_live_bidirectional_traffic_and_closes_on_drop() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = tangled_e2e::cluster::Proxy::bind(Some(upstream.local_addr().unwrap()))
        .await
        .unwrap();
    let address = proxy.address;
    let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
    let (mut server, _) = upstream.accept().await.unwrap();
    client.write_all(b"four").await.unwrap();
    let mut four = [0; 4];
    server.read_exact(&mut four).await.unwrap();
    assert_eq!(&four, b"four");
    server.write_all(b"sixsix").await.unwrap();
    let mut six = [0; 6];
    client.read_exact(&mut six).await.unwrap();
    assert_eq!(&six, b"sixsix");
    assert_eq!(proxy.traffic().upload_bytes, 4);
    assert_eq!(proxy.traffic().download_bytes, 6);
    drop(proxy);
    let mut byte = [0];
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(2), client.read(&mut byte))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while tokio::net::TcpStream::connect(address).await.is_ok() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("proxy listener did not close after task cancellation");
}

#[tokio::test]
async fn readiness_timeout_reaps_the_owned_child() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = std::process::Command::new("sleep");
    command.arg("30");
    let mut child =
        tangled_e2e::cluster::ManagedChild::spawn(&mut command, dir.path().join("child.log"))
            .unwrap();
    let pid = child.pid();
    assert!(child
        .wait_address(std::time::Duration::from_millis(10))
        .await
        .is_err());
    drop(child);
    assert!(rss_bytes(pid).is_err());
}

#[test]
fn startup_failure_keeps_report_and_process_log() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("failed.json");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_tangled-e2e"))
        .args(["--scenario", "reed", "--output"])
        .arg(&output)
        .env("TANGLED_KNOT", "/usr/bin/false")
        .output()
        .unwrap();
    assert!(!result.status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert_eq!(report["status"], "failed");
    assert!(
        dir.path().join("logs/failed/reed-0/knot-b-1.log").is_file(),
        "failed startup log was discarded"
    );
}

#[test]
fn release_benchmarks_reject_mixed_child_binary_paths() {
    use std::path::Path;
    use tangled_e2e::runner::validate_benchmark_binary_paths;
    let current = Path::new("/build/release/tangled-e2e");
    let knot = Path::new("/build/release/tangled-knot");
    validate_benchmark_binary_paths(current, knot, current).unwrap();
    assert!(validate_benchmark_binary_paths(
        current,
        Path::new("/build/debug/tangled-knot"),
        current
    )
    .is_err());
    assert!(
        validate_benchmark_binary_paths(current, knot, Path::new("/build/debug/tangled-e2e"))
            .is_err()
    );
}

#[test]
fn specifications_and_executable_coverage_match() {
    tangled_e2e::runner::check_specifications().unwrap();
}

#[test]
fn invalid_workloads_are_rejected_before_any_node_starts() {
    use tangled_e2e::runner::Config;
    for args in [
        vec!["--operations", "0"],
        vec!["--payload-bytes", "0"],
        vec!["--repetitions", "0"],
        vec!["--scenario", "unknown"],
        vec!["--operations"],
        vec!["--unknown"],
    ] {
        assert!(
            Config::parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).is_err(),
            "accepted {args:?}"
        );
    }
}

#[test]
fn preserve_logs_keeps_failed_startup_files_with_bounded_output() {
    let cluster = tangled_e2e::cluster::Cluster::provider_only().unwrap();
    let source = cluster.root.path().join(".harness-logs");
    std::fs::create_dir_all(&source).unwrap();
    let mut noisy = vec![b'x'; 70_000];
    noisy.extend_from_slice(b"failure-tail");
    std::fs::write(source.join("knot-b-1.log"), &noisy).unwrap();
    let destination = tempfile::tempdir().unwrap();
    cluster.preserve_logs(destination.path()).unwrap();
    let saved = std::fs::read(destination.path().join("knot-b-1.log")).unwrap();
    assert_eq!(saved.len(), 65_536);
    assert!(saved.ends_with(b"failure-tail"));
}

#[test]
fn preserve_logs_accepts_an_empty_unstarted_cluster() {
    let cluster = tangled_e2e::cluster::Cluster::provider_only().unwrap();
    let destination = tempfile::tempdir().unwrap();
    cluster.preserve_logs(destination.path()).unwrap();
    assert_eq!(std::fs::read_dir(destination.path()).unwrap().count(), 0);
}
