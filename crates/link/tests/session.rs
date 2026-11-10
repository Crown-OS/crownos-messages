//! The session against a stand-in crownlink bound in a temporary directory: devices follow
//! crownlink's events, requests come back in Messages' types, a slow request does not hold
//! up a quick one, and the session survives crownlink going away.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{path::Path, sync::Mutex, time::Duration};

use crownlink_linux::ipc::proto as ipc;
use crownos_ipc::{
    MethodMsg, PeerId, RemoteError, Server, ServiceBuilder, adapter::tokio::serve_with_outbox,
    postcard,
};
use crownos_messages_core::model::{DeviceId, MessageChange, MessageId, ThreadId};
use crownos_messages_link::{BlobSource, Link, LinkError, LinkEvent, OutgoingMessage};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver},
    task::JoinHandle,
    time::timeout,
};

const PATIENCE: Duration = Duration::from_secs(10);
const PHONE: ipc::DeviceId = ipc::DeviceId([1; 32]);
const STRANGER: ipc::DeviceId = ipc::DeviceId([2; 32]);

enum Push {
    Connected(bool),
    Message,
    ReleaseBlob,
}

fn info(id: ipc::DeviceId, name: &str, trusted: bool) -> ipc::DeviceInfo {
    ipc::DeviceInfo {
        id,
        name: name.to_owned(),
        class: ipc::DeviceClass::Phone,
        trusted,
        connected: true,
        link: ipc::LinkKind::Lan,
        battery: None,
        features: ipc::FeatureSet::EMPTY.with(ipc::Feature::Messages),
        active: ipc::FeatureSet::EMPTY,
    }
}

fn item(id: &str) -> ipc::MessageItem {
    ipc::MessageItem {
        thread_id: 7,
        message_id: id.to_owned(),
        folder: ipc::MessageFolder::Inbox,
        sender: Some("+15550100".into()),
        unix_ms: 1_000,
        text: Some("hello".into()),
        text_truncated: false,
        read: false,
        status: ipc::DeliveryStatus::None,
        attachments: Vec::new(),
        attachments_omitted: 0,
        sim_slot: None,
    }
}

fn reply<T: serde::Serialize>(
    server: &mut Server,
    peer: PeerId,
    request: u32,
    value: &T,
) -> Result<(), crownos_ipc::Error> {
    server.reply(peer, request, &postcard::to_stdvec(value)?, Vec::new())
}

fn is<M: MethodMsg>(selector: u64) -> bool {
    selector == M::SELECTOR
}

/// Stands in for crownlink: answers pages at once, holds `fetch_blob` until told, records
/// drafts and answers `mark_read` as a crownlink without it would.
fn fake_crownlink(
    dir: &Path,
    drafts: mpsc::UnboundedSender<ipc::MessageDraft>,
) -> (JoinHandle<()>, mpsc::UnboundedSender<Push>) {
    let mut server = ServiceBuilder::new(ipc::SERVICE)
        .directory(dir)
        .build()
        .unwrap();
    let (pushes, mut outbox) = mpsc::unbounded_channel::<Push>();
    let task = tokio::spawn(async move {
        let devices = Mutex::new(vec![
            info(PHONE, "Pixel", true),
            info(STRANGER, "Nearby", false),
        ]);
        let held_blob: Mutex<Option<(PeerId, u32)>> = Mutex::new(None);
        serve_with_outbox(
            &mut server,
            |server, peer, message| {
                let selector = message.selector;
                let request = message.req_id;
                if is::<ipc::devices>(selector) {
                    reply(server, peer, request, &*devices.lock().unwrap())?;
                } else if is::<ipc::conversations>(selector) {
                    let args: ipc::conversations = postcard::from_bytes(&message.payload)?;
                    assert_eq!(args.page.cursor.as_deref(), Some("after-1"));
                    let page = ipc::ConversationPage {
                        items: vec![ipc::Conversation {
                            thread_id: 7,
                            participants: vec![ipc::Participant {
                                address: "+15550100".into(),
                                name: Some("Ada".into()),
                                contact_key: None,
                            }],
                            participant_count: 1,
                            snippet: "hello".into(),
                            last_unix_ms: 1_000,
                            unread: 1,
                            has_attachments: false,
                        }],
                        next: None,
                    };
                    reply(server, peer, request, &page)?;
                } else if is::<ipc::messages>(selector) {
                    let page = ipc::MessagePage {
                        thread_id: 7,
                        items: vec![item("sms/2"), item("sms/1")],
                        next: Some("older".into()),
                    };
                    reply(server, peer, request, &page)?;
                } else if is::<ipc::send_message>(selector) {
                    let args: ipc::send_message = postcard::from_bytes(&message.payload)?;
                    drafts.send(args.draft).unwrap();
                    reply(server, peer, request, &"sms/42".to_owned())?;
                } else if is::<ipc::fetch_blob>(selector) {
                    *held_blob.lock().unwrap() = Some((peer, request));
                } else if is::<ipc::collections>(selector) {
                    reply(server, peer, request, &ipc::Collections::default())?;
                } else {
                    server.reply_err(peer, request, &RemoteError::unknown_method(selector))?;
                }
                Ok(true)
            },
            |cx| outbox.poll_recv(cx),
            |server, push| match push {
                Push::Connected(connected) => {
                    devices.lock().unwrap()[0].connected = connected;
                    server
                        .emit(&ipc::DeviceConnected {
                            id: PHONE,
                            name: "Pixel".into(),
                            connected,
                        })
                        .unwrap();
                }
                Push::Message => server
                    .emit(&ipc::MessageChanged {
                        id: PHONE,
                        change: ipc::MessageChange::Added(item("sms/3")),
                    })
                    .unwrap(),
                Push::ReleaseBlob => {
                    let (peer, request) = held_blob.lock().unwrap().take().unwrap();
                    let blob = ipc::BlobFile {
                        path: "/cache/blob.jpg".into(),
                        mime: "image/jpeg".into(),
                        name: None,
                        len: 9,
                    };
                    reply(server, peer, request, &blob).unwrap();
                }
            },
        )
        .await
        .unwrap();
    });
    (task, pushes)
}

