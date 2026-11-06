//! Provider launch parameters. JSON Schema describes data; `x-bas` describes input semantics.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(feature = "workspace-schema")]
pub mod schema;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderDescriptor {
    pub id: String,
    pub title: String,
    pub schema_version: u32,
    pub schema: Value,
}

/// Application annotations, not additional JSON Schema validation keywords.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub semantic_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Choice {
    pub value: Value,
    pub label: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Choices {
    pub choices: Vec<Choice>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ChoiceQuery {
    pub source: String,
    /// Partial form values, not necessarily a valid launch request yet.
    pub parameters: Value,
    pub query: String,
    pub cursor: Option<String>,
}
