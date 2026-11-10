use std::path::Path;

/// A part of a message that is not its text, fetched from the phone on demand.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Attachment {
    pub key: String,
    pub mime: String,
    pub name: Option<String>,
    pub len: u64,
    pub thumbnail_key: Option<String>,
    /// `0` when unknown.
    pub width: u16,
    /// `0` when unknown.
    pub height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttachmentKind {
    Image,
    Video,
    Audio,
    ContactCard,
    Other,
}

impl AttachmentKind {
    #[must_use]
    pub fn of_mime(mime: &str) -> Self {
        let mime = mime.trim();
        let (family, subtype) = mime.split_once('/').unwrap_or((mime, ""));
        match family.to_ascii_lowercase().as_str() {
            "image" => Self::Image,
            "video" => Self::Video,
            "audio" => Self::Audio,
            "text"
                if subtype.eq_ignore_ascii_case("vcard")
                    || subtype.eq_ignore_ascii_case("x-vcard") =>
            {
                Self::ContactCard
            }
            _ => Self::Other,
        }
    }

    #[must_use]
    pub fn of_path(path: &Path) -> Self {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        match extension.as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "heic" | "heif" | "bmp" | "avif" => {
                Self::Image
            }
            "mp4" | "3gp" | "3gpp" | "webm" | "mkv" | "mov" => Self::Video,
            "mp3" | "m4a" | "aac" | "amr" | "ogg" | "opus" | "wav" | "flac" => Self::Audio,
            "vcf" => Self::ContactCard,
            _ => Self::Other,
        }
    }

    #[must_use]
    pub const fn noun(self) -> &'static str {
        match self {
            Self::Image => "Photo",
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::ContactCard => "Contact card",
            Self::Other => "Attachment",
        }
    }
}

impl Attachment {
    #[must_use]
    pub fn kind(&self) -> AttachmentKind {
        AttachmentKind::of_mime(&self.mime)
    }

    /// The file name, or what kind of attachment it is.
    #[must_use]
    pub fn label(&self) -> &str {
        self.name
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| self.kind().noun())
    }

    /// Width over height, when the phone measured it.
    #[must_use]
    pub fn aspect_ratio(&self) -> Option<f32> {
        (self.width > 0 && self.height > 0).then(|| f32::from(self.width) / f32::from(self.height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_come_from_the_mime_family() {
        assert_eq!(AttachmentKind::of_mime("image/jpeg"), AttachmentKind::Image);
        assert_eq!(AttachmentKind::of_mime("VIDEO/mp4"), AttachmentKind::Video);
        assert_eq!(
            AttachmentKind::of_mime("text/x-vCard"),
            AttachmentKind::ContactCard
        );
        assert_eq!(AttachmentKind::of_mime("text/plain"), AttachmentKind::Other);
        assert_eq!(AttachmentKind::of_mime("garbage"), AttachmentKind::Other);
    }

    #[test]
    fn kinds_come_from_the_extension_for_local_files() {
        assert_eq!(
            AttachmentKind::of_path(Path::new("/a/IMG_1.JPG")),
            AttachmentKind::Image
        );
        assert_eq!(
            AttachmentKind::of_path(Path::new("note.opus")),
            AttachmentKind::Audio
        );
        assert_eq!(
            AttachmentKind::of_path(Path::new("README")),
            AttachmentKind::Other
        );
    }

    #[test]
    fn label_prefers_the_name() {
        let mut attachment = Attachment {
            key: "mms-part/1/2".into(),
            mime: "image/png".into(),
            name: None,
            len: 10,
            thumbnail_key: None,
            width: 0,
            height: 0,
        };
        assert_eq!(attachment.label(), "Photo");
        assert_eq!(attachment.aspect_ratio(), None);
        attachment.name = Some("plan.png".into());
        attachment.width = 200;
        attachment.height = 100;
        assert_eq!(attachment.label(), "plan.png");
        assert_eq!(attachment.aspect_ratio(), Some(2.0));
    }
}
