# Chunk 36: Table Visualization

## Context
Building on line chart. Add table view for detailed click history.

## Goal
Render click history as a scrollable table.

## Prompt

```text
I have line chart rendering. Now add table view.

Add renderTable() function:

```javascript
function renderTable() {
    if (!currentClickData) return;

    const container = document.getElementById('table-container');

    if (currentClickData.history.length === 0) {
        container.innerHTML = '<div class="empty-state"><p>No click history available</p></div>';
        return;
    }

    let html = `
        <table class="analytics-table">
            <thead>
                <tr>
                    <th>#</th>
                    <th>Date & Time</th>
                    <th>Relative Time</th>
                </tr>
            </thead>
            <tbody>
    `;

    currentClickData.history.forEach((click, index) => {
        const date = new Date(click.clicked_at.replace(' ', 'T'));
        const formattedDate = date.toLocaleDateString('en-US', {
            year: 'numeric',
            month: 'short',
            day: 'numeric',
            hour: '2-digit',
            minute: '2-digit'
        });
        const relativeTime = getRelativeTime(date);

        html += `
            <tr>
                <td>${index + 1}</td>
                <td>${formattedDate}</td>
                <td>${relativeTime}</td>
            </tr>
        `;
    });

    html += '</tbody></table>';
    container.innerHTML = html;
}

// Get relative time string
function getRelativeTime(date) {
    const now = new Date();
    const diff = now - date;
    const seconds = Math.floor(diff / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (seconds < 60) {
        return 'Just now';
    } else if (minutes < 60) {
        return `${minutes} minute${minutes !== 1 ? 's' : ''} ago`;
    } else if (hours < 24) {
        return `${hours} hour${hours !== 1 ? 's' : ''} ago`;
    } else if (days < 30) {
        return `${days} day${days !== 1 ? 's' : ''} ago`;
    } else {
        return date.toLocaleDateString();
    }
}
```

Add table styles to styles.css:

```css
/* Analytics Table */
.analytics-table {
  width: 100%;
  border-collapse: collapse;
}

.analytics-table th,
.analytics-table td {
  padding: var(--spacing-sm) var(--spacing-md);
  text-align: left;
  border-bottom: 1px solid var(--rust-gray);
}

.analytics-table th {
  background-color: var(--rust-gray);
  color: var(--text-primary);
  font-weight: 600;
  position: sticky;
  top: 0;
}

.analytics-table tbody tr:hover {
  background-color: rgba(206, 66, 43, 0.1);
}

.analytics-table td {
  color: var(--text-secondary);
}

.analytics-table td:first-child {
  color: var(--text-muted);
  width: 50px;
}
```

Table features:
- Shows individual click timestamps
- Row number for easy counting
- Human-readable dates
- Relative time ("5 minutes ago")
- Scrollable within container
- Sticky header
- Hover highlight
- Limited to recent 1000 (from API)

This completes the three visualization types: line chart, bar chart (using same renderChart function), and table.
```

## Expected Output
- renderTable() function
- Table with headers and rows
- Row numbers
- Formatted timestamps
- Relative time column
- getRelativeTime() helper
- Styled with dark theme
- Sticky header in scroll
- Hover effect on rows
