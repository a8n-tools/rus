# Chunk 28: Dashboard Base Structure

## Context
Building on auth.js updates. Redesign the dashboard layout with Rust theme.

## Goal
Create the dashboard HTML structure with navbar, URL form, and URL list container.

## Prompt

```text
I have auth.js with refresh tokens. Now redesign dashboard structure.

Replace static/dashboard.html with the base structure:

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Dashboard - RUS</title>
    <link rel="stylesheet" href="/styles.css">
</head>
<body>
    <nav class="navbar">
        <div class="navbar-container">
            <a href="/" class="navbar-brand">
                <span class="logo">🦀</span> RUS
            </a>
            <div class="navbar-user">
                <span class="navbar-username">Welcome, <span id="username-display"></span></span>
                <button onclick="logout()" class="btn-logout">Logout</button>
            </div>
        </div>
    </nav>

    <main class="container">
        <section class="dashboard-header">
            <h1>Your URLs</h1>
        </section>

        <section class="url-form-section">
            <div class="card">
                <h3>Shorten a URL</h3>
                <form id="shorten-form" class="shorten-form">
                    <div class="form-row">
                        <input type="url" id="url-input" placeholder="https://example.com/long-url" required>
                        <button type="submit" id="shorten-btn">Shorten</button>
                    </div>
                    <small id="url-length-hint" class="form-hint"></small>
                </form>
                <div id="shorten-result" class="shorten-result" style="display: none;"></div>
                <div id="shorten-error" class="alert alert-error" style="display: none;"></div>
            </div>
        </section>

        <section class="urls-section">
            <div class="urls-header">
                <h3>Your Shortened URLs</h3>
                <button onclick="loadUrls()" class="btn-secondary btn-sm">
                    Refresh
                </button>
            </div>

            <div id="urls-loading" class="loading">Loading your URLs...</div>
            <div id="urls-empty" class="empty-state" style="display: none;">
                <p>You haven't shortened any URLs yet.</p>
                <p>Enter a URL above to get started!</p>
            </div>
            <div id="urls-list" class="urls-list"></div>
        </section>
    </main>

    <script src="/auth.js"></script>
    <script>
        // Check authentication
        if (!isAuthenticated()) {
            window.location.href = '/login.html';
        }

        // Display username
        document.getElementById('username-display').textContent = getUsername();

        // Config will be fetched on load
        let appConfig = { host_url: '', max_url_length: 2048 };

        // Initialize dashboard
        async function init() {
            await fetchConfig();
            await loadUrls();
        }

        // Fetch app config
        async function fetchConfig() {
            try {
                const response = await fetch('/api/config');
                if (response.ok) {
                    appConfig = await response.json();
                }
            } catch (err) {
                console.error('Failed to fetch config:', err);
            }
        }

        // Load user URLs
        async function loadUrls() {
            const loading = document.getElementById('urls-loading');
            const empty = document.getElementById('urls-empty');
            const list = document.getElementById('urls-list');

            loading.style.display = 'block';
            empty.style.display = 'none';
            list.innerHTML = '';

            try {
                const response = await authFetch('/api/urls');
                const urls = await response.json();

                loading.style.display = 'none';

                if (urls.length === 0) {
                    empty.style.display = 'block';
                } else {
                    renderUrls(urls);
                }
            } catch (err) {
                loading.textContent = 'Failed to load URLs. Please try again.';
            }
        }

        // Render URLs (placeholder - will be enhanced)
        function renderUrls(urls) {
            const list = document.getElementById('urls-list');
            urls.forEach(url => {
                const card = document.createElement('div');
                card.className = 'url-card';
                card.innerHTML = `
                    <div class="url-card-header">
                        <span class="url-name">${url.name || url.short_code}</span>
                        <span class="url-clicks">${url.clicks} clicks</span>
                    </div>
                    <div class="url-short">
                        <a href="${appConfig.host_url}/${url.short_code}" target="_blank">
                            ${appConfig.host_url}/${url.short_code}
                        </a>
                    </div>
                    <div class="url-original">${url.original_url}</div>
                    <div class="url-created">Created: ${new Date(url.created_at).toLocaleDateString()}</div>
                `;
                list.appendChild(card);
            });
        }

        // Handle URL shortening
        document.getElementById('shorten-form').addEventListener('submit', async (e) => {
            e.preventDefault();

            const urlInput = document.getElementById('url-input');
            const resultDiv = document.getElementById('shorten-result');
            const errorDiv = document.getElementById('shorten-error');
            const btn = document.getElementById('shorten-btn');

            resultDiv.style.display = 'none';
            errorDiv.style.display = 'none';

            btn.disabled = true;
            btn.textContent = 'Shortening...';

            try {
                const response = await authFetch('/api/shorten', {
                    method: 'POST',
                    body: JSON.stringify({ url: urlInput.value })
                });

                const data = await response.json();

                if (response.ok) {
                    resultDiv.innerHTML = `
                        <strong>Shortened!</strong><br>
                        <a href="${data.short_url}" target="_blank">${data.short_url}</a>
                    `;
                    resultDiv.style.display = 'block';
                    urlInput.value = '';
                    loadUrls(); // Refresh list
                } else {
                    errorDiv.textContent = data.error || 'Failed to shorten URL';
                    errorDiv.style.display = 'block';
                }
            } catch (err) {
                errorDiv.textContent = 'Network error. Please try again.';
                errorDiv.style.display = 'block';
            } finally {
                btn.disabled = false;
                btn.textContent = 'Shorten';
            }
        });

        // Initialize on load
        init();
    </script>
</body>
</html>
```

This is the base structure. Styling and advanced features come in subsequent chunks.
```

## Expected Output
- Navbar with username and logout
- URL shortening form
- URL list container
- Loading and empty states
- Uses authFetch for API calls
- Fetches config for HOST_URL
- Basic URL rendering
