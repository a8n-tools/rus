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
