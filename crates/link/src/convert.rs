//! crownlink's IPC types into Messages' own, so nothing above this crate sees the wire.

use crownlink_linux::ipc::proto as ipc;
use crownos_messages_core::model::{
    Attachment, Collections, Contact, ContactNumber, ContactsStatus, Conversation, Delivery,
    Device, DeviceId, DeviceKind, Direction, Message, MessageChange, MessageId, MessagingStatus,
    Recipient, Sharing, ThreadId, Timestamp,
};

pub(crate) fn device(info: &ipc::DeviceInfo) -> Device {
    Device {
        id: DeviceId(info.id.0),
        name: info.name.clone(),
        kind: match info.class {
            ipc::DeviceClass::Phone => DeviceKind::Phone,
            ipc::DeviceClass::Tablet => DeviceKind::Tablet,
            ipc::DeviceClass::Watch => DeviceKind::Watch,
            ipc::DeviceClass::Computer => DeviceKind::Computer,
        },
        connected: info.connected,
        battery: info.battery.map(|battery| battery.percent),
        sharing: Sharing {
            messages: info.features.contains(ipc::Feature::Messages),
            contacts: info.features.contains(ipc::Feature::Contacts),
            notifications: info.features.contains(ipc::Feature::Notifications),
        },
    }
}

pub(crate) const fn device_id(id: DeviceId) -> ipc::DeviceId {
    ipc::DeviceId(id.0)
}

pub(crate) const fn device_id_from(id: ipc::DeviceId) -> DeviceId {
    DeviceId(id.0)
}

fn recipient(participant: ipc::Participant) -> Recipient {
    Recipient {
        address: participant.address,
        name: participant.name,
        contact_key: participant.contact_key,
    }
}

pub(crate) fn conversation(item: ipc::Conversation) -> Conversation {
    Conversation {
        thread: ThreadId(item.thread_id),
        participants: item.participants.into_iter().map(recipient).collect(),
        participant_count: item.participant_count,
        snippet: item.snippet,
        last_at: Timestamp(item.last_unix_ms),
        unread: item.unread,
        has_attachments: item.has_attachments,
    }
}

fn attachment(item: ipc::Attachment) -> Attachment {
    Attachment {
        key: item.key,
        mime: item.mime,
        name: item.name,
        len: item.len,
        thumbnail_key: item.thumbnail_key,
        width: item.width,
        height: item.height,
    }
}

/// Which way a message went and how far it got, from the box it sits in and its status.
fn direction_and_delivery(
    folder: ipc::MessageFolder,
    status: ipc::DeliveryStatus,
    sender: Option<&str>,
) -> (Direction, Delivery) {
    let direction = match folder {
        ipc::MessageFolder::Inbox => Direction::Incoming,
        ipc::MessageFolder::Sent
        | ipc::MessageFolder::Draft
        | ipc::MessageFolder::Outbox
        | ipc::MessageFolder::Failed
        | ipc::MessageFolder::Queued => Direction::Outgoing,
        ipc::MessageFolder::Other(_) if sender.is_some() => Direction::Incoming,
        ipc::MessageFolder::Other(_) => Direction::Outgoing,
    };
    let delivery = match (folder, status) {
        (ipc::MessageFolder::Failed, _) | (_, ipc::DeliveryStatus::Failed) => Delivery::Failed,
        (_, ipc::DeliveryStatus::Delivered) => Delivery::Delivered,
        (_, ipc::DeliveryStatus::Sent) => Delivery::Sent,
        (ipc::MessageFolder::Outbox | ipc::MessageFolder::Queued, _)
        | (_, ipc::DeliveryStatus::Pending) => Delivery::Pending,
        _ => Delivery::None,
    };
    (direction, delivery)
}

pub(crate) fn message(item: ipc::MessageItem) -> Message {
    let (direction, delivery) =
        direction_and_delivery(item.folder, item.status, item.sender.as_deref());
    Message {
        thread: ThreadId(item.thread_id),
        id: MessageId(item.message_id),
        direction,
        sender: item.sender,
        sent_at: Timestamp(item.unix_ms),
        text: item.text,
        text_truncated: item.text_truncated,
        read: item.read,
        delivery,
        attachments: item.attachments.into_iter().map(attachment).collect(),
        attachments_omitted: item.attachments_omitted,
        sim_slot: item.sim_slot,
    }
}

