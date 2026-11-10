use super::{Message, Recipient, Timestamp};
use crate::util::phone_number::same_address;

/// The phone's id for a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThreadId(pub u64);

/// One conversation as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Conversation {
    pub thread: ThreadId,
    pub participants: Vec<Recipient>,
    /// The real number of participants, which `participants` may fall short of.
    pub participant_count: u16,
    pub snippet: String,
    pub last_at: Timestamp,
    pub unread: u32,
    pub has_attachments: bool,
}

impl Conversation {
    /// The participants' names, with how many more there are than the phone sent.
    #[must_use]
    pub fn title(&self) -> String {
        let names: Vec<&str> = self
            .participants
            .iter()
            .map(Recipient::display_name)
            .collect();
        let hidden = usize::from(self.participant_count).saturating_sub(names.len());
        match (names.is_empty(), hidden) {
            (true, 0) => "Unknown sender".to_owned(),
            (true, _) => format!("{hidden} people"),
            (false, 0) => names.join(", "),
            (false, _) => format!("{} +{hidden}", names.join(", ")),
        }
    }

    #[must_use]
    pub fn is_group(&self) -> bool {
        self.participant_count > 1 || self.participants.len() > 1
    }

    /// Whether `query` is part of a name, a number or the last message.
    #[must_use]
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || self.snippet.to_lowercase().contains(&query)
            || self.participants.iter().any(|participant| {
                participant.display_name().to_lowercase().contains(&query)
                    || participant.address.to_lowercase().contains(&query)
            })
    }

    /// Whether this is the conversation with exactly `addresses`.
    #[must_use]
    pub fn is_with(&self, addresses: &[String]) -> bool {
        usize::from(self.participant_count.max(1)) == addresses.len()
            && self.participants.len() == addresses.len()
            && addresses.iter().all(|address| {
                self.participants
                    .iter()
                    .any(|participant| same_address(&participant.address, address))
            })
    }

    /// Takes `message` in as the newest one, unless the list already shows a newer one.
    pub fn absorb(&mut self, message: &Message) {
        if message.sent_at < self.last_at {
            return;
        }
        self.last_at = message.sent_at;
        self.snippet = message.preview().into_owned();
        self.has_attachments |= !message.attachments.is_empty();
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    pub fn conversation(thread: u64, at: u64, names: &[&str]) -> Conversation {
        Conversation {
            thread: ThreadId(thread),
            participants: names
                .iter()
                .enumerate()
                .map(|(index, name)| Recipient {
                    name: Some((*name).to_owned()),
                    ..Recipient::new(format!("+1555010{thread}{index}"))
                })
                .collect(),
            participant_count: u16::try_from(names.len()).unwrap_or(u16::MAX),
            snippet: format!("Last words of {thread}"),
            last_at: Timestamp(at),
            unread: 0,
            has_attachments: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fixtures::conversation, *};
    use crate::model::{Direction, message::fixtures::message};

    #[test]
    fn titles_name_everyone_the_phone_sent() {
        let mut group = conversation(1, 0, &["Ada", "Grace"]);
        assert_eq!(group.title(), "Ada, Grace");
        group.participant_count = 5;
        assert_eq!(group.title(), "Ada, Grace +3");
        group.participants.clear();
        assert_eq!(group.title(), "5 people");
        group.participant_count = 0;
        assert_eq!(group.title(), "Unknown sender");
    }

    #[test]
    fn search_looks_at_names_numbers_and_snippet() {
        let chat = conversation(4, 0, &["Ada Lovelace"]);
        assert!(chat.matches("love"));
        assert!(chat.matches("55501040"));
        assert!(chat.matches("last words"));
        assert!(!chat.matches("grace"));
        assert!(chat.matches(" "));
    }

    #[test]
    fn is_with_needs_the_same_people() {
        let chat = conversation(4, 0, &["Ada"]);
        assert!(chat.is_with(&["+1 555 010 40".to_owned()]));
        assert!(!chat.is_with(&["+15550104".to_owned(), "+15550199".to_owned()]));
    }

    #[test]
    fn absorbing_keeps_the_newest_message() {
        let mut chat = conversation(4, 100, &["Ada"]);
        chat.absorb(&message(4, "sms/1", 50, "old", Direction::Incoming));
        assert_eq!(chat.snippet, "Last words of 4");
        chat.absorb(&message(4, "sms/2", 150, "new", Direction::Incoming));
        assert_eq!(
            (chat.snippet.as_str(), chat.last_at),
            ("new", Timestamp(150))
        );
    }
}
