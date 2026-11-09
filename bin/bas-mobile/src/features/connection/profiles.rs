use super::{CONNECTIONS_KEY, RPC_TIMEOUT, state, task};
use crate::shared::i18n::tr;
use bas_client::service::{Client, pb};
use nsh_core::{prelude::*, storage::Storage};
use std::collections::BTreeMap;

const NAMES_KEY: &str = "bas.connection-names";
#[derive(Clone, Default)]
pub(crate) struct Profiles {
    pub names: BTreeMap<String, String>,
    pub testing: bool,
    pub message: String,
    pub message_address: String,
}
pub(crate) fn profiles() -> Signal<Profiles> {
    global::<Profiles>()
}
pub(crate) fn initialize() {
    profiles().update(|p| {
        p.names = Storage::get_item(NAMES_KEY)
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default()
    });
}
pub(crate) fn name(address: &str) -> String {
    profiles()
        .with(|p| p.names.get(address).cloned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if address == "local" {
                tr!("local")
            } else {
                address.into()
            }
        })
}
pub(crate) fn save(address: &str, name: &str) {
    let address = address.trim();
    if address.is_empty() {
        return;
    }
    state().update(|s| {
        if !s.addresses.iter().any(|a| a == address) {
            s.addresses.push(address.into());
        }
    });
    profiles().update(|p| {
        p.names.insert(address.into(), name.trim().into());
    });
    let result = Storage::set_item(
        CONNECTIONS_KEY,
        serde_json::to_string(&state().get_untracked().addresses).expect("addresses serialize"),
    )
    .and_then(|_| {
        Storage::set_item(
            NAMES_KEY,
            serde_json::to_string(&profiles().get_untracked().names).expect("names serialize"),
        )
    });
    profiles().update(|p| {
        p.message_address = address.into();
        p.message = match result {
            Ok(()) => tr!("saved"),
            Err(e) => e.to_string(),
        }
    });
}
pub(crate) fn test(address: String) {
    if profiles().get_untracked().testing {
        return;
    }
    profiles().update(|p| {
        p.testing = true;
        p.message.clear();
        p.message_address = address.clone();
    });
    let tested_address = address.clone();
    task(async move {
        let result = background(async move {
            tokio::time::timeout(RPC_TIMEOUT, async move {
                let mut client = if address.starts_with("http://") {
                    Client::connect(address).await?
                } else {
                    Client::connect_target(&address.parse()?).await?
                };
                client
                    .list_workspaces(pb::ListWorkspacesRequest::default())
                    .await?;
                anyhow::Ok(())
            })
            .await?
        })
        .await;
        profiles().update(|p| {
            p.testing = false;
            p.message_address = tested_address;
            p.message = match result {
                Ok(Ok(())) => tr!("connection-success"),
                Ok(Err(e)) => e.to_string(),
                Err(e) => e.to_string(),
            };
        });
    });
}
