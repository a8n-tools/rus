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

#[cfg(test)]
mod tests {
    use super::*;

    // --- generate_short_code ---

    #[test]
    fn short_code_has_correct_length() {
        assert_eq!(generate_short_code().len(), 6);
    }

    #[test]
    fn short_code_uses_alphanumeric_charset() {
        for _ in 0..20 {
            let code = generate_short_code();
            assert!(
                code.chars().all(|c| c.is_ascii_alphanumeric()),
                "non-alphanumeric char in code: {code}"
            );
        }
    }

    #[test]
    fn short_codes_are_random() {
        let codes: std::collections::HashSet<String> =
            (0..100).map(|_| generate_short_code()).collect();
        assert!(
            codes.len() > 90,
            "expected high uniqueness, got {} unique codes out of 100",
            codes.len()
        );
    }

    // --- validate_url ---

    #[test]
    fn validate_accepts_http() {
        assert!(validate_url("http://example.com", 2048).is_ok());
    }

    #[test]
    fn validate_accepts_https() {
        assert!(validate_url("https://example.com/path?q=1#frag", 2048).is_ok());
    }

    #[test]
    fn validate_rejects_url_exceeding_max_length() {
        let long = format!("https://example.com/{}", "a".repeat(2040));
        assert!(long.len() > 2048);
        assert!(validate_url(&long, 2048).is_err());
    }

    #[test]
    fn validate_accepts_url_at_exact_max_length() {
        // Build a URL that is exactly max_length characters.
        let max = 100;
        let base = "https://x.co/";
        let pad = "a".repeat(max - base.len());
        let url = format!("{}{}", base, pad);
        assert_eq!(url.len(), max);
        assert!(validate_url(&url, max).is_ok());
    }

    #[test]
    fn validate_rejects_ftp_scheme() {
        let err = validate_url("ftp://example.com/file.txt", 2048).unwrap_err();
        assert!(err.contains("http"), "unexpected error: {err}");
    }

    #[test]
    fn validate_rejects_javascript_scheme() {
        assert!(validate_url("javascript:alert(1)", 2048).is_err());
    }

    #[test]
    fn validate_rejects_data_uri() {
        assert!(validate_url("data:text/html,<h1>hi</h1>", 2048).is_err());
    }

    #[test]
    fn validate_rejects_file_scheme() {
        assert!(validate_url("file:///etc/passwd", 2048).is_err());
    }

    #[test]
    fn validate_rejects_vbscript_scheme() {
        assert!(validate_url("vbscript:msgbox(1)", 2048).is_err());
    }

    #[test]
    fn validate_rejects_about_scheme() {
        assert!(validate_url("about:blank", 2048).is_err());
    }

    #[test]
    fn validate_rejects_malformed_url() {
        assert!(validate_url("not-a-url-at-all", 2048).is_err());
    }

    #[test]
    fn validate_rejects_empty_string() {
        assert!(validate_url("", 2048).is_err());
    }

    #[test]
    fn validate_rejects_dangerous_pattern_embedded_in_https() {
        // A URL that looks valid but embeds a blocked pattern in the path
        assert!(validate_url("https://example.com/redirect?url=javascript:void(0)", 2048).is_err());
    }
}
