use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

use percent_encoding::percent_decode_str;

/// The local path a `file://` URI names; `None` for any other URI or a remote host.
#[must_use]
pub fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = match rest.strip_prefix("localhost") {
        Some(path) => path,
        None => rest,
    };
    if !path.starts_with('/') {
        return None;
    }
    let bytes: Vec<u8> = percent_decode_str(path).collect();
    Some(PathBuf::from(OsString::from_vec(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_local_file_uris() {
        assert_eq!(
            file_uri_to_path("file:///home/ada/Holiday%20Pics/beach%231.jpg"),
            Some(PathBuf::from("/home/ada/Holiday Pics/beach#1.jpg"))
        );
        assert_eq!(
            file_uri_to_path("file://localhost/tmp/a"),
            Some(PathBuf::from("/tmp/a"))
        );
    }

    #[test]
    fn keeps_non_utf8_bytes() {
        let path = file_uri_to_path("file:///tmp/%FF").unwrap();
        assert_eq!(path.into_os_string().into_vec(), b"/tmp/\xff");
    }

    #[test]
    fn refuses_remote_and_other_uris() {
        assert_eq!(file_uri_to_path("file://server/share/a"), None);
        assert_eq!(file_uri_to_path("https://example.org/a"), None);
    }
}