pub(crate) fn change(change: ipc::MessageChange) -> MessageChange {
    match change {
        ipc::MessageChange::Added(item) => MessageChange::Added(message(item)),
        ipc::MessageChange::Updated(item) => MessageChange::Updated(message(item)),
        ipc::MessageChange::Deleted {
            thread_id,
            message_id,
        } => MessageChange::Deleted {
            thread: ThreadId(thread_id),
            id: MessageId(message_id),
        },
    }
}

pub(crate) fn collections(collections: ipc::Collections) -> Collections {
    Collections {
        messages: collections.messages.map(|counters| MessagingStatus {
            revision: counters.revision,
            unread: counters.unread,
            send_sms: counters.can(ipc::MessageCounters::SEND_SMS),
            send_mms: counters.can(ipc::MessageCounters::SEND_MMS),
            mark_read: counters.can(ipc::MessageCounters::MARK_READ),
            mms_max_bytes: (counters.mms_max_bytes > 0).then_some(counters.mms_max_bytes),
        }),
        contacts: collections.contacts.map(|counters| ContactsStatus {
            revision: counters.revision,
            count: counters.count,
            photos: counters.can(ipc::ContactCounters::PHOTOS),
        }),
    }
}

fn phone_label(phone: &ipc::ContactPhone) -> String {
    match phone.kind {
        ipc::PhoneKind::Home => "Home".to_owned(),
        ipc::PhoneKind::Mobile => "Mobile".to_owned(),
        ipc::PhoneKind::Work => "Work".to_owned(),
        ipc::PhoneKind::Main => "Main".to_owned(),
        ipc::PhoneKind::Other(_) => phone
            .label
            .clone()
            .filter(|label| !label.trim().is_empty())
            .unwrap_or_else(|| "Other".to_owned()),
    }
}

