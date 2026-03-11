use image::{DynamicImage, Luma};
use qrcode::render::svg;
use qrcode::QrCode;

/// Generate QR code as PNG
pub fn generate_qr_code_png(url: &str) -> Result<Vec<u8>, String> {
    let code = QrCode::new(url).map_err(|e| e.to_string())?;

    let image = code
        .render::<Luma<u8>>()
        .min_dimensions(400, 400)
        .build();

    // Convert to PNG bytes
    let mut png_bytes: Vec<u8> = Vec::new();
    DynamicImage::ImageLuma8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .map_err(|e| e.to_string())?;

    Ok(png_bytes)
}

/// Generate QR code as SVG
pub fn generate_qr_code_svg(url: &str) -> Result<String, String> {
    let code = QrCode::new(url).map_err(|e| e.to_string())?;

    let svg_string = code
        .render()
        .min_dimensions(400, 400)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build();

    Ok(svg_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- generate_qr_code_png ---

    #[test]
    fn png_succeeds_for_valid_url() {
        let result = generate_qr_code_png("https://example.com");
        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
    }

    #[test]
    fn png_output_is_non_empty() {
        let bytes = generate_qr_code_png("https://example.com").unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn png_starts_with_png_magic_bytes() {
        let bytes = generate_qr_code_png("https://example.com").unwrap();
        // PNG magic: 0x89 P N G \r \n 0x1a \n
        assert_eq!(
            &bytes[..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
            "bytes do not start with PNG magic"
        );
    }

    #[test]
    fn png_works_for_long_url() {
        let url = format!("https://example.com/{}", "a".repeat(200));
        assert!(generate_qr_code_png(&url).is_ok());
    }

    // --- generate_qr_code_svg ---

    #[test]
    fn svg_succeeds_for_valid_url() {
        assert!(generate_qr_code_svg("https://example.com").is_ok());
    }

    #[test]
    fn svg_output_contains_svg_tags() {
        let svg = generate_qr_code_svg("https://example.com").unwrap();
        assert!(svg.contains("<svg"), "SVG missing opening tag");
        assert!(svg.contains("</svg>"), "SVG missing closing tag");
    }

    #[test]
    fn svg_output_contains_expected_colors() {
        let svg = generate_qr_code_svg("https://example.com").unwrap();
        assert!(
            svg.contains("#000000") || svg.contains("#ffffff"),
            "SVG missing expected color values"
        );
    }

    #[test]
    fn svg_works_for_long_url() {
        let url = format!("https://example.com/{}", "b".repeat(200));
        let svg = generate_qr_code_svg(&url).unwrap();
        assert!(svg.contains("<svg"));
    }
}
