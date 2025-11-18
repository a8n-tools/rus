# Chunk 20: Public Config API Endpoint

## Context
Building on QR API. Frontend needs to know HOST_URL and other config values.

## Goal
Create GET /api/config endpoint to expose public configuration to frontend.

## Prompt

```text
I have QR code API working. Now add public config endpoint for frontend.

Create ConfigResponse struct:
```rust
#[derive(Serialize)]
struct ConfigResponse {
    host_url: String,
    max_url_length: usize,
}
```

Create get_config() handler:
```rust
async fn get_config(data: web::Data<AppState>) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ConfigResponse {
        host_url: data.config.host_url.clone(),
        max_url_length: data.config.max_url_length,
    }))
}
```

Register route as PUBLIC (before authenticated scope):
```rust
.route("/api/config", web::get().to(get_config))
```

Place it near other public routes like /api/register and /api/login.

This endpoint:
1. No authentication required
2. Returns HOST_URL for frontend to display full shortened URLs
3. Returns MAX_URL_LENGTH for client-side validation
4. Only exposes safe, non-sensitive config values

Why this endpoint:
- Frontend can't read environment variables
- HOST_URL is needed to display "your short URL is: https://example.com/abc123"
- MAX_URL_LENGTH helps with client-side validation
- Avoids hardcoding values in JavaScript

What NOT to expose:
- JWT secret
- Database credentials
- Internal paths
- Security thresholds (account lockout, etc.)

Frontend will fetch this on load and use the values to construct URLs and validate input before submitting.

Example response:
```json
{
  "host_url": "https://short.example.com",
  "max_url_length": 2048
}
```
```

## Expected Output
- ConfigResponse struct with safe fields only
- get_config() handler
- /api/config route (public)
- Returns HOST_URL and MAX_URL_LENGTH
- No sensitive data exposed
