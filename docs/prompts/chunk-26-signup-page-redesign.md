# Chunk 26: Signup Page Redesign

## Context
Building on login page. Redesign signup with password requirements hints.

## Goal
Update signup.html with themed form and password validation feedback.

## Prompt

```text
I have login page redesigned. Now update signup page.

Replace static/signup.html:

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Sign Up - RUS</title>
    <link rel="stylesheet" href="/styles.css">
</head>
<body>
    <nav class="navbar">
        <div class="navbar-container">
            <a href="/" class="navbar-brand">
                <span class="logo">🦀</span> RUS
            </a>
            <ul class="navbar-nav">
                <li><a href="/login.html">Login</a></li>
            </ul>
        </div>
    </nav>

    <main class="container">
        <div class="auth-container">
            <div class="auth-card">
                <h2>Create Account</h2>
                <p class="auth-subtitle">Start shortening URLs today</p>

                <div id="error-message" class="alert alert-error" style="display: none;"></div>
                <div id="success-message" class="alert alert-success" style="display: none;"></div>

                <form id="signup-form" class="auth-form">
                    <div class="form-group">
                        <label for="username">Username</label>
                        <input type="text" id="username" name="username" required minlength="3" autocomplete="username">
                        <small class="form-hint">At least 3 characters</small>
                    </div>

                    <div class="form-group">
                        <label for="password">Password</label>
                        <input type="password" id="password" name="password" required minlength="8" autocomplete="new-password">
                        <div class="password-requirements">
                            <small id="req-length" class="requirement">✗ At least 8 characters</small>
                            <small id="req-uppercase" class="requirement">✗ One uppercase letter</small>
                            <small id="req-number" class="requirement">✗ One number</small>
                            <small id="req-special" class="requirement">✗ One special character</small>
                        </div>
                    </div>

                    <div class="form-group">
                        <label for="confirm-password">Confirm Password</label>
                        <input type="password" id="confirm-password" name="confirm-password" required autocomplete="new-password">
                        <small id="password-match" class="form-hint"></small>
                    </div>

                    <button type="submit" id="submit-btn">Create Account</button>
                </form>

                <p class="auth-footer">
                    Already have an account? <a href="/login.html">Sign in</a>
                </p>
            </div>
        </div>
    </main>

    <script src="/auth.js"></script>
    <script>
        if (isAuthenticated()) {
            window.location.href = '/dashboard.html';
        }

        const passwordInput = document.getElementById('password');
        const confirmInput = document.getElementById('confirm-password');
        const matchHint = document.getElementById('password-match');

        // Password validation feedback
        passwordInput.addEventListener('input', () => {
            const password = passwordInput.value;

            // Length check
            const reqLength = document.getElementById('req-length');
            if (password.length >= 8) {
                reqLength.textContent = '✓ At least 8 characters';
                reqLength.classList.add('valid');
            } else {
                reqLength.textContent = '✗ At least 8 characters';
                reqLength.classList.remove('valid');
            }

            // Uppercase check
            const reqUppercase = document.getElementById('req-uppercase');
            if (/[A-Z]/.test(password)) {
                reqUppercase.textContent = '✓ One uppercase letter';
                reqUppercase.classList.add('valid');
            } else {
                reqUppercase.textContent = '✗ One uppercase letter';
                reqUppercase.classList.remove('valid');
            }

            // Number check
            const reqNumber = document.getElementById('req-number');
            if (/[0-9]/.test(password)) {
                reqNumber.textContent = '✓ One number';
                reqNumber.classList.add('valid');
            } else {
                reqNumber.textContent = '✗ One number';
                reqNumber.classList.remove('valid');
            }

            // Special character check
            const reqSpecial = document.getElementById('req-special');
            if (/[^A-Za-z0-9]/.test(password)) {
                reqSpecial.textContent = '✓ One special character';
                reqSpecial.classList.add('valid');
            } else {
                reqSpecial.textContent = '✗ One special character';
                reqSpecial.classList.remove('valid');
            }

            // Check confirm match
            if (confirmInput.value) {
                checkPasswordMatch();
            }
        });

        confirmInput.addEventListener('input', checkPasswordMatch);

        function checkPasswordMatch() {
            if (confirmInput.value === passwordInput.value) {
                matchHint.textContent = '✓ Passwords match';
                matchHint.classList.add('valid');
            } else {
                matchHint.textContent = '✗ Passwords do not match';
                matchHint.classList.remove('valid');
            }
        }

        document.getElementById('signup-form').addEventListener('submit', async (e) => {
            e.preventDefault();

            const errorDiv = document.getElementById('error-message');
            const submitBtn = document.getElementById('submit-btn');

            errorDiv.style.display = 'none';

            const username = document.getElementById('username').value;
            const password = passwordInput.value;
            const confirmPassword = confirmInput.value;

            if (password !== confirmPassword) {
                errorDiv.textContent = 'Passwords do not match';
                errorDiv.style.display = 'block';
                return;
            }

            submitBtn.disabled = true;
            submitBtn.textContent = 'Creating account...';

            try {
                const response = await fetch('/api/register', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ username, password })
                });

                const data = await response.json();

                if (response.ok) {
                    saveAuth(data.token, data.refresh_token, data.username);
                    window.location.href = '/dashboard.html';
                } else {
                    errorDiv.textContent = data.error || 'Registration failed';
                    errorDiv.style.display = 'block';
                }
            } catch (err) {
                errorDiv.textContent = 'Network error. Please try again.';
                errorDiv.style.display = 'block';
            } finally {
                submitBtn.disabled = false;
                submitBtn.textContent = 'Create Account';
            }
        });
    </script>
</body>
</html>
```

Add to styles.css:

```css
.form-hint {
  font-size: 0.8rem;
  color: var(--text-muted);
}

.password-requirements {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-xs);
  margin-top: var(--spacing-xs);
}

.requirement {
  font-size: 0.8rem;
  color: var(--error);
}

.requirement.valid {
  color: var(--success);
}

.form-hint.valid {
  color: var(--success);
}
```
```

## Expected Output
- Consistent navbar
- Password requirements list
- Real-time validation feedback
- Green checkmarks for met requirements
- Password confirmation matching
- Handles refresh token
- Server-side error display
