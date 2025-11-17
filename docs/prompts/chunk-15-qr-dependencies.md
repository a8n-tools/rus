# Chunk 15: QR Code Generation Dependencies

## Context
Building on click history API. We need to add QR code generation capabilities.

## Goal
Add QR code and image processing dependencies to Cargo.toml.

## Prompt

```text
I have click history API working. Now add QR code generation dependencies.

Update Cargo.toml to add these dependencies:

```toml
[dependencies]
# ... existing dependencies ...
qrcode = "0.14"
image = "0.25"
```

The qrcode crate:
- Generates QR codes from strings
- Supports multiple output formats
- Configurable size and error correction

The image crate:
- Image manipulation and processing
- Supports PNG, JPEG, etc.
- We'll use it to embed logo into QR code

Add imports at the top of main.rs:
```rust
use qrcode::QrCode;
use qrcode::render::svg;
use image::{Luma, DynamicImage, ImageBuffer, Rgba};
```

These imports:
- QrCode: Main QR code generator
- svg: SVG rendering support
- Luma: Grayscale pixel type (QR codes are black/white)
- DynamicImage: Generic image type for conversion
- ImageBuffer: Raw pixel buffer for manipulation
- Rgba: RGBA pixel type for color logo overlay

Don't add any implementation yet - just dependencies and imports. Make sure the project compiles with these new imports (they won't be used yet, but Rust will verify they exist).

Run `cargo build` to verify dependencies download and compile correctly.
```

## Expected Output
- qrcode = "0.14" in Cargo.toml
- image = "0.25" in Cargo.toml
- Imports added to main.rs
- Project compiles successfully
- Dependencies downloaded
