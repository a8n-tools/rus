# Chunk 32: Dashboard URL Filtering

## Context
Building on sorting. Add search/filter functionality for URLs.

## Goal
Add filter input to search URLs by name or URL content.

## Prompt

```text
I have sorting working. Now add filtering/search functionality.

Update the urls-header in dashboard.html:

```html
<div class="urls-header">
    <h3>Your Shortened URLs (<span id="url-count">0</span>)</h3>
    <div class="urls-controls">
        <input type="text" id="filter-input" placeholder="Search URLs..." oninput="filterUrls()">
        <select id="sort-select" onchange="applyFiltersAndSort()">
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
```

Update JavaScript:

```javascript
// Filter and sort URLs
function filterUrls() {
    applyFiltersAndSort();
}

function applyFiltersAndSort() {
    const filterText = document.getElementById('filter-input').value.toLowerCase();
    const sortBy = document.getElementById('sort-select').value;

    // Filter
    let filtered = allUrls.filter(url => {
        const name = (url.name || url.short_code).toLowerCase();
        const shortCode = url.short_code.toLowerCase();
        const originalUrl = url.original_url.toLowerCase();

        return name.includes(filterText) ||
               shortCode.includes(filterText) ||
               originalUrl.includes(filterText);
    });

    // Sort
    switch (sortBy) {
        case 'created_desc':
            filtered.sort((a, b) => new Date(b.created_at) - new Date(a.created_at));
            break;
        case 'created_asc':
            filtered.sort((a, b) => new Date(a.created_at) - new Date(b.created_at));
            break;
        case 'clicks_desc':
            filtered.sort((a, b) => b.clicks - a.clicks);
            break;
        case 'clicks_asc':
            filtered.sort((a, b) => a.clicks - b.clicks);
            break;
        case 'name_asc':
            filtered.sort((a, b) => {
                const nameA = (a.name || a.short_code).toLowerCase();
                const nameB = (b.name || b.short_code).toLowerCase();
                return nameA.localeCompare(nameB);
            });
            break;
        case 'name_desc':
            filtered.sort((a, b) => {
                const nameA = (a.name || a.short_code).toLowerCase();
                const nameB = (b.name || b.short_code).toLowerCase();
                return nameB.localeCompare(nameA);
            });
            break;
    }

    // Update count
    document.getElementById('url-count').textContent = filtered.length;

    // Render
    if (filtered.length === 0 && allUrls.length > 0) {
        const list = document.getElementById('urls-list');
        list.innerHTML = '<div class="empty-state"><p>No URLs match your search.</p></div>';
    } else {
        renderUrls(filtered);
    }
}

// Update loadUrls to set count
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
            document.getElementById('url-count').textContent = '0';
        } else {
            document.getElementById('filter-input').value = ''; // Clear filter
            applyFiltersAndSort();
        }
    } catch (err) {
        loading.textContent = 'Failed to load URLs. Please try again.';
    }
}
```

Add to styles.css:

```css
#filter-input {
  width: 200px;
}

@media (max-width: 768px) {
  .urls-controls {
    flex-wrap: wrap;
  }

  #filter-input {
    width: 100%;
    order: -1;
  }
}
```

Features:
- Search input with live filtering
- Searches name, short code, and original URL
- Combined filter + sort
- Shows count of filtered results
- "No results" message when filter matches nothing
- Clears filter on refresh
```

## Expected Output
- Search input in header
- Real-time filtering as you type
- Searches multiple fields
- Combined with sorting
- Shows filtered count
- Empty state for no matches
