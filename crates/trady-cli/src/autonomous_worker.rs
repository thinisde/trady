//! Private bounded IPC for analyst work. This process receives no broker or executor credential.

use super::{RuntimeConfig, models};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use trady_adapters::autonomous::{AutomaticInputs, MarketInputs};
use trady_application::perpetual::PerpetualRepository;
use trady_application::{autonomous::*, contracts::parse_json_bounded};
use trady_domain::*;
use trady_policy::EntryCosts;
use trady_storage::trading_research::PgTradingResearch;

const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
const ANALYSIS_TIMEOUT: Duration = Duration::from_secs(30);

// Secrets travel only in the anonymous stdin pipe; never argv, environment or diagnostics.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Boot {
    config: RuntimeConfig,
    config_path: String,
    analyst_dsn: String,
    token: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    snapshot_id: String,
    failed: bool,
    candidate: Option<EntryCandidate>,
    model: Option<ModelIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelIdentity {
    revision_id: String,
    artifact_sha256: String,
}

fn candidate_model(
    repository: &mut PgTradingResearch,
    candidate: Option<&EntryCandidate>,
) -> Result<Option<ModelIdentity>> {
    candidate
        .map(|candidate| {
            let record = repository
                .load_assessment(&candidate.advisor.assessment_id)?
                .ok_or_else(|| Error("analyst assessment was not persisted".into()))?;
            trady_application::perpetual::validate_record(&record)?;
            let model = record
                .model
                .ok_or_else(|| Error("analyst assessment lacks native model provenance".into()))?;
            require(
                model.model_revision_id == candidate.advisor.model_revision_id,
                "candidate differs from the model used for persisted inference",
            )?;
            Ok(ModelIdentity {
                revision_id: model.model_revision_id,
                artifact_sha256: model.artifact_sha256,
            })
        })
        .transpose()
}

fn approved_model(
    models: &std::collections::BTreeMap<String, String>,
    revision: &str,
    actual: Option<&ModelIdentity>,
) -> Result<()> {
    let actual = actual.ok_or_else(|| Error("analyst model identity missing".into()))?;
    require(
        actual.revision_id == revision && models.get(revision) == Some(&actual.artifact_sha256),
        "analyst used a native model outside the deployment release",
    )
}

fn validate_reply_model(config: &RuntimeConfig, reply: &Reply) -> Result<()> {
    if config.trading.mode == Mode::Live
        && let Some(candidate) = &reply.candidate
    {
        let release = config
            .release_candidate
            .as_ref()
            .ok_or_else(|| Error("live release model bindings missing".into()))?;
        approved_model(
            &release.model_hashes,
            &candidate.advisor.model_revision_id,
            reply.model.as_ref(),
        )?;
    }
    Ok(())
}

struct RetentionWorker {
    stop: Option<mpsc::SyncSender<()>>,
    worker: Option<JoinHandle<Result<()>>>,
}
impl RetentionWorker {
    fn start(mut repository: PgTradingResearch) -> Result<Self> {
        // Fail startup if cleanup is unavailable, then maintain it on its own
        // connection while synchronous native/provider work is running.
        repository.purge_expired()?;
        let (stop, requests) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("analyst-retention".into())
            .spawn(move || {
                loop {
                    match requests.recv_timeout(Duration::from_secs(50)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                        Err(mpsc::RecvTimeoutError::Timeout) => repository.purge_expired()?,
                    }
                }
            })
            .map_err(|_| Error("cannot start analyst retention worker".into()))?;
        Ok(Self {
            stop: Some(stop),
            worker: Some(worker),
        })
    }
    fn check(&self) -> Result<()> {
        require(
            self.worker
                .as_ref()
                .is_some_and(|worker| !worker.is_finished()),
            "analyst retention maintenance stopped",
        )
    }
}
impl Drop for RetentionWorker {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.try_send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn encode(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes =
        serde_json::to_vec(value).map_err(|_| Error("analyst IPC encoding failed".into()))?;
    require(
        bytes.len() < MAX_MESSAGE_BYTES,
        "analyst IPC message too large",
    )?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn read_message(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>> {
    let mut bytes = vec![];
    let n = reader
        .take(MAX_MESSAGE_BYTES as u64 + 1)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| Error("analyst IPC read failed".into()))?;
    if n == 0 {
        return Ok(None);
    }
    require(
        bytes.len() <= MAX_MESSAGE_BYTES && bytes.last() == Some(&b'\n'),
        "oversized or truncated analyst IPC message",
    )?;
    Ok(Some(bytes))
}
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error("invalid analyst IPC text".into()))?;
    serde_json::from_value(parse_json_bounded(text, MAX_MESSAGE_BYTES)?)
        .map_err(|_| Error("invalid analyst IPC document".into()))
}

