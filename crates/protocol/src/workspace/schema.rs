//! Schema generation and validation are delegated to Schemars and jsonschema.
use schemars::{JsonSchema, generate::SchemaSettings};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

pub use schemars;

pub const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";
pub const SERVER_DIRECTORY_PATH: &str = "ServerDirectoryPath";

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
#[schemars(inline, extend("x-bas" = {"type": "ServerDirectoryPath"}))]
pub struct ServerDirectoryPath(#[schemars(length(min = 1))] pub String);

pub fn generate<T: JsonSchema>() -> Value {
    SchemaSettings::draft2020_12()
        .into_generator()
        .into_root_schema_for::<T>()
        .to_value()
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unsupported workspace schema dialect")]
    Dialect,
    #[error("invalid workspace schema: {0}")]
    Schema(String),
    #[error("{path}: {message}")]
    Parameters { path: String, message: String },
    #[error("invalid workspace parameters: {0}")]
    Decode(#[from] serde_json::Error),
}

pub struct Validator(jsonschema::Validator);

impl Validator {
    /// # Errors
    /// Rejects invalid schemas and unsupported dialects. External retrieval is disabled.
    pub fn new(schema: &Value) -> Result<Self, Error> {
        if schema.get("$schema").and_then(Value::as_str) != Some(DIALECT) {
            return Err(Error::Dialect);
        }
        jsonschema::draft202012::options()
            .build(schema)
            .map(Self)
            .map_err(|error| Error::Schema(error.to_string()))
    }

    /// # Errors
    /// Returns the failing instance location and validator message.
    pub fn validate(&self, parameters: &Value) -> Result<(), Error> {
        self.0
            .validate(parameters)
            .map_err(|error| Error::Parameters {
                path: error.instance_path().to_string(),
                message: error.to_string(),
            })
    }

    /// # Errors
    /// Rejects invalid parameters before applying the provider's Serde defaults.
    pub fn decode<T: DeserializeOwned>(&self, parameters: Value) -> Result<T, Error> {
        self.validate(&parameters)?;
        Ok(serde_json::from_value(parameters)?)
    }
}

#[cfg(test)]
mod tests;
