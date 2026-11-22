use std::{path::PathBuf, sync::Arc};

use crownos_messages_core::model::DeviceId;
use crownos_messages_link::{BlobSource, Link, LinkError, portal::pick_attachments};
use tokio::{
    process::Command as Process,
    sync::{
        Semaphore,
        mpsc::{UnboundedReceiver, UnboundedSender},
    },
    task::spawn_blocking,
};

use super::{Command, Inbound};
use crate::util::picture::decode_picture;

/// Pictures fetched and decoded at once; the rest wait their turn.
const PARALLEL_PICTURES: usize = 4;
const CHOOSER_TITLE: &str = "Attach files";

/// Serves `commands` until the window closes, each concurrently, and forwards crownlink's
/// events as they come.
pub async fn run_link(inbound: UnboundedSender<Inbound>, mut commands: UnboundedReceiver<Command>) {
    let (link, session, mut events) = Link::new();
    tokio::spawn(session.run());
    let pictures = Arc::new(Semaphore::new(PARALLEL_PICTURES));
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(command) => {
                    tokio::spawn(perform(link.clone(), command, inbound.clone(), pictures.clone()));
                }
                None => return,
            },
            Some(event) = events.recv() => {
                if inbound.send(Inbound::Link(event)).is_err() {
                    return;
                }
            }
        }
    }
}

async fn perform(link: Link, command: Command, inbound: UnboundedSender<Inbound>, pictures: Arc<Semaphore>) {
    let answer = match command {
        Command::Collections { device } => Inbound::Collections {
            device,
            result: link.collections(device).await.map_err(describe),
        },
        Command::Conversations { device, cursor } => Inbound::Conversations {
            device,
            newest: cursor.is_none(),
            result: link.conversations(device, cursor).await.map_err(describe),
        },
        Command::Messages { device, thread, cursor } => Inbound::Messages {
            device,
            thread,
            newest: cursor.is_none(),
            result: link.messages(device, thread, cursor).await.map_err(describe),
        },
        Command::Send {
            device,
            placeholder,
            message,
        } => Inbound::Sent {
            device,
            placeholder,
            result: link.send(device, message).await.map_err(describe),
        },
        Command::MarkRead { device, thread } => Inbound::MarkedRead {
            device,
            thread,
            result: link.mark_read(device, thread).await.map_err(describe),
        },
        Command::Contacts { device } => Inbound::Contacts {
            device,
            result: all_contacts(&link, device).await.map_err(describe),
        },
        Command::Thumbnail { device, source, key } => {
            let result = picture(&link, device, source, key.clone(), &pictures).await;
            Inbound::Thumbnail { source, key, result }
        }
        Command::FullText {
            device,
            thread,
            message,
            key,
        } => Inbound::FullText {
            device,
            thread,
            message,
            result: full_text(&link, device, key).await,
        },
        Command::Open { device, key } => Inbound::Opened(open(&link, device, key).await),
        Command::PickAttachments => Inbound::Picked(pick_attachments(CHOOSER_TITLE).await.map_err(|error| error.to_string())),
    };
    inbound.send(answer).ok();
}

fn describe(error: LinkError) -> String {
    error.to_string()
}

async fn all_contacts(
    link: &Link,
    device: DeviceId,
) -> Result<Vec<crownos_messages_core::model::Contact>, LinkError> {
    let mut contacts = Vec::new();
    let mut cursor = None;
    loop {
        let page = link.contacts(device, cursor).await?;
        contacts.extend(page.items);
        match page.next {
            Some(next) => cursor = Some(next),
            None => return Ok(contacts),
        }
    }
}

async fn picture(
    link: &Link,
    device: DeviceId,
    source: BlobSource,
    key: String,
    pictures: &Semaphore,
) -> Result<crate::util::picture::Picture, String> {
    let _turn = pictures.acquire().await.map_err(|error| error.to_string())?;
    let blob = link.fetch_blob(device, source, key).await.map_err(describe)?;
    spawn_blocking(move || decode_picture(&blob.path))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

async fn full_text(link: &Link, device: DeviceId, key: String) -> Result<String, String> {
    let blob = link.fetch_blob(device, BlobSource::Messages, key).await.map_err(describe)?;
    tokio::fs::read_to_string(&blob.path)
        .await
        .map_err(|error| error.to_string())
}

async fn open(link: &Link, device: DeviceId, key: String) -> Result<(), String> {
    let blob = link.fetch_blob(device, BlobSource::Messages, key).await.map_err(describe)?;
    launch(blob.path).await
}

async fn launch(path: PathBuf) -> Result<(), String> {
    let status = Process::new("xdg-open")
        .arg(&path)
        .status()
        .await
        .map_err(|error| format!("Couldn't open the attachment: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("No app opens this kind of attachment".to_owned())
    }
}