/// Called only as the private child entrypoint, before any credential-store or broker setup.
pub(super) fn serve() -> Result<()> {
    let mut input = BufReader::new(std::io::stdin());
    let boot: Boot = decode(
        &read_message(&mut input)?
            .ok_or_else(|| Error("analyst IPC initialization required".into()))?,
    )?;
    boot.config.validate()?;
    require(boot.config.enabled, "analyst runtime disabled")?;
    let advisor = models(&boot.config, &boot.config_path)?;
    let repository = PgTradingResearch::connect(
        &boot.analyst_dsn,
        boot.config.trading.owner_id.clone(),
        boot.config.trading.account_id.clone(),
    )?;
    let retention = RetentionWorker::start(PgTradingResearch::connect_retention(
        &boot.analyst_dsn,
        boot.config.trading.owner_id.clone(),
        boot.config.trading.account_id.clone(),
    )?)?;
    let mut inputs = AutomaticInputs::new(
        boot.config.inputs,
        boot.config.trading.clone(),
        boot.config.universe,
        advisor,
        repository,
        boot.token,
    )?;
    let (jobs, requests) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("analyst-requests".into())
        .spawn(move || {
            loop {
                let message = read_message(&mut input);
                let ended = !matches!(&message, Ok(Some(_)));
                if jobs.send(message).is_err() || ended {
                    break;
                }
            }
        })
        .map_err(|_| Error("cannot start analyst request reader".into()))?;
    let mut output = std::io::stdout().lock();
    loop {
        // Retention maintenance continues while the executor manages a holding.
        retention.check()?;
        let bytes = match requests.recv_timeout(Duration::from_secs(60)) {
            Ok(Ok(Some(bytes))) => bytes,
            Ok(Ok(None)) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(Err(error)) => return Err(error),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
        };
        let snapshot: AccountSnapshot = decode(&bytes)?;
        snapshot.validate()?;
        require(
            snapshot.account_id == boot.config.trading.account_id
                && snapshot.mode == boot.config.trading.mode
                && snapshot.spendable_cash.currency == boot.config.trading.base_currency,
            "analyst snapshot scope mismatch",
        )?;
        let result = inputs.candidate(&snapshot, Utc::now());
        let model = candidate_model(
            &mut inputs.repository,
            result.as_ref().ok().and_then(|c| c.as_ref()),
        )?;
        let reply = Reply {
            snapshot_id: snapshot.snapshot_id,
            failed: result.is_err(),
            candidate: result.unwrap_or(None),
            model,
        };
        output
            .write_all(&encode(&reply)?)
            .and_then(|_| output.flush())
            .map_err(|_| Error("analyst IPC write failed".into()))?;
    }
    Ok(())
}

