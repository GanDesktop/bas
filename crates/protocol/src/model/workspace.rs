use super::{Timestamp, WorkspaceId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A daemon-owned project, independent of any Agent or session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub cwd: PathBuf,
    pub archived: bool,
    pub revision: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}
