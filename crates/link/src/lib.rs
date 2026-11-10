//! How CrownOS Messages reaches a phone: the crownlink session, the file chooser portal and
//! desktop notifications. Everything it hands out is a `crownos-messages-core` type.

pub mod client;
mod convert;
pub mod error;
pub mod events;
pub mod notifier;
pub mod portal;
pub mod requests;
mod util;

pub use client::{Link, Session};
pub use error::LinkError;
pub use events::LinkEvent;
pub use requests::{BlobFile, BlobSource, OutgoingMessage};
