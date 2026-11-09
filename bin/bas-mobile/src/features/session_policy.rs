use crate::shared::{api, i18n::tr};
use bas_client::{
    config::{AcpLifecycle, AgentRegistration, agent::Kind},
    service::pb,
};
use nsh_core::prelude::*;
use std::num::NonZeroU64;

#[derive(Clone, Default)]
pub(crate) struct State {
    pub baseline: Option<pb::AcpConfiguration>,
    pub agents: Vec<AgentRegistration>,
    pub policy: AcpLifecycle,
    pub idle: String,
    pub size: String,
    pub overrides: Vec<(String, String)>,
    pub busy: bool,
    pub error: String,
    pub saved: bool,
    pub edit: u64,
    epoch: u64,
}
pub(crate) fn state() -> Signal<State> {
    global::<State>()
}

pub(crate) fn load() {
    let edit = state().get_untracked().edit + 1;
    let epoch = state().get_untracked().epoch + 1;
    state().set(State {
        edit,
        epoch,
        busy: true,
        ..Default::default()
    });
    api::run(
        |mut client| async move {
            Ok(client
                .get_acp_configuration(pb::GetAcpConfigurationRequest {})
                .await?
                .into_inner())
        },
        move |result| {
            if state().get_untracked().epoch == epoch {
                accept(result)
            }
        },
    );
}
fn accept(result: anyhow::Result<pb::AcpConfiguration>) {
    let result = result.and_then(|response| {
        let policy: AcpLifecycle = serde_json::from_slice(&response.policy_json)?;
        let agents: Vec<AgentRegistration> = serde_json::from_slice(&response.registrations_json)?;
        Ok((response, policy, agents))
    });
    state().update(|s| {
        s.busy = false;
        match result {
            Ok((response, policy, agents)) => {
                s.idle = policy
                    .idle_timeout_secs
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                s.size = policy.pool_size.to_string();
                s.overrides = policy
                    .pools
                    .iter()
                    .map(|(id, size)| (id.to_string(), size.to_string()))
                    .collect();
                s.policy = policy;
                s.agents = agents;
                s.baseline = Some(response);
                s.error.clear();
            }
            Err(error) => s.error = error.to_string(),
        }
    });
}
pub(crate) fn agent_ids() -> Vec<String> {
    state().with(|s| {
        s.agents
            .iter()
            .filter(|a| a.as_custom().is_none_or(|a| a.kind == Kind::Acp))
            .map(|a| a.id().to_string())
            .collect()
    })
}
pub(crate) fn edit_idle(value: &str) {
    state().update(|s| {
        s.idle = value.into();
        s.saved = false;
    });
    changed();
}
pub(crate) fn edit_size(value: &str) {
    state().update(|s| {
        s.size = value.into();
        s.saved = false;
    });
    changed();
}
pub(crate) fn choose_default(value: &Option<SharedString>) {
    state().update(|s| {
        s.policy.default_agent = value
            .as_ref()
            .filter(|s| s.as_str() != "@last")
            .map(|s| s.to_string().into());
        s.saved = false;
    });
    changed();
}
pub(crate) fn add_override(value: &Option<SharedString>) {
    if let Some(value) = value {
        state().update(|s| {
            if !s.overrides.iter().any(|(id, _)| id == value.as_str()) {
                s.overrides.push((value.to_string(), "0".into()));
            }
            s.saved = false;
        });
        changed();
    }
}
pub(crate) fn save() {
    let s = state().get_untracked();
    if s.busy {
        return;
    }
    let edit = s.edit;
    let epoch = s.epoch;
    let Some(baseline) = s.baseline else {
        return;
    };
    let candidate = (|| {
        let mut policy = s.policy;
        policy.idle_timeout_secs = if s.idle.trim().is_empty() {
            None
        } else {
            Some(
                NonZeroU64::new(s.idle.trim().parse()?)
                    .ok_or_else(|| anyhow::anyhow!(tr!("invalid-idle")))?,
            )
        };
        policy.pool_size = s.size.trim().parse()?;
        policy.pools = s
            .overrides
            .iter()
            .map(|(id, size)| Ok((id.clone().into(), size.trim().parse()?)))
            .collect::<anyhow::Result<_>>()?;
        policy.validate(&s.agents)?;
        anyhow::Ok(policy)
    })();
    let policy = match candidate {
        Ok(policy) => policy,
        Err(error) => {
            state().update(|s| s.error = error.to_string());
            return;
        }
    };
    state().update(|s| {
        s.busy = true;
        s.saved = false;
        s.error.clear();
    });
    api::run(
        move |mut client| async move {
            Ok(client
                .save_acp_configuration(pb::SaveAcpConfigurationRequest {
                    profile: baseline.profile,
                    revision: baseline.revision,
                    policy_json: serde_json::to_vec(&policy)?,
                })
                .await?
                .into_inner())
        },
        move |result| {
            if state().get_untracked().epoch != epoch {
                return;
            }
            if state().get_untracked().edit == edit {
                let saved = result.is_ok();
                accept(result);
                state().update(|s| s.saved = saved);
            } else {
                state().update(|s| {
                    s.busy = false;
                    match result {
                        Ok(response) => s.baseline = Some(response),
                        Err(error) => s.error = error.to_string(),
                    }
                });
                if state().get_untracked().error.is_empty() {
                    save();
                }
            }
        },
    );
}

fn changed() {
    state().update(|s| {
        s.edit += 1;
        s.saved = false;
        s.error.clear();
    });
    let edit = state().get_untracked().edit;
    let context = api::context();
    api::spawn_task(async move {
        timer(std::time::Duration::from_millis(450)).await;
        if context.is_some_and(|c| c.current()) && state().get_untracked().edit == edit {
            save();
        }
    });
}
pub(crate) fn edit_override(id: &str, value: &str) {
    state().update(|s| {
        if let Some((_, v)) = s.overrides.iter_mut().find(|(key, _)| key == id) {
            *v = value.into();
        }
    });
    changed();
}
pub(crate) fn remove_override(id: &str) {
    state().update(|s| s.overrides.retain(|(key, _)| key != id));
    changed();
}
