use std::time::{SystemTime, UNIX_EPOCH};

/// A moment as milliseconds since the Unix epoch, the unit the phone reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub u64);

impl Timestamp {
    #[must_use]
    pub fn now() -> Self {
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis());
        Self(u64::try_from(elapsed).unwrap_or(u64::MAX))
    }

    #[must_use]
    pub const fn unix_ms(self) -> u64 {
        self.0
    }
}
