pub(crate) mod browser;

#[cfg(target_os = "android")]
mod android;

use crate::shared::i18n::tr;
use nsh_core::prelude::*;
use serde::Deserialize;

#[derive(Clone, Default, Deserialize)]
pub(crate) struct Status {
    pub available: bool,
    pub phase: String,
    pub error: String,
    pub endpoint: String,
    #[serde(default)]
    pub browser_directory: String,
}

#[derive(Clone, Default)]
struct Tasks {
    owner: Option<Owner>,
}

pub(crate) fn status() -> Signal<Status> {
    global::<Status>()
}

pub(crate) fn initialize() {
    global::<Tasks>().update(|tasks| tasks.owner = Some(Owner::current()));
    #[cfg(target_os = "android")]
    spawn(async {
        loop {
            refresh("status").await;
            timer(std::time::Duration::from_secs(1)).await;
        }
    });
}

pub(crate) fn start() {
    command("start");
}
pub(crate) fn stop() {
    command("stop");
}

/// Creates the initial phone workspace after a real RPC health check.
///
/// # Errors
/// Returns connection, timeout, or workspace creation failures.
pub(crate) async fn prepare(endpoint: String) -> anyhow::Result<()> {
    background(async move {
        tokio::time::timeout(std::time::Duration::from_secs(30), async move {
            use bas_client::service::{Client, pb};
            let mut client = Client::connect_target(&endpoint.parse()?).await?;
            let workspaces = client
                .list_workspaces(pb::ListWorkspacesRequest::default())
                .await?;
            if workspaces.into_inner().workspaces.is_empty() {
                client
                    .open_workspace(pb::OpenWorkspaceRequest {
                        provider_id: "directory".into(),
                        schema_version: 1,
                        parameters_json: br#"{"directory":"/home/bas/workspace"}"#.to_vec(),
                        name: None,
                    })
                    .await?;
            }
            anyhow::Ok(())
        })
        .await?
    })
    .await
    .map_err(|error| anyhow::anyhow!(error.to_string()))?
}

fn command(action: &'static str) {
    task(async move {
        refresh(action).await;
    });
}

fn task(future: impl std::future::Future<Output = ()> + 'static) {
    global::<Tasks>()
        .get_untracked()
        .owner
        .expect("runtime initialized")
        .run(|| {
            spawn(future);
        });
}

async fn refresh(action: &'static str) {
    let result = background(async move { call(action) }).await;
    match result {
        Ok(Ok(value)) => {
            let directory = (value.phase == "running" && !value.browser_directory.is_empty())
                .then(|| std::path::PathBuf::from(&value.browser_directory));
            status().set(value);
            browser::watch(directory);
        }
        Ok(Err(error)) => status().update(|s| s.error = error.to_string()),
        Err(error) => status().update(|s| s.error = error.to_string()),
    }
}

fn call(action: &str) -> anyhow::Result<Status> {
    #[cfg(target_os = "android")]
    {
        android::call(action)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = action;
        Ok(Status::default())
    }
}

pub(crate) fn busy() -> bool {
    status().with(|s| matches!(s.phase.as_str(), "starting" | "stopping"))
}

pub(crate) fn description() -> String {
    status().with(|s| {
        if !s.error.is_empty() {
            return s.error.clone();
        }
        match s.phase.as_str() {
            "starting" => tr!("local-service-starting"),
            "running" => tr!("local-service-running"),
            "stopping" => tr!("local-service-stopping"),
            _ => tr!("local-service-stopped"),
        }
    })
}