struct WorkerProcess {
    child: Child,
    requests: Option<mpsc::SyncSender<Vec<u8>>>,
    responses: Option<mpsc::Receiver<Result<Vec<u8>>>>,
    writer: Option<JoinHandle<()>>,
    reader: Option<JoinHandle<()>>,
}
impl WorkerProcess {
    fn start(mut command: Command, boot: Vec<u8>) -> Result<Self> {
        let mut child = command
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| Error("cannot start analyst worker".into()))?;
        let Some(mut stdin) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error("analyst stdin unavailable".into()));
        };
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error("analyst stdout unavailable".into()));
        };
        // Only boot + one outstanding snapshot may be queued. Pipe I/O never blocks the executor.
        let (send, incoming) = mpsc::sync_channel::<Vec<u8>>(2);
        let (outgoing, receive) = mpsc::sync_channel(1);
        let mut worker = Self {
            child,
            requests: Some(send),
            responses: Some(receive),
            writer: None,
            reader: None,
        };
        worker.writer = Some(
            thread::Builder::new()
                .name("analyst-ipc-writer".into())
                .spawn(move || {
                    for bytes in incoming {
                        if stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_err() {
                            break;
                        }
                    }
                })
                .map_err(|_| Error("cannot start analyst IPC writer".into()))?,
        );
        worker.reader = Some(
            thread::Builder::new()
                .name("analyst-ipc-reader".into())
                .spawn(move || {
                    let mut reader = BufReader::new(stdout);
                    loop {
                        let result = read_message(&mut reader).and_then(|v| {
                            v.ok_or_else(|| Error("analyst worker closed IPC".into()))
                        });
                        let failed = result.is_err();
                        if outgoing.send(result).is_err() || failed {
                            break;
                        }
                    }
                })
                .map_err(|_| Error("cannot start analyst IPC reader".into()))?,
        );
        worker.send(boot)?;
        Ok(worker)
    }
    fn send(&self, bytes: Vec<u8>) -> Result<()> {
        self.requests
            .as_ref()
            .ok_or_else(|| Error("analyst IPC closed".into()))?
            .try_send(bytes)
            .map_err(|_| Error("analyst IPC unavailable".into()))
    }
    fn exchange(
        &mut self,
        bytes: Vec<u8>,
        timeout: Duration,
        stopped: &AtomicBool,
    ) -> Result<Vec<u8>> {
        self.send(bytes)?;
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| Error("invalid analyst deadline".into()))?;
        loop {
            require(
                !stopped.load(Ordering::Relaxed),
                "analyst cancelled for shutdown",
            )?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            require(!remaining.is_zero(), "analyst worker deadline exceeded")?;
            let reply = self
                .responses
                .as_ref()
                .ok_or_else(|| Error("analyst IPC closed".into()))?
                .recv_timeout(remaining.min(Duration::from_millis(100)));
            match reply {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(Error("analyst worker stopped".into()));
                }
            }
        }
    }
}
impl Drop for WorkerProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.requests.take();
        self.responses.take();
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

pub(super) struct IsolatedInputs {
    market: MarketInputs,
    boot: Boot,
    process: Option<WorkerProcess>,
    stopped: Arc<AtomicBool>,
    external_socket: Option<String>,
}
impl IsolatedInputs {
    pub(super) fn new(
        config: RuntimeConfig,
        path: &str,
        analyst_dsn: String,
        token: Option<String>,
        stopped: Arc<AtomicBool>,
    ) -> Result<Self> {
        let path = Path::new(path);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|_| Error("cannot locate runtime directory".into()))?
                .join(path)
        };
        let config_path = path
            .to_str()
            .ok_or_else(|| Error("invalid runtime path".into()))?
            .to_owned();
        let market = MarketInputs::new(config.inputs.clone(), &config.trading, token.clone())?;
        Ok(Self {
            market,
            boot: Boot {
                config,
                config_path,
                analyst_dsn,
                token,
            },
            process: None,
            stopped,
            external_socket: None,
        })
    }
    pub(super) fn new_external(
        config: RuntimeConfig,
        path: &str,
        token: Option<String>,
        stopped: Arc<AtomicBool>,
    ) -> Result<Self> {
        let socket = config
            .analyst_socket
            .clone()
            .ok_or_else(|| Error("analyst socket is missing".into()))?;
        let mut inputs = Self::new(config, path, String::new(), token, stopped)?;
        inputs.external_socket = Some(socket);
        Ok(inputs)
    }
}
impl TradingInputs for IsolatedInputs {
    fn session(&mut self, account: &AccountId, _: DateTime<Utc>) -> Result<Option<MarketSession>> {
        self.market.session(account)
    }
    fn quote(&mut self, instrument: &InstrumentId) -> Result<Quote> {
        self.market.quote(instrument)
    }
    fn current_exit_costs(
        &mut self,
        instrument: &InstrumentId,
        now: DateTime<Utc>,
    ) -> Result<Option<EntryCosts>> {
        self.market.current_exit_costs(instrument, now)
    }
    fn candidate(
        &mut self,
        snapshot: &AccountSnapshot,
        _: DateTime<Utc>,
    ) -> Result<Option<EntryCandidate>> {
        if let Some(socket) = &self.external_socket {
            let reply: Reply =
                decode(&socket_exchange(socket, &encode(snapshot)?, &self.stopped)?)?;
            require(
                reply.snapshot_id == snapshot.snapshot_id && !reply.failed,
                "separate analyst returned unusable or differently bound advice",
            )?;
            validate_reply_model(&self.boot.config, &reply)?;
            return Ok(reply.candidate);
        }
        if self.process.is_none() {
            let mut command = Command::new(
                std::env::current_exe()
                    .map_err(|_| Error("cannot locate analyst executable".into()))?,
            );
            command.args(["autonomous", "analyst-worker"]);
            self.process = Some(WorkerProcess::start(command, encode(&self.boot)?)?);
        }
        let result = self
            .process
            .as_mut()
            .ok_or_else(|| Error("analyst worker missing".into()))?
            .exchange(encode(snapshot)?, ANALYSIS_TIMEOUT, &self.stopped)
            .and_then(|bytes| decode::<Reply>(&bytes));
        let reply = match result {
            Ok(reply) => reply,
            Err(error) => {
                self.process.take();
                return Err(error);
            }
        };
        if reply.snapshot_id != snapshot.snapshot_id {
            self.process.take();
            return Err(Error("analyst reply snapshot mismatch".into()));
        }
        require(
            !reply.failed,
            "analyst worker could not produce a usable candidate",
        )?;
        validate_reply_model(&self.boot.config, &reply)?;
        Ok(reply.candidate)
    }
}

