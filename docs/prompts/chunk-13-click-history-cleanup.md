# Chunk 13: Click History Automatic Cleanup

## Context
Building on click recording. We need to remove old click history based on retention policy.

## Goal
Implement automatic cleanup of click records older than retention period.

## Prompt

```text
I have click history being recorded. Now add automatic cleanup.

Create cleanup_old_clicks() function:
```rust
fn cleanup_old_clicks(db: &Connection, retention_days: i64) {
    let cutoff = Utc::now() - Duration::days(retention_days);
    let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

    let _ = db.execute(
        "DELETE FROM click_history WHERE clicked_at < ?1",
        params![cutoff_str],
    );
}
```

This function:
1. Calculates cutoff date (now minus retention days)
2. Formats as SQLite datetime string
3. Deletes all records older than cutoff
4. Ignores errors (maintenance task)

Integrate into redirect_url() handler - run periodically, not every request:
```rust
// After recording click in history
// Cleanup old clicks periodically (1% chance)
if rand::thread_rng().gen_range(0..100) == 0 {
    cleanup_old_clicks(&db, data.config.click_retention_days);
}
```

Why probabilistic cleanup:
- Runs on ~1% of redirects
- Distributes cleanup load over time
- No dedicated background job needed
- Simple for Phase 1 requirements

The cleanup:
- Uses CLICK_RETENTION_DAYS from config (default 30)
- Removes historical data older than retention
- Total click count in urls.clicks is NOT affected
- Keeps storage bounded

For Phase 1 (low volume, self-hosted), this approach is sufficient. Phase 2 would use proper background jobs.

Make sure rand::Rng is imported (should be from short code generation).
```

## Expected Output
- cleanup_old_clicks() function
- Probabilistic execution (1% of redirects)
- Uses config retention days
- Deletes old click_history records
- Does not affect urls.clicks counter
