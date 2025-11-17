# Chunk 12: Recording Click Events

## Context
Building on click history schema. We need to record each click in the redirect handler.

## Goal
Update redirect_url() to record individual clicks in click_history table.

## Prompt

```text
I have click_history table schema. Now record clicks during redirects.

Update redirect_url() handler to:
1. Get the URL id along with original_url
2. Record click in click_history table
3. Keep existing click counter increment for backward compatibility

Current redirect_url() gets original_url by short_code. Update to also get id:

```rust
async fn redirect_url(
    data: web::Data<AppState>,
    code: web::Path<String>,
) -> Result<HttpResponse> {
    let db = data.db.lock().unwrap();

    // Get URL ID and original URL
    let result: rusqlite::Result<(i64, String)> = db.query_row(
        "SELECT id, original_url FROM urls WHERE short_code = ?1",
        params![code.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    );

    match result {
        Ok((url_id, original_url)) => {
            // Increment click count (legacy counter)
            let _ = db.execute(
                "UPDATE urls SET clicks = clicks + 1 WHERE id = ?1",
                params![url_id],
            );

            // Record click in history
            let _ = db.execute(
                "INSERT INTO click_history (url_id) VALUES (?1)",
                params![url_id],
            );

            Ok(HttpResponse::Found()
                .append_header(("Location", original_url))
                .finish())
        }
        Err(_) => Ok(HttpResponse::NotFound().body("Short URL not found")),
    }
}
```

Key changes:
1. SELECT now fetches id and original_url
2. UPDATE uses id (more efficient than short_code)
3. INSERT adds row to click_history (timestamp is automatic)
4. Error handling remains same

The click_history INSERT:
- url_id: Links this click to the URL
- clicked_at: Automatically set to CURRENT_TIMESTAMP by SQLite

We keep the clicks counter for:
- Backward compatibility
- Quick total count without COUNT(*) query
- Legacy dashboard support

Both operations ignore errors (fire and forget) to not block redirects.
```

## Expected Output
- redirect_url() fetches URL id
- Click count still incremented
- Click recorded in history table
- Timestamp automatically set
- No blocking on database errors