#[cfg(unix)]
fn private_socket(path: &Path) -> Result<u32> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
    require(
        path.is_absolute()
            && !path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
        "analyst endpoint must use an absolute path without parent traversal",
    )?;
    let parent = path
        .parent()
        .ok_or_else(|| Error("analyst socket needs a parent directory".into()))?;
    let directory = std::fs::symlink_metadata(parent)
        .map_err(|_| Error("analyst socket directory is unavailable".into()))?;
    require(
        directory.is_dir()
            && !directory.file_type().is_symlink()
            && directory.permissions().mode() & 0o022 == 0,
        "analyst socket directory must not be writable by group or other users",
    )?;
    let owner = directory.uid();
    for ancestor in parent.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor)
            .map_err(|_| Error("analyst socket ancestor is unavailable".into()))?;
        let mode = metadata.permissions().mode();
        // A root-owned sticky directory such as /tmp protects an existing private
        // child from replacement by other users. All other ancestors are immutable
        // to identities outside the analyst/root trust boundary.
        require(
            metadata.is_dir()
                && !metadata.file_type().is_symlink()
                && (metadata.uid() == 0 || metadata.uid() == owner)
                && (mode & 0o022 == 0
                    || (metadata.uid() == 0 && mode & 0o1000 != 0 && ancestor != parent)),
            "analyst socket ancestors must be trusted directories without symlinks",
        )?;
    }
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        require(
            metadata.file_type().is_socket()
                && metadata.uid() == owner
                && metadata.permissions().mode() & 0o007 == 0,
            "analyst endpoint must be a private Unix socket",
        )?;
    }
    Ok(owner)
}

