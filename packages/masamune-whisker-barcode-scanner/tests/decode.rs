//! Decoding of image fixtures generated from a known payload.

use masamune_whisker_barcode_scanner::{
    DecodeError, decode_qr_codes, decode_qr_from_bytes, decode_qr_from_image,
};

const PAYLOAD: &str = "https://example.com/masamune-whisker-barcode-scanner?id=42";

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn decodes_a_png_file() {
    assert_eq!(
        decode_qr_from_image(fixture("qr_example.png")).as_deref(),
        Ok(PAYLOAD)
    );
}

#[test]
fn decodes_a_rotated_jpeg_photo_from_bytes() {
    let bytes = std::fs::read(fixture("qr_example_photo.jpg")).unwrap();
    assert_eq!(decode_qr_from_bytes(&bytes).as_deref(), Ok(PAYLOAD));
    assert_eq!(decode_qr_codes(&bytes), Ok(vec![PAYLOAD.to_string()]));
}

#[test]
fn decodes_an_inverted_code() {
    let mut image = image::open(fixture("qr_example.png")).unwrap().to_luma8();
    image::imageops::invert(&mut image);
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    assert_eq!(decode_qr_from_bytes(png.get_ref()).as_deref(), Ok(PAYLOAD));
}

#[test]
fn decodes_a_large_image_after_scaling_it_down() {
    let image = image::open(fixture("qr_example.png")).unwrap().to_luma8();
    // Larger than the 1280 px detection size, so the decoder scales it down first.
    let size = 1700;
    let large = image::GrayImage::from_fn(size, size, |x, y| {
        *image.get_pixel(x * image.width() / size, y * image.height() / size)
    });
    let mut png = std::io::Cursor::new(Vec::new());
    large.write_to(&mut png, image::ImageFormat::Png).unwrap();
    assert_eq!(decode_qr_from_bytes(png.get_ref()).as_deref(), Ok(PAYLOAD));
}

#[test]
fn reports_missing_codes_and_bad_input() {
    assert_eq!(
        decode_qr_from_image(fixture("no_code.png")),
        Err(DecodeError::NotFound)
    );
    assert_eq!(
        decode_qr_from_bytes(b"GIF89a"),
        Err(DecodeError::UnsupportedImage)
    );
    assert_eq!(
        decode_qr_from_image(fixture("missing.png")),
        Err(DecodeError::Io)
    );
}
