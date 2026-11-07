use bas_protocol::protocol::pb;
use std::path::{Path, PathBuf};

const PAGE_SIZE: usize = 128;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot browse directory: {0}")]
    Io(#[from] std::io::Error),
    #[error("directory path is not UTF-8")]
    Encoding,
}

fn path_string(path: &Path) -> Result<String, Error> {
    path.to_str().map(str::to_owned).ok_or(Error::Encoding)
}

/// # Errors
/// Returns filesystem errors and rejects paths that cannot be represented on the wire.
pub async fn directories(
    request: pb::BrowseServerDirectoriesRequest,
) -> Result<pb::BrowseServerDirectoriesResponse, Error> {
    let path = match request.path {
        Some(path) => PathBuf::from(path),
        None => dirs::home_dir().map_or_else(std::env::current_dir, Ok)?,
    };
    let path = tokio::fs::canonicalize(path).await?;
    let mut reader = tokio::fs::read_dir(&path).await?;
    // Keep only the next page while scanning; memory is independent of directory size.
    let mut entries = std::collections::BTreeMap::new();
    while let Some(entry) = reader.next_entry().await? {
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| Error::Encoding)?;
        if request
            .cursor
            .as_ref()
            .is_some_and(|cursor| name <= *cursor)
        {
            continue;
        }
        let kind = entry.file_type().await?;
        let directory = if kind.is_symlink() {
            match tokio::fs::metadata(entry.path()).await {
                Ok(metadata) => metadata.is_dir(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => return Err(error.into()),
            }
        } else {
            kind.is_dir()
        };
        if directory {
            entries.insert(
                name.clone(),
                pb::ServerDirectory {
                    name,
                    path: path_string(&entry.path())?,
                },
            );
            if entries.len() > PAGE_SIZE + 1 {
                entries.pop_last();
            }
        }
    }
    let next_cursor = if entries.len() > PAGE_SIZE {
        entries.pop_last();
        entries.last_key_value().map(|(name, _)| name.clone())
    } else {
        None
    };
    Ok(pb::BrowseServerDirectoriesResponse {
        path: path_string(&path)?,
        parent: path.parent().map(path_string).transpose()?,
        directories: entries.into_values().collect(),
        next_cursor,
    })
}
