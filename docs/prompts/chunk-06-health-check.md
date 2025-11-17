# Chunk 6: Health Check Endpoint

## Context
Building reliability features. We need a health check endpoint for monitoring and Docker health checks.

## Goal
Add /health endpoint that reports service status, version, and uptime.

## Prompt

```text
I have a Rust URL shortener with URL validation. Now add a health check endpoint.

Create response struct:
```rust
#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    uptime_seconds: u64,
}
```

Update AppState to track start time:
```rust
struct AppState {
    db: Mutex<Connection>,
    config: Config,
    start_time: std::time::Instant,
}
```

Update AppState::new() to initialize start_time:
```rust
Ok(AppState {
    db: Mutex::new(conn),
    config,
    start_time: std::time::Instant::now(),
})
```

Create health_check handler:
```rust
async fn health_check(data: web::Data<AppState>) -> Result<HttpResponse> {
    let uptime = data.start_time.elapsed().as_secs();

    Ok(HttpResponse::Ok().json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime,
    }))
}
```

Register the route in main() HttpServer configuration:
- Add BEFORE the /{code} catch-all route
- Path: "/health"
- Method: GET
- No authentication required (public endpoint)

```rust
.route("/health", web::get().to(health_check))
```

The health check should:
1. Return HTTP 200 with JSON body
2. Include "healthy" status
3. Include package version from Cargo.toml
4. Include uptime in seconds since server start
5. Be accessible without authentication

This endpoint will be used by Docker health checks and monitoring systems.
```

## Expected Output
- HealthResponse struct
- AppState with start_time tracking
- health_check handler
- /health route registered
- Returns version and uptime
