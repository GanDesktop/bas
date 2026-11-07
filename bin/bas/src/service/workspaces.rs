//! Workspace providers, workspace records, and server directory access.
use super::status::metadata_error;
use super::*;

impl Service {
    pub(super) async fn handle_list_workspace_providers(
        &self,
        _: Request<pb::ListWorkspaceProvidersRequest>,
    ) -> Result<Response<pb::ListWorkspaceProvidersResponse>, Status> {
        let providers = self
            .providers
            .descriptors()
            .map(|descriptor| {
                Ok(pb::WorkspaceProvider {
                    id: descriptor.id.clone(),
                    title: descriptor.title.clone(),
                    schema_version: descriptor.schema_version,
                    schema_json: serde_json::to_vec(&descriptor.schema)
                        .map_err(|error| Status::internal(error.to_string()))?,
                })
            })
            .collect::<Result<_, Status>>()?;
        Ok(Response::new(pb::ListWorkspaceProvidersResponse {
            providers,
        }))
    }

    pub(super) async fn handle_open_workspace(
        &self,
        request: Request<pb::OpenWorkspaceRequest>,
    ) -> Result<Response<pb::Workspace>, Status> {
        let workspace = self
            .providers
            .open(&self.registry, request.into_inner())
            .await
            .map_err(workspace_error)?;
        Ok(Response::new(
            pb::Workspace::try_from(&workspace).map_err(metadata_error)?,
        ))
    }

    pub(super) async fn handle_query_workspace_choices(
        &self,
        request: Request<pb::QueryWorkspaceChoicesRequest>,
    ) -> Result<Response<pb::QueryWorkspaceChoicesResponse>, Status> {
        let page = self
            .providers
            .choices(request.into_inner())
            .await
            .map_err(workspace_error)?;
        let choices = page
            .choices
            .into_iter()
            .map(|choice| {
                Ok(pb::WorkspaceChoice {
                    value_json: serde_json::to_vec(&choice.value)
                        .map_err(|error| Status::internal(error.to_string()))?,
                    label: choice.label,
                })
            })
            .collect::<Result<_, Status>>()?;
        Ok(Response::new(pb::QueryWorkspaceChoicesResponse {
            choices,
            next_cursor: page.next_cursor,
        }))
    }

    pub(super) async fn handle_browse_server_directories(
        &self,
        request: Request<pb::BrowseServerDirectoriesRequest>,
    ) -> Result<Response<pb::BrowseServerDirectoriesResponse>, Status> {
        let page = crate::workspace::browse::directories(request.into_inner())
            .await
            .map_err(|error| match &error {
                crate::workspace::browse::Error::Io(io)
                    if io.kind() == std::io::ErrorKind::PermissionDenied =>
                {
                    Status::permission_denied(error.to_string())
                }
                crate::workspace::browse::Error::Io(io)
                    if io.kind() == std::io::ErrorKind::NotFound =>
                {
                    Status::not_found(error.to_string())
                }
                _ => Status::invalid_argument(error.to_string()),
            })?;
        Ok(Response::new(page))
    }

    pub(super) async fn handle_create_workspace(
        &self,
        request: Request<pb::CreateWorkspaceRequest>,
    ) -> Result<Response<pb::Workspace>, Status> {
        let request = request.into_inner();
        let workspace = self
            .registry
            .create_workspace(request.cwd.into(), request.name)
            .await?;
        Ok(Response::new(
            pb::Workspace::try_from(&workspace).map_err(metadata_error)?,
        ))
    }

    pub(super) async fn handle_list_workspaces(
        &self,
        request: Request<pb::ListWorkspacesRequest>,
    ) -> Result<Response<pb::ListWorkspacesResponse>, Status> {
        let include_archived = request.into_inner().include_archived;
        let workspaces = self
            .registry
            .workspaces()
            .await
            .into_iter()
            .filter(|workspace| include_archived || !workspace.archived)
            .map(|workspace| pb::Workspace::try_from(&workspace))
            .collect::<Result<_, _>>()
            .map_err(metadata_error)?;
        Ok(Response::new(pb::ListWorkspacesResponse { workspaces }))
    }

    pub(super) async fn handle_get_workspace(
        &self,
        request: Request<pb::GetWorkspaceRequest>,
    ) -> Result<Response<pb::Workspace>, Status> {
        let id = request.into_inner().workspace_id.parse().map_err(
            |error: bas_protocol::model::ParseIdError| Status::invalid_argument(error.to_string()),
        )?;
        let workspace = self.registry.workspace(id).await?;
        Ok(Response::new(
            pb::Workspace::try_from(&workspace).map_err(metadata_error)?,
        ))
    }

    pub(super) async fn handle_set_workspace_archived(
        &self,
        request: Request<pb::SetWorkspaceArchivedRequest>,
    ) -> Result<Response<pb::Workspace>, Status> {
        let request = request.into_inner();
        let id =
            request
                .workspace_id
                .parse()
                .map_err(|error: bas_protocol::model::ParseIdError| {
                    Status::invalid_argument(error.to_string())
                })?;
        let workspace = self
            .registry
            .set_workspace_archived(id, request.archived)
            .await?;
        Ok(Response::new(
            pb::Workspace::try_from(&workspace).map_err(metadata_error)?,
        ))
    }

    pub(super) async fn handle_delete_workspace(
        &self,
        request: Request<pb::DeleteWorkspaceRequest>,
    ) -> Result<Response<pb::DeleteWorkspaceResponse>, Status> {
        let request = request.into_inner();
        let id =
            request
                .workspace_id
                .parse()
                .map_err(|error: bas_protocol::model::ParseIdError| {
                    Status::invalid_argument(error.to_string())
                })?;
        self.registry.delete_workspace(id, request.force).await?;
        Ok(Response::new(pb::DeleteWorkspaceResponse {}))
    }
}

fn workspace_error(error: crate::workspace::Error) -> Status {
    use crate::workspace::Error;
    match error {
        Error::Create(error) => error.into(),
        Error::Unknown(_) => Status::not_found(error.to_string()),
        Error::Version => Status::failed_precondition(error.to_string()),
        _ => Status::invalid_argument(error.to_string()),
    }
}
