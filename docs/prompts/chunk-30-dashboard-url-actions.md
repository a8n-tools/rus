# Chunk 30: Dashboard URL Actions (Copy, Delete, Rename)

## Context
Building on URL card styling. Add action buttons for each URL.

## Goal
Enhance URL cards with copy, delete, rename, and view analytics buttons.

## Prompt

```text
I have URL card styling. Now add action buttons and functionality.

Update the renderUrls() function in dashboard.html:

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

// Copy URL to clipboard
async function copyToClipboard(text) {
    try {
        await navigator.clipboard.writeText(text);
        showNotification('URL copied to clipboard!', 'success');
    } catch (err) {
        showNotification('Failed to copy URL', 'error');
    }
}

// Delete URL
async function deleteUrl(shortCode) {
    if (!confirm('Are you sure you want to delete this URL? This cannot be undone.')) {
        return;
    }

    try {
        const response = await authFetch(`/api/urls/${shortCode}`, {
            method: 'DELETE'
        });

        if (response.ok) {
            showNotification('URL deleted successfully', 'success');
            // Remove card from DOM
            const card = document.getElementById(`url-${shortCode}`);
            if (card) {
                card.remove();
            }
            // Check if list is now empty
            if (document.getElementById('urls-list').children.length === 0) {
                document.getElementById('urls-empty').style.display = 'block';
            }
        } else {
            const data = await response.json();
            showNotification(data.error || 'Failed to delete URL', 'error');
        }
    } catch (err) {
        showNotification('Network error', 'error');
    }
}

// Show rename modal
function showRenameModal(shortCode, currentName) {
    const newName = prompt('Enter new name for this URL:', currentName);
    if (newName !== null) {
        renameUrl(shortCode, newName);
    }
}

// Rename URL
async function renameUrl(shortCode, newName) {
    try {
        const response = await authFetch(`/api/urls/${shortCode}/name`, {
            method: 'PATCH',
            body: JSON.stringify({ name: newName || null })
        });

        if (response.ok) {
            showNotification('URL renamed successfully', 'success');
            // Update name in DOM
            const nameEl = document.getElementById(`name-${shortCode}`);
            if (nameEl) {
                nameEl.textContent = newName || shortCode;
            }
        } else {
            const data = await response.json();
            showNotification(data.error || 'Failed to rename URL', 'error');
        }
    } catch (err) {
        showNotification('Network error', 'error');
    }
}

// Show notification
function showNotification(message, type = 'info') {
    const notification = document.createElement('div');
    notification.className = `notification notification-${type}`;
    notification.textContent = message;
    document.body.appendChild(notification);

    // Fade in
    setTimeout(() => notification.classList.add('show'), 10);

    // Remove after 3 seconds
    setTimeout(() => {
        notification.classList.remove('show');
        setTimeout(() => notification.remove(), 300);
    }, 3000);
}
```

Add to styles.css:

```css
/* URL Actions */
.url-actions {
  display: flex;
  gap: var(--spacing-sm);
  margin-top: var(--spacing-md);
  padding-top: var(--spacing-md);
  border-top: 1px solid var(--rust-gray);
}

.btn-action {
  background-color: var(--rust-gray);
  padding: var(--spacing-xs) var(--spacing-sm);
  font-size: 0.85rem;
}

.btn-action:hover {
  background-color: var(--rust-light-gray);
}

.btn-danger {
  background-color: var(--error);
}

.btn-danger:hover {
  background-color: #DC2626;
}

/* Notifications */
.notification {
  position: fixed;
  bottom: var(--spacing-lg);
  right: var(--spacing-lg);
  padding: var(--spacing-md) var(--spacing-lg);
  border-radius: var(--radius-md);
  color: var(--text-primary);
  font-weight: 500;
  transform: translateY(100px);
  opacity: 0;
  transition: all var(--transition-normal);
  z-index: 2000;
}

.notification.show {
  transform: translateY(0);
  opacity: 1;
}

.notification-success {
  background-color: var(--success);
}

.notification-error {
  background-color: var(--error);
}

.notification-info {
  background-color: var(--rust-orange);
}
```
```

## Expected Output
- Action buttons row on each card
- Copy to clipboard functionality
- Delete with confirmation
- Rename with prompt
- Toast notifications
- Real-time DOM updates
- Error handling
