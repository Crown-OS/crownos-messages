use std::borrow::Cow;

use super::{Attachment, ThreadId, Timestamp};

/// The phone's id for a message ("sms/12", "mms/7", "pending/3"), or a local placeholder
/// ("local/2") for one this computer is still handing to the phone.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageId(pub String);

impl MessageId {
    const LOCAL: &'static str = "local/";
    const PENDING: &'static str = "pending/";

    #[must_use]
    pub fn local(sequence: u64) -> Self {
        Self(format!("{}{sequence}", Self::LOCAL))
    }

    /// A message this computer is sending that the phone has not taken yet.
    #[must_use]
    pub fn is_local(&self) -> bool {
        self.0.starts_with(Self::LOCAL)
    }

    /// A message the phone sent but could not find in its own store yet.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.0.starts_with(Self::PENDING)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Incoming,
    Outgoing,
}

/// How far an outgoing message got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Delivery {
    None,
    Pending,
    Sent,
    Delivered,
    Failed,
}

/// One SMS or MMS.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Message {
    pub thread: ThreadId,
    pub id: MessageId,
    pub direction: Direction,
    /// Who sent an incoming message; `None` for the phone's own.
    pub sender: Option<String>,
    pub sent_at: Timestamp,
    pub text: Option<String>,
    /// Only the start of the text came along.
    pub text_truncated: bool,
    pub read: bool,
    pub delivery: Delivery,
    pub attachments: Vec<Attachment>,
    /// Attachments the phone left out to keep the message small.
    pub attachments_omitted: u16,
    pub sim_slot: Option<u8>,
}

impl Message {
    /// One line standing for the message in a list: its text, else what it carries.
    #[must_use]
    pub fn preview(&self) -> Cow<'_, str> {
        if let Some(text) = self
            .text
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            return Cow::Borrowed(text);
        }
        match self.attachments.as_slice() {
            [] => Cow::Borrowed(""),
            [only] => Cow::Borrowed(only.kind().noun()),
            many => Cow::Owned(format!("{} attachments", many.len())),
        }
    }

    /// Whether `other` is the same outgoing message seen twice: once as this computer's or the
    /// phone's placeholder and once as the stored row.
    #[must_use]
    pub fn echoes(&self, other: &Self) -> bool {
        self.direction == Direction::Outgoing
            && other.direction == Direction::Outgoing
            && self.thread == other.thread
            && self.text.as_deref().map(str::trim) == other.text.as_deref().map(str::trim)
            && self.attachments.len() == other.attachments.len()
    }

    pub(crate) fn order(&self) -> (Timestamp, &MessageId) {
        (self.sent_at, &self.id)
    }
}

/// A message arrived, changed or went away on the phone.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MessageChange {
    Added(Message),
    Updated(Message),
    Deleted { thread: ThreadId, id: MessageId },
}

impl MessageChange {
    #[must_use]
    pub const fn thread(&self) -> ThreadId {
        match self {
            Self::Added(message) | Self::Updated(message) => message.thread,
            Self::Deleted { thread, .. } => *thread,
        }
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    pub fn message(thread: u64, id: &str, at: u64, text: &str, direction: Direction) -> Message {
        Message {
            thread: ThreadId(thread),
            id: MessageId(id.to_owned()),
            direction,
            sender: (direction == Direction::Incoming).then(|| "+15550100".to_owned()),
            sent_at: Timestamp(at),
            text: Some(text.to_owned()),
            text_truncated: false,
            read: direction == Direction::Outgoing,
            delivery: Delivery::None,
            attachments: Vec::new(),
            attachments_omitted: 0,
            sim_slot: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fixtures::message, *};

    #[test]
    fn preview_falls_back_to_the_attachments() {
        let mut item = message(1, "mms/1", 0, "  ", Direction::Incoming);
        assert_eq!(item.preview(), "");
        let photo = Attachment {
            key: "mms-part/1/0".into(),
            mime: "image/jpeg".into(),
            name: None,
            len: 1,
            thumbnail_key: None,
            width: 0,
            height: 0,
        };
        item.attachments.push(photo.clone());
        assert_eq!(item.preview(), "Photo");
        item.attachments.push(photo);
        assert_eq!(item.preview(), "2 attachments");
    }

    #[test]
    fn ids_tell_placeholders_apart() {
        assert!(MessageId::local(3).is_local());
        assert!(MessageId("pending/9".into()).is_pending());
        assert!(!MessageId("sms/9".into()).is_local());
    }

    #[test]
    fn an_outgoing_echo_matches_its_stored_row() {
        let echo = message(1, "local/1", 10, "hi ", Direction::Outgoing);
        let stored = message(1, "sms/4", 12, "hi", Direction::Outgoing);
        assert!(echo.echoes(&stored));
        assert!(!echo.echoes(&message(1, "sms/5", 12, "hi", Direction::Incoming)));
    }
}
