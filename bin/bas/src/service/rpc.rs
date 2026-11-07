//! BAS RPC entry points. Domain handlers own validation and orchestration.
use super::*;

#[tonic::async_trait]
impl Bas for Service {
    async fn list_workspace_providers(
        &self,
        request: Request<pb::ListWorkspaceProvidersRequest>,
    ) -> Result<Response<pb::ListWorkspaceProvidersResponse>, Status> {
        self.handle_list_workspace_providers(request).await
    }

    async fn open_workspace(
        &self,
        request: Request<pb::OpenWorkspaceRequest>,
    ) -> Result<Response<pb::Workspace>, Status> {
        self.handle_open_workspace(request).await
    }

    async fn query_workspace_choices(
        &self,
        request: Request<pb::QueryWorkspaceChoicesRequest>,
    ) -> Result<Response<pb::QueryWorkspaceChoicesResponse>, Status> {
        self.handle_query_workspace_choices(request).await
    }

    async fn browse_server_directories(
        &self,
        request: Request<pb::BrowseServerDirectoriesRequest>,
    ) -> Result<Response<pb::BrowseServerDirectoriesResponse>, Status> {
        self.handle_browse_server_directories(request).await
    }

    async fn create_workspace(
        &self,
        request: Request<pb::CreateWorkspaceRequest>,
    ) -> Result<Response<pb::Workspace>, Status> {
        self.handle_create_workspace(request).await
    }

    async fn list_workspaces(
        &self,
        request: Request<pb::ListWorkspacesRequest>,
    ) -> Result<Response<pb::ListWorkspacesResponse>, Status> {
        self.handle_list_workspaces(request).await
    }

    async fn get_workspace(
        &self,
        request: Request<pb::GetWorkspaceRequest>,
    ) -> Result<Response<pb::Workspace>, Status> {
        self.handle_get_workspace(request).await
    }

    async fn set_workspace_archived(
        &self,
        request: Request<pb::SetWorkspaceArchivedRequest>,
    ) -> Result<Response<pb::Workspace>, Status> {
        self.handle_set_workspace_archived(request).await
    }

    async fn delete_workspace(
        &self,
        request: Request<pb::DeleteWorkspaceRequest>,
    ) -> Result<Response<pb::DeleteWorkspaceResponse>, Status> {
        self.handle_delete_workspace(request).await
    }

    async fn get_acp_configuration(
        &self,
        request: Request<pb::GetAcpConfigurationRequest>,
    ) -> Result<Response<pb::AcpConfiguration>, Status> {
        self.handle_get_acp_configuration(request).await
    }

    async fn save_acp_configuration(
        &self,
        request: Request<pb::SaveAcpConfigurationRequest>,
    ) -> Result<Response<pb::AcpConfiguration>, Status> {
        self.handle_save_acp_configuration(request).await
    }

    async fn get_agent_configuration(
        &self,
        request: Request<pb::GetAgentConfigurationRequest>,
    ) -> Result<Response<pb::AgentConfiguration>, Status> {
        self.handle_get_agent_configuration(request).await
    }

    async fn save_agent_configuration(
        &self,
        request: Request<pb::SaveAgentConfigurationRequest>,
    ) -> Result<Response<pb::AgentConfiguration>, Status> {
        self.handle_save_agent_configuration(request).await
    }

    async fn authenticate_agent_interactive(
        &self,
        request: Request<tonic::Streaming<pb::AuthenticationInput>>,
    ) -> Result<Response<Self::AuthenticateAgentInteractiveStream>, Status> {
        self.handle_authenticate_agent_interactive(request).await
    }

    async fn list_agent_catalog(
        &self,
        request: Request<pb::ListAgentCatalogRequest>,
    ) -> Result<Response<pb::ListAgentCatalogResponse>, Status> {
        self.handle_list_agent_catalog(request).await
    }

    async fn install_agent(
        &self,
        request: Request<pb::InstallAgentRequest>,
    ) -> Result<Response<Self::InstallAgentStream>, Status> {
        self.handle_install_agent(request).await
    }

    async fn remove_agent(
        &self,
        request: Request<pb::RemoveAgentRequest>,
    ) -> Result<Response<pb::RemoveAgentResponse>, Status> {
        self.handle_remove_agent(request).await
    }

    async fn logout_agent(
        &self,
        request: Request<pb::LogoutAgentRequest>,
    ) -> Result<Response<pb::LogoutAgentResponse>, Status> {
        self.handle_logout_agent(request).await
    }

    async fn authenticate_agent(
        &self,
        request: Request<pb::AuthenticateAgentRequest>,
    ) -> Result<Response<pb::AuthenticateAgentResponse>, Status> {
        self.handle_authenticate_agent(request).await
    }

    async fn list_agents(
        &self,
        request: Request<pb::ListAgentsRequest>,
    ) -> Result<Response<pb::ListAgentsResponse>, Status> {
        self.handle_list_agents(request).await
    }

    async fn check_agent(
        &self,
        request: Request<pb::CheckAgentRequest>,
    ) -> Result<Response<pb::CheckAgentResponse>, Status> {
        self.handle_check_agent(request).await
    }

    async fn create_session(
        &self,
        request: Request<pb::CreateSessionRequest>,
    ) -> Result<Response<pb::CreateSessionResponse>, Status> {
        self.handle_create_session(request).await
    }

    async fn list_agent_sessions(
        &self,
        request: Request<pb::ListAgentSessionsRequest>,
    ) -> Result<Response<pb::ListAgentSessionsResponse>, Status> {
        self.handle_list_agent_sessions(request).await
    }

    async fn import_agent_session(
        &self,
        request: Request<pb::ImportAgentSessionRequest>,
    ) -> Result<Response<pb::CreateSessionResponse>, Status> {
        self.handle_import_agent_session(request).await
    }

