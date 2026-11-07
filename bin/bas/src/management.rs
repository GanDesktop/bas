//! The daemon owns ACP downloads, runtime provisioning and settings registration.

mod artifact;
mod authentication;
mod catalog;
mod configuration;
pub(crate) mod installation;
mod policy;
mod progress;
mod registry;
mod runtime;
use configuration::configuration_error;
use progress::Progress;

pub use authentication::invocation as authentication_invocation;

use anyhow::{Context, Result};
use bas_config::{AgentRegistration, AgentSpec, DaemonStore as Store};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf, time::Duration};
use tokio::sync::Mutex;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("installation cancelled")]
    Cancelled,
    #[error(
        "this daemon uses command-line agent overrides; managed installation requires a settings store"
    )]
    NoStore,
    #[error("agent {0} was not found in the ACP registry")]
    UnknownAgent(String),
    #[error(
        "agent {0} is configured manually; use another ID or remove the custom configuration first"
    )]
    RegistrationConflict(String),
    #[error("agent {0} is not a managed installation")]
    NotManaged(String),
    #[error("downloaded archive checksum does not match the registry")]
    Checksum,
    #[error("download or archive exceeds the supported size/file limit")]
    ArchiveLimit,
    #[error("unsupported archive format")]
    UnsupportedArchive,
    #[error("archive contains a link or special file: {0}")]
    ArchiveLink(String),
    #[error("managed storage cannot contain a symbolic link: {0}")]
    StorageLink(PathBuf),
    #[error("managed installation has no regular executable at {0}")]
    MissingExecutable(PathBuf),
    #[error("official Node runtime is unavailable for this architecture")]
    UnsupportedRuntime,
    #[error("installed npm package version differs from the registry")]
    PackageVersion,
    #[error("managed installation metadata has an unsupported schema version")]
    RecordsVersion,
    #[error("another daemon is installing or removing agents")]
    Busy,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Record {
    manifest: registry::Manifest,
    directory: PathBuf,
    spec: AgentSpec,
    runtime: Option<runtime::Runtime>,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
    #[serde(default)]
    launch: Option<authentication::LaunchBoundary>,
}

#[derive(Default)]
struct RegistryMemory {
    registry: Option<registry::Registry>,
    warning: Option<String>,
}

pub struct Manager {
    store: Option<Store>,
    profile: String,
    root: PathBuf,
    url: String,
    client: reqwest::Client,
    registry: Mutex<RegistryMemory>,
    mutation: Mutex<()>,
    installations: TaskTracker,
    stopping: CancellationToken,
    admission: std::sync::Mutex<()>,
}

impl Manager {
    /// # Errors
    /// Rejects unsafe data directories/registry URLs and HTTP client initialization failures.
    pub fn new(store: Option<Store>) -> Result<Self> {
        let root = match std::env::var_os("BAS_ACP_DATA_DIR") {
            Some(path) => PathBuf::from(path),
            None => dirs::data_local_dir()
                .context("user data directory is unavailable")?
                .join("bas/acp"),
        };
        if !root.is_absolute()
            || root
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            anyhow::bail!("BAS_ACP_DATA_DIR must be absolute without parent traversal");
        }
        artifact::reject_directory_links(&root)?;
        let url =
            std::env::var("BAS_ACP_REGISTRY_URL").unwrap_or_else(|_| registry::REGISTRY_URL.into());
        registry::download_url(&url)?;
        let client = reqwest::Client::builder()
            .user_agent(concat!("bas/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(300))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 10 {
                    attempt.error("too many redirects")
                } else if registry::download_url(attempt.url().as_str()).is_err() {
                    attempt.error("unsafe download redirect")
                } else {
                    attempt.follow()
                }
            }))
            .build()?;
        Ok(Self {
            store,
            profile: uuid::Uuid::new_v4().to_string(),
            root,
            url,
            client,
            registry: Mutex::new(RegistryMemory::default()),
            mutation: Mutex::new(()),
            installations: TaskTracker::new(),
            stopping: CancellationToken::new(),
            admission: std::sync::Mutex::new(()),
        })
    }

    pub fn store(&self) -> Option<&Store> {
        self.store.as_ref()
    }

    /// Cancels installation work and waits for staging and process cleanup.
    pub async fn shutdown(&self) {
        {
            let _admission = self.admission.lock().expect("installation admission lock");
            self.stopping.cancel();
            self.installations.close();
        }
        self.installations.wait().await;
    }

    fn mutation_lock(&self) -> Result<DiskLock> {
        artifact::safe_directory(&self.root)?;
        let path = self.root.join("operation.lock");
        artifact::reject_file_link(&path)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .read(true)
            .open(path)?;
        file.try_lock().map_err(|error| match error {
            fs::TryLockError::WouldBlock => anyhow::Error::from(Error::Busy),
            fs::TryLockError::Error(error) => error.into(),
        })?;
        Ok(DiskLock(file))
    }
}

struct DiskLock(fs::File);

impl Drop for DiskLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
