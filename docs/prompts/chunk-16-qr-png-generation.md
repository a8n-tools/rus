# Chunk 16: QR Code PNG Generation

## Context
Building on QR dependencies. We need to generate QR codes as PNG images.

## Goal
Create function to generate QR code as PNG bytes.

## Prompt

```text
I have QR code dependencies. Now implement PNG generation.

Create generate_qr_code_png() function that generates a basic QR code (logo branding comes next chunk):

```rust
fn generate_qr_code_png(url: &str) -> Result<Vec<u8>, String> {
    let code = QrCode::new(url).map_err(|e| e.to_string())?;

    let image = code.render::<Luma<u8>>()
        .min_dimensions(400, 400)
        .build();

    // Convert to RGBA for later logo overlay
    let mut rgba_image: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(image.width(), image.height());
    for (x, y, pixel) in image.enumerate_pixels() {
        let luma = pixel.0[0];
        rgba_image.put_pixel(x, y, Rgba([luma, luma, luma, 255]));
    }

    // Convert to PNG bytes
    let mut png_bytes: Vec<u8> = Vec::new();
    let dynamic_image = DynamicImage::ImageRgba8(rgba_image);
    dynamic_image.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;

    Ok(png_bytes)
}
```

This function:
1. Creates QR code from URL string
2. Renders as grayscale image (Luma<u8>)
3. Sets minimum size to 400x400 pixels
4. Converts to RGBA format (prepares for logo overlay)
5. Encodes as PNG bytes
6. Returns Vec<u8> or error string

The 400x400 size:
- Large enough for clear scanning
- Small enough for fast generation
- Standard size for downloads

The RGBA conversion:
- Enables color logo overlay (next chunk)
- Full opacity (alpha = 255)
- Grayscale values copied to RGB channels

Error handling:
- QrCode::new() can fail for invalid data
- write_to() can fail for encoding issues
- Both return descriptive error strings

This is a foundation - logo branding is added in the next chunk.
```

## Expected Output
- generate_qr_code_png() function
- Creates 400x400 QR code
- Converts to RGBA format
- Returns PNG as Vec<u8>
- Proper error handling
