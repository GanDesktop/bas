use bas_client::service::pb;
use nsh_core::prelude::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Key {
    pub backend: String,
    pub agent: String,
}

impl Key {
    pub fn new(backend: impl Into<String>, agent: impl Into<String>) -> Self {
        Self {
            backend: backend.into(),
            agent: agent.into(),
        }
    }

    pub fn status(&self) -> LoginStatus {
        global::<Logins>().with(|logins| logins.0.get(self).copied().unwrap_or_default())
    }

    pub fn record(&self, status: LoginStatus) {
        global::<Logins>().update(|logins| {
            logins.0.insert(self.clone(), status);
        });
    }

    pub fn invalidate(&self) {
        global::<Logins>().update(|logins| {
            logins.0.remove(self);
        });
    }

    pub fn checked(&self, check: &pb::CheckAgentResponse) {
        self.record(match check.status() {
            pb::AgentCheckStatus::Connected => LoginStatus::Ready,
            pb::AgentCheckStatus::AuthenticationRequired => LoginStatus::Required,
            _ => LoginStatus::Unknown,
        });
    }
}

/// Last confirmed readiness, scoped to the backend and agent that supplied it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LoginStatus {
    #[default]
    Unknown,
    Required,
    Ready,
}

#[derive(Clone, Default)]
struct Logins(BTreeMap<Key, LoginStatus>);

pub(crate) fn invalidate_backend(backend: &str) {
    global::<Logins>().update(|logins| logins.0.retain(|key, _| key.backend != backend));
}
