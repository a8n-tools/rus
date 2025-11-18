# Chunk 34: Click History Analytics Modal

## Context
Building on Chart.js setup. Need to fetch and display click history.

## Goal
Add button to open analytics modal and fetch click history data.

## Prompt

```text
I have Chart.js and modal structure. Now fetch click history data.

Update renderUrls() to add analytics button:

```javascript
function renderUrls(urls) {
    const list = document.getElementById('urls-list');
    list.innerHTML = '';

    urls.forEach(url => {
        const card = document.createElement('div');
        card.className = 'url-card';
        card.id = `url-${url.short_code}`;
        card.innerHTML = `
            <div class="url-card-header">
                <span class="url-name" id="name-${url.short_code}">${url.name || url.short_code}</span>
                <span class="url-clicks">${url.clicks} clicks</span>
            </div>
            <div class="url-short">
                <a href="${appConfig.host_url}/${url.short_code}" target="_blank">
                    ${appConfig.host_url}/${url.short_code}
                </a>
            </div>
            <div class="url-original">${url.original_url}</div>
            <div class="url-created">Created: ${new Date(url.created_at).toLocaleDateString()}</div>
            <div class="url-actions">
                <button class="btn-action" onclick="copyToClipboard('${appConfig.host_url}/${url.short_code}')">
                    📋 Copy
                </button>
                <button class="btn-action" onclick="showAnalytics('${url.short_code}', '${url.name || url.short_code}')">
                    📊 Analytics
                </button>
                <button class="btn-action" onclick="showRenameModal('${url.short_code}', '${url.name || ''}')">
                    ✏️ Rename
                </button>
                <button class="btn-action btn-danger" onclick="deleteUrl('${url.short_code}')">
                    🗑️ Delete
                </button>
            </div>
        `;
        list.appendChild(card);
    });
}
```

Add analytics functions:

```javascript
// Show analytics modal
async function showAnalytics(shortCode, name) {
    const modal = document.getElementById('analytics-modal');
    const title = document.getElementById('analytics-title');
    const chartContainer = document.getElementById('chart-container');
    const tableContainer = document.getElementById('table-container');

    title.textContent = `Click Analytics - ${name}`;
    modal.style.display = 'flex';

    // Reset tabs
    document.querySelectorAll('.tab-btn').forEach(btn => btn.classList.remove('active'));
    document.querySelector('.tab-btn').classList.add('active');
    chartContainer.style.display = 'block';
    tableContainer.style.display = 'none';

    // Show loading
    chartContainer.innerHTML = '<div class="loading">Loading analytics...</div>';
    document.getElementById('total-clicks').textContent = '...';
    document.getElementById('week-clicks').textContent = '...';

    try {
        const response = await authFetch(`/api/urls/${shortCode}/clicks`);
        const data = await response.json();

        currentClickData = data;

        // Update summary
        document.getElementById('total-clicks').textContent = data.total_clicks;

        // Calculate last 7 days clicks
        const weekAgo = new Date();
        weekAgo.setDate(weekAgo.getDate() - 7);
        const weekClicks = data.history.filter(h => new Date(h.clicked_at) >= weekAgo).length;
        document.getElementById('week-clicks').textContent = weekClicks;

        // Restore canvas
        chartContainer.innerHTML = '<canvas id="analytics-chart"></canvas>';

        // Show line chart by default
        showChart('line');
    } catch (err) {
        chartContainer.innerHTML = '<div class="empty-state"><p>Failed to load analytics</p></div>';
    }
}

// Switch between chart types
function showChart(type) {
    // Update active tab
    document.querySelectorAll('.tab-btn').forEach(btn => {
        btn.classList.remove('active');
        if (btn.textContent.toLowerCase().includes(type)) {
            btn.classList.add('active');
        }
    });

    const chartContainer = document.getElementById('chart-container');
    const tableContainer = document.getElementById('table-container');

    if (type === 'table') {
        chartContainer.style.display = 'none';
        tableContainer.style.display = 'block';
        renderTable();
    } else {
        chartContainer.style.display = 'block';
        tableContainer.style.display = 'none';
        renderChart(type);
    }
}

// Aggregate clicks by day
function aggregateByDay(history) {
    const counts = {};
    const now = new Date();

    // Initialize last 30 days
    for (let i = 29; i >= 0; i--) {
        const date = new Date(now);
        date.setDate(date.getDate() - i);
        const key = date.toISOString().split('T')[0];
        counts[key] = 0;
    }

    // Count clicks
    history.forEach(h => {
        const date = h.clicked_at.split(' ')[0]; // YYYY-MM-DD
        if (counts.hasOwnProperty(date)) {
            counts[date]++;
        }
    });

    return counts;
}
```

This fetches the data and sets up for chart rendering. Actual charts come next.
```

## Expected Output
- Analytics button on each URL card
- showAnalytics() fetches click history
- Stores data for chart rendering
- Updates summary stats (total, 7-day)
- Tab switching logic
- aggregateByDay() helper
- Loading state in modal
