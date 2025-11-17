# Chunk 17: QR Code Rust Logo Branding

## Context
Building on basic QR PNG generation. We need to add Rust logo in the center.

## Goal
Add Rust-themed logo (orange circle with R) to QR code center.

## Prompt

```text
I have basic QR PNG generation. Now add Rust logo branding.

Update generate_qr_code_png() to add logo AFTER converting to RGBA, BEFORE encoding to PNG:

```rust
fn generate_qr_code_png(url: &str) -> Result<Vec<u8>, String> {
    let code = QrCode::new(url).map_err(|e| e.to_string())?;

    let image = code.render::<Luma<u8>>()
        .min_dimensions(400, 400)
        .build();

    // Convert to RGBA for logo overlay
    let mut rgba_image: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(image.width(), image.height());
    for (x, y, pixel) in image.enumerate_pixels() {
        let luma = pixel.0[0];
        rgba_image.put_pixel(x, y, Rgba([luma, luma, luma, 255]));
    }

    // Create Rust logo (orange circle with R)
    let logo_size = image.width() / 5;
    let logo_x = (image.width() - logo_size) / 2;
    let logo_y = (image.height() - logo_size) / 2;

    // Draw orange circle for logo background
    let center_x = logo_x + logo_size / 2;
    let center_y = logo_y + logo_size / 2;
    let radius = logo_size / 2;

    for y in logo_y..(logo_y + logo_size) {
        for x in logo_x..(logo_x + logo_size) {
            let dx = x as i32 - center_x as i32;
            let dy = y as i32 - center_y as i32;
            if dx * dx + dy * dy <= (radius as i32 * radius as i32) {
                // Rust orange color: #CE422B -> RGB(206, 66, 43)
                rgba_image.put_pixel(x, y, Rgba([206, 66, 43, 255]));
            }
        }
    }

    // Draw "R" in white
    let r_size = logo_size / 2;
    let r_x = center_x - r_size / 3;
    let r_y = center_y - r_size / 2;
    let stroke_width = r_size / 6;

    // Vertical line of R
    for y in r_y..(r_y + r_size) {
        for x in r_x..(r_x + stroke_width) {
            if x < image.width() && y < image.height() {
                rgba_image.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
    }

    // Top horizontal of R
    for y in r_y..(r_y + stroke_width) {
        for x in r_x..(r_x + r_size * 2 / 3) {
            if x < image.width() && y < image.height() {
                rgba_image.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
    }

    // Middle horizontal of R
    for y in (r_y + r_size / 2 - stroke_width / 2)..(r_y + r_size / 2 + stroke_width / 2) {
        for x in r_x..(r_x + r_size * 2 / 3) {
            if x < image.width() && y < image.height() {
                rgba_image.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
    }

    // Right vertical of R (top half)
    for y in r_y..(r_y + r_size / 2) {
        for x in (r_x + r_size * 2 / 3 - stroke_width)..(r_x + r_size * 2 / 3) {
            if x < image.width() && y < image.height() {
                rgba_image.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
    }

    // Diagonal leg of R
    let leg_start_x = r_x + r_size / 3;
    let leg_start_y = r_y + r_size / 2;
    for i in 0..(r_size / 2) {
        let x = leg_start_x + i;
        let y = leg_start_y + i;
        for dx in 0..stroke_width {
            if x + dx < image.width() && y < image.height() {
                rgba_image.put_pixel(x + dx, y, Rgba([255, 255, 255, 255]));
            }
        }
    }

    // Convert to PNG bytes
    let mut png_bytes: Vec<u8> = Vec::new();
    let dynamic_image = DynamicImage::ImageRgba8(rgba_image);
    dynamic_image.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;

    Ok(png_bytes)
}
```

The logo:
- Size: 1/5 of QR code width
- Position: Center of QR code
- Background: Rust orange circle (#CE422B)
- Foreground: White "R" letter
- QR codes have error correction, so center can be obscured
```

## Expected Output
- Orange circle drawn in center
- White "R" letter overlaid
- Rust branding visible
- QR code still scannable
- Bounds checking prevents overflow
