//! QR decoding of still images in pure Rust (`image` for PNG / JPEG, `rqrr` for the code).

use std::path::Path;

use image::{DynamicImage, GrayImage};

/// Long side above which an image is scaled down before detection. Codes in phone photos stay
/// well above the size `rqrr` needs, and detection time grows with the pixel count.
const MAX_SIDE: u32 = 1280;

/// Why no QR code could be read from an image.
///
/// `Display` uses fixed descriptions without paths or image contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The file could not be read.
    Io,
    /// The bytes are not a PNG or JPEG image this crate can decode.
    UnsupportedImage,
    /// No QR code was found.
    NotFound,
    /// A QR code was found but its data could not be decoded (damaged or not UTF-8).
    Unreadable,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Io => "the image file could not be read",
            Self::UnsupportedImage => "the image format is not supported",
            Self::NotFound => "no QR code was found in the image",
            Self::Unreadable => "the QR code could not be decoded",
        })
    }
}

impl std::error::Error for DecodeError {}

/// Reads the first QR code in the PNG or JPEG file at `path` (for example a
/// `masamune-whisker-image-picker` result) and returns its text.
///
/// Decoding runs on the calling thread and takes tens of milliseconds for a phone photo; call it
/// from a background task for large images.
pub fn decode_qr_from_image(path: impl AsRef<Path>) -> Result<String, DecodeError> {
    let bytes = std::fs::read(path).map_err(|_| DecodeError::Io)?;
    decode_qr_from_bytes(&bytes)
}

/// [`decode_qr_from_image`] on a worker thread (`whisker::run_blocking`), for use in async UI code.
pub async fn decode_qr_from_image_async(
    path: impl AsRef<Path> + Send + 'static,
) -> Result<String, DecodeError> {
    whisker::run_blocking(move || decode_qr_from_image(path)).await
}

/// Reads the first QR code in a PNG or JPEG image held in memory.
pub fn decode_qr_from_bytes(bytes: &[u8]) -> Result<String, DecodeError> {
    first(decode_qr_codes(bytes)?)
}

/// Reads every QR code in a PNG or JPEG image held in memory, in detection order. Fails with
/// [`DecodeError::NotFound`] when there is none, and with [`DecodeError::Unreadable`] when codes
/// were found but none could be decoded.
pub fn decode_qr_codes(bytes: &[u8]) -> Result<Vec<String>, DecodeError> {
    let image = image::load_from_memory(bytes).map_err(|_| DecodeError::UnsupportedImage)?;
    decode_qr_codes_in(&image)
}

/// Reads every QR code in a decoded image (see [`decode_qr_codes`]).
pub fn decode_qr_codes_in(image: &DynamicImage) -> Result<Vec<String>, DecodeError> {
    let grey = greyscale(image);
    match detect(&grey) {
        Err(DecodeError::NotFound) => {
            // Light codes on a dark background are only found once inverted.
            let mut inverted = grey;
            image::imageops::invert(&mut inverted);
            detect(&inverted)
        }
        other => other,
    }
}

fn first(codes: Vec<String>) -> Result<String, DecodeError> {
    codes.into_iter().next().ok_or(DecodeError::NotFound)
}

fn greyscale(image: &DynamicImage) -> GrayImage {
    let grey = image.to_luma8();
    let (width, height) = grey.dimensions();
    let long = width.max(height);
    if long <= MAX_SIDE {
        return grey;
    }
    let scale = MAX_SIDE as f64 / long as f64;
    let w = ((width as f64 * scale).round() as u32).max(1);
    let h = ((height as f64 * scale).round() as u32).max(1);
    // Box sampling: fast, and it keeps module edges sharp enough for detection.
    image::imageops::thumbnail(&grey, w, h)
}

fn detect(grey: &GrayImage) -> Result<Vec<String>, DecodeError> {
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(
        grey.width() as usize,
        grey.height() as usize,
        |x, y| grey.get_pixel(x as u32, y as u32).0[0],
    );
    let grids = prepared.detect_grids();
    if grids.is_empty() {
        return Err(DecodeError::NotFound);
    }
    let codes: Vec<String> = grids
        .iter()
        .filter_map(|grid| grid.decode().ok().map(|(_, text)| text))
        .collect();
    if codes.is_empty() {
        Err(DecodeError::Unreadable)
    } else {
        Ok(codes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_have_fixed_descriptions() {
        assert_eq!(
            DecodeError::NotFound.to_string(),
            "no QR code was found in the image"
        );
        assert_eq!(
            decode_qr_from_bytes(b"not an image"),
            Err(DecodeError::UnsupportedImage)
        );
        assert_eq!(
            decode_qr_from_image("/nonexistent/masamune-whisker-barcode-scanner.png"),
            Err(DecodeError::Io)
        );
    }
}
