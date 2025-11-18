# Chunk 14: Click History API Endpoint

## Context
Building on click history storage. We need an API to retrieve click history for analytics.

## Goal
Create GET /api/urls/{code}/clicks endpoint to fetch click history.

## Prompt

```text
I have click history being stored and cleaned. Now add API to retrieve it.

Create get_click_history() handler:
```rust
async fn get_click_history(
    data: web::Data<AppState>,
    code: web::Path<String>,
    http_req: HttpRequest,
) -> Result<HttpResponse> {
    let claims = match get_claims(&http_req) {
        Some(c) => c,
        None => {
            return Ok(HttpResponse::Unauthorized().json(serde_json::json!({
                "error": "Unauthorized"
            })));
        }
    };

    let db = data.db.lock().unwrap();

    // First verify ownership
    let url_id: rusqlite::Result<i64> = db.query_row(
        "SELECT id FROM urls WHERE short_code = ?1 AND user_id = ?2",
        params![code.as_str(), claims.user_id],
        |row| row.get(0),
    );

    let url_id = match url_id {
        Ok(id) => id,
        Err(_) => {
            return Ok(HttpResponse::NotFound().json(serde_json::json!({
                "error": "Short URL not found or not owned by you"
            })));
        }
    };

    // Get total clicks from counter
    let total_clicks: u64 = db.query_row(
        "SELECT clicks FROM urls WHERE id = ?1",
        params![url_id],
        |row| row.get(0),
    ).unwrap_or(0);

    // Get click history (limited to recent 1000)
    let mut stmt = db.prepare(
        "SELECT clicked_at FROM click_history WHERE url_id = ?1 ORDER BY clicked_at DESC LIMIT 1000"
    ).map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?;

    let history: Vec<ClickHistoryEntry> = stmt.query_map(params![url_id], |row| {
        Ok(ClickHistoryEntry {
            clicked_at: row.get(0)?,
        })
    })
    .map_err(|_| actix_web::error::ErrorInternalServerError("Database error"))?
    .filter_map(|r| r.ok())
    .collect();

    Ok(HttpResponse::Ok().json(ClickStats {
        total_clicks,
        history,
    }))
}
```

Register route in protected /api scope:
```rust
.route("/urls/{code}/clicks", web::get().to(get_click_history))
```

The endpoint:
1. Requires authentication (JWT)
2. Verifies URL ownership
3. Returns total click count (from urls.clicks)
4. Returns recent click history (from click_history table)
5. Limits history to 1000 records for performance
6. Ordered by most recent first

Response format:
```json
{
  "total_clicks": 150,
  "history": [
    {"clicked_at": "2024-01-15 10:30:45"},
    {"clicked_at": "2024-01-15 09:15:22"},
    ...
  ]
}
```
```

## Expected Output
- get_click_history() handler
- Ownership verification
- Total clicks from counter
- Recent history from click_history table
- Limited to 1000 records
- Registered in protected /api scope
