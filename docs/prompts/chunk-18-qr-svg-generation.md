# Chunk 18: QR Code SVG Generation

## Context
Building on PNG QR generation. We need to also support SVG format for vector graphics.

## Goal
Create function to generate QR code as SVG with Rust logo.

## Prompt

```text
I have QR PNG with logo. Now add SVG generation.

Create generate_qr_code_svg() function:

```rust
fn generate_qr_code_svg(url: &str) -> Result<String, String> {
    let code = QrCode::new(url).map_err(|e| e.to_string())?;

    let svg_string = code.render()
        .min_dimensions(400, 400)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build();

    // Insert Rust logo into SVG
    let logo_svg = r#"
    <circle cx="50%" cy="50%" r="10%" fill="#CE422B"/>
    <text x="50%" y="50%" text-anchor="middle" dominant-baseline="central"
          font-family="sans-serif" font-weight="bold" font-size="40" fill="white">R</text>
    "#;

    // Insert logo before closing </svg>
    let svg_with_logo = svg_string.replace("</svg>", &format!("{}</svg>", logo_svg));

    Ok(svg_with_logo)
}
```

This function:
1. Creates QR code from URL
2. Renders as SVG string (vector format)
3. Sets minimum dimensions 400x400
4. Uses black and white colors
5. Adds SVG elements for logo:
   - Circle: Orange background, centered, 10% radius
   - Text: White "R", centered, bold

SVG advantages:
- Scalable without quality loss
- Smaller file size
- Easy to edit/customize
- Text is crisp at any size

The logo uses percentage positioning:
- cx="50%" cy="50%": Center of viewBox
- r="10%": Radius relative to size
- text-anchor="middle": Horizontal center
- dominant-baseline="central": Vertical center

The replace() approach:
- Finds closing </svg> tag
- Inserts logo elements before it
- Simple string manipulation
- Maintains valid SVG structure

Make sure svg::Color is imported from qrcode::render::svg.
```

## Expected Output
- generate_qr_code_svg() function
- Returns SVG as String
- Black/white QR code
- Orange circle logo centered
- White "R" text centered
- Valid SVG markup