#[cfg(unix)]
fn socket_write(
    stream: &mut std::os::unix::net::UnixStream,
    bytes: &[u8],
    deadline: Instant,
    stopped: &AtomicBool,
) -> Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        require(
            Instant::now() < deadline && !stopped.load(Ordering::Relaxed),
            "analyst socket deadline or shutdown",
        )?;
        match stream.write(&bytes[offset..]) {
            Ok(0) => return Err(Error("analyst socket closed during write".into())),
            Ok(n) => offset += n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5))
            }
            Err(_) => return Err(Error("analyst socket write failed".into())),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn socket_read(
    stream: &mut std::os::unix::net::UnixStream,
    deadline: Instant,
    stopped: &AtomicBool,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut block = [0_u8; 8192];
    loop {
        require(
            Instant::now() < deadline && !stopped.load(Ordering::Relaxed),
            "analyst socket deadline or shutdown",
        )?;
        match stream.read(&mut block) {
            Ok(0) => {
                return Err(Error(
                    "analyst socket closed before a complete message".into(),
                ));
            }
            Ok(n) => {
                bytes.extend_from_slice(&block[..n]);
                require(
                    bytes.len() <= MAX_MESSAGE_BYTES,
                    "analyst socket message exceeds limit",
                )?;
                if let Some(end) = bytes.iter().position(|&b| b == b'\n') {
                    require(
                        end + 1 == bytes.len(),
                        "unexpected extra analyst socket message",
                    )?;
                    return Ok(bytes);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5))
            }
            Err(_) => return Err(Error("analyst socket read failed".into())),
        }
    }
}

#[cfg(unix)]
fn socket_exchange(path: &str, bytes: &[u8], stopped: &AtomicBool) -> Result<Vec<u8>> {
    let owner = private_socket(Path::new(path))?;
    let deadline = Instant::now() + ANALYSIS_TIMEOUT;
    let socket = socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None)
        .map_err(|_| Error("cannot create analyst connection".into()))?;
    let address =
        socket2::SockAddr::unix(path).map_err(|_| Error("invalid analyst endpoint path".into()))?;
    socket
        .connect_timeout(&address, ANALYSIS_TIMEOUT)
        .map_err(|_| Error("separate analyst is unavailable".into()))?;
    let mut stream: std::os::unix::net::UnixStream = socket.into();
    #[cfg(target_os = "linux")]
    {
        use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
        let peer = getsockopt(&stream, PeerCredentials)
            .map_err(|_| Error("cannot verify analyst peer identity".into()))?;
        require(
            peer.uid() == owner,
            "connected analyst identity differs from the protected endpoint owner",
        )?;
    }
    require(
        private_socket(Path::new(path))? == owner,
        "analyst endpoint owner changed during connection",
    )?;
    stream
        .set_nonblocking(true)
        .map_err(|_| Error("cannot bound analyst socket I/O".into()))?;
    socket_write(&mut stream, bytes, deadline, stopped)?;
    socket_read(&mut stream, deadline, stopped)
}

#[cfg(not(unix))]
fn socket_exchange(_: &str, _: &[u8], _: &AtomicBool) -> Result<Vec<u8>> {
    Err(Error(
        "separate analyst transport requires Unix sockets".into(),
    ))
}

