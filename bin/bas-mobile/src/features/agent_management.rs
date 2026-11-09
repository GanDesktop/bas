pub(crate) mod auth;

use crate::{
    entities::agent::{Key, LoginStatus},
    shared::{api, i18n::tr, routes},
};
use bas_client::{
    config::{AgentRegistration, AgentSpec, agent::Command},
    management::{Catalog, Entry},
    service::pb,
};
use nsh_core::prelude::*;

#[derive(Clone, Default)]
pub(crate) struct Editor {
    pub original: Option<String>,
    pub id: String,
    pub description: String,
    pub program: String,
    pub args: String,
    pub env: String,
    pub grace: String,
}
#[derive(Clone, Default)]
pub(crate) struct State {
    pub catalog: Option<Catalog>,
    pub configuration: Option<pb::AgentConfiguration>,
    pub search: String,
    pub busy: bool,
    pub message: String,
    pub error: String,
    pub editor: Editor,
    pub selected: Option<Entry>,
    pub pending_remove: Option<Entry>,
    pub check: Option<(Entry, pb::CheckAgentResponse)>,
}
pub(crate) fn state() -> Signal<State> {
    global::<State>()
}

pub(crate) fn load(refresh: bool) {
    let selected = state().get_untracked().selected;
    state().set(State {
        selected,
        busy: true,
        ..Default::default()
    });
    api::run(
        move |mut client| async move {
            let catalog = client
                .list_agent_catalog(pb::ListAgentCatalogRequest { refresh })
                .await?
                .into_inner();
            let configuration = client
                .get_agent_configuration(pb::GetAgentConfigurationRequest {})
                .await?
                .into_inner();
            anyhow::Ok((
                serde_json::from_slice::<Catalog>(&catalog.catalog_json)?,
                configuration,
            ))
        },
        |result| {
            state().update(|s| {
                s.busy = false;
                match result {
                    Ok((catalog, configuration)) => {
                        if let Some(selected) = &s.selected {
                            s.selected = catalog
                                .entries
                                .iter()
                                .find(|e| e.id == selected.id)
                                .cloned();
                        }
                        s.catalog = Some(catalog);
                        s.configuration = Some(configuration);
                    }
                    Err(error) => s.error = error.to_string(),
                }
            })
        },
    );
}
pub(crate) fn refresh() {
    load(true);
}
pub(crate) fn search(value: &str) {
    state().update(|s| s.search = value.into());
}
pub(crate) fn entries(catalog: bool) -> Vec<Entry> {
    let mut entries: Vec<Entry> = state().with(|s| {
        s.catalog
            .as_ref()
            .map(|c| {
                c.entries
                    .iter()
                    .filter(|e| {
                        (catalog || e.spec.is_some())
                            && format!("{} {} {}", e.id, e.name, e.description)
                                .to_lowercase()
                                .contains(&s.search.to_lowercase())
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    });
    if !catalog {
        entries.sort_by_key(|e| match e.source {
            bas_client::management::Source::Registry => 0,
            bas_client::management::Source::Custom => 1,
            bas_client::management::Source::Embedded => 2,
        });
    }
    entries
}
pub(crate) fn new_agent() {
    state().update(|s| {
        s.editor = Editor {
            grace: "5000".into(),
            ..Default::default()
        };
        s.error.clear();
    });
    routes::page().set(routes::Page::AgentEditor);
}
pub(crate) fn edit(entry: Entry) {
    let Some(spec) = entry.spec else {
        return;
    };
    state().update(|s| {
        s.editor = Editor {
            original: Some(spec.id.to_string()),
            id: spec.id.to_string(),
            description: spec.description,
            program: spec
                .command
                .as_ref()
                .map(|c| c.program.to_string_lossy().into_owned())
                .unwrap_or_default(),
            args: spec.command.map(|c| c.args.join("\n")).unwrap_or_default(),
            env: spec
                .env
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join("\n"),
            grace: spec.cancellation_grace.as_millis().to_string(),
        };
        s.error.clear();
    });
    routes::page().set(routes::Page::AgentEditor);
}
pub(crate) fn save() {
    let s = state().get_untracked();
    if s.busy {
        return;
    }
    let Some(config) = s.configuration else {
        return;
    };
    let changed = api::context()
        .map(|context| {
            [
                Some(s.editor.id.trim().to_owned()),
                s.editor.original.clone(),
            ]
            .into_iter()
            .flatten()
            .map(|agent| Key::new(context.backend.clone(), agent))
            .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let request = (|| {
        let e = s.editor;
        let mut registrations: Vec<AgentRegistration> =
            serde_json::from_slice(&config.registrations_json)?;
        if e.original.is_none() && registrations.iter().any(|r| r.id().as_str() == e.id.trim()) {
            anyhow::bail!(tr!("duplicate-agent"));
        }
        let mut spec = AgentSpec::acp(
            e.id.trim(),
            Command::new(e.program.trim(), e.args.lines().map(str::to_owned)),
        );
        spec.description = e.description;
        spec.cancellation_grace = std::time::Duration::from_millis(e.grace.parse()?);
        spec.env = e
            .env
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let (key, value) = line
                    .split_once('=')
                    .ok_or_else(|| anyhow::anyhow!(tr!("invalid-env")))?;
                if key.trim().is_empty() {
                    anyhow::bail!(tr!("invalid-env"));
                }
                Ok((key.trim().to_owned(), value.to_owned()))
            })
            .collect::<anyhow::Result<_>>()?;
        if let Some(id) = e.original {
            registrations.retain(|r| r.id().as_str() != id);
        }
        registrations.push(AgentRegistration::Custom(spec));
        bas_client::config::validate_registrations(&registrations)?;
        anyhow::Ok(pb::SaveAgentConfigurationRequest {
            profile: config.profile,
            revision: config.revision,
            registrations_json: serde_json::to_vec(&registrations)?,
        })
    })();
    let request = match request {
        Ok(r) => r,
        Err(e) => {
            failure(e);
            return;
        }
    };
    busy(tr!("saving"));
    api::run(
        move |mut client| async move {
            client.save_agent_configuration(request).await?;
            Ok(())
        },
        move |result| match result {
            Ok(()) => {
                for key in changed {
                    key.invalidate();
                }
                routes::agents();
                load(false);
            }
            Err(error) => failure(error),
        },
    );
}
fn busy(message: String) {
    state().update(|s| {
        s.busy = true;
        s.message = message;
        s.error.clear();
    });
}
fn failure(error: anyhow::Error) {
    state().update(|s| {
        s.busy = false;
        s.message.clear();
        s.error = error.to_string();
    });
}
pub(crate) fn install(entry: Entry) {
    if state().get_untracked().busy {
        return;
    }
    busy(tr!("installing"));
    let key = api::context().map(|context| Key::new(context.backend, &entry.id));
    api::run(
        move |mut client| async move {
            let mut stream = client
                .install_agent(pb::InstallAgentRequest { id: entry.id })
                .await?
                .into_inner();
            while let Some(update) = stream.message().await? {
                if update.complete {
                    return Ok(());
                }
            }
            anyhow::bail!(tr!("incomplete-install"))
        },
        move |result| match result {
            Ok(()) => {
                if let Some(key) = key {
                    key.invalidate();
                }
                load(false);
            }
            Err(e) => failure(e),
        },
    );
}
pub(crate) fn confirm_remove() {
    let s = state().get_untracked();
    if s.busy {
        return;
    }
    let Some(entry) = s.pending_remove else {
        return;
    };
    state().update(|s| s.pending_remove = None);
    busy(tr!("removing"));
    let key = api::context().map(|context| Key::new(context.backend, &entry.id));
    api::run(
        move |mut client| async move {
            client
                .remove_agent(pb::RemoveAgentRequest { id: entry.id })
                .await?;
            Ok(())
        },
        move |result| match result {
            Ok(()) => {
                if let Some(key) = key {
                    key.invalidate();
                }
                load(false);
            }
            Err(e) => failure(e),
        },
    );
}
pub(crate) fn check(entry: Entry) {
    let Some(spec) = entry.spec.clone() else {
        return;
    };
    if state().get_untracked().busy {
        return;
    }
    busy(tr!("checking"));
    let id = entry.id.clone();
    let key = api::context().map(|context| Key::new(context.backend, &id));
    api::run(
        move |mut client| async move {
            Ok(client
                .check_agent(pb::CheckAgentRequest {
                    agent: id,
                    expected_spec_json: serde_json::to_vec(&spec)?,
                    connect: true,
                })
                .await?
                .into_inner())
        },
        move |result| match result {
            Ok(check) => {
                if let Some(key) = key {
                    key.checked(&check);
                }
                state().update(|s| {
                    s.busy = false;
                    s.message.clear();
                    s.check = Some((entry, check));
                });
            }
            Err(e) => failure(e),
        },
    );
}
pub(crate) fn logout(entry: Entry) {
    let key = api::context().map(|context| Key::new(context.backend, &entry.id));
    let Some(spec) = entry.spec else {
        return;
    };
    if state().get_untracked().busy {
        return;
    }
    busy(tr!("signing-out"));
    api::run(
        move |mut client| async move {
            client
                .logout_agent(pb::LogoutAgentRequest {
                    agent: entry.id,
                    expected_spec_json: serde_json::to_vec(&spec)?,
                })
                .await?;
            Ok(())
        },
        move |result| match result {
            Ok(()) => {
                if let Some(key) = key {
                    key.record(LoginStatus::Required);
                }
                load(false);
            }
            Err(e) => failure(e),
        },
    );
}

pub(crate) fn details(entry: Entry) {
    state().update(|s| {
        s.selected = Some(entry);
        s.check = None;
        s.error.clear();
    });
    routes::open(routes::Page::AgentDetail);
}
