use crate::util::phone_number::{matching_digits, same_address};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContactNumber {
    pub number: String,
    /// "Mobile", "Work" or the user's own label.
    pub label: String,
}

/// A person from the phone's address book, as recipient suggestions need them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Contact {
    pub key: String,
    pub name: String,
    pub starred: bool,
    pub numbers: Vec<ContactNumber>,
    pub photo_key: Option<String>,
}

impl Contact {
    /// Whether `query` is part of the name or of one of the numbers.
    #[must_use]
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim();
        if query.is_empty() {
            return false;
        }
        let digits = matching_digits(query);
        self.name.to_lowercase().contains(&query.to_lowercase())
            || (!digits.is_empty()
                && self
                    .numbers
                    .iter()
                    .any(|number| matching_digits(&number.number).contains(digits.as_str())))
    }

    /// Whether `address` is one of this contact's numbers.
    #[must_use]
    pub fn owns(&self, address: &str) -> bool {
        self.numbers
            .iter()
            .any(|number| same_address(&number.number, address))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ada() -> Contact {
        Contact {
            key: "ada".into(),
            name: "Ada Lovelace".into(),
            starred: false,
            numbers: vec![ContactNumber {
                number: "+44 20 7946 0018".into(),
                label: "Mobile".into(),
            }],
            photo_key: None,
        }
    }

    #[test]
    fn matches_names_and_digits() {
        assert!(ada().matches("love"));
        assert!(ada().matches("7946"));
        assert!(!ada().matches("babbage"));
        assert!(!ada().matches("  "));
    }

    #[test]
    fn owns_its_numbers_however_they_are_written() {
        assert!(ada().owns("+442079460018"));
        assert!(!ada().owns("+442079460019"));
    }
}
