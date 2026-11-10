/// A paired device's identity: its long-term public key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId(pub [u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceKind {
    Phone,
    Tablet,
    Watch,
    Computer,
}

/// What the user lets a device share with this computer, as far as Messages cares.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Sharing {
    pub messages: bool,
    pub contacts: bool,
    pub notifications: bool,
}

/// A paired device Messages can talk through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    pub name: String,
    pub kind: DeviceKind,
    pub connected: bool,
    pub battery: Option<u8>,
    pub sharing: Sharing,
}

impl Device {
    /// Reachable right now and sharing its messages.
    #[must_use]
    pub const fn can_message(&self) -> bool {
        self.connected && self.sharing.messages
    }

    /// The device Messages opens on: the first that can message, else the first that shares
    /// messages at all, else the first phone.
    #[must_use]
    pub fn preferred(devices: &[Self]) -> Option<DeviceId> {
        devices
            .iter()
            .find(|device| device.can_message())
            .or_else(|| devices.iter().find(|device| device.sharing.messages))
            .or_else(|| {
                devices
                    .iter()
                    .find(|device| device.kind == DeviceKind::Phone)
            })
            .or_else(|| devices.first())
            .map(|device| device.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(byte: u8, connected: bool, messages: bool) -> Device {
        Device {
            id: DeviceId([byte; 32]),
            name: format!("Phone {byte}"),
            kind: DeviceKind::Phone,
            connected,
            battery: None,
            sharing: Sharing {
                messages,
                ..Sharing::default()
            },
        }
    }

    #[test]
    fn prefers_a_connected_device_that_shares_messages() {
        let devices = [
            device(1, true, false),
            device(2, false, true),
            device(3, true, true),
        ];
        assert_eq!(Device::preferred(&devices), Some(DeviceId([3; 32])));
    }

    #[test]
    fn falls_back_to_a_sharing_device_then_any_phone() {
        assert_eq!(
            Device::preferred(&[device(1, true, false), device(2, false, true)]),
            Some(DeviceId([2; 32]))
        );
        assert_eq!(
            Device::preferred(&[device(1, false, false)]),
            Some(DeviceId([1; 32]))
        );
        assert_eq!(Device::preferred(&[]), None);
    }
}
