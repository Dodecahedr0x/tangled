use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};

pub fn state_bytes(path: &Path) -> Result<u64> {
    let meta =
        std::fs::symlink_metadata(path).with_context(|| format!("stat {}", path.display()))?;
    if meta.file_type().is_symlink() {
        return Ok(0);
    }
    if meta.is_file() {
        return Ok(meta.len());
    }
    let mut total = 0u64;
    for entry in std::fs::read_dir(path)? {
        total = total
            .checked_add(state_bytes(&entry?.path())?)
            .context("state size overflow")?;
    }
    Ok(total)
}

/// ps returns KiB on Linux and macOS; unavailable samples are errors.
pub fn rss_bytes(pid: u32) -> Result<u64> {
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .context("run ps for RSS")?;
    ensure!(
        output.status.success(),
        "RSS unavailable for pid {pid}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let kib: u64 = String::from_utf8(output.stdout)?
        .trim()
        .parse()
        .context("empty/invalid RSS sample")?;
    ensure!(kib > 0, "zero RSS sample for pid {pid}");
    kib.checked_mul(1024).context("RSS overflow")
}

pub fn percentile(samples: &[f64], quantile: f64) -> Result<f64> {
    ensure!(!samples.is_empty(), "no percentile samples");
    ensure!(
        quantile.is_finite() && quantile > 0.0 && quantile <= 1.0,
        "invalid quantile"
    );
    ensure!(samples.iter().all(|v| v.is_finite()), "invalid sample");
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    Ok(sorted[(quantile * sorted.len() as f64).ceil() as usize - 1])
}

pub fn verify_coverage(required: &[&str], passed: &[&str]) -> Result<()> {
    let wanted: BTreeSet<_> = required.iter().copied().collect();
    let actual: BTreeSet<_> = passed.iter().copied().collect();
    ensure!(
        wanted.len() == required.len(),
        "duplicate required feature IDs"
    );
    ensure!(actual.len() == passed.len(), "duplicate passed feature IDs");
    if wanted != actual {
        bail!(
            "coverage mismatch: missing {:?}, unknown {:?}",
            wanted.difference(&actual).collect::<Vec<_>>(),
            actual.difference(&wanted).collect::<Vec<_>>()
        );
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub struct Rss {
    pub baseline_bytes: u64,
    pub sampled_peak_bytes: u64,
    pub end_bytes: u64,
    pub samples: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MemoryReport {
    pub processes: BTreeMap<String, Rss>,
    pub simultaneous_peak_bytes: u64,
    pub target_interval_ms: u64,
    pub errors: Vec<String>,
}

pub struct Sampler {
    report: Arc<Mutex<MemoryReport>>,
    task: tokio::task::JoinHandle<()>,
}

impl Sampler {
    pub fn start() -> Result<Self> {
        let mut report = MemoryReport {
            processes: BTreeMap::new(),
            simultaneous_peak_bytes: 0,
            target_interval_ms: 100,
            errors: vec![],
        };
        sample(&mut report)?;
        let report = Arc::new(Mutex::new(report));
        let copy = report.clone();
        let task = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let mut r = copy.lock().expect("memory report");
                if let Err(e) = sample(&mut r) {
                    r.errors.push(format!("{e:#}"));
                }
            }
        });
        Ok(Self { report, task })
    }

    pub fn finish(self) -> Result<MemoryReport> {
        self.task.abort();
        let mut report = self.report.lock().expect("memory report");
        sample(&mut report)?;
        let result = report.clone();
        ensure!(
            result.errors.is_empty(),
            "memory sampling failed: {:?}",
            result.errors
        );
        Ok(result)
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn sample(report: &mut MemoryReport) -> Result<()> {
    use std::sync::atomic::{AtomicU32, Ordering};
    let mut pids = crate::cluster::registered_pids();
    pids.push((
        "driver".into(),
        Arc::new(AtomicU32::new(std::process::id())),
    ));
    let mut sum = 0;
    for (name, slot) in pids {
        let pid = slot.load(Ordering::SeqCst);
        if pid == 0 {
            continue;
        }
        let bytes = match rss_bytes(pid) {
            Ok(bytes) => bytes,
            Err(_) if slot.load(Ordering::SeqCst) != pid => continue,
            Err(error) => return Err(error),
        };
        let rss = report.processes.entry(name.clone()).or_insert(Rss {
            baseline_bytes: bytes,
            sampled_peak_bytes: bytes,
            end_bytes: bytes,
            samples: 0,
        });
        rss.sampled_peak_bytes = rss.sampled_peak_bytes.max(bytes);
        rss.end_bytes = bytes;
        rss.samples += 1;
        sum += bytes;
    }
    report.simultaneous_peak_bytes = report.simultaneous_peak_bytes.max(sum);
    Ok(())
}
