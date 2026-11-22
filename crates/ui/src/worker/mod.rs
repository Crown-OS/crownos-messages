//! The crownlink worker: what the window asks of the phone and what comes back.

mod link;

use std::path::PathBuf;

use crownos_messages_core::{
    model::{Collections, Contact, Conversation, DeviceId, Message, MessageId, ThreadId},
    store::Page,
};
use crownos_messages_link::{BlobSource, LinkEvent, OutgoingMessage};

pub use link::run_link;

use crate::util::picture::Picture;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Collections {
        device: DeviceId,
    },
    /// A page of conversations; `None` asks for the newest.
    Conversations {
        device: DeviceId,
        cursor: Option<String>,
    },
    /// A page of one conversation; `None` asks for the newest.
    Messages {
        device: DeviceId,
        thread: ThreadId,
        cursor: Option<String>,
    },
    Send {
        device: DeviceId,
        placeholder: MessageId,
        message: OutgoingMessage,
    },
    MarkRead {
        device: DeviceId,
        thread: ThreadId,
    },
    /// Every page of the phone's contacts.
    Contacts {
        device: DeviceId,
    },
    Thumbnail {
        device: DeviceId,
        source: BlobSource,
        key: String,
    },
    /// The whole text of a message the phone cut short.
    FullText {
        device: DeviceId,
        thread: ThreadId,
        message: MessageId,
        key: String,
    },
    /// Downloads an attachment and opens it in the default app.
    Open {
        device: DeviceId,
        key: String,
    },
    PickAttachments,
}

#[derive(Debug)]
pub enum Inbound {
    Link(LinkEvent),
    Collections {
        device: DeviceId,
        result: Result<Collections, String>,
    },
    Conversations {
        device: DeviceId,
        newest: bool,
        result: Result<Page<Conversation>, String>,
    },
    Messages {
        device: DeviceId,
        thread: ThreadId,
        newest: bool,
        result: Result<Page<Message>, String>,
    },
    Sent {
        device: DeviceId,
        placeholder: MessageId,
        result: Result<MessageId, String>,
    },
    MarkedRead {
        device: DeviceId,
        thread: ThreadId,
        result: Result<(), String>,
    },
    Contacts {
        device: DeviceId,
        result: Result<Vec<Contact>, String>,
    },
    Thumbnail {
        source: BlobSource,
        key: String,
        result: Result<Picture, String>,
    },
    FullText {
        device: DeviceId,
        thread: ThreadId,
        message: MessageId,
        result: Result<String, String>,
    },
    Opened(Result<(), String>),
    Picked(Result<Vec<PathBuf>, String>),
}
