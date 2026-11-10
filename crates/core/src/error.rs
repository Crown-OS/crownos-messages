use thiserror::Error;

/// Why a link could not be read as an `sms:` link.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UriError {
    #[error("not an sms:, smsto:, mms: or mmsto: link")]
    NotSms,
    #[error("the link is longer than {max} bytes")]
    TooLong { max: usize },
    #[error("the link names more than {max} recipients")]
    TooManyRecipients { max: usize },
    #[error("the link's message is longer than {max} bytes")]
    BodyTooLong { max: usize },
    #[error("the link is not valid UTF-8 once decoded")]
    NotUtf8,
}

/// Why a draft cannot be sent as it is.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DraftError {
    #[error("Write a message first")]
    Empty,
    #[error("Add someone to send to")]
    NoRecipients,
    #[error("A message can go to at most {max} people")]
    TooManyRecipients { max: usize },
    #[error("This message is {} KB over the 32 KB limit", .excess.div_ceil(1024))]
    TextTooLong { excess: usize },
    #[error("A message can carry at most {max} attachments")]
    TooManyAttachments { max: usize },
    #[error("Attachments add up to more than the {} KB the carrier allows", .max / 1024)]
    AttachmentsTooLarge { max: u64 },
    #[error("The phone can't send attachments")]
    AttachmentsUnsupported,
    #[error("The phone can't send messages")]
    SendingUnsupported,
}
