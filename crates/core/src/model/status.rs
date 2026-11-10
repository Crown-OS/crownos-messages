/// What a phone last said about its messages; `None` wherever it is used means the phone has
/// not said, or does not share its messages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct MessagingStatus {
    /// Changes whenever any conversation does.
    pub revision: u64,
    pub unread: u32,
    pub send_sms: bool,
    pub send_mms: bool,
    pub mark_read: bool,
    /// The carrier's MMS size limit, when the phone knows it.
    pub mms_max_bytes: Option<u32>,
}

/// What a phone last said about its contacts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ContactsStatus {
    pub revision: u64,
    pub count: u32,
    pub photos: bool,
}
