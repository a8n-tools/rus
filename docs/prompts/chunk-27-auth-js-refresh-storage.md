# Chunk 27: Auth.js Refresh Token Storage

## Context
Building on signup page. Need to update auth.js to handle refresh tokens.

## Goal
Update auth helper functions to store and retrieve refresh tokens.

## Prompt

```text
I have redesigned auth pages. Now update auth.js for refresh token support.

Replace static/auth.js entirely:

```javascript
// Auth helper functions for RUS

const TOKEN_KEY = 'rus_token';
const REFRESH_TOKEN_KEY = 'rus_refresh_token';
const USERNAME_KEY = 'rus_username';

// Save authentication data
function saveAuth(token, refreshToken, username) {
    localStorage.setItem(TOKEN_KEY, token);
    localStorage.setItem(REFRESH_TOKEN_KEY, refreshToken);
    localStorage.setItem(USERNAME_KEY, username);
}

// Get access token
function getToken() {
    return localStorage.getItem(TOKEN_KEY);
}

// Get refresh token
function getRefreshToken() {
    return localStorage.getItem(REFRESH_TOKEN_KEY);
}

// Get username
function getUsername() {
    return localStorage.getItem(USERNAME_KEY);
}

// Check if user is authenticated
function isAuthenticated() {
    return !!getToken();
}

// Clear all auth data
function logout() {
    localStorage.removeItem(TOKEN_KEY);
    localStorage.removeItem(REFRESH_TOKEN_KEY);
    localStorage.removeItem(USERNAME_KEY);
    window.location.href = '/login.html';
}

// Create authorization header
function getAuthHeader() {
    const token = getToken();
    return token ? { 'Authorization': `Bearer ${token}` } : {};
}

// Make authenticated API request with automatic token refresh
async function authFetch(url, options = {}) {
    const headers = {
        'Content-Type': 'application/json',
        ...getAuthHeader(),
        ...options.headers
    };

    let response = await fetch(url, { ...options, headers });

    // If unauthorized, try to refresh token
    if (response.status === 401) {
        const refreshed = await refreshAccessToken();
        if (refreshed) {
            // Retry with new token
            headers.Authorization = `Bearer ${getToken()}`;
            response = await fetch(url, { ...options, headers });
        } else {
            // Refresh failed, logout
            logout();
            throw new Error('Session expired. Please login again.');
        }
    }

    return response;
}

// Refresh the access token using refresh token
async function refreshAccessToken() {
    const refreshToken = getRefreshToken();
    if (!refreshToken) {
        return false;
    }

    try {
        const response = await fetch('/api/refresh', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ refresh_token: refreshToken })
        });

        if (response.ok) {
            const data = await response.json();
            // Update tokens (keep username)
            localStorage.setItem(TOKEN_KEY, data.token);
            localStorage.setItem(REFRESH_TOKEN_KEY, data.refresh_token);
            return true;
        }
    } catch (err) {
        console.error('Token refresh failed:', err);
    }

    return false;
}
```

Key additions:
1. REFRESH_TOKEN_KEY constant
2. saveAuth() now takes refreshToken parameter
3. getRefreshToken() to retrieve it
4. authFetch() wrapper that auto-refreshes on 401
5. refreshAccessToken() calls /api/refresh endpoint

The authFetch() function:
- Makes authenticated requests
- Automatically adds Authorization header
- On 401, tries to refresh token
- If refresh succeeds, retries original request
- If refresh fails, logs out user

This is the foundation. The dashboard will use authFetch() for all API calls.
```

## Expected Output
- saveAuth() stores 3 values
- getRefreshToken() retrieves it
- authFetch() auto-refreshes on 401
- refreshAccessToken() calls API
- Token rotation handled
- Logout clears all auth data