pub(crate) fn contact(item: ipc::Contact) -> Contact {
    Contact {
        numbers: item
            .phones
            .iter()
            .map(|phone| ContactNumber {
                number: phone.number.clone(),
                label: phone_label(phone),
            })
            .collect(),
        key: item.contact_key,
        name: item.name,
        starred: item.starred,
        photo_key: item.photo_key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(
        folder: ipc::MessageFolder,
        status: ipc::DeliveryStatus,
        sender: Option<&str>,
    ) -> ipc::MessageItem {
        ipc::MessageItem {
            thread_id: 4,
            message_id: "sms/9".into(),
            folder,
            sender: sender.map(str::to_owned),
            unix_ms: 1_000,
            text: Some("hi".into()),
            text_truncated: false,
            read: true,
            status,
            attachments: vec![ipc::Attachment {
                key: "mms-part/3/1000".into(),
                mime: "image/jpeg".into(),
                name: None,
                len: 2048,
                thumbnail_key: Some("mms-thumb/3/1000/480".into()),
                width: 640,
                height: 480,
            }],
            attachments_omitted: 2,
            sim_slot: Some(1),
        }
    }

    #[test]
    fn a_message_keeps_every_field() {
        let converted = message(item(
            ipc::MessageFolder::Inbox,
            ipc::DeliveryStatus::None,
            Some("+1555"),
        ));
        assert_eq!(converted.thread, ThreadId(4));
        assert_eq!(converted.id, MessageId("sms/9".into()));
        assert_eq!(
            (converted.direction, converted.delivery),
            (Direction::Incoming, Delivery::None)
        );
        assert_eq!(converted.sent_at, Timestamp(1_000));
        assert_eq!(converted.attachments_omitted, 2);
        assert_eq!(converted.sim_slot, Some(1));
        let photo = converted.attachments.first().unwrap();
        assert_eq!(photo.thumbnail_key.as_deref(), Some("mms-thumb/3/1000/480"));
        assert_eq!((photo.width, photo.height, photo.len), (640, 480, 2048));
    }

    #[test]
    fn folders_and_statuses_decide_direction_and_delivery() {
        let read = |folder, status, sender| {
            let converted = message(item(folder, status, sender));
            (converted.direction, converted.delivery)
        };
        use ipc::{DeliveryStatus as S, MessageFolder as F};
        assert_eq!(
            read(F::Sent, S::Delivered, None),
            (Direction::Outgoing, Delivery::Delivered)
        );
        assert_eq!(
            read(F::Outbox, S::None, None),
            (Direction::Outgoing, Delivery::Pending)
        );
        assert_eq!(
            read(F::Failed, S::Sent, None),
            (Direction::Outgoing, Delivery::Failed)
        );
        assert_eq!(
            read(F::Other(9), S::Other(7), Some("+1")),
            (Direction::Incoming, Delivery::None)
        );
        assert_eq!(
            read(F::Other(9), S::Pending, None),
            (Direction::Outgoing, Delivery::Pending)
        );
    }

    #[test]
    fn counters_become_capabilities() {
        let converted = collections(ipc::Collections {
            messages: Some(ipc::MessageCounters {
                revision: 7,
                unread: 3,
                capabilities: ipc::MessageCounters::SEND_SMS | ipc::MessageCounters::MARK_READ,
                mms_max_bytes: 0,
            }),
            calls: None,
            contacts: Some(ipc::ContactCounters {
                revision: 2,
                count: 40,
                capabilities: ipc::ContactCounters::PHOTOS,
            }),
        });
        assert_eq!(
            converted.messages,
            Some(MessagingStatus {
                revision: 7,
                unread: 3,
                send_sms: true,
                send_mms: false,
                mark_read: true,
                mms_max_bytes: None,
            })
        );
        assert_eq!(
            converted.contacts,
            Some(ContactsStatus {
                revision: 2,
                count: 40,
                photos: true
            })
        );
    }

    #[test]
    fn contacts_label_their_numbers() {
        let converted = contact(ipc::Contact {
            contact_key: "k".into(),
            name: "Ada".into(),
            starred: true,
            phones: vec![
                ipc::ContactPhone {
                    number: "1".into(),
                    kind: ipc::PhoneKind::Mobile,
                    label: None,
                },
                ipc::ContactPhone {
                    number: "2".into(),
                    kind: ipc::PhoneKind::Other(0),
                    label: Some("Lab".into()),
                },
                ipc::ContactPhone {
                    number: "3".into(),
                    kind: ipc::PhoneKind::Other(19),
                    label: None,
                },
            ],
            emails: vec![],
            organization: None,
            photo_key: Some("contact-photo/1/2".into()),
        });
        let labels: Vec<&str> = converted
            .numbers
            .iter()
            .map(|number| number.label.as_str())
            .collect();
        assert_eq!(labels, ["Mobile", "Lab", "Other"]);
        assert_eq!(converted.photo_key.as_deref(), Some("contact-photo/1/2"));
    }

    #[test]
    fn devices_report_what_they_share() {
        let info = ipc::DeviceInfo {
            id: ipc::DeviceId([7; 32]),
            name: "Pixel".into(),
            class: ipc::DeviceClass::Phone,
            trusted: true,
            connected: true,
            link: ipc::LinkKind::Lan,
            battery: Some(ipc::Battery {
                percent: 64,
                charging: false,
            }),
            features: ipc::FeatureSet::EMPTY
                .with(ipc::Feature::Messages)
                .with(ipc::Feature::Notifications),
            active: ipc::FeatureSet::EMPTY,
        };
        let converted = device(&info);
        assert_eq!(converted.battery, Some(64));
        assert_eq!(
            converted.sharing,
            Sharing {
                messages: true,
                contacts: false,
                notifications: true
            }
        );
        assert_eq!(device_id(converted.id), info.id);
    }

    #[test]
    fn deletions_carry_their_ids() {
        let deleted = change(ipc::MessageChange::Deleted {
            thread_id: 3,
            message_id: "mms/1".into(),
        });
        assert_eq!(
            deleted,
            MessageChange::Deleted {
                thread: ThreadId(3),
                id: MessageId("mms/1".into())
            }
        );
    }
}
