// Authentication helper functions

// LocalStorage keys
const TOKEN_KEY = 'rus_token';
const REFRESH_TOKEN_KEY = 'rus_refresh_token';
const USERNAME_KEY = 'rus_username';

// Save authentication data
function saveAuth(token, username, refreshToken) {
    localStorage.setItem(TOKEN_KEY, token);
    localStorage.setItem(USERNAME_KEY, username);
    if (refreshToken) {
        localStorage.setItem(REFRESH_TOKEN_KEY, refreshToken);
    }
}

// Get token
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

// Logout (clear auth data)
function logout() {
    localStorage.removeItem(TOKEN_KEY);
    localStorage.removeItem(REFRESH_TOKEN_KEY);
    localStorage.removeItem(USERNAME_KEY);
}

// Check if user is authenticated
function isAuthenticated() {
    return !!getToken();
}

// Attempt to refresh the auth token using the stored refresh token
async function refreshAuthToken() {
    const refreshToken = getRefreshToken();
    if (!refreshToken) {
        return null;
    }

    try {
        const response = await fetch('/api/refresh', {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({ refresh_token: refreshToken }),
        });

        if (!response.ok) {
            return null;
        }

        const data = await response.json();
        // Save the new tokens (preserve existing username)
        saveAuth(data.token, getUsername(), data.refresh_token);
        return data.token;
    } catch {
        return null;
    }
}

// Wrapper for authenticated fetch that handles token refresh on 401
async function authenticatedFetch(url, options = {}) {
    const token = getToken();
    if (!token) {
        return null;
    }

    // Set Authorization header
    options.headers = options.headers || {};
    options.headers['Authorization'] = `Bearer ${token}`;

    let response = await fetch(url, options);

    // If 401, try refreshing the token
    if (response.status === 401) {
        const newToken = await refreshAuthToken();
        if (newToken) {
            options.headers['Authorization'] = `Bearer ${newToken}`;
            response = await fetch(url, options);
        }
    }

    return response;
}
