# Chunk 19: QR Code API Endpoint

## Context
Building on QR generation functions. We need an API endpoint to serve QR codes.

## Goal
Create GET /api/urls/{code}/qr/{format} endpoint to generate and download QR codes.

## Prompt

```text
I have QR generation for PNG and SVG. Now create the API endpoint.

Create get_qr_code() handler:

```rust
async fn get_qr_code(
    data: web::Data<AppState>,
    path: web::Path<(String, String)>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let (code, format) = path.into_inner();

    let claims = match get_claims(&http_req) {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap();

    // Verify ownership
    let exists: bool = db.query_row(
        "SELECT COUNT(*) FROM urls WHERE short_code = ?1 AND user_id = ?2",
        params![&code, claims.user_id],
        |row| row.get::<_, i64>(0),
    ).map(|count| count > 0).unwrap_or(false);

    if !exists {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Short URL not found or not owned by you"
        })));
    }

    let full_url = format!("{}/{}", data.config.host_url, code);
    drop(db); // Release lock before heavy computation

    match format.as_str() {
        "png" => {
            match generate_qr_code_png(&full_url) {
                Ok(png_bytes) => Ok(HttpResponse::Ok()
                    .content_type("image/png")
                    .append_header(("Content-Disposition", format!("attachment; filename=\"{}.png\"", code)))
                    .body(png_bytes)),
                Err(e) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                    "error": format!("Failed to generate QR code: {}", e)
                }))),
            }
        }
        "svg" => {
            match generate_qr_code_svg(&full_url) {
                Ok(svg_string) => Ok(HttpResponse::Ok()
                    .content_type("image/svg+xml")
                    .append_header(("Content-Disposition", format!("attachment; filename=\"{}.svg\"", code)))
                    .body(svg_string)),
                Err(e) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                    "error": format!("Failed to generate QR code: {}", e)
                }))),
            }
        }
        _ => Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Invalid format. Use 'png' or 'svg'"
        }))),
    }
}
```

Register route in protected /api scope:
```rust
.route("/urls/{code}/qr/{format}", web::get().to(get_qr_code))
```

The endpoint:
1. Requires authentication
2. Extracts short_code and format from path
3. Verifies URL ownership
4. Uses HOST_URL from config for full URL
5. Releases DB lock before QR generation
6. Sets appropriate Content-Type
7. Sets Content-Disposition for download with filename
8. Returns binary (PNG) or text (SVG) body

Supported formats:
- png: Binary image, branded logo
- svg: Vector graphics, scalable

Error cases:
- Unauthorized: No valid JWT
- NotFound: URL doesn't exist or not owned
- BadRequest: Invalid format specified
- InternalServerError: QR generation failed
```

## Expected Output
- get_qr_code() handler
- Path parameters: {code} and {format}
- Ownership verification
- PNG and SVG support
- Content-Disposition for download
- Registered in /api scope