async fn next<T>(receiver: &mut UnboundedReceiver<T>) -> T {
    timeout(PATIENCE, receiver.recv()).await.unwrap().unwrap()
}

async fn next_devices(
    events: &mut UnboundedReceiver<LinkEvent>,
) -> Vec<crownos_messages_core::model::Device> {
    match next(events).await {
        LinkEvent::Devices(devices) => devices,
        other => panic!("expected devices, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pages_send_and_follow_crownlink() {
    let dir = tempfile::tempdir().unwrap();
    let (drafts, mut sent) = mpsc::unbounded_channel();
    let (server, pushes) = fake_crownlink(dir.path(), drafts.clone());
    let (link, session, mut events) = Link::in_directory(dir.path());
    tokio::spawn(session.run());
    let phone = DeviceId(PHONE.0);

    let devices = next_devices(&mut events).await;
    assert_eq!(devices.len(), 1);
    assert!(devices[0].can_message());

    let page = link
        .conversations(phone, Some("after-1".into()))
        .await
        .unwrap();
    assert_eq!(page.items[0].title(), "Ada");
    let page = link.messages(phone, ThreadId(7), None).await.unwrap();
    let ids: Vec<&str> = page
        .items
        .iter()
        .map(|message| message.id.0.as_str())
        .collect();
    assert_eq!(
        (ids, page.next.as_deref()),
        (vec!["sms/2", "sms/1"], Some("older"))
    );

    let blob = tokio::spawn({
        let link = link.clone();
        async move {
            link.fetch_blob(phone, BlobSource::Messages, "mms-part/1/2".into())
                .await
        }
    });
    let message = OutgoingMessage {
        thread: Some(ThreadId(7)),
        recipients: vec!["+15550100".into()],
        text: "on my way".into(),
        ..OutgoingMessage::default()
    };
    assert_eq!(
        link.send(phone, message).await.unwrap(),
        MessageId("sms/42".into())
    );
    assert_eq!(next(&mut sent).await.text, "on my way");
    assert!(!blob.is_finished());
    pushes.send(Push::ReleaseBlob).unwrap();
    assert_eq!(
        blob.await.unwrap().unwrap().path.to_str(),
        Some("/cache/blob.jpg")
    );

    assert!(matches!(
        link.mark_read(phone, ThreadId(7)).await,
        Err(LinkError::Outdated)
    ));
    assert_eq!(link.collections(phone).await.unwrap(), Default::default());

    pushes.send(Push::Message).unwrap();
    match next(&mut events).await {
        LinkEvent::MessageChanged {
            device,
            change: MessageChange::Added(message),
        } => {
            assert_eq!((device, message.id.0.as_str()), (phone, "sms/3"));
        }
        other => panic!("expected a message, got {other:?}"),
    }
    pushes.send(Push::Connected(false)).unwrap();
    assert!(!next_devices(&mut events).await[0].connected);

    server.abort();
    let _ = server.await;
    assert_eq!(next(&mut events).await, LinkEvent::Unavailable);
    assert!(matches!(
        link.conversations(phone, None).await,
        Err(LinkError::Unavailable)
    ));

    let (_server, _pushes) = fake_crownlink(dir.path(), drafts);
    assert_eq!(next_devices(&mut events).await.len(), 1);
}
