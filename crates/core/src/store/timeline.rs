use jiff::civil::Date;

use crate::{
    model::{Direction, Message, MessageId},
    util::time_label::TimeLabels,
};

/// Messages closer together than this from one sender read as one group of bubbles.
const GROUP_GAP_MS: u64 = 5 * 60 * 1000;

/// One row of an open conversation: a day heading or a message bubble.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TimelineRow {
    Day {
        date: Date,
        label: String,
    },
    Bubble {
        message: Message,
        /// The first bubble of a run from one sender.
        opens_group: bool,
        /// The last bubble of a run, which carries the time and the tail.
        closes_group: bool,
        /// Name the sender above the bubble, as a group conversation needs.
        show_sender: bool,
    },
}

/// What keeps a row's identity while the timeline changes around it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RowKey {
    Day(Date),
    Message(MessageId),
}

impl TimelineRow {
    #[must_use]
    pub fn key(&self) -> RowKey {
        match self {
            Self::Day { date, .. } => RowKey::Day(*date),
            Self::Bubble { message, .. } => RowKey::Message(message.id.clone()),
        }
    }
}

/// Lays `messages` (oldest first) out as rows: a heading before each new day, and bubbles
/// grouped by sender and time.
#[must_use]
pub fn timeline(
    messages: &[Message],
    labels: &TimeLabels,
    group_conversation: bool,
) -> Vec<TimelineRow> {
    let mut rows = Vec::with_capacity(messages.len() + 4);
    let mut previous_date = None;
    for (index, message) in messages.iter().enumerate() {
        let date = labels.date(message.sent_at);
        if date.is_some() && date != previous_date {
            if let Some(date) = date {
                rows.push(TimelineRow::Day {
                    date,
                    label: labels.day(message.sent_at),
                });
            }
            previous_date = date;
        }
        let before = index.checked_sub(1).and_then(|before| messages.get(before));
        let after = messages.get(index + 1);
        let joins = |earlier: &Message, later: &Message| {
            same_sender(earlier, later)
                && later.sent_at.0.saturating_sub(earlier.sent_at.0) < GROUP_GAP_MS
                && labels.date(earlier.sent_at) == labels.date(later.sent_at)
        };
        let opens_group = before.is_none_or(|before| !joins(before, message));
        rows.push(TimelineRow::Bubble {
            message: message.clone(),
            opens_group,
            closes_group: after.is_none_or(|after| !joins(message, after)),
            show_sender: group_conversation
                && opens_group
                && message.direction == Direction::Incoming,
        });
    }
    rows
}

fn same_sender(left: &Message, right: &Message) -> bool {
    left.direction == right.direction && left.sender == right.sender
}

#[cfg(test)]
mod tests {
    use jiff::tz::TimeZone;

    use super::*;
    use crate::model::{Timestamp, message::fixtures::message};

    const NOON: u64 = 1_793_966_400_000;
    const MINUTE: u64 = 60_000;

    fn shape(rows: &[TimelineRow]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                TimelineRow::Day { label, .. } => format!("# {label}"),
                TimelineRow::Bubble {
                    message,
                    opens_group,
                    closes_group,
                    show_sender,
                } => format!(
                    "{}{}{}{}",
                    message.id.0,
                    if *opens_group { " open" } else { "" },
                    if *closes_group { " close" } else { "" },
                    if *show_sender { " named" } else { "" },
                ),
            })
            .collect()
    }

    #[test]
    fn days_head_their_messages_and_runs_group() {
        let labels = TimeLabels::at(Timestamp(NOON), TimeZone::UTC);
        let messages = [
            message(
                1,
                "sms/1",
                NOON - 24 * 60 * MINUTE,
                "a",
                Direction::Incoming,
            ),
            message(1, "sms/2", NOON - 10 * MINUTE, "b", Direction::Incoming),
            message(1, "sms/3", NOON - 9 * MINUTE, "c", Direction::Incoming),
            message(1, "sms/4", NOON - 8 * MINUTE, "d", Direction::Outgoing),
            message(1, "sms/5", NOON, "e", Direction::Outgoing),
        ];
        assert_eq!(
            shape(&timeline(&messages, &labels, true)),
            [
                "# Yesterday",
                "sms/1 open close named",
                "# Today",
                "sms/2 open named",
                "sms/3 close",
                "sms/4 open close",
                "sms/5 open close",
            ]
        );
        assert_eq!(timeline(&messages, &labels, false).len(), 7);
    }
}
