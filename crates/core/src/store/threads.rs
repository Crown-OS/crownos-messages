use std::cmp::Reverse;

use super::Page;
use crate::model::{Conversation, Direction, Message, MessageChange, MessageId, ThreadId};

/// What a live change did to what the window holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Applied,
    /// The change is for something not loaded here; the window is behind.
    Stale,
    Ignored,
}

/// The conversations loaded so far, newest first, and where the next page starts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConversationList {
    items: Vec<Conversation>,
    next: Option<String>,
    loaded: bool,
}

impl ConversationList {
    #[must_use]
    pub fn items(&self) -> &[Conversation] {
        &self.items
    }

    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next.as_deref()
    }

    /// Whether a first page arrived since the list was last emptied.
    #[must_use]
    pub const fn is_loaded(&self) -> bool {
        self.loaded
    }

    #[must_use]
    pub fn get(&self, thread: ThreadId) -> Option<&Conversation> {
        self.items.iter().find(|item| item.thread == thread)
    }

    #[must_use]
    pub fn unread(&self) -> u32 {
        self.items.iter().map(|item| item.unread).sum()
    }

    /// The conversation with exactly `addresses`, if one is loaded.
    #[must_use]
    pub fn find_with(&self, addresses: &[String]) -> Option<ThreadId> {
        self.items
            .iter()
            .find(|item| item.is_with(addresses))
            .map(|item| item.thread)
    }

    /// Takes a fresh first page: it replaces everything as new as it reaches, and older
    /// conversations loaded before stay behind it.
    pub fn refresh(&mut self, page: Page<Conversation>) {
        let oldest = page.items.last().map(|item| item.last_at);
        let kept: Vec<Conversation> = match oldest {
            Some(oldest) if self.loaded => self
                .items
                .drain(..)
                .filter(|item| {
                    item.last_at < oldest
                        && !page.items.iter().any(|fresh| fresh.thread == item.thread)
                })
                .collect(),
            _ => Vec::new(),
        };
        if kept.is_empty() {
            self.next = page.next;
        }
        self.items = page.items;
        self.items.extend(kept);
        self.loaded = true;
        self.sort();
    }

    /// Adds an older page behind what is loaded.
    pub fn append(&mut self, page: Page<Conversation>) {
        for item in page.items {
            match self
                .items
                .iter_mut()
                .find(|known| known.thread == item.thread)
            {
                Some(known) => *known = item,
                None => self.items.push(item),
            }
        }
        self.next = page.next;
        self.loaded = true;
        self.sort();
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn apply(&mut self, change: &MessageChange) -> Outcome {
        let MessageChange::Added(message) = change else {
            return match change {
                MessageChange::Deleted { thread, .. } if self.get(*thread).is_some() => {
                    Outcome::Stale
                }
                _ => Outcome::Ignored,
            };
        };
        let Some(conversation) = self
            .items
            .iter_mut()
            .find(|item| item.thread == message.thread)
        else {
            return Outcome::Stale;
        };
        conversation.absorb(message);
        if message.direction == Direction::Incoming && !message.read {
            conversation.unread = conversation.unread.saturating_add(1);
        }
        self.sort();
        Outcome::Applied
    }

    pub fn mark_read(&mut self, thread: ThreadId) -> bool {
        self.items
            .iter_mut()
            .find(|item| item.thread == thread && item.unread > 0)
            .map(|item| item.unread = 0)
            .is_some()
    }

    fn sort(&mut self) {
        self.items
            .sort_by_key(|item| Reverse((item.last_at, item.thread)));
    }
}

/// One open conversation: the messages loaded so far, oldest first, and where older ones
/// start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadMessages {
    thread: ThreadId,
    messages: Vec<Message>,
    older: Option<String>,
    loaded: bool,
}

impl ThreadMessages {
    #[must_use]
    pub const fn new(thread: ThreadId) -> Self {
        Self {
            thread,
            messages: Vec::new(),
            older: None,
            loaded: false,
        }
    }

    #[must_use]
    pub const fn thread(&self) -> ThreadId {
        self.thread
    }

