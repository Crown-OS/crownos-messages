//! Pictures from the phone, decoded off the UI thread and shrunk to what a bubble shows.

use std::path::Path;

use image::{ImageReader, imageops::FilterType};

/// The longest edge kept: a bubble is at most this wide at 2x scale.
const MAX_EDGE: u32 = 720;

/// Straight-alpha RGBA8 pixels, as the renderer takes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Decodes the picture at `path`, shrunk to fit `MAX_EDGE`.
///
/// # Errors
///
/// When the file cannot be read or is not a picture this build decodes.
pub fn decode_picture(path: &Path) -> Result<Picture, image::ImageError> {
    let decoded = ImageReader::open(path)?.with_guessed_format()?.decode()?;
    let fitted = if decoded.width() > MAX_EDGE || decoded.height() > MAX_EDGE {
        decoded.resize(MAX_EDGE, MAX_EDGE, FilterType::Triangle)
    } else {
        decoded
    };
    let rgba = fitted.into_rgba8();
    Ok(Picture {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_pictures_shrink_to_the_bubble() {
        let dir = std::env::temp_dir().join(format!("crownos-messages-picture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wide.png");
        image::RgbaImage::new(1440, 360).save(&path).unwrap();
        let picture = decode_picture(&path).unwrap();
        assert_eq!((picture.width, picture.height), (720, 180));
        assert_eq!(picture.rgba.len(), 720 * 180 * 4);
        assert!(decode_picture(&dir.join("missing.png")).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
