//! Optimistic agent-configuration reads and publication.
use super::*;
use sha2::{Digest, Sha256};

impl Manager {
    /// # Errors
    /// Rejects fixed catalogs and unreadable settings.
    pub fn configuration(
        &self,
    ) -> Result<bas_protocol::protocol::pb::AgentConfiguration, tonic::Status> {
        let store = self.store.as_ref().ok_or_else(|| {
            tonic::Status::failed_precondition("This daemon uses a fixed agent catalog")
        })?;
        let document = store.load().map_err(configuration_error)?;
        self.configuration_snapshot(&document.settings.agents)
    }

    fn configuration_snapshot(
        &self,
        agents: &[AgentRegistration],
    ) -> Result<bas_protocol::protocol::pb::AgentConfiguration, tonic::Status> {
        let registrations_json = serde_json::to_vec(agents)
            .map_err(|error| tonic::Status::internal(error.to_string()))?;
        Ok(bas_protocol::protocol::pb::AgentConfiguration {
            profile: self.profile.clone(),
            revision: format!("{:x}", Sha256::digest(&registrations_json)),
            registrations_json,
        })
    }

    /// # Errors
    /// Rejects changed profiles, concurrent edits, invalid registrations and failed persistence.
    pub async fn save_configuration(
        &self,
        request: bas_protocol::protocol::pb::SaveAgentConfigurationRequest,
        registry: &crate::session::Registry,
    ) -> Result<bas_protocol::protocol::pb::AgentConfiguration, tonic::Status> {
        let _guard = self.mutation.lock().await;
        let store = self.store.as_ref().ok_or_else(|| {
            tonic::Status::failed_precondition("This daemon uses a fixed agent catalog")
        })?;
        if request.profile != self.profile {
            return Err(tonic::Status::failed_precondition(
                "Agent profile changed; reload before saving",
            ));
        }
        let mut document = store.load().map_err(configuration_error)?;
        if self
            .configuration_snapshot(&document.settings.agents)?
            .revision
            != request.revision
        {
            return Err(tonic::Status::aborted(
                "Agent configuration changed; reload before saving",
            ));
        }
        let agents: Vec<AgentRegistration> = serde_json::from_slice(&request.registrations_json)
            .map_err(|error| tonic::Status::invalid_argument(error.to_string()))?;
        bas_config::validate_registrations(&agents)
            .map_err(|error| tonic::Status::invalid_argument(error.to_string()))?;
        // Resolve managed registrations on the host before persisting anything.
        crate::agent::Catalog::from_registrations(agents.clone())
            .map_err(|error| tonic::Status::invalid_argument(error.to_string()))?;
        let mut save_error = None;
        registry
            .reload_catalog(|| {
                if let Err(error) = store.save_agents(&mut document, agents.clone()) {
                    save_error = Some(error);
                    anyhow::bail!("Agent configuration save failed");
                }
                Ok(document.settings.agents.clone())
            })
            .map_err(|error| {
                save_error
                    .map(configuration_error)
                    .unwrap_or_else(|| tonic::Status::internal(error.to_string()))
            })?;
        self.configuration_snapshot(&document.settings.agents)
    }
}

pub(super) fn configuration_error(error: bas_config::Error) -> tonic::Status {
    match error {
        bas_config::Error::Conflict { .. } | bas_config::Error::Busy { .. } => {
            tonic::Status::aborted(error.to_string())
        }
        _ => tonic::Status::failed_precondition(error.to_string()),
    }
}