/// Deployment-owned analyst service. It loads only analyst credentials; the
/// executor sends a bounded account snapshot through a filesystem-protected socket.
#[cfg(unix)]
pub(super) fn serve_external(config: RuntimeConfig, config_path: &str) -> Result<()> {
    use std::os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::UnixListener,
    };
    let path = config
        .analyst_socket
        .as_ref()
        .ok_or_else(|| Error("analyst_socket is required".into()))?
        .clone();
    require(config.enabled, "analyst service is disabled")?;
    private_socket(Path::new(&path))?;
    require(
        !Path::new(&path)
            .try_exists()
            .map_err(|_| Error("cannot inspect analyst endpoint".into()))?,
        "analyst socket already exists; inspect its owner before recovery",
    )?;
    let credentials = trady_adapters::credentials::CredentialStore::load()?;
    let repository = PgTradingResearch::connect(
        credentials.required("TRADY_ANALYST_DSN")?.expose(),
        config.trading.owner_id.clone(),
        config.trading.account_id.clone(),
    )?;
    let retention = RetentionWorker::start(PgTradingResearch::connect_retention(
        credentials.required("TRADY_ANALYST_DSN")?.expose(),
        config.trading.owner_id.clone(),
        config.trading.account_id.clone(),
    )?)?;
    let advisor = models(&config, config_path)?;
    let mut inputs = AutomaticInputs::new(
        config.inputs.clone(),
        config.trading.clone(),
        config.universe.clone(),
        advisor,
        repository,
        credentials
            .optional("TRADY_TRADING_INPUTS_TOKEN")?
            .map(|s| s.into_inner()),
    )?;
    let listener =
        UnixListener::bind(&path).map_err(|_| Error("cannot bind analyst socket".into()))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660))
        .map_err(|_| Error("cannot restrict analyst socket".into()))?;
    struct Endpoint {
        path: String,
        dev: u64,
        ino: u64,
    }
    impl Drop for Endpoint {
        fn drop(&mut self) {
            if std::fs::symlink_metadata(&self.path)
                .is_ok_and(|m| m.dev() == self.dev && m.ino() == self.ino)
            {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|_| Error("analyst endpoint disappeared".into()))?;
    let _endpoint = Endpoint {
        path,
        dev: metadata.dev(),
        ino: metadata.ino(),
    };
    listener
        .set_nonblocking(true)
        .map_err(|_| Error("cannot bound analyst listener".into()))?;
    let stopped = Arc::new(AtomicBool::new(false));
    let signal = stopped.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))
        .map_err(|_| Error("cannot install analyst shutdown handler".into()))?;
    while !stopped.load(Ordering::Relaxed) {
        retention.check()?;
        let (mut stream, _) = match listener.accept() {
            Ok(pair) => pair,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(_) => return Err(Error("analyst listener failed".into())),
        };
        stream
            .set_nonblocking(true)
            .map_err(|_| Error("cannot bound analyst request".into()))?;
        let deadline = Instant::now() + ANALYSIS_TIMEOUT;
        let request = socket_read(&mut stream, deadline, &stopped)
            .and_then(|b| decode::<AccountSnapshot>(&b));
        let Ok(snapshot) = request else {
            continue;
        };
        if snapshot.validate().is_err()
            || snapshot.account_id != config.trading.account_id
            || snapshot.mode != config.trading.mode
            || snapshot.spendable_cash.currency != config.trading.base_currency
        {
            continue;
        }
        let result = inputs.candidate(&snapshot, Utc::now());
        let model = candidate_model(
            &mut inputs.repository,
            result.as_ref().ok().and_then(|c| c.as_ref()),
        )?;
        let reply = Reply {
            snapshot_id: snapshot.snapshot_id,
            failed: result.is_err(),
            candidate: result.unwrap_or(None),
            model,
        };
        let _ = socket_write(&mut stream, &encode(&reply)?, deadline, &stopped);
    }
    Ok(())
}

