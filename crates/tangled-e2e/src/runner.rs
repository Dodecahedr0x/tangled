use crate::{
    cluster::{total_traffic, Cluster},
    metrics::{percentile, state_bytes, verify_coverage, Sampler},
    scenarios::{apps, protocol, provider},
};
use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

const SURFACES: &[&str] = &["reed", "loomed", "weft", "provider", "protocol"];

#[derive(Clone, Debug, Serialize)]
pub struct Config {
    pub operations: usize,
    pub payload_bytes: usize,
    pub repetitions: usize,
    pub warmups: usize,
    pub benchmark: bool,
    pub scenario: String,
    pub output: PathBuf,
}

impl Config {
    pub fn parse(args: &[String]) -> Result<Self> {
        let benchmark = args.iter().any(|s| s == "--benchmark");
        let mut config = Self {
            operations: if benchmark { 10 } else { 3 },
            payload_bytes: 256,
            repetitions: if benchmark { 3 } else { 1 },
            warmups: usize::from(benchmark),
            benchmark,
            scenario: "all".into(),
            output: "target/e2e/report.json".into(),
        };
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            if arg == "--benchmark" {
                continue;
            }
            let value = args
                .next()
                .with_context(|| format!("missing value for {arg}"))?;
            match arg.as_str() {
                "--operations" => config.operations = value.parse()?,
                "--payload-bytes" => config.payload_bytes = value.parse()?,
                "--repetitions" => config.repetitions = value.parse()?,
                "--warmups" => config.warmups = value.parse()?,
                "--scenario" => config.scenario = value.clone(),
                "--output" => config.output = value.into(),
                _ => bail!("unknown argument {arg}"),
            }
        }
        ensure!(
            (1..=10000).contains(&config.operations),
            "operations must be 1..=10000"
        );
        ensure!(
            (1..=16384).contains(&config.payload_bytes),
            "payload bytes must be 1..=16384"
        );
        ensure!(
            (1..=100).contains(&config.repetitions) && config.warmups <= 20,
            "repetitions must be 1..=100, warmups 0..=20"
        );
        ensure!(
            config.scenario == "all"
                || SURFACES.contains(&config.scenario.as_str())
                || config.scenario == "throughput",
            "unknown scenario {}",
            config.scenario
        );
        Ok(config)
    }
}

pub fn required(surface: &str) -> &'static [&'static str] {
    match surface {
        "reed" => apps::REED_FEATURES,
        "loomed" => apps::LOOMED_FEATURES,
        "weft" => apps::WEFT_FEATURES,
        "provider" => provider::FEATURES,
        "protocol" => protocol::FEATURES,
        _ => &[],
    }
}

/// The acceptance specs, rather than the result list, define required coverage.
pub fn check_specifications() -> Result<()> {
    for (surface, document) in [
        ("reed", include_str!("../../../docs/spec/e2e/reed.md")),
        ("loomed", include_str!("../../../docs/spec/e2e/loomed.md")),
        ("weft", include_str!("../../../docs/spec/e2e/weft.md")),
        (
            "provider",
            include_str!("../../../docs/spec/e2e/provider.md"),
        ),
        (
            "protocol",
            include_str!("../../../docs/spec/e2e/protocol.md"),
        ),
    ] {
        let ids: Vec<_> = document
            .lines()
            .filter_map(|line| {
                let id = line
                    .strip_prefix('|')?
                    .split('|')
                    .next()?
                    .trim()
                    .trim_matches('`');
                (id.starts_with("REED-")
                    || id.starts_with("LOOMED-")
                    || id.starts_with("WEFT-")
                    || id.starts_with("provider-")
                    || id.starts_with("PROTOCOL-"))
                .then_some(id)
            })
            .collect();
        verify_coverage(&ids, required(surface))
            .with_context(|| format!("{surface} specification coverage"))?;
    }
    Ok(())
}

pub fn validate_benchmark_binary_paths(current: &Path, knot: &Path, worker: &Path) -> Result<()> {
    let expected = current
        .parent()
        .context("runner directory")?
        .join("tangled-knot");
    ensure!(
        knot == expected,
        "benchmark knot must be the sibling release binary; remove TANGLED_KNOT override"
    );
    ensure!(
        worker == current,
        "benchmark provider worker must use this runner; remove TANGLED_E2E_BIN override"
    );
    Ok(())
}

fn executable_provenance(path: &Path) -> Result<Value> {
    use std::io::Read;
    let path = path.canonicalize()?;
    let mut file = std::fs::File::open(&path)?;
    let mut hasher = blake3::Hasher::new();
    let mut bytes = [0u8; 65536];
    loop {
        let read = file.read(&mut bytes)?;
        if read == 0 {
            break;
        }
        hasher.update(&bytes[..read]);
    }
    Ok(
        json!({"path":path,"bytes":file.metadata()?.len(),"blake3":hasher.finalize().to_hex().to_string()}),
    )
}

