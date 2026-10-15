//! Recognition errors retain their category until the automation boundary.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CvError {
    TemplateLoad {
        message: String,
        key: Option<String>,
        path: Option<String>,
    },
    Other(String),
}

pub type CvResult<T> = Result<T, CvError>;

impl fmt::Display for CvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TemplateLoad { message, .. } => write!(f, "模板加载失败: {message}"),
            Self::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for CvError {}

impl From<String> for CvError {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl From<&str> for CvError {
    fn from(message: &str) -> Self {
        Self::Other(message.to_string())
    }
}

impl From<CvError> for String {
    fn from(error: CvError) -> Self {
        error.to_string()
    }
}

pub(super) fn template_response_error(value: &serde_json::Value) -> Option<CvError> {
    if value.get("errorCode").and_then(|value| value.as_str()) != Some("template_load_failed") {
        return None;
    }
    let key = value
        .get("templateKey")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let path = value
        .get("templatePath")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let message = value
        .get("error")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .or_else(|| key.clone())
        .or_else(|| path.clone())
        .unwrap_or_else(|| "unknown template".into());
    Some(CvError::TemplateLoad { message, key, path })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_error_code_preserves_details_without_matching_error_text() {
        let error = template_response_error(&serde_json::json!({
            "errorCode": "template_load_failed",
            "templateKey": "battle/attack",
            "templatePath": "/tmp/attack.png",
            "error": "unreadable file"
        }))
        .unwrap();
        assert!(matches!(error, CvError::TemplateLoad { message, key, path }
            if message == "unreadable file" && key.as_deref() == Some("battle/attack")
            && path.as_deref() == Some("/tmp/attack.png")));
        assert!(
            template_response_error(&serde_json::json!({"error":"template load failed"})).is_none()
        );
    }
}
