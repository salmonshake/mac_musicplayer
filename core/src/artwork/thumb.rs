//! Thumbnail generation: decode any supported image, shrink to fit, encode as JPEG.

use image::codecs::jpeg::JpegEncoder;

/// Longest side of cached thumbnails (Retina for a 160pt header).
pub const THUMB_SIZE: u32 = 320;

/// Decoded RGBA pixels ready to upload as a texture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumb {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Decode any supported image, shrink it to fit max x max (never enlarge), and encode as JPEG (quality 85).
pub fn make_thumbnail_jpeg(bytes: &[u8], max: u32) -> Option<Vec<u8>> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = if img.width() > max || img.height() > max {
        img.thumbnail(max, max)
    } else {
        img
    };
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 85).encode_image(&rgb).ok()?;
    Some(out)
}

/// Decode image bytes (e.g. a cached JPEG) to RGBA.
pub fn decode_rgba(bytes: &[u8]) -> Option<Thumb> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    Some(Thumb {
        width: img.width(),
        height: img.height(),
        rgba: img.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a test PNG with the given dimensions and a solid red color.
    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([200, 0, 0]));
        let mut buf = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        buf
    }

    #[test]
    fn thumbnail_shrinks_keeping_aspect() {
        let src = png(1000, 500);
        let jpeg = make_thumbnail_jpeg(&src, THUMB_SIZE);
        let thumb = decode_rgba(&jpeg.unwrap());
        assert_eq!(thumb.as_ref().unwrap().width, 320);
        assert_eq!(thumb.as_ref().unwrap().height, 160);
        assert_eq!(thumb.as_ref().unwrap().rgba.len(), (320 * 160 * 4) as usize);
    }

    #[test]
    fn thumbnail_does_not_enlarge() {
        let src = png(100, 50);
        let jpeg = make_thumbnail_jpeg(&src, THUMB_SIZE);
        let thumb = decode_rgba(&jpeg.unwrap());
        assert_eq!(thumb.as_ref().unwrap().width, 100);
        assert_eq!(thumb.as_ref().unwrap().height, 50);
    }

    #[test]
    fn garbage_is_none() {
        assert!(make_thumbnail_jpeg(b"not an image", THUMB_SIZE).is_none());
        assert!(decode_rgba(b"xx").is_none());
    }
}
