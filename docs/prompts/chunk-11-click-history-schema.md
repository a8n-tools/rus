# Chunk 11: Click History Database Schema

## Context
Building on token refresh. We need to track individual clicks with timestamps for analytics.

## Goal
Create the click_history table to store each click event.

## Prompt

```text
I have refresh token mechanism working. Now add click history tracking schema.

In AppState::new() execute_batch(), add the click_history table:

```sql
CREATE TABLE IF NOT EXISTS click_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url_id INTEGER NOT NULL,
    clicked_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (url_id) REFERENCES urls(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_click_history_url_id ON click_history(url_id);
CREATE INDEX IF NOT EXISTS idx_click_history_clicked_at ON click_history(clicked_at);
```

Add data structures for click history API:

```rust
#[derive(Serialize)]
struct ClickHistoryEntry {
    clicked_at: String,
}

#[derive(Serialize)]
struct ClickStats {
    total_clicks: u64,
    history: Vec<ClickHistoryEntry>,
}
```

The table structure:
- id: Auto-increment primary key
- url_id: Links to urls table (ON DELETE CASCADE removes history when URL deleted)
- clicked_at: Timestamp of the click (defaults to now)

Indexes for performance:
- idx_click_history_url_id: Fast lookup of clicks for a specific URL
- idx_click_history_clicked_at: Fast cleanup of old records, time-based queries

The ON DELETE CASCADE ensures:
- When a URL is deleted, all its click history is automatically removed
- No orphaned click records

This is just the schema - recording clicks comes next.
```

## Expected Output
- click_history table created on startup
- ClickHistoryEntry struct
- ClickStats struct
- ON DELETE CASCADE for data integrity
- Indexes for query performance
