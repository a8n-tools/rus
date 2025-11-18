# Chunk 31: Dashboard URL Sorting

## Context
Building on URL actions. Add ability to sort URLs by date, clicks, or name.

## Goal
Add sorting controls and logic for URL list.

## Prompt

```text
I have URL actions working. Now add sorting functionality.

In dashboard.html, update the urls-header section:

```html
<section class="urls-section">
    <div class="urls-header">
        <h3>Your Shortened URLs</h3>
        <div class="urls-controls">
            <select id="sort-select" onchange="sortUrls()">
                <option value="created_desc">Newest First</option>
                <option value="created_asc">Oldest First</option>
                <option value="clicks_desc">Most Clicks</option>
                <option value="clicks_asc">Least Clicks</option>
                <option value="name_asc">Name A-Z</option>
                <option value="name_desc">Name Z-A</option>
            </select>
            <button onclick="loadUrls()" class="btn-secondary btn-sm">
                Refresh
            </button>
        </div>
    </div>
    <!-- rest remains same -->
</section>
```

Add to the script section:

```javascript
// Store URLs globally for sorting
let allUrls = [];

// Update loadUrls to store data
async function loadUrls() {
    const loading = document.getElementById('urls-loading');
    const empty = document.getElementById('urls-empty');
    const list = document.getElementById('urls-list');

    loading.style.display = 'block';
    empty.style.display = 'none';
    list.innerHTML = '';

    try {
        const response = await authFetch('/api/urls');
        allUrls = await response.json();

        loading.style.display = 'none';

        if (allUrls.length === 0) {
            empty.style.display = 'block';
        } else {
            sortUrls(); // Apply current sort
        }
    } catch (err) {
        loading.textContent = 'Failed to load URLs. Please try again.';
    }
}

// Sort URLs based on selected option
function sortUrls() {
    const sortBy = document.getElementById('sort-select').value;
    let sorted = [...allUrls];

    switch (sortBy) {
        case 'created_desc':
            sorted.sort((a, b) => new Date(b.created_at) - new Date(a.created_at));
            break;
        case 'created_asc':
            sorted.sort((a, b) => new Date(a.created_at) - new Date(b.created_at));
            break;
        case 'clicks_desc':
            sorted.sort((a, b) => b.clicks - a.clicks);
            break;
        case 'clicks_asc':
            sorted.sort((a, b) => a.clicks - b.clicks);
            break;
        case 'name_asc':
            sorted.sort((a, b) => {
                const nameA = (a.name || a.short_code).toLowerCase();
                const nameB = (b.name || b.short_code).toLowerCase();
                return nameA.localeCompare(nameB);
            });
            break;
        case 'name_desc':
            sorted.sort((a, b) => {
                const nameA = (a.name || a.short_code).toLowerCase();
                const nameB = (b.name || b.short_code).toLowerCase();
                return nameB.localeCompare(nameA);
            });
            break;
    }

    renderUrls(sorted);
}
```

Add to styles.css:

```css
.urls-controls {
  display: flex;
  gap: var(--spacing-sm);
  align-items: center;
}

select {
  font-family: inherit;
  font-size: 0.9rem;
  padding: var(--spacing-xs) var(--spacing-sm);
  border: 1px solid var(--rust-gray);
  border-radius: var(--radius-md);
  background-color: var(--rust-dark);
  color: var(--text-primary);
  cursor: pointer;
}

select:focus {
  outline: none;
  border-color: var(--rust-orange);
}
```

Features:
- Dropdown with 6 sort options
- Client-side sorting (no API calls)
- Sorts on selection change
- Default: Newest First
- URLs stored globally for re-sorting
```

## Expected Output
- Sort dropdown in header
- 6 sorting options
- Client-side sorting logic
- URLs stored for quick re-sorting
- Maintains sort on refresh
- Select input styled
