use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
};

use crate::{
    error::DraftError,
    model::{AttachmentKind, MessagingStatus, ThreadId},
};

/// The longest text one message may carry, in bytes.
pub const MAX_TEXT_BYTES: usize = 32 * 1024;
pub const MAX_RECIPIENTS: usize = 32;
pub const MAX_ATTACHMENTS: usize = 16;
/// The attachment budget when the carrier's MMS limit is unknown.
pub const DEFAULT_MMS_BYTES: u64 = 1024 * 1024;

/// Which composer a draft belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DraftTarget {
    Thread(ThreadId),
    NewMessage,
}

/// A local file waiting to go out with a message.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DraftAttachment {
    pub path: PathBuf,
    pub name: String,
    pub len: u64,
    pub kind: AttachmentKind,
}

impl DraftAttachment {
    /// Reads the size of the file at `path`.
    ///
    /// # Errors
    ///
    /// When `path` cannot be read or is not a regular file.
    pub fn from_path(path: PathBuf) -> io::Result<Self> {
        let metadata = std::fs::metadata(&path)?;
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a regular file",
            ));
        }
        Ok(Self {
            name: file_name(&path),
            kind: AttachmentKind::of_path(&path),
            len: metadata.len(),
            path,
        })
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// What the phone lets this computer send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendRules {
    pub send_sms: bool,
    pub send_mms: bool,
    pub max_attachment_bytes: u64,
}

impl SendRules {
    #[must_use]
    pub fn from_status(status: &MessagingStatus) -> Self {
        Self {
            send_sms: status.send_sms,
            send_mms: status.send_mms,
            max_attachment_bytes: status
                .mms_max_bytes
                .filter(|max| *max > 0)
                .map_or(DEFAULT_MMS_BYTES, u64::from),
        }
    }
}

/// What the user typed and attached in one composer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Draft {
    pub text: String,
    pub attachments: Vec<DraftAttachment>,
}

impl Draft {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.attachments.is_empty()
    }

    #[must_use]
    pub fn attachment_bytes(&self) -> u64 {
        self.attachments
            .iter()
            .map(|attachment| attachment.len)
            .sum()
    }

    /// Whether this draft may go to `recipients` people under `rules`.
    ///
    /// # Errors
    ///
    /// The first rule the draft breaks.
    pub fn check(&self, recipients: usize, rules: &SendRules) -> Result<(), DraftError> {
        if self.is_empty() {
            return Err(DraftError::Empty);
        }
        if recipients == 0 {
            return Err(DraftError::NoRecipients);
        }
        if recipients > MAX_RECIPIENTS {
            return Err(DraftError::TooManyRecipients {
                max: MAX_RECIPIENTS,
            });
        }
        if self.text.len() > MAX_TEXT_BYTES {
            return Err(DraftError::TextTooLong {
                excess: self.text.len() - MAX_TEXT_BYTES,
            });
        }
        if self.attachments.is_empty() {
            return if rules.send_sms || rules.send_mms {
                Ok(())
            } else {
                Err(DraftError::SendingUnsupported)
            };
        }
        if !rules.send_mms {
            return Err(DraftError::AttachmentsUnsupported);
        }
        if self.attachments.len() > MAX_ATTACHMENTS {
            return Err(DraftError::TooManyAttachments {
                max: MAX_ATTACHMENTS,
            });
        }
        if self.attachment_bytes() > rules.max_attachment_bytes {
            return Err(DraftError::AttachmentsTooLarge {
                max: rules.max_attachment_bytes,
            });
        }
        Ok(())
    }
}

/// The drafts of the composers not on screen, so switching conversations keeps them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drafts(HashMap<DraftTarget, Draft>);

impl Drafts {
    /// Takes the draft of `target` out, or an empty one.
    pub fn take(&mut self, target: DraftTarget) -> Draft {
        self.0.remove(&target).unwrap_or_default()
    }

    /// Keeps `draft` for `target` until it is taken again; empty drafts are dropped.
    pub fn keep(&mut self, target: DraftTarget, draft: Draft) {
        if draft.is_empty() {
            self.0.remove(&target);
        } else {
            self.0.insert(target, draft);
        }
    }

