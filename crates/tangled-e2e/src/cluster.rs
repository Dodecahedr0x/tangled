use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;
use std::{
    fs::File,
    net::SocketAddr,
    path::{Path, PathBuf},
    pin::Pin,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
    task::{Context as TaskContext, Poll},
    time::{Duration, Instant},
};
use tangled::{http::KnotClient, identity::Identity};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    task::{JoinHandle, JoinSet},
};

#[derive(Clone, Copy, Default, Debug, Serialize)]
pub struct Traffic {
    pub upload_bytes: u64,
    pub download_bytes: u64,
}

impl Traffic {
    pub fn since(self, baseline: Self) -> Self {
        Self {
            upload_bytes: self.upload_bytes - baseline.upload_bytes,
            download_bytes: self.download_bytes - baseline.download_bytes,
        }
    }
}

static TOTAL_UPLOAD: AtomicU64 = AtomicU64::new(0);
static TOTAL_DOWNLOAD: AtomicU64 = AtomicU64::new(0);

pub fn total_traffic() -> Traffic {
    Traffic {
        upload_bytes: TOTAL_UPLOAD.load(Ordering::Relaxed),
        download_bytes: TOTAL_DOWNLOAD.load(Ordering::Relaxed),
    }
}

struct Counted {
    stream: TcpStream,
    read_bytes: Arc<AtomicU64>,
    total: &'static AtomicU64,
}

impl AsyncRead for Counted {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let result = Pin::new(&mut self.stream).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = result {
            let bytes = (buf.filled().len() - before) as u64;
            self.read_bytes.fetch_add(bytes, Ordering::Relaxed);
            self.total.fetch_add(bytes, Ordering::Relaxed);
        }
        result
    }
}

