# Chunk 37: QR Code Download Modal

## Context
Building on analytics. Add QR code display and download functionality.

## Goal
Create modal to display and download QR codes in PNG and SVG formats.

## Prompt

```text
I have analytics visualizations. Now add QR code modal.

Add QR modal HTML after analytics modal:

```html
<!-- QR Code Modal -->
<div id="qr-modal" class="modal" style="display: none;">
    <div class="modal-content modal-small">
        <div class="modal-header">
            <h3>QR Code</h3>
            <button onclick="closeQRModal()" class="modal-close">&times;</button>
        </div>
        <div class="modal-body">
            <div id="qr-preview" class="qr-preview"></div>
            <div class="qr-actions">
                <button onclick="downloadQR('png')" class="btn-primary">
                    Download PNG
                </button>
                <button onclick="downloadQR('svg')" class="btn-secondary">
                    Download SVG
                </button>
            </div>
        </div>
    </div>
</div>
```

Add QR button to URL actions in renderUrls():

```javascript
<button class="btn-action" onclick="showQRCode('${url.short_code}')">
    📱 QR Code
</button>
```

Full updated actions:
```javascript
<div class="url-actions">
    <button class="btn-action" onclick="copyToClipboard('${appConfig.host_url}/${url.short_code}')">
        📋 Copy
    </button>
    <button class="btn-action" onclick="showQRCode('${url.short_code}')">
        📱 QR Code
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
```

Add JavaScript functions:

```javascript
let currentQRCode = null;

function showQRCode(shortCode) {
    currentQRCode = shortCode;
    const modal = document.getElementById('qr-modal');
    const preview = document.getElementById('qr-preview');

    preview.innerHTML = '<div class="loading">Generating QR Code...</div>';
    modal.style.display = 'flex';

    // Load SVG for preview (faster than PNG)
    loadQRPreview(shortCode);
}

async function loadQRPreview(shortCode) {
    const preview = document.getElementById('qr-preview');

    try {
        const response = await authFetch(`/api/urls/${shortCode}/qr/svg`);
        if (response.ok) {
            const svgText = await response.text();
            preview.innerHTML = svgText;
        } else {
            preview.innerHTML = '<div class="empty-state"><p>Failed to generate QR code</p></div>';
        }
    } catch (err) {
        preview.innerHTML = '<div class="empty-state"><p>Network error</p></div>';
    }
}

async function downloadQR(format) {
    if (!currentQRCode) return;

    try {
        const response = await authFetch(`/api/urls/${currentQRCode}/qr/${format}`);

        if (response.ok) {
            const blob = await response.blob();
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = `${currentQRCode}.${format}`;
            document.body.appendChild(a);
            a.click();
            document.body.removeChild(a);
            URL.revokeObjectURL(url);

            showNotification(`QR code downloaded as ${format.toUpperCase()}`, 'success');
        } else {
            showNotification('Failed to download QR code', 'error');
        }
    } catch (err) {
        showNotification('Network error', 'error');
    }
}

function closeQRModal() {
    document.getElementById('qr-modal').style.display = 'none';
    currentQRCode = null;
}

// Close on outside click
document.getElementById('qr-modal').addEventListener('click', (e) => {
    if (e.target.id === 'qr-modal') {
        closeQRModal();
    }
});
```

Add CSS:

```css
.modal-small {
  max-width: 500px;
}

.qr-preview {
  background-color: white;
  padding: var(--spacing-lg);
  border-radius: var(--radius-md);
  margin-bottom: var(--spacing-lg);
  display: flex;
  justify-content: center;
}

.qr-preview svg {
  max-width: 100%;
  height: auto;
}

.qr-actions {
  display: flex;
  gap: var(--spacing-md);
  justify-content: center;
}
```

Features:
- QR button on each URL
- Modal with SVG preview
- White background for QR visibility
- PNG and SVG download buttons
- Downloads trigger browser save dialog
- Notifications on success/failure
```

## Expected Output
- QR Code button on URL cards
- Modal with live QR preview
- Download PNG button
- Download SVG button
- File download via blob URL
- Success notifications
- Clean modal styling
