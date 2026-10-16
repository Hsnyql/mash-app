//! Errors crossing automation steps retain their underlying source and category.

use crate::screen::CvError;
use std::{fmt, io};

#[derive(Debug)]
pub(crate) enum AutomationError {
    Cv(CvError),
    Io(io::Error),
    Decode(base64::DecodeError),
    Clock(std::time::SystemTimeError),
    Message(String),
    Context { message: String, source: Box<Self> },
}

pub(crate) type AutomationResult<T> = Result<T, AutomationError>;

impl AutomationError {
    pub(crate) fn context(self, message: impl Into<String>) -> Self {
        Self::Context {
            message: message.into(),
            source: Box::new(self),
        }
    }

    fn is_template_load_failure(&self) -> bool {
        match self {
            Self::Cv(CvError::TemplateLoad { .. }) => true,
            Self::Context { source, .. } => source.is_template_load_failure(),
            _ => false,
        }
    }
}

impl fmt::Display for AutomationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cv(error) => error.fmt(f),
            Self::Io(error) => error.fmt(f),
            Self::Decode(error) => error.fmt(f),
            Self::Clock(error) => error.fmt(f),
            Self::Message(message) => f.write_str(message),
            Self::Context { message, source } => write!(f, "{message}: {source}"),
        }
    }
}

impl std::error::Error for AutomationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Cv(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Decode(error) => Some(error),
            Self::Clock(error) => Some(error),
            Self::Context { source, .. } => Some(source.as_ref()),
            Self::Message(_) => None,
        }
    }
}

impl From<CvError> for AutomationError {
    fn from(error: CvError) -> Self {
        Self::Cv(error)
    }
}

impl From<io::Error> for AutomationError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<String> for AutomationError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<base64::DecodeError> for AutomationError {
    fn from(error: base64::DecodeError) -> Self {
        Self::Decode(error)
    }
}

impl From<std::time::SystemTimeError> for AutomationError {
    fn from(error: std::time::SystemTimeError) -> Self {
        Self::Clock(error)
    }
}

impl From<&str> for AutomationError {
    fn from(message: &str) -> Self {
        Self::Message(message.into())
    }
}

/// Preserve existing retries for ordinary errors; required-template failures
/// escape the enclosing step through `?` before any fallback can consume them.
pub(crate) trait AutomationResultExt<T> {
    fn recoverable(self) -> AutomationResult<Result<T, AutomationError>>;
    fn context(self, message: impl Into<String>) -> AutomationResult<T>;
}

impl<T, E: Into<AutomationError>> AutomationResultExt<T> for Result<T, E> {
    fn recoverable(self) -> AutomationResult<Result<T, AutomationError>> {
        match self.map_err(Into::into) {
            Err(error) if error.is_template_load_failure() => Err(error),
            result => Ok(result),
        }
    }

    fn context(self, message: impl Into<String>) -> AutomationResult<T> {
        self.map_err(|error| error.into().context(message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn nested_steps_preserve_template_details_and_stop_before_actions() {
        fn probe() -> Result<bool, CvError> {
            Err(CvError::TemplateLoad {
                message: "unreadable template".into(),
                key: Some("battle/attack".into()),
                path: Some("/tmp/attack.png".into()),
            })
        }
        fn step(acted: &mut bool) -> AutomationResult<()> {
            let _ = probe().context("识别攻击按钮").recoverable()?.ok();
            *acted = true;
            Ok(())
        }
        let mut acted = false;
        let error = step(&mut acted).context("执行战斗步骤").unwrap_err();
        assert!(!acted);
        assert!(error.is_template_load_failure());
        let AutomationError::Context { source, .. } = error else {
            panic!()
        };
        let AutomationError::Context { source, .. } = *source else {
            panic!()
        };
        assert!(
            matches!(*source, AutomationError::Cv(CvError::TemplateLoad { key, path, .. })
            if key.as_deref() == Some("battle/attack") && path.as_deref() == Some("/tmp/attack.png"))
        );
    }

    #[test]
    fn filesystem_failures_keep_io_kind_and_source_through_fallback_boundary() {
        let result: io::Result<()> = Err(io::Error::from(io::ErrorKind::PermissionDenied));
        let error = result
            .context("写入截图失败")
            .recoverable()
            .unwrap()
            .unwrap_err();
        assert!(error.to_string().starts_with("写入截图失败: "));
        assert!(error
            .source()
            .unwrap()
            .downcast_ref::<AutomationError>()
            .is_some());
        let AutomationError::Context { source, .. } = error else {
            panic!()
        };
        let AutomationError::Io(source) = *source else {
            panic!()
        };
        assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn ordinary_recognition_errors_and_misses_keep_existing_retry_behavior() {
        let miss: Result<bool, CvError> = Ok(false);
        assert!(!miss.recoverable().unwrap().unwrap());
        let result: Result<bool, CvError> = Err(CvError::Other("frame unavailable".into()));
        assert!(matches!(result.recoverable().unwrap(),
            Err(AutomationError::Cv(CvError::Other(message))) if message == "frame unavailable"));
    }

    #[test]
    fn decoding_and_clock_failures_remain_typed_sources() {
        use base64::Engine;
        let error = base64::engine::general_purpose::STANDARD
            .decode("!")
            .context("视频帧解码失败")
            .unwrap_err();
        let source = error.source().unwrap().source().unwrap();
        assert!(source.downcast_ref::<base64::DecodeError>().is_some());
        assert!(!error.is_template_load_failure());

        let error = std::time::UNIX_EPOCH
            .duration_since(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1))
            .context("获取时间戳失败")
            .unwrap_err();
        let source = error.source().unwrap().source().unwrap();
        assert!(source
            .downcast_ref::<std::time::SystemTimeError>()
            .is_some());
        assert!(!error.is_template_load_failure());
    }
}
