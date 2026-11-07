//! Immutable version descriptors and the single settings activation boundary.

use super::{Error, Manager, Progress, Record, artifact, authentication, registry, runtime};
use anyhow::{Context, Result};
use bas_config::{
    AgentRegistration, AgentSpec, DaemonDocument as Document, DaemonStore as Store,
    INSTALLATION_FILE, InstallationRef, ManagedAgent, agent::Command,
};
use bas_protocol::{management::Phase, protocol::pb::AgentOperation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Component, sync::Arc};
use tokio::sync::mpsc;

const SCHEMA_VERSION: u32 = 1;

#[derive(Deserialize, Serialize)]
struct Descriptor {
    schema_version: u32,
    #[serde(flatten)]
    record: Record,
}

#[derive(Debug, thiserror::Error)]
enum DescriptorError {
    #[error("managed installation metadata has an invalid identity or directory")]
    Identity,
    #[error("managed installation differs from the selected launch configuration")]
    Spec,
    #[error("managed installation has no trusted launch boundary")]
    Launch,
}

/// # Errors
/// Rejects missing, altered, unsupported, or incorrectly bound descriptors.
pub(crate) fn load_record(managed: &ManagedAgent) -> Result<Record> {
    let bytes = managed.installation.read()?;
    let descriptor: Descriptor =
        serde_json::from_slice(&bytes).context("invalid managed installation metadata")?;
    if descriptor.schema_version != SCHEMA_VERSION {
        return Err(Error::RecordsVersion.into());
    }
    let record = descriptor.record;
    registry::validate_component(managed.id.as_str())?;
    if record.manifest.id != managed.id.as_str()
        || record.spec.id != managed.id
        || record.spec != managed.resolve()?
        || managed.installation.path.file_name() != Some(INSTALLATION_FILE.as_ref())
        || managed.installation.path.parent() != Some(record.directory.as_path())
        || !record.directory.is_absolute()
        || record
            .directory
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(DescriptorError::Identity.into());
    }
    artifact::reject_directory_links(&record.directory)?;
    Ok(record)
}

/// Resolves the full daemon-owned descriptor, not just its shared launch projection.
///
/// # Errors
/// Rejects invalid or incomplete installation metadata.
pub(crate) fn resolve(managed: &ManagedAgent) -> Result<AgentSpec> {
    Ok(load_record(managed)?.spec)
}

/// # Errors
/// Rejects stale, missing or modified managed descriptors before starting a process.
pub(crate) fn validate_spec(managed: &ManagedAgent, spec: &AgentSpec) -> Result<()> {
    if resolve(managed)? != *spec {
        return Err(DescriptorError::Spec.into());
    }
    Ok(())
}

/// # Errors
/// Rejects any custom registration using the requested identity.
pub(super) fn check_identity(id: &str, agents: &[AgentRegistration]) -> Result<()> {
    if agents
        .iter()
        .any(|agent| agent.id().as_str() == id && agent.as_custom().is_some())
    {
        return Err(Error::RegistrationConflict(id.into()).into());
    }
    Ok(())
}

/// # Errors
/// Reports filesystem or descriptor failures before a version can be activated.
pub(super) fn finalize(staging: tempfile::TempDir, mut record: Record) -> Result<ManagedAgent> {
    // Runtime launch boundaries can depend on files at their final absolute paths.
    fs::rename(staging.path(), &record.directory)?;
    let mut unfinished = UnfinishedDirectory(Some(record.directory.clone()));
    record.launch =
        Some(authentication::LaunchBoundary::from_record(&record).ok_or(DescriptorError::Launch)?);
    let path = record.directory.join(INSTALLATION_FILE);
    let id = record.spec.id.clone();
    artifact::atomic_json(
        &path,
        &Descriptor {
            schema_version: SCHEMA_VERSION,
            record,
        },
    )?;
    let installation = InstallationRef::from_bytes(path.clone(), &fs::read(&path)?)?;
    let managed = ManagedAgent { id, installation };
    load_record(&managed)?;
    // After this point a settings commit may have succeeded even when it reports an error.
    // Retain the complete version independently of activation's outcome.
    unfinished.0 = None;
    Ok(managed)
}

/// # Errors
/// Rejects custom collisions, invalid descriptors, and stale or unwritable settings.
pub(super) fn activate(
    store: &Store,
    document: &mut Document,
    managed: ManagedAgent,
) -> Result<()> {
    load_record(&managed)?;
    let mut agents = document.settings.agents.clone();
    check_identity(managed.id.as_str(), &agents)?;
    if let Some(existing) = agents.iter_mut().find(|agent| agent.id() == &managed.id) {
        *existing = AgentRegistration::Managed(managed);
    } else {
        agents.push(AgentRegistration::Managed(managed));
    }
    store.save_agents(document, agents)?;
    Ok(())
}

struct UnfinishedDirectory(Option<std::path::PathBuf>);

impl Drop for UnfinishedDirectory {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = fs::remove_dir_all(path);
        }
    }
}

