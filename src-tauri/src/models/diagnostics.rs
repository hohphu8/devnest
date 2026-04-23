use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticItem {
    pub id: String,
    pub project_id: String,
    pub level: DiagnosticLevel,
    pub code: String,
    pub title: String,
    pub message: String,
    pub suggestion: Option<String>,
    pub created_at: String,
}

impl DiagnosticItem {
    pub fn new(
        project_id: impl Into<String>,
        level: DiagnosticLevel,
        code: impl Into<String>,
        title: impl Into<String>,
        message: impl Into<String>,
        suggestion: Option<String>,
        created_at: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            project_id: project_id.into(),
            level,
            code: code.into(),
            title: title.into(),
            message: message.into(),
            suggestion,
            created_at: created_at.into(),
        }
    }
}
