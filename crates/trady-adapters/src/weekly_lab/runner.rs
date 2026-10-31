//! Fixed argv, pinned executables, empty environment, bounded pipes and OS limits.
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use trady_application::optimization::{ToolReport, valid_hash, valid_id};
use trady_domain::{Error, Result, require};

pub const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
pub fn bytes_hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedFile {
    pub path: PathBuf,
    pub sha256: String,
}
impl PinnedFile {
    pub fn validate(&self) -> Result<()> {
        require(
            self.path.is_absolute() && valid_hash(&self.sha256),
            "helper file must have an absolute path and SHA256",
        )?;
        let meta = fs::symlink_metadata(&self.path)
            .map_err(|_| Error("pinned helper file unavailable".into()))?;
        require(
            meta.is_file() && meta.len() <= 64 * 1024 * 1024,
            "invalid pinned helper file",
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            require(
                meta.mode() & 0o022 == 0 && meta.uid() == 0,
                "helper files must be root-owned and not group/world writable",
            )?;
        }
        let bytes =
            fs::read(&self.path).map_err(|_| Error("pinned helper file unreadable".into()))?;
        require(
            bytes_hash(&bytes) == self.sha256,
            "pinned helper file hash mismatch",
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelperCommand {
    pub tool_id: String,
    pub executable: PinnedFile,
    pub arguments: Vec<String>,
    pub dependencies: Vec<PinnedFile>,
    pub working_directory: PathBuf,
    pub artifact_directory: PathBuf,
    pub timeout_seconds: u32,
    pub cpu_seconds: u32,
    pub memory_bytes: u64,
    pub max_output_bytes: usize,
}
impl HelperCommand {
    pub fn validate(&self) -> Result<()> {
        require(
            valid_id(&self.tool_id)
                && self.working_directory.is_absolute()
                && self.artifact_directory.is_absolute()
                && self.working_directory.is_dir()
                && (1..=1800).contains(&self.timeout_seconds)
                && (1..=1800).contains(&self.cpu_seconds)
                && (32 * 1024 * 1024..=16 * 1024 * 1024 * 1024).contains(&self.memory_bytes)
                && (1..=MAX_OUTPUT_BYTES).contains(&self.max_output_bytes)
                && self.arguments.len() <= 32
                && self
                    .arguments
                    .iter()
                    .all(|a| a.len() <= 4096 && !a.contains('\0')),
            "invalid helper command limits",
        )?;
        // Script interpreters are permitted only when the entire script is pinned.
        // No shell or code-evaluation switch can be supplied by a candidate/request.
        let name = self
            .executable
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        require(
            !matches!(name, "sh" | "bash" | "dash" | "zsh" | "env" | "sudo" | "su")
                && !self
                    .arguments
                    .iter()
                    .any(|a| matches!(a.as_str(), "-c" | "--command" | "-e" | "--eval")),
            "shell/eval helpers are forbidden",
        )?;
        self.executable.validate()?;
        for file in &self.dependencies {
            file.validate()?;
        }
        Ok(())
    }
    pub fn run(&self, input: &[u8], input_hash: &str) -> Result<HelperOutput> {
        self.validate()?;
        require(
            input.len() <= MAX_INPUT_BYTES && valid_hash(input_hash),
            "helper input exceeds limit or has invalid binding",
        )?;
        #[cfg(not(target_os = "linux"))]
        {
            return Err(Error(
                "weekly helper isolation requires Linux prlimit".into(),
            ));
        }
        #[cfg(target_os = "linux")]
        self.run_linux(input, input_hash)
    }
    #[cfg(target_os = "linux")]
    fn run_linux(&self, input: &[u8], input_hash: &str) -> Result<HelperOutput> {
        use std::os::unix::process::CommandExt;
        let started = Instant::now();
        let mut command = Command::new("/usr/bin/prlimit");
        command.args([
            format!("--cpu={}:{}", self.cpu_seconds, self.cpu_seconds),
            format!("--as={}:{}", self.memory_bytes, self.memory_bytes),
            "--nofile=64:64".into(),
            "--nproc=64:64".into(),
            "--core=0:0".into(),
            "--fsize=67108864:67108864".into(),
            "--".into(),
        ]);
        command
            .arg(&self.executable.path)
            .args(&self.arguments)
            .current_dir(&self.working_directory)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LANG", "C.UTF-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut child = command
            .spawn()
            .map_err(|_| Error("weekly helper unavailable (Linux prlimit required)".into()))?;
        let pid = child.id();
        let overflow = Arc::new(AtomicBool::new(false));
        let out = read_bounded(
            child
                .stdout
                .take()
                .ok_or_else(|| Error("helper stdout missing".into()))?,
            self.max_output_bytes,
            overflow.clone(),
        );
        let err = read_bounded(
            child
                .stderr
                .take()
                .ok_or_else(|| Error("helper stderr missing".into()))?,
            64 * 1024,
            overflow.clone(),
        );
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error("helper stdin missing".into()))?;
        let input_bytes_hash = bytes_hash(input);
        let input = input.to_vec();
        let writer = thread::spawn(move || stdin.write_all(&input));
        let status = loop {
            if overflow.load(Ordering::SeqCst)
                || started.elapsed() >= Duration::from_secs(u64::from(self.timeout_seconds))
            {
                kill_group(pid);
                let _ = child.kill();
                let _ = child.wait();
                let cleanup = Instant::now() + Duration::from_millis(250);
                let _ = finish_thread(writer, cleanup);
                let _ = finish_thread(out, cleanup);
                let _ = finish_thread(err, cleanup);
                return Err(Error(
                    if overflow.load(Ordering::SeqCst) {
                        "HELPER_OUTPUT_LIMIT"
                    } else {
                        "HELPER_DEADLINE_EXCEEDED"
                    }
                    .into(),
                ));
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(_) => {
                    kill_group(pid);
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error("helper wait failed".into()));
                }
            }
        };
        // Kill descendants that retained a pipe after the direct child exited.
        kill_group(pid);
        let cleanup = Instant::now() + Duration::from_millis(250);
        let wrote = finish_thread(writer, cleanup)?;
        let stdout = finish_thread(out, cleanup)??;
        let stderr = finish_thread(err, cleanup)??;
        require(!overflow.load(Ordering::SeqCst), "HELPER_OUTPUT_LIMIT")?;
        require(status.success() && wrote.is_ok(), "HELPER_PROCESS_FAILED")?;
        let report = ToolReport {
            report_id: format!("tool_{}", bytes_hash(&stdout)),
            tool_id: self.tool_id.clone(),
            input_hash: input_hash.into(),
            output_hash: bytes_hash(&stdout),
            exit_status: status.code().unwrap_or(-1),
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            recorded_at: Utc::now(),
        };
        persist_artifact(&self.artifact_directory, &report.output_hash, &stdout)?;
        let stderr_hash = bytes_hash(&stderr);
        persist_artifact(&self.artifact_directory, &stderr_hash, &stderr)?;
        let receipt = serde_json::to_vec(&serde_json::json!({"schema_version":1,"report":report,
            "executable_sha256":self.executable.sha256,"arguments":self.arguments,
            "input_bytes_hash":input_bytes_hash,"stderr_hash":stderr_hash}))
        .map_err(|_| Error("helper receipt encoding failed".into()))?;
        persist_artifact(&self.artifact_directory, &bytes_hash(&receipt), &receipt)?;
        Ok(HelperOutput {
            stdout,
            stderr_hash: bytes_hash(&stderr),
            report,
        })
    }
}
fn finish_thread<T>(handle: thread::JoinHandle<T>, deadline: Instant) -> Result<T> {
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            return Err(Error("HELPER_PIPE_DEADLINE".into()));
        }
        thread::sleep(Duration::from_millis(5));
    }
    handle
        .join()
        .map_err(|_| Error("helper pipe worker failed".into()))
}
fn persist_artifact(directory: &Path, hash: &str, bytes: &[u8]) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(directory)
        .map_err(|_| Error("helper artifact directory unavailable".into()))?;
    let meta = fs::symlink_metadata(directory)
        .map_err(|_| Error("helper artifact directory unavailable".into()))?;
    require(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "invalid helper artifact directory",
    )?;
    let path = directory.join(hash);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        require(
            meta.mode() & 0o022 == 0,
            "helper artifact directory cannot be group/other writable",
        )?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(mut file) => {
            file.write_all(bytes)
                .and_then(|()| file.sync_all())
                .map_err(|_| Error("helper artifact durable write failed".into()))?;
            fs::File::open(directory)
                .and_then(|d| d.sync_all())
                .map_err(|_| Error("helper artifact directory sync failed".into()))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let meta = fs::symlink_metadata(&path)
                .map_err(|_| Error("existing helper artifact unavailable".into()))?;
            require(
                meta.is_file() && meta.len() <= MAX_OUTPUT_BYTES as u64,
                "invalid helper artifact",
            )?;
            require(
                bytes_hash(
                    &fs::read(&path).map_err(|_| Error("helper artifact read failed".into()))?,
                ) == hash,
                "helper artifact content tampered",
            )?;
        }
        Err(_) => return Err(Error("helper artifact write failed".into())),
    };
    Ok(())
}
fn read_bounded<R: Read + Send + 'static>(
    mut reader: R,
    limit: usize,
    overflow: Arc<AtomicBool>,
) -> thread::JoinHandle<Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut result = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            let n = reader
                .read(&mut buffer)
                .map_err(|_| Error("helper pipe read failed".into()))?;
            if n == 0 {
                return Ok(result);
            }
            if result.len().saturating_add(n) > limit {
                overflow.store(true, Ordering::SeqCst);
                return Ok(result);
            }
            result.extend_from_slice(&buffer[..n]);
        }
    })
}
#[cfg(target_os = "linux")]
fn kill_group(pid: u32) {
    let _ = Command::new("/bin/kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
pub struct HelperOutput {
    pub stdout: Vec<u8>,
    pub stderr_hash: String,
    pub report: ToolReport,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response<T> {
    pub schema_version: u32,
    pub input_hash: String,
    pub payload: T,
}
impl HelperOutput {
    pub fn payload<T: serde::de::DeserializeOwned>(&self, expected: &str) -> Result<T> {
        let text = std::str::from_utf8(&self.stdout)
            .map_err(|_| Error("helper returned invalid UTF-8".into()))?;
        let value = trady_application::contracts::parse_json_bounded(text, MAX_OUTPUT_BYTES)?;
        let response: Response<T> = serde_json::from_value(value)
            .map_err(|_| Error("helper response schema mismatch".into()))?;
        require(
            response.schema_version == 1 && response.input_hash == expected,
            "helper response input binding mismatch",
        )?;
        Ok(response.payload)
    }
}

pub fn trusted_file(path: &Path) -> Result<()> {
    let meta =
        fs::symlink_metadata(path).map_err(|_| Error("weekly configuration missing".into()))?;
    require(
        meta.is_file() && meta.len() <= MAX_INPUT_BYTES as u64,
        "invalid weekly configuration file",
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        require(
            meta.uid() == 0 && meta.mode() & 0o022 == 0,
            "weekly helper configuration must be root-owned and not group/world writable",
        )?;
    }
    Ok(())
}
