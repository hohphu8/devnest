use crate::models::runtime::RuntimeType;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeConfigFieldKind {
    Toggle,
    Number,
    Size,
    Text,
    Select,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConfigFieldOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConfigField {
    pub key: String,
    pub label: String,
    pub description: Option<String>,
    pub kind: RuntimeConfigFieldKind,
    pub placeholder: Option<String>,
    pub options: Vec<RuntimeConfigFieldOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConfigSection {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub fields: Vec<RuntimeConfigField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConfigSchema {
    pub runtime_id: String,
    pub runtime_type: RuntimeType,
    pub runtime_version: String,
    pub config_path: String,
    pub supports_editor: bool,
    pub open_file_only: bool,
    pub sections: Vec<RuntimeConfigSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConfigValues {
    pub runtime_id: String,
    pub runtime_type: RuntimeType,
    pub runtime_version: String,
    pub config_path: String,
    pub values: BTreeMap<String, String>,
    pub updated_at: String,
}
