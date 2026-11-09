//! Browser handoff over an app-private directory shared with the Linux guest.
//! The launcher waits for an explicit host response; it never opens a URL itself.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub const DIRECTORY_ENV: &str = "BAS_BROWSER_DIRECTORY";
const MAX_URL_BYTES: usize = 8192;
const OPEN_TIMEOUT: Duration = Duration::from_secs(300);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("browser bridge is unavailable: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid browser bridge message: {0}")]
    Json(#[from] serde_json::Error),
    #[error("only absolute HTTP or HTTPS URLs without embedded credentials are supported")]
    InvalidUrl,
    #[error("browser request was cancelled")]
    Cancelled,
    #[error("timed out waiting for the app to open the browser")]
    Timeout,
    #[error("browser request is no longer pending")]
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Request {
    pub id: Uuid,
    pub url: String,
    expires_at: u64,
    process: u32,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Response {
    Opened,
    Cancelled,
}

/// Check a browser destination before displaying or forwarding it.
/// # Errors
/// Rejects non-web URLs, credentials and oversized input.
pub fn web_url(value: &str) -> Result<url::Url, Error> {
    if value.len() > MAX_URL_BYTES || value.chars().any(char::is_control) {
        return Err(Error::InvalidUrl);
    }
    let url = url::Url::parse(value).map_err(|_| Error::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Error::InvalidUrl);
    }
    Ok(url)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    let mut file =
        tempfile::NamedTempFile::new_in(path.parent().expect("bridge path has a parent"))?;
    serde_json::to_writer(&mut file, value)?;
    file.flush()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

fn read_request(path: &Path) -> Result<Request, Error> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take((MAX_URL_BYTES * 2) as u64)
        .read_to_end(&mut bytes)?;
    let request: Request = serde_json::from_slice(&bytes)?;
    web_url(&request.url)?;
    Ok(request)
}

#[cfg(unix)]
fn alive(pid: u32) -> bool {
    // Both processes run under the app's UID; the guest does not virtualize PIDs.
    pid > 0 && pid <= i32::MAX as u32 && unsafe { libc::kill(pid as i32, 0) == 0 }
}
#[cfg(not(unix))]
fn alive(_pid: u32) -> bool {
    true
}

#[cfg(unix)]
fn parent() -> u32 {
    unsafe { libc::getppid() as u32 }
}
#[cfg(not(unix))]
fn parent() -> u32 {
    0
}

/// Read live requests from a directory created by the Android foreground service.
/// # Errors
/// Returns directory I/O errors. Incomplete, expired and malformed entries are ignored.
pub fn pending(directory: &Path) -> Result<Vec<Request>, Error> {
    let mut requests = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json")
            || !entry.file_type()?.is_file()
        {
            continue;
        }
        let Ok(request) = read_request(&path) else {
            continue;
        };
        if path.file_stem().and_then(|s| s.to_str()) != Some(request.id.to_string().as_str()) {
            continue;
        }
        let response = directory.join(format!("{}.response", request.id));
        if request.expires_at <= now() || !alive(request.process) {
            let _ = fs::remove_file(path);
            let _ = fs::remove_file(response);
        } else if !response.exists() {
            requests.push(request);
        }
    }
    requests.sort_by_key(|r| (r.expires_at, r.id));
    Ok(requests)
}

/// Deliver the user's browser decision to the waiting launcher.
/// # Errors
/// Rejects requests that have ended or changed, and reports write errors.
pub fn respond(directory: &Path, request: &Request, response: Response) -> Result<(), Error> {
    let path = directory.join(format!("{}.json", request.id));
    if read_request(&path)? != *request || request.expires_at <= now() || !alive(request.process) {
        return Err(Error::Expired);
    }
    atomic_json(
        &directory.join(format!("{}.response", request.id)),
        &response,
    )
}

struct Files {
    request: PathBuf,
    response: PathBuf,
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.request);
        let _ = fs::remove_file(&self.response);
    }
}

/// Queue a URL and wait for the app's browser action, or cancellation/expiry.
/// # Errors
/// Reports invalid URLs, unavailable bridge, cancellation or a five-minute handoff timeout.
pub fn open(directory: &Path, url: &str) -> Result<(), Error> {
    web_url(url)?;
    let request = Request {
        id: Uuid::new_v4(),
        url: url.to_owned(),
        expires_at: now() + OPEN_TIMEOUT.as_secs(),
        process: std::process::id(),
    };
    let files = Files {
        request: directory.join(format!("{}.json", request.id)),
        response: directory.join(format!("{}.response", request.id)),
    };
    let owner = parent();
    atomic_json(&files.request, &request)?;
    loop {
        match fs::read(&files.response) {
            Ok(bytes) => {
                return match serde_json::from_slice(&bytes)? {
                    Response::Opened => Ok(()),
                    Response::Cancelled => Err(Error::Cancelled),
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        if parent() != owner {
            return Err(Error::Cancelled);
        }
        if now() >= request.expires_at {
            return Err(Error::Timeout);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}
