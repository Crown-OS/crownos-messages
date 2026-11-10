use std::path::PathBuf;

use crownos_ipc::{Error as IpcError, RemoteError};
use thiserror::Error;

/// Why a request to the phone did not get its answer.
#[derive(Debug, Error)]
pub enum LinkError {
    #[error("CrownLink isn't running")]
    Unavailable,
    #[error("The phone didn't answer in time")]
    Timeout,
    #[error("CrownLink is too old for Messages; update it")]
    Outdated,
    #[error("{0}")]
    Refused(String),
    #[error("{} can't be sent: its name is not UTF-8", .0.display())]
    NonUtf8Path(PathBuf),
    #[error(transparent)]
    Ipc(IpcError),
}

impl From<IpcError> for LinkError {
    fn from(error: IpcError) -> Self {
        match error {
            IpcError::Remote(RemoteError { code, .. }) if code == RemoteError::UNKNOWN_METHOD => {
                Self::Outdated
            }
            IpcError::Remote(RemoteError { message, .. }) => Self::Refused(message),
            IpcError::Io(_)
            | IpcError::Disconnected
            | IpcError::ServiceUnavailable { .. }
            | IpcError::RuntimeDir => Self::Unavailable,
            IpcError::Timeout => Self::Timeout,
            other => Self::Ipc(other),
        }
    }
}

impl LinkError {
    /// Whether the connection to crownlink is gone, rather than one request failing.
    #[must_use]
    pub const fn is_disconnect(&self) -> bool {
        matches!(self, Self::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_errors_keep_the_daemon_s_words() {
        let refused = LinkError::from(IpcError::Remote(RemoteError::new(
            RemoteError::HANDLER,
            "Messages is not shared",
        )));
        assert_eq!(refused.to_string(), "Messages is not shared");
        let outdated = LinkError::from(IpcError::Remote(RemoteError::unknown_method(7)));
        assert!(matches!(outdated, LinkError::Outdated));
        assert!(LinkError::from(IpcError::Disconnected).is_disconnect());
    }
}
