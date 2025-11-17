# Chunk 35: Line Chart Visualization

## Context
Building on analytics modal. Render clicks over time as line chart.

## Goal
Create line chart showing daily click trends.

## Prompt

```text
I have analytics data fetching. Now render line chart.

Add renderChart() function for line chart:

```javascript
function renderChart(type) {
    if (!currentClickData) return;

    // Destroy existing chart
    if (currentChart) {
        currentChart.destroy();
    }

    const aggregated = aggregateByDay(currentClickData.history);
    const labels = Object.keys(aggregated);
    const data = Object.values(aggregated);

    const ctx = document.getElementById('analytics-chart').getContext('2d');

    const chartConfig = {
        type: type === 'line' ? 'line' : 'bar',
        data: {
            labels: labels.map(date => {
                const d = new Date(date);
                return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
            }),
            datasets: [{
                label: 'Clicks',
                data: data,
                borderColor: '#CE422B',
                backgroundColor: type === 'line' ? 'rgba(206, 66, 43, 0.1)' : 'rgba(206, 66, 43, 0.8)',
                borderWidth: 2,
                fill: type === 'line',
                tension: 0.3,
                pointBackgroundColor: '#CE422B',
                pointBorderColor: '#CE422B',
                pointRadius: 4,
                pointHoverRadius: 6
            }]
        },
        options: {
            responsive: true,
            maintainAspectRatio: false,
            plugins: {
                legend: {
                    display: false
                },
                tooltip: {
                    backgroundColor: '#1A1A1A',
                    titleColor: '#FFFFFF',
                    bodyColor: '#CCCCCC',
                    borderColor: '#CE422B',
                    borderWidth: 1,
                    padding: 12,
                    displayColors: false,
                    callbacks: {
                        title: function(context) {
                            const index = context[0].dataIndex;
                            const date = new Date(labels[index]);
                            return date.toLocaleDateString('en-US', {
                                weekday: 'long',
                                year: 'numeric',
                                month: 'long',
                                day: 'numeric'
                            });
                        },
                        label: function(context) {
                            return `${context.parsed.y} click${context.parsed.y !== 1 ? 's' : ''}`;
                        }
                    }
                }
            },
            scales: {
                x: {
                    grid: {
                        color: 'rgba(255, 255, 255, 0.1)'
                    },
                    ticks: {
                        color: '#888888',
                        maxRotation: 45,
                        minRotation: 45
                    }
                },
                y: {
                    beginAtZero: true,
                    grid: {
                        color: 'rgba(255, 255, 255, 0.1)'
                    },
                    ticks: {
                        color: '#888888',
                        stepSize: 1,
                        callback: function(value) {
                            if (Math.floor(value) === value) {
                                return value;
                            }
                        }
                    }
                }
            },
            interaction: {
                intersect: false,
                mode: 'index'
            }
        }
    };

    currentChart = new Chart(ctx, chartConfig);
}
```

Chart features:
- Shows last 30 days of activity
- Rust orange color (#CE422B)
- Smooth line with tension
- Filled area under line
- Custom tooltips with full date
- Responsive sizing
- Integer-only Y axis (can't have 0.5 clicks)
- Rotated X-axis labels for readability
- Dark theme matching app

The same function handles both line and bar by changing type parameter.
```

## Expected Output
- Line chart rendering
- 30-day view
- Rust orange theming
- Custom tooltips
- Responsive sizing
- Dark theme grid lines
- Integer Y-axis ticks
- Smooth curve with fill
