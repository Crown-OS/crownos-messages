//! Desktop notifications for incoming messages, for phones that do not mirror their own
//! notifications here; activating one opens its conversation.

use std::collections::HashMap;

use crownos_messages_core::model::{DeviceId, ThreadId};
use futures_util::StreamExt;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use zbus::{Connection, proxy, zvariant::Value};

const APP_NAME: &str = "Messages";
const DESKTOP_ENTRY: &str = "org.crownos.messages";
const DEFAULT_ACTION: &str = "default";
const NEVER_EXPIRE: i32 = 0;
const MAX_BODY_CHARS: usize = 240;

#[proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications",
    gen_blocking = false
)]
trait Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: HashMap<&str, Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;

    fn close_notification(&self, id: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn action_invoked(&self, id: u32, action_key: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn notification_closed(&self, id: u32, reason: u32) -> zbus::Result<()>;
}

/// What the window wants the desktop to show or take back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// A message arrived in a conversation; replaces that conversation's last notification.
    Arrived {
        device: DeviceId,
        thread: ThreadId,
        title: String,
        body: String,
    },
    /// The user opened the conversation here; its notification is no longer news.
    Seen { device: DeviceId, thread: ThreadId },
}

type ConversationKey = (DeviceId, ThreadId);

/// Shows `notices` until the window closes, and reports each conversation the user opens
/// from a notification on `opened`.
///
/// # Errors
///
/// When no notification server is on the session bus or it stops answering.
pub async fn serve_notifications(
    mut notices: UnboundedReceiver<Notice>,
    opened: UnboundedSender<ConversationKey>,
) -> zbus::Result<()> {
    let connection = Connection::session().await?;
    let server = NotificationsProxy::new(&connection).await?;
    let mut activations = server.receive_action_invoked().await?;
    let mut closings = server.receive_notification_closed().await?;
    let mut shown: HashMap<ConversationKey, u32> = HashMap::new();
    loop {
        tokio::select! {
            notice = notices.recv() => match notice {
                Some(Notice::Arrived { device, thread, title, body }) => {
                    let replaces = shown.get(&(device, thread)).copied().unwrap_or_default();
                    let id = server
                        .notify(
                            APP_NAME,
                            replaces,
                            DESKTOP_ENTRY,
                            &title,
                            &shorten(&body),
                            &[DEFAULT_ACTION, "Open"],
                            hints(),
                            NEVER_EXPIRE,
                        )
                        .await?;
                    shown.insert((device, thread), id);
                }
                Some(Notice::Seen { device, thread }) => {
                    if let Some(id) = shown.remove(&(device, thread)) {
                        server.close_notification(id).await.ok();
                    }
                }
                None => return Ok(()),
            },
            Some(activation) = activations.next() => {
                let args = activation.args()?;
                if args.action_key == DEFAULT_ACTION
                    && let Some(key) = conversation_of(&shown, *args.id())
                {
                    opened.send(key).ok();
                }
            }
            Some(closing) = closings.next() => {
                let id = *closing.args()?.id();
                shown.retain(|_, shown_id| *shown_id != id);
            }
        }
    }
}

fn conversation_of(shown: &HashMap<ConversationKey, u32>, id: u32) -> Option<ConversationKey> {
    shown
        .iter()
        .find_map(|(key, shown_id)| (*shown_id == id).then_some(*key))
}

fn hints() -> HashMap<&'static str, Value<'static>> {
    HashMap::from([
        ("desktop-entry", Value::from(DESKTOP_ENTRY)),
        ("category", Value::from("im.received")),
    ])
}

fn shorten(body: &str) -> String {
    match body.char_indices().nth(MAX_BODY_CHARS) {
        Some((cut, _)) => format!("{}…", body[..cut].trim_end()),
        None => body.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_bodies_are_cut_on_a_character() {
        assert_eq!(shorten("short"), "short");
        let long = "é".repeat(MAX_BODY_CHARS + 5);
        let cut = shorten(&long);
        assert_eq!(cut.chars().count(), MAX_BODY_CHARS + 1);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn activations_find_their_conversation() {
        let key = (DeviceId([1; 32]), ThreadId(4));
        let shown = HashMap::from([(key, 9)]);
        assert_eq!(conversation_of(&shown, 9), Some(key));
        assert_eq!(conversation_of(&shown, 8), None);
    }
}