    #[must_use]
    pub fn has(&self, target: DraftTarget) -> bool {
        self.0.contains_key(&target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULES: SendRules = SendRules {
        send_sms: true,
        send_mms: true,
        max_attachment_bytes: 1000,
    };

    fn attachment(len: u64) -> DraftAttachment {
        DraftAttachment {
            path: PathBuf::from("/tmp/photo.jpg"),
            name: "photo.jpg".into(),
            len,
            kind: AttachmentKind::Image,
        }
    }

    fn text(text: &str) -> Draft {
        Draft {
            text: text.to_owned(),
            attachments: Vec::new(),
        }
    }

    #[test]
    fn a_plain_message_needs_text_and_someone() {
        assert_eq!(text(" ").check(1, &RULES), Err(DraftError::Empty));
        assert_eq!(text("hi").check(0, &RULES), Err(DraftError::NoRecipients));
        assert_eq!(
            text("hi").check(33, &RULES),
            Err(DraftError::TooManyRecipients { max: 32 })
        );
        assert_eq!(text("hi").check(32, &RULES), Ok(()));
    }

    #[test]
    fn text_over_32_kib_is_refused() {
        let long = text(&"a".repeat(MAX_TEXT_BYTES + 1));
        assert_eq!(
            long.check(1, &RULES),
            Err(DraftError::TextTooLong { excess: 1 })
        );
        assert_eq!(text(&"a".repeat(MAX_TEXT_BYTES)).check(1, &RULES), Ok(()));
    }

    #[test]
    fn attachments_follow_the_phone_s_rules() {
        let draft = Draft {
            text: String::new(),
            attachments: vec![attachment(600), attachment(600)],
        };
        assert_eq!(
            draft.check(1, &RULES),
            Err(DraftError::AttachmentsTooLarge { max: 1000 })
        );
        let no_mms = SendRules {
            send_mms: false,
            ..RULES
        };
        assert_eq!(
            draft.check(1, &no_mms),
            Err(DraftError::AttachmentsUnsupported)
        );
        let many = Draft {
            text: String::new(),
            attachments: vec![attachment(1); 17],
        };
        assert_eq!(
            many.check(1, &RULES),
            Err(DraftError::TooManyAttachments { max: 16 })
        );
    }

    #[test]
    fn a_phone_that_cannot_send_refuses_everything() {
        let silent = SendRules {
            send_sms: false,
            send_mms: false,
            ..RULES
        };
        assert_eq!(
            text("hi").check(1, &silent),
            Err(DraftError::SendingUnsupported)
        );
    }

    #[test]
    fn rules_default_the_mms_budget() {
        let status = MessagingStatus {
            send_mms: true,
            mms_max_bytes: None,
            ..MessagingStatus::default()
        };
        assert_eq!(
            SendRules::from_status(&status).max_attachment_bytes,
            DEFAULT_MMS_BYTES
        );
        let status = MessagingStatus {
            mms_max_bytes: Some(300_000),
            ..status
        };
        assert_eq!(
            SendRules::from_status(&status).max_attachment_bytes,
            300_000
        );
    }

    #[test]
    fn drafts_are_kept_per_composer() {
        let mut drafts = Drafts::default();
        drafts.keep(DraftTarget::Thread(ThreadId(1)), text("later"));
        drafts.keep(DraftTarget::NewMessage, text("  "));
        assert!(drafts.has(DraftTarget::Thread(ThreadId(1))));
        assert!(!drafts.has(DraftTarget::NewMessage));
        assert_eq!(drafts.take(DraftTarget::Thread(ThreadId(1))).text, "later");
        assert!(drafts.take(DraftTarget::Thread(ThreadId(1))).is_empty());
    }

    #[test]
    fn local_files_are_measured() {
        let dir =
            std::env::temp_dir().join(format!("crownos-messages-draft-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clip.MP4");
        std::fs::write(&path, b"12345").unwrap();
        let attachment = DraftAttachment::from_path(path).unwrap();
        assert_eq!(
            (attachment.name.as_str(), attachment.len, attachment.kind),
            ("clip.MP4", 5, AttachmentKind::Video)
        );
        assert!(DraftAttachment::from_path(dir.clone()).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
