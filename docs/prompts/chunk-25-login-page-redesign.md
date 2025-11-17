# Chunk 25: Login Page Redesign

## Context
Building on landing page. Redesign login page with Rust theme and consistent navbar.

## Goal
Update login.html with themed form and better error handling.

## Prompt

```text
I have the landing page redesigned. Now update the login page.

Replace static/login.html:

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Login - RUS</title>
    <link rel="stylesheet" href="/styles.css">
</head>
<body>
    <nav class="navbar">
        <div class="navbar-container">
            <a href="/" class="navbar-brand">
                <span class="logo">🦀</span> RUS
            </a>
            <ul class="navbar-nav">
                <li><a href="/signup.html">Sign Up</a></li>
            </ul>
        </div>
    </nav>

    <main class="container">
        <div class="auth-container">
            <div class="auth-card">
                <h2>Welcome Back</h2>
                <p class="auth-subtitle">Sign in to your account</p>

                <div id="error-message" class="alert alert-error" style="display: none;"></div>
                <div id="lockout-message" class="alert alert-warning" style="display: none;"></div>

                <form id="login-form" class="auth-form">
                    <div class="form-group">
                        <label for="username">Username</label>
                        <input type="text" id="username" name="username" required autocomplete="username">
                    </div>

                    <div class="form-group">
                        <label for="password">Password</label>
                        <input type="password" id="password" name="password" required autocomplete="current-password">
                    </div>

                    <button type="submit" id="submit-btn">Sign In</button>
                </form>

                <p class="auth-footer">
                    Don't have an account? <a href="/signup.html">Sign up</a>
                </p>
            </div>
        </div>
    </main>

    <script src="/auth.js"></script>
    <script>
        if (isAuthenticated()) {
            window.location.href = '/dashboard.html';
        }

        document.getElementById('login-form').addEventListener('submit', async (e) => {
            e.preventDefault();

            const errorDiv = document.getElementById('error-message');
            const lockoutDiv = document.getElementById('lockout-message');
            const submitBtn = document.getElementById('submit-btn');

            errorDiv.style.display = 'none';
            lockoutDiv.style.display = 'none';
            submitBtn.disabled = true;
            submitBtn.textContent = 'Signing in...';

            const username = document.getElementById('username').value;
            const password = document.getElementById('password').value;

            try {
                const response = await fetch('/api/login', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ username, password })
                });

                const data = await response.json();

                if (response.ok) {
                    saveAuth(data.token, data.refresh_token, data.username);
                    window.location.href = '/dashboard.html';
                } else if (response.status === 429) {
                    lockoutDiv.textContent = data.error;
                    lockoutDiv.style.display = 'block';
                } else {
                    errorDiv.textContent = data.error || 'Login failed';
                    errorDiv.style.display = 'block';
                }
            } catch (err) {
                errorDiv.textContent = 'Network error. Please try again.';
                errorDiv.style.display = 'block';
            } finally {
                submitBtn.disabled = false;
                submitBtn.textContent = 'Sign In';
            }
        });
    </script>
</body>
</html>
```

Add auth form styles to styles.css:

```css
/* Auth Forms */
.auth-container {
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: calc(100vh - 200px);
}

.auth-card {
  background-color: var(--rust-dark);
  padding: var(--spacing-xl);
  border-radius: var(--radius-lg);
  border: 1px solid var(--rust-gray);
  width: 100%;
  max-width: 400px;
}

.auth-card h2 {
  text-align: center;
  margin-bottom: var(--spacing-xs);
}

.auth-subtitle {
  text-align: center;
  color: var(--text-muted);
  margin-bottom: var(--spacing-lg);
}

.auth-form {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-xs);
}

.form-group label {
  font-weight: 500;
  color: var(--text-secondary);
}

.auth-form button {
  margin-top: var(--spacing-sm);
}

.auth-footer {
  text-align: center;
  margin-top: var(--spacing-lg);
  color: var(--text-muted);
}

/* Alerts */
.alert {
  padding: var(--spacing-sm) var(--spacing-md);
  border-radius: var(--radius-md);
  margin-bottom: var(--spacing-md);
}

.alert-error {
  background-color: rgba(239, 68, 68, 0.2);
  border: 1px solid var(--error);
  color: var(--error);
}

.alert-warning {
  background-color: rgba(245, 158, 11, 0.2);
  border: 1px solid var(--warning);
  color: var(--warning);
}

.alert-success {
  background-color: rgba(16, 185, 129, 0.2);
  border: 1px solid var(--success);
  color: var(--success);
}
```
```

## Expected Output
- Consistent navbar
- Centered auth card
- Form with labels
- Error and lockout alerts
- Handles refresh token storage
- HTTP 429 handling for lockout
- Loading state on button
