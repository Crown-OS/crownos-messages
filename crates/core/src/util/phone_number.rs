use std::borrow::Cow;

/// How many trailing digits two numbers must share to be the same line: a national number
/// and its international spelling agree on these.
const SIGNIFICANT_DIGITS: usize = 10;
/// Numbers shorter than this are short codes and must match exactly.
const SHORT_CODE_DIGITS: usize = 7;
const VISUAL_SEPARATORS: [char; 5] = [' ', '-', '.', '(', ')'];

/// The digits of `address`; empty for an e-mail address or an alphanumeric sender.
#[must_use]
pub fn matching_digits(address: &str) -> String {
    if address.contains('@') || address.chars().any(char::is_alphabetic) {
        return String::new();
    }
    address.chars().filter(char::is_ascii_digit).collect()
}

/// `address` without the spaces, dashes, dots and brackets people write numbers with
/// (RFC 3966 visual separators). E-mail addresses are left alone.
#[must_use]
pub fn strip_visual_separators(address: &str) -> Cow<'_, str> {
    if address.contains('@') || !address.contains(VISUAL_SEPARATORS) {
        return Cow::Borrowed(address.trim());
    }
    Cow::Owned(
        address
            .chars()
            .filter(|character| !VISUAL_SEPARATORS.contains(character))
            .collect(),
    )
}

/// Whether two addresses reach the same line: equal ignoring separators and case, or sharing
/// their last ten digits (so `+1 555 010 0123` matches `(555) 010-0123`).
#[must_use]
pub fn same_address(left: &str, right: &str) -> bool {
    let (left_digits, right_digits) = (matching_digits(left), matching_digits(right));
    if left_digits.is_empty() || right_digits.is_empty() {
        return strip_visual_separators(left).eq_ignore_ascii_case(&strip_visual_separators(right));
    }
    if left_digits.len() < SHORT_CODE_DIGITS || right_digits.len() < SHORT_CODE_DIGITS {
        return left_digits == right_digits;
    }
    let tail = |digits: &str| digits[digits.len().saturating_sub(SIGNIFICANT_DIGITS)..].to_owned();
    tail(&left_digits) == tail(&right_digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separators_are_stripped_from_numbers_only() {
        assert_eq!(strip_visual_separators("+1 (555) 010-0123"), "+15550100123");
        assert_eq!(strip_visual_separators(" +15550100123 "), "+15550100123");
        assert_eq!(
            strip_visual_separators("ada.l@example.org"),
            "ada.l@example.org"
        );
    }

    #[test]
    fn national_and_international_spellings_match() {
        assert!(same_address("+1 555 010 0123", "(555) 010-0123"));
        assert!(same_address("+44 20 7946 0018", "020 7946 0018"));
        assert!(!same_address("+1 555 010 0123", "+1 555 010 0124"));
    }

    #[test]
    fn short_codes_and_names_match_exactly() {
        assert!(same_address("72345", "72345"));
        assert!(!same_address("72345", "172345"));
        assert!(same_address("BANK", "bank"));
        assert!(!same_address("BANK", "72345"));
    }
}