async fn guarded_run<F>(future: F) -> Result<Value>
where
    F: std::future::Future<Output = Result<Value>>,
{
    use futures_util::FutureExt;
    match std::panic::AssertUnwindSafe(future).catch_unwind().await {
        Ok(result) => result,
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown scenario panic".into());
            bail!("scenario panicked: {message}")
        }
    }
}

pub async fn run(config: Config) -> Result<()> {
    check_specifications()?;
    let mut surfaces = if config.scenario == "all" {
        SURFACES.to_vec()
    } else {
        vec![config.scenario.as_str()]
    };
    if config.benchmark && config.scenario == "all" {
        surfaces.push("throughput");
    }
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    if config.benchmark {
        ensure!(
            profile == "release",
            "benchmark requires --release to keep driver and nodes comparable"
        );
    }
    let current = std::env::current_exe()?.canonicalize()?;
    let knot = crate::cluster::binary("tangled-knot")?.canonicalize()?;
    let worker = crate::cluster::binary("tangled-e2e")?.canonicalize()?;
    if config.benchmark {
        validate_benchmark_binary_paths(&current, &knot, &worker)?;
    }
    let executables = json!({"driver":executable_provenance(&current)?, "knot":executable_provenance(&knot)?, "provider_worker":executable_provenance(&worker)?});
    let mut report = json!({
        "schema_version":1, "status":"running", "config":config,
        "environment": {"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"logical_cpus":std::thread::available_parallelism().map(|n|n.get()).ok(), "profile":profile, "executables":executables, "git_revision":command_text("git", &["rev-parse","HEAD"]), "git_dirty":!command_text("git", &["status","--porcelain"]).is_empty(), "kernel":command_text("uname", &["-srm"]), "rustc":command_text("rustc", &["--version"])},
        "boundaries": {"state":"logical file bytes in node/replica/recovery directories; harness logs excluded", "bandwidth":"TCP bytes at counted knot/provider ingress, including HTTP headers and WebSocket frames; excludes TCP/IP, TLS and unproxied AP/SNS fixture traffic", "wall_time":"full asserted workflow including in-scenario fixtures; throughput scenario separately times each publish/pull/import/materialize operation", "memory":"sampled RSS for registered nodes (including transient fixture nodes) and driver; 100 ms target plus ps time, sequential samples approximate simultaneity; newly started processes use first observation as baseline; expected restarts may have unsampled intervals", "warmups":"fresh clusters, reported but excluded from summaries", "percentile":"nearest rank; median is p50 (lower median for even sample counts)"},
        "runs":[], "summary":{}, "failure":null
    });
    save(&config.output, &report)?;
    for iteration in 0..config.warmups + config.repetitions {
        let warmup = iteration < config.warmups;
        for surface in &surfaces {
            eprintln!(
                "{} {surface} iteration {}",
                if warmup { "warmup" } else { "measure" },
                iteration + 1
            );
            let result = guarded_run(run_one(surface, &config, iteration, warmup)).await;
            let entry = match result {
                Ok(entry) => entry,
                Err(error) => {
                    json!({"surface":surface,"iteration":iteration,"warmup":warmup,"status":"failed","error":format!("{error:#}")})
                }
            };
            let failed = entry["status"] != "passed";
            report["runs"]
                .as_array_mut()
                .context("runs array")?
                .push(entry);
            report["summary"] = summarize(report["runs"].as_array().context("runs")?)?;
            if failed {
                report["status"] = json!("failed");
                report["failure"] = report["runs"]
                    .as_array()
                    .context("runs")?
                    .last()
                    .cloned()
                    .unwrap_or(Value::Null);
                save(&config.output, &report)?;
                bail!("{surface} failed; diagnostics: {}", config.output.display());
            }
            save(&config.output, &report)?;
        }
    }
    report["status"] = json!("passed");
    save(&config.output, &report)?;
    println!("{}", serde_json::to_string_pretty(&report["summary"])?);
    println!("report: {}", config.output.display());
    Ok(())
}

async fn run_one(surface: &str, config: &Config, iteration: usize, warmup: bool) -> Result<Value> {
    let logs = config
        .output
        .parent()
        .unwrap_or(Path::new("."))
        .join("logs")
        .join(config.output.file_stem().unwrap_or_default())
        .join(format!("{surface}-{iteration}"));
    let mut cluster = if surface == "provider" {
        Cluster::provider_only()?
    } else {
        Cluster::start_with_logs(&logs).await?
    };
    cluster.set_log_destination(&logs);
    if surface == "provider" {
        provider::setup(&mut cluster).await?;
    }
    let state_before = state_snapshot(cluster.root.path())?;
    let mut nodes_before = BTreeMap::new();
    for node in cluster.all_nodes() {
        nodes_before.insert(
            node.name.clone(),
            (
                node.traffic(),
                node.client()
                    .stats()
                    .await
                    .map(|s| json!(s.counts))
                    .unwrap_or(Value::Null),
                node.pid(),
            ),
        );
    }
    // Take all byte baselines after public-count probe traffic has completed.
    for node in cluster.all_nodes() {
        nodes_before.get_mut(&node.name).expect("node baseline").0 = node.traffic();
    }
    let sampler = Sampler::start()?;
    let traffic_before = total_traffic();
    let started = Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(180), async {
        match surface {
            "reed" => apps::reed(&mut cluster, config.operations, config.payload_bytes).await,
            "loomed" => apps::loomed(&mut cluster, config.operations, config.payload_bytes).await,
            "weft" => apps::weft(&mut cluster, config.operations, config.payload_bytes).await,
            "provider" => {
                provider::run(&mut cluster, config.operations, config.payload_bytes).await
            }
            "protocol" => {
                protocol::run(&mut cluster, config.operations, config.payload_bytes).await
            }
            "throughput" => throughput(&mut cluster, config.operations, config.payload_bytes).await,
            _ => unreachable!(),
        }
    })
    .await
    .context("scenario timeout")
    .and_then(|r| r);
    let wall_ms = started.elapsed().as_secs_f64() * 1000.0;
    let traffic = total_traffic().since(traffic_before);
    let traffic_after: BTreeMap<_, _> = cluster
        .all_nodes()
        .map(|node| (node.name.clone(), node.traffic()))
        .collect();
    let memory = sampler.finish();
    let mut error = result.as_ref().err().map(|e| format!("{e:#}"));
    let passed = result.unwrap_or_default();
    if error.is_none() {
        if let Err(e) = verify_coverage(required(surface), &passed) {
            error = Some(format!("{e:#}"));
        }
    }
    if let Err(e) = &memory {
        error = Some(format!("{}; memory: {e:#}", error.unwrap_or_default()));
    }
    let state_after = state_snapshot(cluster.root.path())?;
    let mut state = BTreeMap::new();
    for name in state_before.keys().chain(state_after.keys()) {
        let before = state_before.get(name).copied().unwrap_or(0);
        let after = state_after.get(name).copied().unwrap_or(0);
        state.insert(name, json!({"before_bytes":before,"after_bytes":after,"growth_bytes":i128::from(after)-i128::from(before)}));
    }
    let mut nodes = BTreeMap::new();
    for node in cluster.all_nodes() {
        let baseline = nodes_before.get(&node.name);
        let before = baseline.map(|b| b.0).unwrap_or_default();
        let counts = baseline.map(|b| b.1.clone()).unwrap_or(Value::Null);
        let pid_before = baseline.map(|b| b.2).filter(|pid| *pid != 0);
        let pid_after = Some(node.pid()).filter(|pid| *pid != 0);
        nodes.insert(&node.name, json!({"traffic":traffic_after[&node.name].since(before), "public_counts_before":counts,"public_counts_after":node.client().stats().await.map(|s|json!(s.counts)).unwrap_or(Value::Null),"pid_before":pid_before,"pid_after":pid_after,"started_during_workflow":baseline.is_none()}));
    }
    let logs = config
        .output
        .parent()
        .unwrap_or(Path::new("."))
        .join("logs")
        .join(config.output.file_stem().unwrap_or_default())
        .join(format!("{surface}-{iteration}"));
    cluster.preserve_logs(&logs)?;
    let total_before: u64 = state_before.values().sum();
    let total_after: u64 = state_after.values().sum();
    Ok(
        json!({"surface":surface,"iteration":iteration,"warmup":warmup,"status":if error.is_some(){"failed"}else{"passed"},"error":error,"passed_features":passed,"wall_ms":wall_ms,"traffic":traffic,"bandwidth_bytes_per_second":(traffic.upload_bytes+traffic.download_bytes) as f64/(wall_ms/1000.0),"state":state,"total_state_before_bytes":total_before,"total_state_after_bytes":total_after,"state_growth_bytes":i128::from(total_after)-i128::from(total_before),"memory":memory.ok(),"nodes":nodes,"operation_latencies_ms":cluster.operation_latencies_ms,"logs":logs}),
    )
}

