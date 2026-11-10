//! The model of CrownOS Messages: devices, conversations, messages, drafts and `sms:` links,
//! with no UI and no IPC types in it.

pub mod error;
pub mod model;
pub mod store;
pub mod uri;
pub mod util;

pub use error::{DraftError, UriError};