impl Manager {
    /// # Errors
    /// Reports unknown/unsupported agents, cancellation, download/runtime failures and settings conflicts.
    pub async fn install(
        self: &Arc<Self>,
        id: &str,
        progress: mpsc::Sender<AgentOperation>,
    ) -> Result<AgentSpec> {
        registry::validate_component(id)?;
        let cancellation = self.stopping.child_token();
        let _cancel_on_drop = cancellation.clone().drop_guard();
        let task = {
            // Registration and closing the tracker are atomic relative to one another.
            let _admission = self.admission.lock().expect("installation admission lock");
            if self.stopping.is_cancelled() {
                return Err(Error::Cancelled.into());
            }
            let manager = Arc::clone(self);
            let id = id.to_owned();
            let progress = Progress {
                sender: Some(progress),
                cancellation,
            };
            self.installations
                .spawn(async move { manager.install_owned(&id, progress).await })
        };
        // Dropping this waiter detaches the tracked task, never its cleanup.
        task.await.context("installation task failed")?
    }

    async fn install_owned(&self, id: &str, progress: Progress) -> Result<AgentSpec> {
        progress.check()?;
        let _guard = tokio::select! {
            biased;
            _ = progress.closed() => return Err(Error::Cancelled.into()),
            guard = self.mutation.lock() => guard,
        };
        progress.check()?;
        let _disk_lock = self.mutation_lock()?;
        let store = self.store.as_ref().ok_or(Error::NoStore)?;
        // Preserve this snapshot throughout downloads so external edits are never overwritten.
        let mut document = store.load()?;
        let old_agents = document.settings.agents.clone();
        check_identity(id, &old_agents)?;
        progress.send(Phase::Resolving).await?;
        let snapshot = self.load_registry(false, &progress).await?;
        let registry = snapshot.registry.context("ACP registry is unavailable")?;
        let manifest = registry
            .agents
            .into_iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| Error::UnknownAgent(id.into()))?;
        let selected = manifest.select()?;
        let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&manifest)?));
        let versions = self.root.join("agents").join(id);
        progress.check()?;
        artifact::safe_directory(&versions)?;
        // Different runtime/dependency resolutions must never reuse a previous version's contents.
        let destination = versions.join(format!(
            "{}-{}-{}",
            manifest.version,
            &fingerprint[..12],
            uuid::Uuid::new_v4().simple()
        ));
        artifact::reject_file_link(&destination)?;
        let mut staging = tempfile::Builder::new()
            .prefix(".install-")
            .tempdir_in(&versions)?;
        let (spec, runtime, dependencies) = match selected {
            registry::Selected::Binary(binary) => {
                progress.send(Phase::Downloading).await?;
                let archive = staging.path().join("download");
                artifact::download(
                    &self.client,
                    &binary.archive,
                    &archive,
                    binary.sha256.as_deref(),
                    &progress,
                )
                .await?;
                progress.send(Phase::Extracting).await?;
                let url = binary.archive.clone();
                let executable = binary.cmd.clone();
                staging = tokio::task::spawn_blocking(move || -> Result<tempfile::TempDir> {
                    artifact::extract(&archive, &url, staging.path(), &executable, false)?;
                    fs::remove_file(archive)?;
                    artifact::executable(
                        &staging.path().join(registry::safe_relative(&executable)?),
                    )?;
                    Ok(staging)
                })
                .await??;
                let mut spec = AgentSpec::acp(
                    id,
                    Command::new(
                        destination.join(registry::safe_relative(&binary.cmd)?),
                        binary.args.clone(),
                    ),
                );
                spec.env = binary.env.clone();
                (spec, None, BTreeMap::new())
            }
            registry::Selected::Npm(npm) => {
                let installed = runtime::install(
                    &self.client,
                    &self.root,
                    staging.path(),
                    &destination,
                    &manifest,
                    npm,
                    &progress,
                )
                .await?;
                let mut spec = AgentSpec::acp(id, installed.command);
                spec.env = installed.env;
                (spec, Some(installed.runtime), installed.dependencies)
            }
        };
        progress.send(Phase::Registering).await?;
        progress.check()?;
        let record = Record {
            manifest,
            directory: destination,
            spec: spec.clone(),
            runtime,
            dependencies,
            launch: None,
        };
        let managed = finalize(staging, record)?;
        progress.check()?;
        activate(store, &mut document, managed)?;
        Ok(spec)
    }

    /// # Errors
    /// Rejects custom registrations and failed settings saves.
    pub async fn remove(&self, id: &str) -> Result<Vec<AgentRegistration>> {
        registry::validate_component(id)?;
        let _guard = self.mutation.lock().await;
        let _disk_lock = self.mutation_lock()?;
        let store = self.store.as_ref().ok_or(Error::NoStore)?;
        let mut document = store.load()?;
        let old_agents = document.settings.agents.clone();
        if !old_agents
            .iter()
            .any(|agent| agent.id().as_str() == id && agent.managed().is_some())
        {
            return Err(Error::NotManaged(id.into()).into());
        }
        let mut agents: Vec<_> = old_agents
            .iter()
            .filter(|agent| agent.id().as_str() != id)
            .cloned()
            .collect();
        if agents.is_empty() {
            agents.push(AgentSpec::embedded().into());
        }
        store.save_agents(&mut document, agents.clone())?;
        Ok(agents)
    }
}
