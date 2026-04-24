use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeType {
    Php,
    Apache,
    Nginx,
    Frankenphp,
    Mysql,
}

impl RuntimeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Php => "php",
            Self::Apache => "apache",
            Self::Nginx => "nginx",
            Self::Frankenphp => "frankenphp",
            Self::Mysql => "mysql",
        }
    }
}

impl FromStr for RuntimeType {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "php" => Ok(Self::Php),
            "apache" => Ok(Self::Apache),
            "nginx" => Ok(Self::Nginx),
            "frankenphp" => Ok(Self::Frankenphp),
            "mysql" => Ok(Self::Mysql),
            _ => Err("Invalid runtime type"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeVersion {
    pub id: String,
    pub runtime_type: RuntimeType,
    pub version: String,
    pub path: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeSource {
    Bundled,
    Downloaded,
    Imported,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeHealthStatus {
    Available,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInventoryItem {
    pub id: String,
    pub runtime_type: RuntimeType,
    pub version: String,
    pub php_family: Option<String>,
    pub path: String,
    pub is_active: bool,
    pub source: RuntimeSource,
    pub status: RuntimeHealthStatus,
    pub created_at: String,
    pub updated_at: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeArchiveKind {
    Zip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimePackageManifest {
    pub packages: Vec<RuntimePackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimePackage {
    pub id: String,
    pub runtime_type: RuntimeType,
    pub version: String,
    pub php_family: Option<String>,
    pub platform: String,
    pub arch: String,
    pub display_name: String,
    pub download_url: String,
    pub checksum_sha256: String,
    pub archive_kind: RuntimeArchiveKind,
    pub entry_binary: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeInstallStage {
    Queued,
    Downloading,
    Verifying,
    Extracting,
    Registering,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInstallTask {
    pub package_id: String,
    pub display_name: String,
    pub runtime_type: RuntimeType,
    pub version: String,
    pub stage: RuntimeInstallStage,
    pub message: String,
    pub updated_at: String,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpExtensionState {
    pub runtime_id: String,
    pub runtime_version: String,
    pub extension_name: String,
    pub dll_file: String,
    pub enabled: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpExtensionInstallResult {
    pub runtime_id: String,
    pub runtime_version: String,
    pub installed_extensions: Vec<String>,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PhpExtensionPackageKind {
    Zip,
    Binary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PhpExtensionThreadSafety {
    Ts,
    Nts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpExtensionPackageManifest {
    pub packages: Vec<PhpExtensionPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpExtensionPackage {
    pub id: String,
    pub extension_name: String,
    pub php_family: String,
    pub thread_safety: Option<PhpExtensionThreadSafety>,
    pub version: String,
    pub platform: String,
    pub arch: String,
    pub display_name: String,
    pub download_url: String,
    pub checksum_sha256: Option<String>,
    pub package_kind: PhpExtensionPackageKind,
    pub dll_file: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpFunctionState {
    pub runtime_id: String,
    pub runtime_version: String,
    pub function_name: String,
    pub enabled: bool,
    pub updated_at: String,
}
