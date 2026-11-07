use super::*;
use crate::Daemon;

#[derive(Clone, Debug)]
pub struct DaemonStore(Store);

#[derive(Clone, Debug)]
pub struct DaemonDocument {
    pub settings: Daemon,
    inner: Document,
}

impl From<Document> for DaemonDocument {
    fn from(inner: Document) -> Self {
        Self {
            settings: inner.settings.daemon.clone(),
            inner,
        }
    }
}

impl DaemonDocument {
    pub fn source_paths(&self) -> &[PathBuf] {
        self.inner.source_paths()
    }
}

impl DaemonStore {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self(Store::daemon(path))
    }
    pub fn at(path: PathBuf) -> Self {
        Self::new(Some(path))
    }
    pub fn path(&self) -> &Path {
        self.0.path()
    }

    /// # Errors
    /// Rejects invalid backend settings without decoding frontend preferences.
    pub fn load(&self) -> Result<DaemonDocument, Error> {
        self.0.load().map(Into::into)
    }

    /// # Errors
    /// Validates registrations and ACP policy together, preserving concurrent policy edits.
    pub fn save_agents(
        &self,
        document: &mut DaemonDocument,
        value: Vec<AgentRegistration>,
    ) -> Result<(), Error> {
        self.0.save_agents(&mut document.inner, value)?;
        *document = document.inner.clone().into();
        Ok(())
    }

    /// # Errors
    /// Rejects stale policy/catalog snapshots, invalid targets and failed writes.
    pub fn save_acp(
        &self,
        document: &mut DaemonDocument,
        value: crate::AcpLifecycle,
    ) -> Result<(), Error> {
        self.0.save(&mut document.inner, Section::Acp(value))?;
        *document = document.inner.clone().into();
        Ok(())
    }
}
