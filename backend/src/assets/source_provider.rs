use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::time::{timeout_at, Instant};

use crate::lake::{LakeError, LakeKey, LakeStore};

use super::{AssetId, AssetPartition, AssetSourceInputs};

const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const DEFAULT_MAX_STDOUT_BYTES: usize = 16 * 1024 * 1024;

/// Typed control-plane request sent to an external source collector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInputRequest {
    pub project_root: PathBuf,
    pub partition: AssetPartition,
    pub planned_at: DateTime<Utc>,
    #[serde(default)]
    pub requested_assets: Vec<AssetId>,
}

/// Loads ephemeral source records for one asset DAG run.
///
/// Providers only assemble typed inputs. Durable artifacts and promotion stay
/// inside the Rust asset executor.
pub trait SourceInputProvider: Send + Sync {
    fn load<'a>(
        &'a self,
        request: &'a SourceInputRequest,
        lake: &'a LakeStore,
    ) -> BoxFuture<'a, Result<Option<AssetSourceInputs>, SourceInputProviderError>>;
}

#[derive(Debug, Clone)]
pub struct LocalFileSourceInputProvider {
    path: PathBuf,
}

impl LocalFileSourceInputProvider {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl SourceInputProvider for LocalFileSourceInputProvider {
    fn load<'a>(
        &'a self,
        _request: &'a SourceInputRequest,
        _lake: &'a LakeStore,
    ) -> BoxFuture<'a, Result<Option<AssetSourceInputs>, SourceInputProviderError>> {
        async move {
            let bytes = tokio::fs::read(&self.path).await?;
            Ok(Some(serde_json::from_slice(&bytes)?))
        }
        .boxed()
    }
}

#[derive(Debug, Clone)]
pub struct LakeObjectSourceInputProvider {
    key: LakeKey,
}

impl LakeObjectSourceInputProvider {
    pub fn new(key: LakeKey) -> Self {
        Self { key }
    }
}

impl SourceInputProvider for LakeObjectSourceInputProvider {
    fn load<'a>(
        &'a self,
        _request: &'a SourceInputRequest,
        lake: &'a LakeStore,
    ) -> BoxFuture<'a, Result<Option<AssetSourceInputs>, SourceInputProviderError>> {
        async move { Ok(Some(lake.get_json(&self.key).await?)) }.boxed()
    }
}

#[derive(Debug, Clone)]
pub struct CommandSourceInputProvider {
    program: PathBuf,
    args: Vec<OsString>,
    timeout: Duration,
    max_stdout_bytes: usize,
}

impl CommandSourceInputProvider {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            timeout: DEFAULT_COMMAND_TIMEOUT,
            max_stdout_bytes: DEFAULT_MAX_STDOUT_BYTES,
        }
    }

    pub fn with_arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn with_args(mut self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_max_stdout_bytes(mut self, max_stdout_bytes: usize) -> Self {
        self.max_stdout_bytes = max_stdout_bytes;
        self
    }
}

impl SourceInputProvider for CommandSourceInputProvider {
    fn load<'a>(
        &'a self,
        request: &'a SourceInputRequest,
        _lake: &'a LakeStore,
    ) -> BoxFuture<'a, Result<Option<AssetSourceInputs>, SourceInputProviderError>> {
        async move {
            let mut child = Command::new(&self.program)
                .args(&self.args)
                .current_dir(&request.project_root)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .kill_on_drop(true)
                .spawn()
                .map_err(|source| SourceInputProviderError::Spawn {
                    program: self.program.clone(),
                    source,
                })?;

            let request_bytes = serde_json::to_vec(request)?;
            let mut stdin = child
                .stdin
                .take()
                .ok_or(SourceInputProviderError::MissingStdin)?;
            stdin.write_all(&request_bytes).await?;
            stdin.shutdown().await?;
            drop(stdin);

            let stdout = child
                .stdout
                .take()
                .ok_or(SourceInputProviderError::MissingStdout)?;
            let deadline = Instant::now() + self.timeout;
            let mut output = Vec::new();
            let mut limited_stdout = stdout.take(self.max_stdout_bytes.saturating_add(1) as u64);
            match timeout_at(deadline, limited_stdout.read_to_end(&mut output)).await {
                Ok(result) => result?,
                Err(_) => {
                    terminate_child(&mut child).await;
                    return Err(SourceInputProviderError::TimedOut {
                        program: self.program.clone(),
                        timeout: self.timeout,
                    });
                }
            };
            if output.len() > self.max_stdout_bytes {
                terminate_child(&mut child).await;
                return Err(SourceInputProviderError::OutputTooLarge {
                    program: self.program.clone(),
                    max_bytes: self.max_stdout_bytes,
                });
            }

            let status = match timeout_at(deadline, child.wait()).await {
                Ok(result) => result?,
                Err(_) => {
                    terminate_child(&mut child).await;
                    return Err(SourceInputProviderError::TimedOut {
                        program: self.program.clone(),
                        timeout: self.timeout,
                    });
                }
            };
            if !status.success() {
                return Err(SourceInputProviderError::CommandFailed {
                    program: self.program.clone(),
                    exit_code: status.code(),
                });
            }

            Ok(Some(serde_json::from_slice(&output)?))
        }
        .boxed()
    }
}

async fn terminate_child(child: &mut Child) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}

#[derive(Debug)]
pub enum SourceInputProviderError {
    CommandFailed {
        program: PathBuf,
        exit_code: Option<i32>,
    },
    Io(std::io::Error),
    Json(serde_json::Error),
    Lake(LakeError),
    MissingStdin,
    MissingStdout,
    OutputTooLarge {
        program: PathBuf,
        max_bytes: usize,
    },
    Spawn {
        program: PathBuf,
        source: std::io::Error,
    },
    TimedOut {
        program: PathBuf,
        timeout: Duration,
    },
}

impl fmt::Display for SourceInputProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandFailed { program, exit_code } => write!(
                f,
                "source collector {} exited unsuccessfully with code {}",
                program.display(),
                exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "signal".to_string())
            ),
            Self::Io(err) => write!(f, "source input IO error: {err}"),
            Self::Json(err) => write!(f, "source input JSON error: {err}"),
            Self::Lake(err) => write!(f, "source input lake error: {err}"),
            Self::MissingStdin => f.write_str("source collector stdin was not available"),
            Self::MissingStdout => f.write_str("source collector stdout was not available"),
            Self::OutputTooLarge { program, max_bytes } => write!(
                f,
                "source collector {} exceeded the {max_bytes}-byte stdout limit",
                program.display()
            ),
            Self::Spawn { program, source } => write!(
                f,
                "failed to start source collector {}: {source}",
                program.display()
            ),
            Self::TimedOut { program, timeout } => write!(
                f,
                "source collector {} exceeded its {}-second timeout",
                program.display(),
                timeout.as_secs()
            ),
        }
    }
}

impl std::error::Error for SourceInputProviderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Json(err) => Some(err),
            Self::Lake(err) => Some(err),
            Self::Spawn { source, .. } => Some(source),
            Self::CommandFailed { .. }
            | Self::MissingStdin
            | Self::MissingStdout
            | Self::OutputTooLarge { .. }
            | Self::TimedOut { .. } => None,
        }
    }
}

impl From<std::io::Error> for SourceInputProviderError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for SourceInputProviderError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<LakeError> for SourceInputProviderError {
    fn from(value: LakeError) -> Self {
        Self::Lake(value)
    }
}