    async fn delete_agent_session(
        &self,
        request: Request<pb::DeleteAgentSessionRequest>,
    ) -> Result<Response<pb::DeleteSessionResponse>, Status> {
        self.handle_delete_agent_session(request).await
    }

    async fn close_session(
        &self,
        request: Request<pb::GetSessionRequest>,
    ) -> Result<Response<pb::GetSessionResponse>, Status> {
        self.handle_close_session(request).await
    }

    async fn restart_session(
        &self,
        request: Request<pb::GetSessionRequest>,
    ) -> Result<Response<pb::GetSessionResponse>, Status> {
        self.handle_restart_session(request).await
    }

    async fn list_sessions(
        &self,
        request: Request<pb::ListSessionsRequest>,
    ) -> Result<Response<pb::ListSessionsResponse>, Status> {
        self.handle_list_sessions(request).await
    }

    async fn get_session(
        &self,
        request: Request<pb::GetSessionRequest>,
    ) -> Result<Response<pb::GetSessionResponse>, Status> {
        self.handle_get_session(request).await
    }

    async fn delete_session(
        &self,
        request: Request<pb::DeleteSessionRequest>,
    ) -> Result<Response<pb::DeleteSessionResponse>, Status> {
        self.handle_delete_session(request).await
    }

    async fn attach(
        &self,
        request: Request<pb::AttachRequest>,
    ) -> Result<Response<Self::AttachStream>, Status> {
        self.handle_attach(request).await
    }

    async fn observe_acp(
        &self,
        request: Request<pb::AttachRequest>,
    ) -> Result<Response<Self::ObserveAcpStream>, Status> {
        self.handle_observe_acp(request).await
    }

    async fn prompt(
        &self,
        request: Request<pb::PromptSubmission>,
    ) -> Result<Response<pb::PromptSubmissionResponse>, Status> {
        self.handle_prompt(request).await
    }

    async fn get_prompt_acceptance(
        &self,
        request: Request<pb::PromptAcceptanceRequest>,
    ) -> Result<Response<pb::PromptAcceptanceResponse>, Status> {
        self.handle_get_prompt_acceptance(request).await
    }

    async fn cancel(
        &self,
        request: Request<pb::AcpPayload>,
    ) -> Result<Response<pb::CancelResponse>, Status> {
        self.handle_cancel(request).await
    }

    async fn watch_permissions(
        &self,
        request: Request<pb::WatchPermissionsRequest>,
    ) -> Result<Response<Self::WatchPermissionsStream>, Status> {
        self.handle_watch_permissions(request).await
    }

    async fn respond_permission(
        &self,
        request: Request<pb::RespondPermissionRequest>,
    ) -> Result<Response<pb::RespondPermissionResponse>, Status> {
        self.handle_respond_permission(request).await
    }

    async fn watch_elicitations(
        &self,
        request: Request<pb::WatchElicitationsRequest>,
    ) -> Result<Response<Self::WatchElicitationsStream>, Status> {
        self.handle_watch_elicitations(request).await
    }

    async fn respond_elicitation(
        &self,
        request: Request<pb::RespondElicitationRequest>,
    ) -> Result<Response<pb::RespondElicitationResponse>, Status> {
        self.handle_respond_elicitation(request).await
    }

    async fn set_config_option(
        &self,
        request: Request<pb::AcpPayload>,
    ) -> Result<Response<pb::AcpPayload>, Status> {
        self.handle_set_config_option(request).await
    }

    async fn set_session_directories(
        &self,
        request: Request<pb::SetSessionDirectoriesRequest>,
    ) -> Result<Response<pb::GetSessionResponse>, Status> {
        self.handle_set_session_directories(request).await
    }

    async fn set_mode(
        &self,
        request: Request<pb::AcpPayload>,
    ) -> Result<Response<pb::AcpPayload>, Status> {
        self.handle_set_mode(request).await
    }

    async fn set_archived(
        &self,
        request: Request<pb::SetArchivedRequest>,
    ) -> Result<Response<pb::SetArchivedResponse>, Status> {
        self.handle_set_archived(request).await
    }

    async fn set_title(
        &self,
        request: Request<pb::SetTitleRequest>,
    ) -> Result<Response<pb::SetTitleResponse>, Status> {
        self.handle_set_title(request).await
    }

    async fn load_history(
        &self,
        request: Request<pb::LoadHistoryRequest>,
    ) -> Result<Response<pb::LoadHistoryResponse>, Status> {
        self.handle_load_history(request).await
    }

    async fn load_history_window(
        &self,
        request: Request<pb::LoadHistoryWindowRequest>,
    ) -> Result<Response<pb::LoadHistoryWindowResponse>, Status> {
        self.handle_load_history_window(request).await
    }

    async fn load_history_overview(
        &self,
        request: Request<pb::LoadHistoryOverviewRequest>,
    ) -> Result<Response<pb::LoadHistoryOverviewResponse>, Status> {
        self.handle_load_history_overview(request).await
    }

    async fn load_history_window_at(
        &self,
        request: Request<pb::LoadHistoryWindowAtRequest>,
    ) -> Result<Response<pb::LoadHistoryWindowAtResponse>, Status> {
        self.handle_load_history_window_at(request).await
    }

    type AuthenticateAgentInteractiveStream = EventStream<pb::AuthenticationEvent>;

    type InstallAgentStream = EventStream<pb::AgentOperation>;

    type AttachStream = EventStream<pb::ManagementEvent>;

    type ObserveAcpStream = EventStream<pb::AcpObservation>;

    type WatchPermissionsStream = EventStream<pb::PermissionSnapshot>;

    type WatchElicitationsStream = EventStream<pb::ElicitationSnapshot>;
}
