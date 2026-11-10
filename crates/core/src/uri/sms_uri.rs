//! `sms:` links (RFC 5724) and their `smsto:`, `mms:` and `mmsto:` spellings.

use percent_encoding::percent_decode_str;

use crate::{
    error::UriError,
    store::drafts::{MAX_RECIPIENTS, MAX_TEXT_BYTES},
    util::phone_number::strip_visual_separators,
};

/// The longest link accepted, as crownlink caps links between devices.
pub const MAX_URI_BYTES: usize = 2048;
const SCHEMES: [&str; 4] = ["sms", "smsto", "mms", "mmsto"];

/// Who a link asks to write to, and what it suggests saying.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct SmsLink {
    pub recipients: Vec<String>,
    pub body: Option<String>,
}

impl SmsLink {
    /// Reads `sms:+1555,+1666?body=hi%20there`, `smsto:+1555:hi` and their `mms` twins.
    ///
    /// # Errors
    ///
    /// When `uri` is not one of these links, is too long, names too many recipients or
    /// suggests too long a message.
    pub fn parse(uri: &str) -> Result<Self, UriError> {
        let uri = uri.trim();
        if uri.len() > MAX_URI_BYTES {
            return Err(UriError::TooLong { max: MAX_URI_BYTES });
        }
        let (scheme, rest) = uri.split_once(':').ok_or(UriError::NotSms)?;
        if !SCHEMES
            .iter()
            .any(|known| known.eq_ignore_ascii_case(scheme))
        {
            return Err(UriError::NotSms);
        }
        let rest = rest.strip_prefix("//").unwrap_or(rest);
        let (targets, query) = rest.split_once('?').unwrap_or((rest, ""));
        let (targets, inline_body) = if scheme.to_ascii_lowercase().ends_with("to") {
            targets
                .split_once(':')
                .map_or((targets, None), |(to, body)| (to, Some(body)))
        } else {
            (targets, None)
        };
        let recipients = targets
            .split(',')
            .map(recipient)
            .filter_map(Result::transpose)
            .collect::<Result<Vec<_>, _>>()?;
        if recipients.len() > MAX_RECIPIENTS {
            return Err(UriError::TooManyRecipients {
                max: MAX_RECIPIENTS,
            });
        }
        let body = query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, _)| key.eq_ignore_ascii_case("body"))
            .map(|(_, value)| value)
            .or(inline_body)
            .map(decode)
            .transpose()?
            .filter(|body| !body.is_empty());
        if body
            .as_ref()
            .is_some_and(|body| body.len() > MAX_TEXT_BYTES)
        {
            return Err(UriError::BodyTooLong {
                max: MAX_TEXT_BYTES,
            });
        }
        Ok(Self { recipients, body })
    }
}

/// One recipient without its `;parameters` and visual separators; `None` when empty.
fn recipient(raw: &str) -> Result<Option<String>, UriError> {
    let decoded = decode(raw)?;
    let address = decoded.split(';').next().unwrap_or_default();
    let address = strip_visual_separators(address);
    Ok((!address.is_empty()).then(|| address.into_owned()))
}

fn decode(raw: &str) -> Result<String, UriError> {
    percent_decode_str(raw)
        .decode_utf8()
        .map(|decoded| decoded.trim().to_owned())
        .map_err(|_| UriError::NotUtf8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(uri: &str) -> SmsLink {
        SmsLink::parse(uri).unwrap()
    }

    #[test]
    fn reads_recipients_and_body() {
        assert_eq!(
            parse("sms:+1-555-0100,+1%20555%200111?body=hi%20there"),
            SmsLink {
                recipients: vec!["+15550100".into(), "+15550111".into()],
                body: Some("hi there".into()),
            }
        );
    }

    #[test]
    fn plus_is_a_literal_not_a_space() {
        assert_eq!(parse("sms:+15550100?body=1+1").body.as_deref(), Some("1+1"));
    }

    #[test]
    fn schemes_are_case_insensitive_and_have_aliases() {
        assert_eq!(parse("SMS:5550100").recipients, ["5550100"]);
        assert_eq!(parse("mms://5550100").recipients, ["5550100"]);
        assert_eq!(
            parse("smsto:+15550100:See%20you"),
            SmsLink {
                recipients: vec!["+15550100".into()],
                body: Some("See you".into())
            }
        );
        assert_eq!(parse("MMSTO:+15550100").recipients, ["+15550100"]);
    }

    #[test]
    fn a_link_may_name_nobody() {
        assert_eq!(
            parse("sms:?body=Vote%20now"),
            SmsLink {
                recipients: vec![],
                body: Some("Vote now".into())
            }
        );
        assert_eq!(parse("sms:"), SmsLink::default());
    }

    #[test]
    fn parameters_and_unknown_fields_are_ignored() {
        let link = parse("sms:+15550100;phone-context=example.com?subject=x&BODY=ok&x=1");
        assert_eq!(link.recipients, ["+15550100"]);
        assert_eq!(link.body.as_deref(), Some("ok"));
    }

    #[test]
    fn e_mail_recipients_keep_their_dots() {
        assert_eq!(
            parse("mms:ada.l@example.org").recipients,
            ["ada.l@example.org"]
        );
    }

    #[test]
    fn refuses_other_and_oversized_links() {
        assert_eq!(SmsLink::parse("tel:+15550100"), Err(UriError::NotSms));
        assert_eq!(SmsLink::parse("+15550100"), Err(UriError::NotSms));
        let long = format!("sms:{}", "1".repeat(MAX_URI_BYTES));
        assert_eq!(
            SmsLink::parse(&long),
            Err(UriError::TooLong { max: MAX_URI_BYTES })
        );
        let crowd = format!("sms:{}", vec!["5550100"; 33].join(","));
        assert_eq!(
            SmsLink::parse(&crowd),
            Err(UriError::TooManyRecipients { max: 32 })
        );
        assert_eq!(SmsLink::parse("sms:%FF"), Err(UriError::NotUtf8));
    }
}