#[cfg(not(unix))]
pub(super) fn serve_external(_: RuntimeConfig, _: &str) -> Result<()> {
    Err(Error(
        "separate analyst transport requires Unix sockets".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn malformed_or_oversized_worker_output_is_rejected() {
        assert!(read_message(&mut Cursor::new(b"truncated")).is_err());
        assert!(read_message(&mut Cursor::new(vec![b'x'; MAX_MESSAGE_BYTES + 1])).is_err());
        assert!(decode::<Reply>(b"{\"status\":\"invented\"}\n").is_err());
    }
    #[test]
    fn external_inference_must_use_the_released_model_bytes() {
        let allowed = std::collections::BTreeMap::from([("native_fixture".into(), "a".repeat(64))]);
        let actual = ModelIdentity {
            revision_id: "native_fixture".into(),
            artifact_sha256: "a".repeat(64),
        };
        approved_model(&allowed, "native_fixture", Some(&actual)).unwrap();
        assert!(approved_model(&allowed, "native_fixture", None).is_err());
        let changed = ModelIdentity {
            artifact_sha256: "b".repeat(64),
            ..actual.clone()
        };
        assert!(approved_model(&allowed, "native_fixture", Some(&changed)).is_err());
        let changed = ModelIdentity {
            revision_id: "other_native".into(),
            ..actual
        };
        assert!(approved_model(&allowed, "other_native", Some(&changed)).is_err());
        assert!(approved_model(&allowed, "native_fixture", Some(&changed)).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn separate_socket_rejects_public_endpoints_and_writable_directories() {
        use std::os::unix::{fs::PermissionsExt, net::UnixListener};
        let directory =
            std::env::temp_dir().join(trady_application::harness::new_id("analyst_socket_test"));
        std::fs::create_dir(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o750)).unwrap();
        let path = directory.join("analyst.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660)).unwrap();
        assert!(private_socket(&path).is_ok());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
        assert!(private_socket(&path).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660)).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o770)).unwrap();
        assert!(private_socket(&path).is_err());
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o750)).unwrap();
        let link = directory.with_extension("symlink");
        std::os::unix::fs::symlink(&directory, &link).unwrap();
        assert!(private_socket(&link.join("analyst.sock")).is_err());
        std::fs::remove_file(&link).unwrap();
        let child = directory.join("private");
        std::fs::create_dir(&child).unwrap();
        std::fs::set_permissions(&child, std::fs::Permissions::from_mode(0o750)).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(private_socket(&child.join("analyst.sock")).is_err());
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o750)).unwrap();
        std::fs::remove_dir(&child).unwrap();
        drop(listener);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&directory).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn separate_socket_enforces_deadlines_shutdown_and_message_bounds() {
        use std::os::unix::net::UnixStream;
        let (mut reader, mut writer) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        let running = AtomicBool::new(false);
        let started = Instant::now();
        assert!(socket_read(&mut reader, started + Duration::from_millis(20), &running).is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(
            socket_read(
                &mut reader,
                Instant::now() + ANALYSIS_TIMEOUT,
                &AtomicBool::new(true)
            )
            .is_err()
        );
        writer.write_all(b"{}\n{}\n").unwrap();
        assert!(socket_read(&mut reader, Instant::now() + ANALYSIS_TIMEOUT, &running).is_err());
        let (mut reader, mut writer) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        let sender = thread::spawn(move || {
            let _ = writer.write_all(&vec![b'x'; MAX_MESSAGE_BYTES + 1]);
        });
        assert!(socket_read(&mut reader, Instant::now() + ANALYSIS_TIMEOUT, &running).is_err());
        drop(reader);
        sender.join().unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn separate_socket_authenticates_the_listener_and_exchanges_one_message() {
        use std::os::unix::{fs::PermissionsExt, net::UnixListener};
        let directory =
            std::env::temp_dir().join(trady_application::harness::new_id("analyst_peer_test"));
        std::fs::create_dir(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o750)).unwrap();
        let path = directory.join("analyst.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o660)).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 3];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(&request, b"{}\n");
            stream.write_all(b"{\"fixture\":true}\n").unwrap();
        });
        assert_eq!(
            socket_exchange(path.to_str().unwrap(), b"{}\n", &AtomicBool::new(false)).unwrap(),
            b"{\"fixture\":true}\n"
        );
        server.join().unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn hung_worker_is_terminated_and_reaped_without_waiting_for_its_work() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exec /bin/sleep 20"]);
        let start = Instant::now();
        let mut process = WorkerProcess::start(command, b"{}\n".to_vec()).expect("worker");
        assert!(
            process
                .exchange(
                    b"{}\n".to_vec(),
                    Duration::from_millis(50),
                    &AtomicBool::new(false)
                )
                .is_err()
        );
        let _ = process.child.kill();
        assert!(process.child.wait().expect("reaped child").code().is_none());
        drop(process);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    #[cfg(unix)]
    #[test]
    fn worker_does_not_inherit_parent_credentials() {
        let mut command = Command::new("/bin/sh");
        command
            .env("TRADY_TEST_CREDENTIAL", "fixture-secret")
            .args(["-c", "printf '%s\\n' \"${TRADY_TEST_CREDENTIAL-cleared}\""]);
        let mut process = WorkerProcess::start(command, b"{}\n".to_vec()).expect("worker");
        let bytes = process
            .exchange(
                b"{}\n".to_vec(),
                Duration::from_secs(2),
                &AtomicBool::new(false),
            )
            .expect("worker response");
        assert_eq!(bytes, b"cleared\n");
    }
    #[cfg(unix)]
    #[test]
    fn shutdown_interrupts_the_analysis_wait() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exec /bin/sleep 20"]);
        let start = Instant::now();
        let mut process = WorkerProcess::start(command, b"{}\n".to_vec()).expect("worker");
        assert!(
            process
                .exchange(b"{}\n".to_vec(), ANALYSIS_TIMEOUT, &AtomicBool::new(true))
                .is_err()
        );
        drop(process);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
