use crownos_messages_core::model::{Collections, Device, DeviceId, MessageChange};

/// What crownlink tells Messages without being asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkEvent {
    /// The paired devices, after connecting and after any change.
    Devices(Vec<Device>),
    /// crownlink went away; the session keeps retrying and sends `Devices` once back.
    Unavailable,
    MessageChanged {
        device: DeviceId,
        change: MessageChange,
    },
    CollectionsChanged {
        device: DeviceId,
        collections: Collections,
    },
}
