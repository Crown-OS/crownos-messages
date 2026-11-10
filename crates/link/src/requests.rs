//! What Messages asks crownlink, in Messages' own types.

use std::{path::PathBuf, time::Duration};

use crownlink_linux::ipc::proto as ipc;
use crownos_messages_core::{
    model::{Collections, Contact, Conversation, DeviceId, Message, MessageId, ThreadId},
    store::Page,
};

use crate::{
    client::Link,
    convert::{self, device_id},
    error::LinkError,
};

/// Conversations per page: a screenful and then some.
pub const CONVERSATION_PAGE: u16 = 40;
/// Messages per page of one conversation.
pub const MESSAGE_PAGE: u16 = 50;
/// Contacts per page, the phone's own cap.
pub const CONTACT_PAGE: u16 = 200;

/// crownlink gives up on a page after 20 s without progress; wait a little longer.
const PAGE_PATIENCE: Duration = Duration::from_secs(25);
const QUICK_PATIENCE: Duration = Duration::from_secs(10);
/// Sends stream their attachments first and blobs time out only when idle, so a large one
/// may take minutes; crownlink answers with its own failure well before this.
const TRANSFER_PATIENCE: Duration = Duration::from_secs(15 * 60);

/// A message to send from the phone, into a conversation or to new recipients.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutgoingMessage {
    pub thread: Option<ThreadId>,
    pub recipients: Vec<String>,
    pub text: String,
    pub attachments: Vec<PathBuf>,
    pub sim_slot: Option<u8>,
}

/// Which shared collection a blob key comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlobSource {
    Messages,
    Contacts,
}

/// A blob crownlink downloaded into its cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobFile {
    pub path: PathBuf,
    pub mime: String,
    pub name: Option<String>,
    pub len: u64,
}

fn page_request(cursor: Option<String>, limit: u16) -> ipc::PageRequest {
    ipc::PageRequest { cursor, limit }
}

impl Link {
    /// One page of the phone's conversations, newest first.
    ///
    /// # Errors
    ///
    /// When crownlink or the phone cannot answer.
    pub async fn conversations(
        &self,
        device: DeviceId,
        cursor: Option<String>,
    ) -> Result<Page<Conversation>, LinkError> {
        let args = ipc::conversations {
            id: device_id(device),
            page: page_request(cursor, CONVERSATION_PAGE),
        };
        let page = self.call::<ipc::conversations>(args, PAGE_PATIENCE).await?;
        Ok(Page {
            items: page.items.into_iter().map(convert::conversation).collect(),
            next: page.next,
        })
    }

    /// One page of one conversation's messages, newest first.
    ///
    /// # Errors
    ///
    /// When crownlink or the phone cannot answer.
    pub async fn messages(
        &self,
        device: DeviceId,
        thread: ThreadId,
        cursor: Option<String>,
    ) -> Result<Page<Message>, LinkError> {
        let args = ipc::messages {
            id: device_id(device),
            thread_id: thread.0,
            page: page_request(cursor, MESSAGE_PAGE),
        };
        let page = self.call::<ipc::messages>(args, PAGE_PATIENCE).await?;
        Ok(Page {
            items: page.items.into_iter().map(convert::message).collect(),
            next: page.next,
        })
    }

    /// Sends `message` from the phone; answers the phone's id for it once it is queued there.
    ///
    /// # Errors
    ///
    /// When an attachment path is not UTF-8, or crownlink or the phone refuses the message.
    pub async fn send(
        &self,
        device: DeviceId,
        message: OutgoingMessage,
    ) -> Result<MessageId, LinkError> {
        let attachments = message
            .attachments
            .into_iter()
            .map(|path| {
                path.into_os_string()
                    .into_string()
                    .map_err(|path| LinkError::NonUtf8Path(path.into()))
            })
            .collect::<Result<_, _>>()?;
        let args = ipc::send_message {
            id: device_id(device),
            draft: ipc::MessageDraft {
                thread_id: message.thread.map(|thread| thread.0),
                recipients: message.recipients,
                text: message.text,
                attachments,
                sim_slot: message.sim_slot,
            },
        };
        self.call::<ipc::send_message>(args, TRANSFER_PATIENCE)
            .await
            .map(MessageId)
    }

    /// Marks a conversation read on the phone.
    ///
    /// # Errors
    ///
    /// When the phone cannot mark messages read (it is not the default SMS app) or cannot
    /// be reached.
    pub async fn mark_read(&self, device: DeviceId, thread: ThreadId) -> Result<(), LinkError> {
        let args = ipc::mark_read {
            id: device_id(device),
            thread_id: thread.0,
        };
        self.call::<ipc::mark_read>(args, PAGE_PATIENCE).await
    }

    /// Downloads the blob `key` names into crownlink's cache.
    ///
    /// # Errors
    ///
    /// When the collection is not shared, the key is unknown or the transfer fails.
    pub async fn fetch_blob(
        &self,
        device: DeviceId,
        source: BlobSource,
        key: String,
    ) -> Result<BlobFile, LinkError> {
        let args = ipc::fetch_blob {
            id: device_id(device),
            feature: match source {
                BlobSource::Messages => ipc::Feature::Messages,
                BlobSource::Contacts => ipc::Feature::Contacts,
            },
            key,
        };
        let blob = self
            .call::<ipc::fetch_blob>(args, TRANSFER_PATIENCE)
            .await?;
        Ok(BlobFile {
            path: PathBuf::from(blob.path),
            mime: blob.mime,
            name: blob.name,
            len: blob.len,
        })
    }

    /// One page of the phone's contacts, by name.
    ///
    /// # Errors
    ///
    /// When contacts are not shared or the phone cannot answer.
    pub async fn contacts(
        &self,
        device: DeviceId,
        cursor: Option<String>,
    ) -> Result<Page<Contact>, LinkError> {
        let args = ipc::contacts {
            id: device_id(device),
            page: page_request(cursor, CONTACT_PAGE),
        };
        let page = self.call::<ipc::contacts>(args, PAGE_PATIENCE).await?;
        Ok(Page {
            items: page.items.into_iter().map(convert::contact).collect(),
            next: page.next,
        })
    }

    /// What the phone last said about its collections.
    ///
    /// # Errors
    ///
    /// When crownlink cannot be reached.
    pub async fn collections(&self, device: DeviceId) -> Result<Collections, LinkError> {
        let args = ipc::collections {
            id: device_id(device),
        };
        let collections = self.call::<ipc::collections>(args, QUICK_PATIENCE).await?;
        Ok(convert::collections(collections))
    }
}