    #[must_use]
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    #[must_use]
    pub fn older_cursor(&self) -> Option<&str> {
        self.older.as_deref()
    }

    #[must_use]
    pub const fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Whether an incoming message here is still unread.
    #[must_use]
    pub fn has_unread(&self) -> bool {
        self.messages
            .iter()
            .any(|message| message.direction == Direction::Incoming && !message.read)
    }

    /// Takes the newest page: it replaces everything as recent as it reaches, while older
    /// messages loaded before and this computer's unconfirmed placeholders stay.
    pub fn refresh(&mut self, page: Page<Message>) {
        let oldest = page
            .items
            .last()
            .map(Message::order)
            .map(|(at, id)| (at, id.clone()));
        let fresh: Vec<Message> = page
            .items
            .into_iter()
            .filter(|item| item.thread == self.thread)
            .collect();
        let is_older = |known: &Message| {
            oldest
                .as_ref()
                .is_some_and(|(at, id)| (known.sent_at, &known.id) < (*at, id))
        };
        let superseded = |known: &Message| {
            fresh
                .iter()
                .any(|item| item.id == known.id || (known.id.is_local() && known.echoes(item)))
        };
        let kept: Vec<Message> = self
            .messages
            .drain(..)
            .filter(|known| (is_older(known) || known.id.is_local()) && !superseded(known))
            .collect();
        let had_older = kept.iter().any(|known| !known.id.is_local());
        if !had_older {
            self.older = page.next;
        }
        self.messages = kept;
        fresh.into_iter().for_each(|item| self.upsert(item));
        self.loaded = true;
    }

    /// Adds a page of older messages in front.
    pub fn prepend(&mut self, page: Page<Message>) {
        let thread = self.thread;
        page.items
            .into_iter()
            .filter(|item| item.thread == thread)
            .for_each(|item| self.upsert(item));
        self.older = page.next;
        self.loaded = true;
    }

    pub fn apply(&mut self, change: &MessageChange) -> Outcome {
        if change.thread() != self.thread {
            return Outcome::Ignored;
        }
        match change {
            MessageChange::Added(message) | MessageChange::Updated(message) => {
                let replaced = |known: &Message| {
                    (known.id.is_local() || known.id.is_pending())
                        && !message.id.is_pending()
                        && known.id != message.id
                        && known.echoes(message)
                };
                self.messages.retain(|known| !replaced(known));
                self.upsert(message.clone());
            }
            MessageChange::Deleted { id, .. } => self.messages.retain(|known| known.id != *id),
        }
        Outcome::Applied
    }

    /// Shows `message`, which this computer is sending, before the phone confirms it.
    pub fn push_placeholder(&mut self, message: Message) {
        self.upsert(message);
    }

    /// The phone took the message `placeholder` stood for and calls it `id`.
    pub fn confirm(&mut self, placeholder: &MessageId, id: MessageId) {
        if self.messages.iter().any(|known| known.id == id) {
            self.messages.retain(|known| known.id != *placeholder);
            return;
        }
        if let Some(known) = self
            .messages
            .iter_mut()
            .find(|known| known.id == *placeholder)
        {
            known.id = id;
        }
    }

    /// The phone refused the message `placeholder` stood for; hands it back.
    pub fn withdraw(&mut self, placeholder: &MessageId) -> Option<Message> {
        let index = self
            .messages
            .iter()
            .position(|known| known.id == *placeholder)?;
        Some(self.messages.remove(index))
    }

    pub fn mark_read(&mut self) {
        self.messages
            .iter_mut()
            .filter(|message| message.direction == Direction::Incoming)
            .for_each(|message| message.read = true);
    }

