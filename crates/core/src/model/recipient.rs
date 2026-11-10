use crate::util::phone_number::matching_digits;

/// Someone a conversation is with.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Recipient {
    pub address: String,
    /// Known only when the phone also shares its contacts.
    pub name: Option<String>,
    pub contact_key: Option<String>,
}

impl Recipient {
    #[must_use]
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            name: None,
            contact_key: None,
        }
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        self.name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&self.address)
    }

    /// Up to two capitals from the name's first words; `None` for an unnamed number.
    #[must_use]
    pub fn initials(&self) -> Option<String> {
        let initials: String = self
            .name
            .as_deref()?
            .split_whitespace()
            .filter_map(|word| word.chars().find(|letter| letter.is_alphabetic()))
            .take(2)
            .flat_map(char::to_uppercase)
            .collect();
        (!initials.is_empty()).then_some(initials)
    }

    /// A stable pick among `choices` avatar tints, the same for every spelling of one number.
    #[must_use]
    pub fn tint_index(&self, choices: usize) -> usize {
        let digits = matching_digits(&self.address);
        let source = if digits.is_empty() {
            self.address.as_bytes()
        } else {
            digits.as_bytes()
        };
        let hash = source.iter().fold(0x811c_9dc5_u32, |hash, byte| {
            (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
        });
        usize::try_from(hash).unwrap_or_default() % choices.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> Recipient {
        Recipient {
            name: Some(name.to_owned()),
            ..Recipient::new("+15550100")
        }
    }

    #[test]
    fn display_name_falls_back_to_the_address() {
        assert_eq!(named("Ada Lovelace").display_name(), "Ada Lovelace");
        assert_eq!(named("  ").display_name(), "+15550100");
        assert_eq!(Recipient::new("+15550100").display_name(), "+15550100");
    }

    #[test]
    fn initials_come_from_the_first_two_words() {
        assert_eq!(
            named("ada lovelace byron").initials().as_deref(),
            Some("AL")
        );
        assert_eq!(named("(Mum)").initials().as_deref(), Some("M"));
        assert_eq!(Recipient::new("+15550100").initials(), None);
    }

    #[test]
    fn tint_ignores_how_the_number_is_written() {
        let plain = Recipient::new("+1 555 0100");
        let dashed = Recipient::new("+1-555-0100");
        assert_eq!(plain.tint_index(8), dashed.tint_index(8));
        assert!(plain.tint_index(8) < 8);
    }
}
