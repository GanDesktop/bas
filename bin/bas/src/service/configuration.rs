//! Read and publish daemon agent and ACP policy configuration.
use super::*;

impl Service {
    pub(super) async fn handle_get_acp_configuration(
        &self,
        _: Request<pb::GetAcpConfigurationRequest>,
    ) -> Result<Response<pb::AcpConfiguration>, Status> {
        Ok(Response::new(self.manager()?.acp_configuration()?))
    }

    pub(super) async fn handle_save_acp_configuration(
        &self,
        request: Request<pb::SaveAcpConfigurationRequest>,
    ) -> Result<Response<pb::AcpConfiguration>, Status> {
        Ok(Response::new(
            self.manager()?
                .save_acp_configuration(request.into_inner(), &self.registry)
                .await?,
        ))
    }

    pub(super) async fn handle_get_agent_configuration(
        &self,
        _request: Request<pb::GetAgentConfigurationRequest>,
    ) -> Result<Response<pb::AgentConfiguration>, Status> {
        Ok(Response::new(self.manager()?.configuration()?))
    }

    pub(super) async fn handle_save_agent_configuration(
        &self,
        request: Request<pb::SaveAgentConfigurationRequest>,
    ) -> Result<Response<pb::AgentConfiguration>, Status> {
        Ok(Response::new(
            self.manager()?
                .save_configuration(request.into_inner(), &self.registry)
                .await?,
        ))
    }
}
