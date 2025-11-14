// Authentication helper functions

// LocalStorage keys
const TOKEN_KEY = 'rus_token';
const USERNAME_KEY = 'rus_username';

// Save authentication data
function saveAuth(token, username) {
    localStorage.setItem(TOKEN_KEY, token);
    localStorage.setItem(USERNAME_KEY, username);
}

// Get token
function getToken() {
    return localStorage.getItem(TOKEN_KEY);
}

// Get username
function getUsername() {
    return localStorage.getItem(USERNAME_KEY);
}

// Logout (clear auth data)
function logout() {
    localStorage.removeItem(TOKEN_KEY);
    localStorage.removeItem(USERNAME_KEY);
}

// Check if user is authenticated
function isAuthenticated() {
    return !!getToken();
}
