# Chunk 5: URL Input Validation

## Context
Building on security hardening. We need to validate URLs before shortening to prevent malicious inputs.

## Goal
Add comprehensive URL validation: length limits, allowed schemes, blocking dangerous patterns.

## Prompt

```text
I have a Rust URL shortener with account lockout. Now add URL input validation.

First, ensure these dependencies are in Cargo.toml:
```toml
url = "2.5"
```

Import at the top of main.rs:
```rust
use url::Url;
```

Create a validate_url() function:
1. Takes: url_str: &str, max_length: usize
2. Returns: Result<(), String> where Err contains descriptive message
3. Validation steps:
   a. Check length: if url_str.len() > max_length, error
   b. Parse URL: Url::parse(url_str), error if invalid format
   c. Check scheme: must be "http" or "https" only
   d. Block dangerous patterns in lowercase URL:
      - "javascript:"
      - "data:"
      - "file:"
      - "vbscript:"
      - "about:"

```rust
fn validate_url(url_str: &str, max_length: usize) -> Result<(), String> {
    if url_str.len() > max_length {
        return Err(format!("URL exceeds maximum length of {} characters", max_length));
    }

    let parsed = Url::parse(url_str).map_err(|_| "Invalid URL format")?;

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("Only http:// and https:// URLs are allowed".to_string());
    }

    // Block dangerous patterns
    let dangerous_patterns = [
        "javascript:",
        "data:",
        "file:",
        "vbscript:",
        "about:",
    ];

    let url_lower = url_str.to_lowercase();
    for pattern in &dangerous_patterns {
        if url_lower.contains(pattern) {
            return Err(format!("URL contains blocked pattern: {}", pattern));
        }
    }

    Ok(())
}
```

Integrate into shorten_url() handler:
1. After checking URL is not empty
2. Call validate_url(&req_payload.url, data.config.max_url_length)
3. If Err, return BadRequest with JSON: {"error": "the error message"}

Also update the shorten response to use HOST_URL from config:
```rust
short_url: format!("{}/{}", data.config.host_url, short_code),
```

Replace the hardcoded "http://localhost:8080" with config value.
```

## Expected Output
- url crate dependency added
- validate_url() function with all checks
- Integrated into shorten_url()
- Descriptive error messages
- HOST_URL used in responses