    fn upsert(&mut self, message: Message) {
        self.messages.retain(|known| known.id != message.id);
        let at = self
            .messages
            .partition_point(|known| known.order() <= message.order());
        self.messages.insert(at, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Delivery, Timestamp, conversation::fixtures::conversation, message::fixtures::message,
    };

    fn page<T>(items: Vec<T>, next: Option<&str>) -> Page<T> {
        Page {
            items,
            next: next.map(str::to_owned),
        }
    }

    fn ids(thread: &ThreadMessages) -> Vec<&str> {
        thread
            .messages()
            .iter()
            .map(|message| message.id.0.as_str())
            .collect()
    }

    fn threads(list: &ConversationList) -> Vec<u64> {
        list.items().iter().map(|item| item.thread.0).collect()
    }

    #[test]
    fn conversation_pages_stack_newest_first() {
        let mut list = ConversationList::default();
        list.refresh(page(
            vec![conversation(1, 300, &["A"]), conversation(2, 200, &["B"])],
            Some("c1"),
        ));
        list.append(page(vec![conversation(3, 100, &["C"])], None));
        assert_eq!(threads(&list), [1, 2, 3]);
        assert_eq!(list.next_cursor(), None);
    }

    #[test]
    fn a_refresh_keeps_older_pages_and_drops_deleted_recent_ones() {
        let mut list = ConversationList::default();
        list.refresh(page(
            vec![conversation(1, 300, &["A"]), conversation(2, 200, &["B"])],
            Some("c1"),
        ));
        list.append(page(vec![conversation(3, 100, &["C"])], Some("c2")));
        list.refresh(page(
            vec![conversation(4, 400, &["D"]), conversation(1, 300, &["A"])],
            Some("fresh"),
        ));
        assert_eq!(threads(&list), [4, 1, 2, 3]);
        assert_eq!(list.next_cursor(), Some("c2"));

        list.refresh(page(
            vec![conversation(1, 500, &["A"]), conversation(3, 100, &["C"])],
            Some("again"),
        ));
        assert_eq!(threads(&list), [1, 3]);
        assert_eq!(list.next_cursor(), Some("again"));
    }

    #[test]
    fn a_new_message_moves_its_conversation_up() {
        let mut list = ConversationList::default();
        list.refresh(page(
            vec![conversation(1, 300, &["A"]), conversation(2, 200, &["B"])],
            None,
        ));
        let incoming = message(2, "sms/9", 400, "hello", Direction::Incoming);
        assert_eq!(
            list.apply(&MessageChange::Added(incoming)),
            Outcome::Applied
        );
        assert_eq!(threads(&list), [2, 1]);
        assert_eq!(
            list.get(ThreadId(2))
                .map(|item| (item.unread, item.snippet.as_str())),
            Some((1, "hello"))
        );
        assert!(list.mark_read(ThreadId(2)));
        assert_eq!(list.unread(), 0);
    }

    #[test]
    fn changes_to_unknown_conversations_are_stale() {
        let mut list = ConversationList::default();
        list.refresh(page(vec![conversation(1, 300, &["A"])], None));
        let elsewhere = message(7, "sms/1", 400, "hi", Direction::Incoming);
        assert_eq!(list.apply(&MessageChange::Added(elsewhere)), Outcome::Stale);
        let gone = MessageChange::Deleted {
            thread: ThreadId(1),
            id: MessageId("sms/1".into()),
        };
        assert_eq!(list.apply(&gone), Outcome::Stale);
    }

    #[test]
    fn thread_pages_read_oldest_first() {
        let mut thread = ThreadMessages::new(ThreadId(1));
        thread.refresh(page(
            vec![
                message(1, "sms/3", 30, "c", Direction::Incoming),
                message(1, "sms/2", 20, "b", Direction::Outgoing),
            ],
            Some("older"),
        ));
        thread.prepend(page(
            vec![message(1, "sms/1", 10, "a", Direction::Incoming)],
            None,
        ));
        assert_eq!(ids(&thread), ["sms/1", "sms/2", "sms/3"]);
        assert_eq!(thread.older_cursor(), None);
    }

    #[test]
    fn same_time_messages_order_by_id() {
        let mut thread = ThreadMessages::new(ThreadId(1));
        thread.refresh(page(
            vec![
                message(1, "sms/b", 10, "b", Direction::Incoming),
                message(1, "mms/a", 10, "a", Direction::Incoming),
            ],
            None,
        ));
        assert_eq!(ids(&thread), ["mms/a", "sms/b"]);
    }

    #[test]
    fn a_placeholder_becomes_the_phone_s_message() {
        let mut thread = ThreadMessages::new(ThreadId(1));
        let placeholder = MessageId::local(1);
        let mut sending = message(1, &placeholder.0, 50, "hi", Direction::Outgoing);
        sending.delivery = Delivery::Pending;
        thread.push_placeholder(sending);
        thread.confirm(&placeholder, MessageId("sms/8".into()));
        assert_eq!(ids(&thread), ["sms/8"]);
        let mut stored = message(1, "sms/8", 51, "hi", Direction::Outgoing);
        stored.delivery = Delivery::Sent;
        thread.apply(&MessageChange::Updated(stored));
        assert_eq!(ids(&thread), ["sms/8"]);
        assert_eq!(
            thread.messages().first().map(|item| item.delivery),
            Some(Delivery::Sent)
        );
    }

    #[test]
    fn the_stored_row_arriving_first_replaces_the_placeholder() {
        let mut thread = ThreadMessages::new(ThreadId(1));
        let placeholder = MessageId::local(2);
        thread.push_placeholder(message(1, &placeholder.0, 50, "hi", Direction::Outgoing));
        thread.apply(&MessageChange::Added(message(
            1,
            "sms/8",
            51,
            "hi",
            Direction::Outgoing,
        )));
        assert_eq!(ids(&thread), ["sms/8"]);
        thread.confirm(&placeholder, MessageId("sms/8".into()));
        assert_eq!(ids(&thread), ["sms/8"]);
        assert!(thread.withdraw(&placeholder).is_none());
    }

    #[test]
    fn the_phone_s_pending_row_gives_way_to_the_stored_one() {
        let mut thread = ThreadMessages::new(ThreadId(1));
        thread.apply(&MessageChange::Added(message(
            1,
            "pending/4",
            50,
            "pic",
            Direction::Outgoing,
        )));
        thread.apply(&MessageChange::Added(message(
            1,
            "mms/2",
            52,
            "pic",
            Direction::Outgoing,
        )));
        assert_eq!(ids(&thread), ["mms/2"]);
    }

    #[test]
    fn a_refresh_keeps_placeholders_and_older_messages() {
        let mut thread = ThreadMessages::new(ThreadId(1));
        thread.refresh(page(
            vec![message(1, "sms/2", 20, "b", Direction::Incoming)],
            Some("o1"),
        ));
        thread.prepend(page(
            vec![message(1, "sms/1", 10, "a", Direction::Incoming)],
            Some("o2"),
        ));
        thread.push_placeholder(message(1, "local/1", 40, "sending", Direction::Outgoing));
        thread.refresh(page(
            vec![
                message(1, "sms/3", 30, "c", Direction::Incoming),
                message(1, "sms/2", 20, "b", Direction::Incoming),
            ],
            Some("o9"),
        ));
        assert_eq!(ids(&thread), ["sms/1", "sms/2", "sms/3", "local/1"]);
        assert_eq!(thread.older_cursor(), Some("o2"));
    }

    #[test]
    fn deletions_and_other_threads() {
        let mut thread = ThreadMessages::new(ThreadId(1));
        thread.refresh(page(
            vec![message(1, "sms/2", 20, "b", Direction::Incoming)],
            None,
        ));
        let other = MessageChange::Added(message(2, "sms/9", 30, "x", Direction::Incoming));
        assert_eq!(thread.apply(&other), Outcome::Ignored);
        let gone = MessageChange::Deleted {
            thread: ThreadId(1),
            id: MessageId("sms/2".into()),
        };
        assert_eq!(thread.apply(&gone), Outcome::Applied);
        assert!(thread.messages().is_empty());
        assert_eq!(Timestamp::default(), Timestamp(0));
    }

    #[test]
    fn marking_read_clears_unread_incoming() {
        let mut thread = ThreadMessages::new(ThreadId(1));
        thread.refresh(page(
            vec![message(1, "sms/2", 20, "b", Direction::Incoming)],
            None,
        ));
        assert!(thread.has_unread());
        thread.mark_read();
        assert!(!thread.has_unread());
    }
}
