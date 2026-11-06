//! Workspace acquisition is independent of the client and its filesystem.
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

use bas::workspace::{ChoiceQuery, Choices, ProviderDescriptor, schema};
use serde_json::Value;

use crate::session::Registry;

pub mod browse;
mod directory;

pub type Result<T> = std::result::Result<T, Error>;
pub type Operation<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown workspace provider: {0}")]
    Unknown(String),
    #[error("workspace provider schema changed; reload the form")]
    Version,
    #[error("unsupported workspace choice source: {0}")]
    Choices(String),
    #[error("duplicate workspace provider: {0}")]
    Duplicate(String),
    #[error(transparent)]
    Schema(#[from] schema::Error),
    #[error(transparent)]
    Create(#[from] crate::session::CreateError),
}

pub trait Provider: Send + Sync {
    fn descriptor(&self) -> ProviderDescriptor;
    /// Parameters have already passed the descriptor's JSON Schema validation.
    fn open<'a>(
        &'a self,
        registry: &'a Registry,
        parameters: Value,
        name: Option<String>,
    ) -> Operation<'a, bas::model::Workspace>;

    fn choices(&self, query: ChoiceQuery) -> Operation<'_, Choices> {
        Box::pin(async move { Err(Error::Choices(query.source)) })
    }
}

struct Entry {
    provider: Arc<dyn Provider>,
    descriptor: ProviderDescriptor,
    validator: schema::Validator,
}

pub struct Providers(BTreeMap<String, Entry>);

impl Default for Providers {
    fn default() -> Self {
        Self::new([Arc::new(directory::Directory) as Arc<dyn Provider>])
            .expect("built-in directory provider schema is valid")
    }
}

impl Providers {
    /// # Errors
    /// Rejects duplicate providers and invalid schemas during registration.
    pub fn new(providers: impl IntoIterator<Item = Arc<dyn Provider>>) -> Result<Self> {
        let mut entries = BTreeMap::new();
        for provider in providers {
            let descriptor = provider.descriptor();
            let validator = schema::Validator::new(&descriptor.schema)?;
            let id = descriptor.id.clone();
            if entries
                .insert(
                    id.clone(),
                    Entry {
                        provider,
                        descriptor,
                        validator,
                    },
                )
                .is_some()
            {
                return Err(Error::Duplicate(id));
            }
        }
        Ok(Self(entries))
    }

    pub fn descriptors(&self) -> impl Iterator<Item = &ProviderDescriptor> {
        self.0.values().map(|entry| &entry.descriptor)
    }

    fn entry(&self, id: &str, version: u32) -> Result<&Entry> {
        let entry = self.0.get(id).ok_or_else(|| Error::Unknown(id.into()))?;
        if entry.descriptor.schema_version != version {
            return Err(Error::Version);
        }
        Ok(entry)
    }

    /// # Errors
    /// Rejects unknown providers, stale schemas, invalid parameters and acquisition failures.
    pub async fn open(
        &self,
        registry: &Registry,
        request: bas::service::pb::OpenWorkspaceRequest,
    ) -> Result<bas::model::Workspace> {
        let entry = self.entry(&request.provider_id, request.schema_version)?;
        let parameters =
            serde_json::from_slice(&request.parameters_json).map_err(schema::Error::from)?;
        entry.validator.validate(&parameters)?;
        entry
            .provider
            .open(registry, parameters, request.name)
            .await
    }

    /// # Errors
    /// Rejects unknown providers, stale schemas, malformed partial parameters and query failures.
    pub async fn choices(
        &self,
        request: bas::service::pb::QueryWorkspaceChoicesRequest,
    ) -> Result<Choices> {
        let entry = self.entry(&request.provider_id, request.schema_version)?;
        let parameters =
            serde_json::from_slice(&request.parameters_json).map_err(schema::Error::from)?;
        entry
            .provider
            .choices(ChoiceQuery {
                source: request.source,
                parameters,
                query: request.query,
                cursor: request.cursor,
            })
            .await
    }
}
