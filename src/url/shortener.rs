use rand::Rng;
use url::Url;

/// Generate a random short code
pub fn generate_short_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    const CODE_LENGTH: usize = 6;

    let mut rng = rand::thread_rng();
    (0..CODE_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Validate URL for shortening
pub fn validate_url(url_str: &str, max_length: usize) -> Result<(), String> {
    if url_str.len() > max_length {
        return Err(format!(
            "URL exceeds maximum length of {} characters",
            max_length
        ));
    }

    let parsed = Url::parse(url_str).map_err(|_| "Invalid URL format".to_string())?;

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
