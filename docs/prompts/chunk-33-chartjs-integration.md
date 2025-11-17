# Chunk 33: Chart.js Integration

## Context
Building on filtering. Need charting library for click analytics visualization.

## Goal
Add Chart.js CDN and prepare for chart rendering.

## Prompt

```text
I have filtering working. Now integrate Chart.js for visualizations.

In dashboard.html, add Chart.js CDN in the <head>:

```html
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Dashboard - RUS</title>
    <link rel="stylesheet" href="/styles.css">
    <script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.0/dist/chart.umd.min.js"></script>
</head>
```

Add a modal structure for analytics (after main content):

```html
<!-- Analytics Modal -->
<div id="analytics-modal" class="modal" style="display: none;">
    <div class="modal-content">
        <div class="modal-header">
            <h3 id="analytics-title">Click Analytics</h3>
            <button onclick="closeAnalyticsModal()" class="modal-close">&times;</button>
        </div>
        <div class="modal-body">
            <div class="chart-tabs">
                <button class="tab-btn active" onclick="showChart('line')">Line Chart</button>
                <button class="tab-btn" onclick="showChart('bar')">Bar Chart</button>
                <button class="tab-btn" onclick="showChart('table')">Table</button>
            </div>
            <div id="chart-container" class="chart-container">
                <canvas id="analytics-chart"></canvas>
            </div>
            <div id="table-container" class="table-container" style="display: none;"></div>
            <div class="analytics-summary">
                <div class="stat">
                    <span class="stat-label">Total Clicks</span>
                    <span id="total-clicks" class="stat-value">0</span>
                </div>
                <div class="stat">
                    <span class="stat-label">Last 7 Days</span>
                    <span id="week-clicks" class="stat-value">0</span>
                </div>
            </div>
        </div>
    </div>
</div>
```

Add CSS for modal and charts:

```css
/* Modal */
.modal {
  position: fixed;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  background-color: rgba(0, 0, 0, 0.8);
  display: flex;
  justify-content: center;
  align-items: center;
  z-index: 3000;
}

.modal-content {
  background-color: var(--rust-dark);
  border-radius: var(--radius-lg);
  border: 1px solid var(--rust-gray);
  width: 90%;
  max-width: 800px;
  max-height: 90vh;
  overflow-y: auto;
}

.modal-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--spacing-lg);
  border-bottom: 1px solid var(--rust-gray);
}

.modal-header h3 {
  margin: 0;
  color: var(--rust-orange);
}

.modal-close {
  background: none;
  border: none;
  color: var(--text-muted);
  font-size: 1.5rem;
  cursor: pointer;
  padding: 0;
}

.modal-close:hover {
  color: var(--text-primary);
}

.modal-body {
  padding: var(--spacing-lg);
}

/* Chart Tabs */
.chart-tabs {
  display: flex;
  gap: var(--spacing-sm);
  margin-bottom: var(--spacing-lg);
}

.tab-btn {
  background-color: var(--rust-gray);
  padding: var(--spacing-xs) var(--spacing-md);
  font-size: 0.9rem;
}

.tab-btn.active {
  background-color: var(--rust-orange);
}

/* Chart Container */
.chart-container {
  background-color: var(--rust-black);
  padding: var(--spacing-md);
  border-radius: var(--radius-md);
  margin-bottom: var(--spacing-lg);
  height: 300px;
}

.table-container {
  max-height: 300px;
  overflow-y: auto;
}

/* Analytics Summary */
.analytics-summary {
  display: flex;
  gap: var(--spacing-lg);
}

.stat {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-xs);
}

.stat-label {
  color: var(--text-muted);
  font-size: 0.9rem;
}

.stat-value {
  font-size: 1.5rem;
  font-weight: 600;
  color: var(--rust-orange);
}
```

Add basic JavaScript for modal:

```javascript
let currentChart = null;
let currentClickData = null;

function closeAnalyticsModal() {
    document.getElementById('analytics-modal').style.display = 'none';
    if (currentChart) {
        currentChart.destroy();
        currentChart = null;
    }
}

// Close modal on outside click
document.getElementById('analytics-modal').addEventListener('click', (e) => {
    if (e.target.id === 'analytics-modal') {
        closeAnalyticsModal();
    }
});
```

This sets up the infrastructure. Actual chart rendering comes in the next chunks.
```

## Expected Output
- Chart.js CDN loaded
- Modal HTML structure
- Tab buttons for chart types
- Chart canvas element
- Modal styling (overlay, content box)
- Tab button states
- Chart container sizing
- Close button and outside click
