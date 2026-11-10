//! Picking files to attach through the desktop's file chooser portal.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

use futures_util::StreamExt;
use thiserror::Error;
use zbus::{
    Connection, proxy,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

use crate::util::file_uri::file_uri_to_path;

const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const RESPONSE_SUCCESS: u32 = 0;
const RESPONSE_CANCELLED: u32 = 1;

static NEXT_TOKEN: AtomicU32 = AtomicU32::new(1);

#[derive(Debug, Error)]
pub enum PortalError {
    #[error("The file chooser isn't available: {0}")]
    Unavailable(#[from] zbus::Error),
    #[error("The file chooser failed")]
    Failed,
}

#[proxy(
    interface = "org.freedesktop.portal.FileChooser",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop",
    gen_blocking = false
)]
trait FileChooser {
    fn open_file(
        &self,
        parent_window: &str,
        title: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(
    interface = "org.freedesktop.portal.Request",
    default_service = "org.freedesktop.portal.Desktop",
    gen_blocking = false
)]
trait Request {
    #[zbus(signal)]
    fn response(&self, response: u32, results: HashMap<String, OwnedValue>) -> zbus::Result<()>;
}

/// Asks the user for files to attach; an empty list when they cancel.
///
/// # Errors
///
/// When no file chooser portal answers, or it reports a failure.
pub async fn pick_attachments(title: &str) -> Result<Vec<PathBuf>, PortalError> {
    let connection = Connection::session().await?;
    let token = format!(
        "crownos_messages_{}_{}",
        std::process::id(),
        NEXT_TOKEN.fetch_add(1, Ordering::Relaxed)
    );
    let sender = connection
        .unique_name()
        .map(|name| name.trim_start_matches(':').replace('.', "_"))
        .unwrap_or_default();
    let expected = format!("{PORTAL_PATH}/request/{sender}/{token}");
    let request = RequestProxy::builder(&connection)
        .path(expected.clone())?
        .build()
        .await?;
    let mut responses = request.receive_response().await?;

    let chooser = FileChooserProxy::new(&connection).await?;
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("multiple", Value::from(true));
    options.insert("filters", Value::from(filters()));
    let handle = chooser.open_file("", title, options).await?;

    let response = if handle.as_str() == expected {
        responses.next().await
    } else {
        let moved = RequestProxy::builder(&connection)
            .path(handle)?
            .build()
            .await?;
        moved.receive_response().await?.next().await
    };
    let response = response.ok_or(PortalError::Failed)?;
    let args = response.args()?;
    match *args.response() {
        RESPONSE_SUCCESS => Ok(chosen_paths(args.results())),
        RESPONSE_CANCELLED => Ok(Vec::new()),
        _ => Err(PortalError::Failed),
    }
}

fn filters() -> Vec<(&'static str, Vec<(u32, &'static str)>)> {
    vec![
        ("Photos and videos", vec![(1, "image/*"), (1, "video/*")]),
        ("All files", vec![(0, "*")]),
    ]
}

fn chosen_paths(results: &HashMap<String, OwnedValue>) -> Vec<PathBuf> {
    results
        .get("uris")
        .and_then(|uris| Vec::<String>::try_from(uris.clone()).ok())
        .unwrap_or_default()
        .iter()
        .filter_map(|uri| file_uri_to_path(uri))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chosen_uris_become_paths() {
        let uris =
            OwnedValue::try_from(Value::from(vec!["file:///home/a/b%20c.png", "https://x/y"]))
                .unwrap();
        let results = HashMap::from([("uris".to_owned(), uris)]);
        assert_eq!(chosen_paths(&results), [PathBuf::from("/home/a/b c.png")]);
        assert!(chosen_paths(&HashMap::new()).is_empty());
    }
}