fn state_snapshot(root: &Path) -> Result<BTreeMap<String, u64>> {
    let mut result = BTreeMap::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name != ".harness-logs" {
            result.insert(name, state_bytes(&entry.path())?);
        }
    }
    Ok(result)
}

fn summarize(runs: &[Value]) -> Result<Value> {
    let mut summary = BTreeMap::new();
    for surface in SURFACES
        .iter()
        .copied()
        .chain(std::iter::once("throughput"))
    {
        let selected: Vec<_> = runs
            .iter()
            .filter(|r| r["surface"] == surface && r["warmup"] == false && r["status"] == "passed")
            .collect();
        if selected.is_empty() {
            continue;
        }
        let wall: Vec<_> = selected
            .iter()
            .filter_map(|r| r["wall_ms"].as_f64())
            .collect();
        let operations: Vec<_> = selected
            .iter()
            .flat_map(|r| r["operation_latencies_ms"].as_array().into_iter().flatten())
            .filter_map(Value::as_f64)
            .collect();
        let state: Vec<_> = selected
            .iter()
            .filter_map(|r| r["state_growth_bytes"].as_f64())
            .collect();
        let traffic: Vec<_> = selected
            .iter()
            .map(|r| {
                r["traffic"]["upload_bytes"].as_f64().unwrap_or(0.0)
                    + r["traffic"]["download_bytes"].as_f64().unwrap_or(0.0)
            })
            .collect();
        let rss: Vec<_> = selected
            .iter()
            .filter_map(|r| r["memory"]["simultaneous_peak_bytes"].as_f64())
            .collect();
        let mut value = json!({"repetitions":selected.len(),"workflow_wall_median_ms":percentile(&wall,0.5)?,"workflow_wall_p95_ms":percentile(&wall,0.95)?,"state_growth_median_bytes":percentile(&state,0.5)?,"traffic_median_bytes":percentile(&traffic,0.5)?,"sampled_cluster_and_driver_peak_median_bytes":percentile(&rss,0.5)?});
        if !operations.is_empty() {
            value["completed_operations"] = json!(operations.len());
            value["operation_latency_median_ms"] = json!(percentile(&operations, 0.5)?);
            value["operation_latency_p95_ms"] = json!(percentile(&operations, 0.95)?);
            value["operations_per_second"] =
                json!(operations.len() as f64 / (operations.iter().sum::<f64>() / 1000.0));
        }
        summary.insert(surface, value);
    }
    Ok(json!(summary))
}

