use super::*;
use schema::{ServerDirectoryPath, schemars::JsonSchema};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(crate = "schema::schemars")]
struct Parameters {
    #[schemars(title = "Directory")]
    directory: ServerDirectoryPath,
}

pub struct Directory;

impl Provider for Directory {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: "directory".into(),
            title: "Folder".into(),
            schema_version: 1,
            schema: schema::generate::<Parameters>(),
        }
    }

    fn open<'a>(
        &'a self,
        registry: &'a Registry,
        parameters: Value,
        name: Option<String>,
    ) -> Operation<'a, bas::model::Workspace> {
        Box::pin(async move {
            let parameters: Parameters =
                serde_json::from_value(parameters).map_err(schema::Error::from)?;
            Ok(registry
                .create_workspace(parameters.directory.0.into(), name)
                .await?)
        })
    }
}