impl AsyncWrite for Counted {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

/// Count reads immediately, including keep-alive and WebSocket connections.
/// Dropping the accept task also aborts every owned forwarding task.
pub struct Proxy {
    pub address: SocketAddr,
    target: Arc<Mutex<Option<SocketAddr>>>,
    upload: Arc<AtomicU64>,
    download: Arc<AtomicU64>,
    task: JoinHandle<()>,
}

impl Proxy {
    pub async fn bind(target: Option<SocketAddr>) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let target = Arc::new(Mutex::new(target));
        let upload = Arc::new(AtomicU64::new(0));
        let download = Arc::new(AtomicU64::new(0));
        let (dest, up, down) = (target.clone(), upload.clone(), download.clone());
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((incoming, _)) = accepted else { break };
                        let address = *dest.lock().expect("proxy destination");
                        let (up, down) = (up.clone(), down.clone());
                        connections.spawn(async move {
                            if let Some(address) = address {
                                if let Ok(outgoing) = TcpStream::connect(address).await {
                                    let mut incoming = Counted { stream: incoming, read_bytes: up, total: &TOTAL_UPLOAD };
                                    let mut outgoing = Counted { stream: outgoing, read_bytes: down, total: &TOTAL_DOWNLOAD };
                                    let _ = tokio::io::copy_bidirectional(&mut incoming, &mut outgoing).await;
                                }
                            }
                        });
                    }
                    _ = connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        Ok(Self {
            address,
            target,
            upload,
            download,
            task,
        })
    }

    pub fn set_target(&self, address: SocketAddr) {
        *self.target.lock().expect("proxy destination") = Some(address);
    }
    pub fn traffic(&self) -> Traffic {
        Traffic {
            upload_bytes: self.upload.load(Ordering::Relaxed),
            download_bytes: self.download.load(Ordering::Relaxed),
        }
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub struct ManagedChild {
    child: Child,
    log: PathBuf,
}

impl ManagedChild {
    pub fn spawn(command: &mut Command, log: PathBuf) -> Result<Self> {
        let file = File::create(&log)?;
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(file)
            .spawn()
            .with_context(|| format!("spawn {command:?}"))?;
        Ok(Self { child, log })
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub async fn wait_address(&mut self, timeout: Duration) -> Result<SocketAddr> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait()? {
                bail!("node exited {status}: {}", self.logs());
            }
            let log = self.logs();
            if let Some(address) = log.lines().find_map(|line| {
                line.strip_prefix("listening on ")
                    .and_then(|s| s.split_whitespace().next())
                    .and_then(|s| s.parse().ok())
            }) {
                return Ok(address);
            }
            ensure!(Instant::now() < deadline, "node readiness timed out: {log}");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    pub fn logs(&self) -> String {
        // Keep diagnostics bounded even when a child is noisy.
        use std::io::{Read, Seek, SeekFrom};
        let mut text = String::new();
        if let Ok(mut file) = File::open(&self.log) {
            if let Ok(meta) = file.metadata() {
                let _ = file.seek(SeekFrom::Start(meta.len().saturating_sub(65536)));
            }
            let _ = file.take(65536).read_to_string(&mut text);
        }
        text
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

type ProcessRegistry = Mutex<Vec<(String, Weak<AtomicU32>)>>;
static PROCESSES: OnceLock<ProcessRegistry> = OnceLock::new();

pub fn registered_pids() -> Vec<(String, Arc<AtomicU32>)> {
    let mut registry = PROCESSES
        .get_or_init(Default::default)
        .lock()
        .expect("process registry");
    registry.retain(|(_, pid)| pid.strong_count() > 0);
    registry
        .iter()
        .filter_map(|(name, pid)| pid.upgrade().map(|pid| (name.clone(), pid)))
        .collect()
}

#[derive(Clone, Copy)]
enum Kind {
    Knot,
    Provider,
}

pub struct Node {
    pub url: String,
    pub dir: PathBuf,
    pub identity: Identity,
    pub name: String,
    proxy: Proxy,
    process: Option<ManagedChild>,
    kind: Kind,
    peers: Vec<String>,
    solana_rpc: Option<String>,
    incarnation: usize,
    pid_slot: Arc<AtomicU32>,
}

impl Node {
    pub async fn knot(root: &Path, name: &str, peers: &[String]) -> Result<Self> {
        Self::start(root, name, Kind::Knot, peers, None).await
    }
    pub async fn knot_with_solana_rpc(
        root: &Path,
        name: &str,
        peers: &[String],
        rpc: &str,
    ) -> Result<Self> {
        Self::start(root, name, Kind::Knot, peers, Some(rpc.into())).await
    }
    pub async fn provider(root: &Path, name: &str) -> Result<Self> {
        Self::start(root, name, Kind::Provider, &[], None).await
    }

    async fn start(
        root: &Path,
        name: &str,
        kind: Kind,
        peers: &[String],
        rpc: Option<String>,
    ) -> Result<Self> {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir)?;
        let proxy = Proxy::bind(None).await?;
        let url = format!("http://{}", proxy.address);
        let mut node = Self {
            url,
            dir,
            identity: Identity::generate(),
            name: name.into(),
            proxy,
            process: None,
            kind,
            peers: peers.to_vec(),
            solana_rpc: rpc,
            incarnation: 0,
            pid_slot: Arc::new(AtomicU32::new(0)),
        };
        PROCESSES
            .get_or_init(Default::default)
            .lock()
            .expect("process registry")
            .push((name.into(), Arc::downgrade(&node.pid_slot)));
        node.launch().await?;
        let (file, ed, x) = match kind {
            Kind::Knot => ("identity.json", "ed25519", "x25519"),
            Kind::Provider => ("operator.json", "ed", "x"),
        };
        let json: serde_json::Value = serde_json::from_slice(&std::fs::read(node.dir.join(file))?)?;
        let ed: [u8; 32] = hex::decode(json[ed].as_str().context("identity ed seed")?)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("seed size"))?;
        let x: [u8; 32] = hex::decode(json[x].as_str().context("identity agreement seed")?)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("seed size"))?;
        node.identity = Identity::from_seeds(ed, x);
        Ok(node)
    }

    async fn launch(&mut self) -> Result<()> {
        let mut command = match self.kind {
            Kind::Knot => {
                let mut cmd = Command::new(binary("tangled-knot")?);
                cmd.args(["--listen", "127.0.0.1:0", "--data"])
                    .arg(&self.dir)
                    .args(["--public-url", &self.url]);
                for peer in &self.peers {
                    cmd.args(["--peer", peer]);
                }
                cmd
            }
            Kind::Provider => {
                let mut cmd = Command::new(binary("tangled-e2e")?);
                cmd.arg("--provider-worker").arg(&self.dir).arg(&self.url);
                cmd
            }
        };
        // Ambient peer/RPC/deployment configuration must not reach isolated fixtures.
        for var in [
            "TANGLED_PEERS",
            "SOLANA_RPC",
            "SOLANA_RPC_URL",
            "PUBLIC_URL",
            "RENDER_EXTERNAL_URL",
            "PORT",
            "DATA_DIR",
        ] {
            command.env_remove(var);
        }
        if let Some(rpc) = &self.solana_rpc {
            command.env("SOLANA_RPC", rpc).env("SOLANA_RPC_URL", rpc);
        }
        // The knot logs its bind before ActivityPub RSA initialization finishes.
        // Give the entire startup (log discovery + HTTP readiness) one budget.
        let startup_deadline = Instant::now() + Duration::from_secs(30);
        self.incarnation += 1;
        let logs = self
            .dir
            .parent()
            .context("node root")?
            .join(".harness-logs");
        std::fs::create_dir_all(&logs)?;
        let mut child = ManagedChild::spawn(
            &mut command,
            logs.join(format!("{}-{}.log", self.name, self.incarnation)),
        )?;
        let address = child
            .wait_address(startup_deadline.saturating_duration_since(Instant::now()))
            .await?;
        self.proxy.set_target(address);
        self.pid_slot.store(child.pid(), Ordering::SeqCst);
        self.process = Some(child);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .no_proxy()
            .build()?;
        let deadline = startup_deadline;
        loop {
            if let Ok(response) = client
                .get(format!("{}/xrpc/tangled.node.describe", self.url))
                .send()
                .await
            {
                if response.status().is_success() {
                    return Ok(());
                }
            }
            ensure!(
                Instant::now() < deadline,
                "{} HTTP readiness timed out (RSA initialized: {})",
                self.name,
                self.dir.join("ap-rsa.pem").exists()
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    pub fn client(&self) -> KnotClient {
        KnotClient::http(&self.url)
    }
    pub fn pid(&self) -> u32 {
        self.pid_slot.load(Ordering::SeqCst)
    }
    pub fn traffic(&self) -> Traffic {
        self.proxy.traffic()
    }
    pub async fn restart(&mut self) -> Result<()> {
        self.pid_slot.store(0, Ordering::SeqCst);
        drop(self.process.take());
        self.launch().await
    }
    pub async fn siws(&self, identity: &Identity) -> Result<KnotClient> {
        let client = self.client();
        let challenge = client.session_challenge(&identity.did_string()).await?;
        let signature = tangled::session::sign_siws(identity.signing_key(), &challenge.message);
        let (_, token) = client
            .session_verify(&tangled::session::SiwsVerifyRequest {
                did: identity.did_string(),
                nonce: challenge.nonce,
                signature,
            })
            .await?;
        Ok(client.with_session_cookie(token))
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        self.pid_slot.store(0, Ordering::SeqCst);
        drop(self.process.take());
    }
}

pub fn binary(name: &str) -> Result<PathBuf> {
    let variable = if name == "tangled-knot" {
        "TANGLED_KNOT"
    } else {
        "TANGLED_E2E_BIN"
    };
    if let Some(path) = std::env::var_os(variable) {
        return Ok(PathBuf::from(path));
    }
    let current = std::env::current_exe()?;
    let mut parent = current.parent().context("executable parent")?;
    if parent.file_name().is_some_and(|p| p == "deps") {
        parent = parent.parent().context("target profile directory")?;
    }
    let candidate = parent.join(name);
    ensure!(
        candidate.is_file(),
        "{} missing; build tangled-e2e and tangled-knot with the same Cargo profile",
        candidate.display()
    );
    Ok(candidate)
}

pub struct Cluster {
    // Fields are ordered so children are torn down before their temporary root.
    pub nodes: Vec<Node>,
    pub providers: Vec<Node>,
    pub client: reqwest::Client,
    pub root: tempfile::TempDir,
    pub operation_latencies_ms: Vec<f64>,
    log_destination: Option<PathBuf>,
}

impl Cluster {
    pub fn provider_only() -> Result<Self> {
        Ok(Self {
            nodes: vec![],
            providers: vec![],
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(20))
                .no_proxy()
                .build()?,
            root: tempfile::tempdir()?,
            operation_latencies_ms: vec![],
            log_destination: None,
        })
    }

    pub async fn start() -> Result<Self> {
        Self::start_logged(None).await
    }

    pub async fn start_with_logs(destination: &Path) -> Result<Self> {
        Self::start_logged(Some(destination)).await
    }

    async fn start_logged(destination: Option<&Path>) -> Result<Self> {
        let mut cluster = Self::provider_only()?;
        cluster.log_destination = destination.map(PathBuf::from);
        let second = Node::knot(cluster.root.path(), "knot-b", &[]).await?;
        cluster.nodes.push(second);
        let third = Node::knot(cluster.root.path(), "knot-c", &[]).await?;
        cluster.nodes.push(third);
        let peers = cluster
            .nodes
            .iter()
            .map(|node| node.url.clone())
            .collect::<Vec<_>>();
        let home = Node::knot(cluster.root.path(), "knot-a", &peers).await?;
        cluster.nodes.insert(0, home);
        Ok(cluster)
    }

    pub fn set_log_destination(&mut self, destination: &Path) {
        self.log_destination = Some(destination.into());
    }

    pub fn all_nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.iter().chain(&self.providers)
    }
    pub fn pids(&self) -> Vec<(String, u32)> {
        self.all_nodes()
            .map(|n| (n.name.clone(), n.pid()))
            .chain(std::iter::once(("driver".into(), std::process::id())))
            .collect()
    }

    pub fn preserve_logs(&self, destination: &Path) -> Result<()> {
        use std::io::{Read, Seek, SeekFrom};
        std::fs::create_dir_all(destination)?;
        let directory = self.root.path().join(".harness-logs");
        if !directory.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let mut file = File::open(entry.path())?;
            let size = file.metadata()?.len();
            file.seek(SeekFrom::Start(size.saturating_sub(65536)))?;
            let mut bytes = Vec::new();
            file.take(65536).read_to_end(&mut bytes)?;
            std::fs::write(destination.join(entry.file_name()), bytes)?;
        }
        Ok(())
    }
}

impl Drop for Cluster {
    fn drop(&mut self) {
        if let Some(destination) = &self.log_destination {
            if let Err(error) = self.preserve_logs(destination) {
                eprintln!("preserve cluster logs: {error:#}");
            }
        }
    }
}

pub async fn provider_worker(dir: &Path, public_url: &str) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let provider =
        tangled_provider::Provider::open(dir, public_url).map_err(|e| anyhow::anyhow!(e))?;
    eprintln!("listening on {}", listener.local_addr()?);
    axum::serve(listener, tangled_provider::router(provider)).await?;
    Ok(())
}