async fn throughput(
    cluster: &mut Cluster,
    operations: usize,
    payload_bytes: usize,
) -> Result<Vec<&'static str>> {
    use tangled::{cid::Cid, http::PutRecordRequest, record::sign_json_record};
    let (ed, agreement) = cluster.nodes[0].identity.to_seeds();
    let identity = tangled::identity::Identity::from_seeds(ed, agreement);
    let source = cluster.nodes[0].siws(&identity).await?;
    let destination = cluster.nodes[1].siws(&identity).await?;
    let text = "x".repeat(payload_bytes);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_micros() as u64;
    let mut cursor = None;
    for i in 0..operations {
        let start = Instant::now();
        let rkey = format!("benchmark-{i}");
        let record = json!({"v":1,"type":Cid::dag_cbor(b"pub.tangled.post").to_string(),"collection":"pub.tangled.post","rkey":rkey,"author":identity.did_string(),"ts":now+i as u64,"prev":[],"vis":"public","body":{"text":text}});
        let signed = sign_json_record(identity.signing_key(), &record)?;
        source
            .put_record(&PutRecordRequest {
                record: signed.json,
            })
            .await?;
        let car = source
            .get_repo(&identity.did_string(), cursor.as_deref(), Some(false))
            .await?;
        destination
            .import_repo(car, true)
            .await
            .context("benchmark import into the existing replica")?;
        let fetched = destination
            .get_record(&identity.did_string(), "pub.tangled.post", &rkey)
            .await?;
        ensure!(
            fetched.record["body"]["text"] == text,
            "benchmark destination materialization differs"
        );
        cursor = Some(signed.cid.to_string());
        cluster
            .operation_latencies_ms
            .push(start.elapsed().as_secs_f64() * 1000.0);
    }
    ensure!(
        cluster.operation_latencies_ms.len() == operations,
        "incomplete workload"
    );
    Ok(vec![])
}

fn command_text(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unavailable".into())
}

fn save(path: &Path, report: &Value) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(report)?)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn panic_becomes_a_reportable_failure() {
        let result = super::guarded_run(async {
            panic!("fixture scenario panicked");
            #[allow(unreachable_code)]
            Ok(serde_json::Value::Null)
        })
        .await;
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("fixture scenario panicked"));
    }
}
