//! Everything the window shows, as signals the views bind to, plus the channels to the
//! workers. Built once per window; tests build one from fixtures.

use std::collections::HashMap;

use crownos_messages_core::{
    model::{Collections, Contact, Device, DeviceId, Recipient, ThreadId},
    store::{ConversationList, DraftAttachment, DraftTarget, Drafts, ThreadMessages},
    uri::SmsLink,
    util::time_label::TimeLabels,
};
use crownos_messages_link::{BlobSource, notifier::Notice};
use crownui::{
    kit::{Toasts, provide_toasts},
    prelude::{Cx, Signal, Size, Store as Cell},
};
use crownui::ext::ImageId;
use tokio::sync::mpsc::UnboundedSender;

use crate::worker::Command;

/// Whether crownlink is reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    Connecting,
    Ready,
    Unavailable,
}

/// Where a load from the phone stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Load {
    Idle,
    Loading,
    Failed(String),
}

impl Load {
    #[must_use]
    pub const fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }
}

/// What the content side shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Selection {
    Nothing,
    Thread(ThreadId),
    NewMessage,
}

impl Selection {
    #[must_use]
    pub const fn draft_target(self) -> Option<DraftTarget> {
        match self {
            Self::Nothing => None,
            Self::Thread(thread) => Some(DraftTarget::Thread(thread)),
            Self::NewMessage => Some(DraftTarget::NewMessage),
        }
    }
}

/// A picture from the phone, decoded and handed to the renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Thumb {
    Loading,
    Ready { image: ImageId, size: Size },
    Failed,
}

pub type ThumbKey = (BlobSource, String);

/// The composer under the open conversation or new message.
#[derive(Clone, Copy)]
pub struct Composer {
    pub text: Signal<String>,
    pub attachments: Signal<Vec<DraftAttachment>>,
    pub sending: Signal<bool>,
}

/// The "To" line of a new message.
#[derive(Clone, Copy)]
pub struct NewMessage {
    pub recipients: Signal<Vec<Recipient>>,
    pub query: Signal<String>,
    /// The recipients of a conversation just started here, until the phone lists it.
    pub awaiting: Cell<Option<Vec<String>>>,
}

#[derive(Clone, Copy)]
pub struct Store {
    pub commands: Cell<Option<UnboundedSender<Command>>>,
    pub notices: Cell<Option<UnboundedSender<Notice>>>,
    pub labels: Signal<TimeLabels>,
    pub link: Signal<LinkState>,
    pub devices: Signal<Vec<Device>>,
    pub device: Signal<Option<DeviceId>>,
    pub collections: Signal<Collections>,
    pub conversations: Signal<ConversationList>,
    pub conversations_load: Signal<Load>,
    pub query: Signal<String>,
    pub selection: Signal<Selection>,
    pub thread: Signal<Option<ThreadMessages>>,
    pub thread_load: Signal<Load>,
    pub older_load: Signal<Load>,
    pub composer: Composer,
    pub drafts: Cell<Drafts>,
    pub new_message: NewMessage,
    pub contacts: Signal<Vec<Contact>>,
    pub thumbs: Signal<HashMap<ThumbKey, Thumb>>,
    pub next_placeholder: Cell<u64>,
    /// An `sms:` link that arrived before the conversations did.
    pub pending_link: Cell<Option<SmsLink>>,
    pub toasts: Toasts,
}

impl Store {
    pub fn new(cx: &mut Cx, labels: TimeLabels) -> Self {
        Self {
            commands: cx.store(None),
            notices: cx.store(None),
            labels: cx.signal(labels),
            link: cx.signal(LinkState::Connecting),
            devices: cx.signal(Vec::new()),
            device: cx.signal(None),
            collections: cx.signal(Collections::default()),
            conversations: cx.signal(ConversationList::default()),
            conversations_load: cx.signal(Load::Idle),
            query: cx.signal(String::new()),
            selection: cx.signal(Selection::Nothing),
            thread: cx.signal(None),
            thread_load: cx.signal(Load::Idle),
            older_load: cx.signal(Load::Idle),
            composer: Composer {
                text: cx.signal(String::new()),
                attachments: cx.signal(Vec::new()),
                sending: cx.signal(false),
            },
            drafts: cx.store(Drafts::default()),
            new_message: NewMessage {
                recipients: cx.signal(Vec::new()),
                query: cx.signal(String::new()),
                awaiting: cx.store(None),
            },
            contacts: cx.signal(Vec::new()),
            thumbs: cx.signal(HashMap::new()),
            next_placeholder: cx.store(1),
            pending_link: cx.store(None),
            toasts: provide_toasts(cx),
        }
    }

    /// Hands `command` to the crownlink worker; dropped when no worker runs.
    pub fn send(self, cx: &mut Cx, command: Command) {
        self.commands.with(cx, |sender| {
            if let Some(sender) = sender {
                sender.send(command).ok();
            }
        });
    }

    pub fn notify(self, cx: &mut Cx, notice: Notice) {
        self.notices.with(cx, |sender| {
            if let Some(sender) = sender {
                sender.send(notice).ok();
            }
        });
    }

    /// The device Messages talks through right now.
    pub fn current_device(self, cx: &mut Cx) -> Option<Device> {
        let id = self.device.get(cx)?;
        self.devices
            .with(cx, |devices| devices.iter().find(|device| device.id == id).cloned())
    }
}
