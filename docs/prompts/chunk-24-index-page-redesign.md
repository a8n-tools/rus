# Chunk 24: Landing Page Redesign

## Context
Building on navigation styles. Redesign the index.html landing page with Rust theme.

## Goal
Update index.html with new navbar, hero section, features, and call-to-action.

## Prompt

```text
I have navigation styles. Now redesign the landing page.

Replace the content of static/index.html:

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>RUS - Rust URL Shortener</title>
    <link rel="stylesheet" href="/styles.css">
</head>
<body>
    <nav class="navbar">
        <div class="navbar-container">
            <a href="/" class="navbar-brand">
                <span class="logo">🦀</span> RUS
            </a>
            <ul class="navbar-nav" id="nav-links">
                <li><a href="/login.html" class="navbar-btn">Login</a></li>
                <li><a href="/signup.html" class="navbar-btn">Sign Up</a></li>
            </ul>
        </div>
    </nav>

    <main class="container">
        <section class="hero">
            <h1>Shorten URLs with <span class="highlight">Rust</span> Speed</h1>
            <p class="hero-subtitle">Fast, secure, and self-hosted URL shortening service built with Rust.</p>
            <div class="hero-actions">
                <a href="/signup.html" class="btn btn-primary">Get Started</a>
                <a href="#features" class="btn btn-secondary">Learn More</a>
            </div>
        </section>

        <section id="features" class="features">
            <h2>Why Choose RUS?</h2>
            <div class="feature-grid">
                <div class="feature-card">
                    <div class="feature-icon">⚡</div>
                    <h3>Blazing Fast</h3>
                    <p>Built with Rust and Actix-web for maximum performance and minimal latency.</p>
                </div>
                <div class="feature-card">
                    <div class="feature-icon">🔒</div>
                    <h3>Secure</h3>
                    <p>JWT authentication, password hashing, account lockout, and input validation.</p>
                </div>
                <div class="feature-card">
                    <div class="feature-icon">📊</div>
                    <h3>Analytics</h3>
                    <p>Track clicks over time with detailed history and visualizations.</p>
                </div>
                <div class="feature-card">
                    <div class="feature-icon">📱</div>
                    <h3>QR Codes</h3>
                    <p>Generate branded QR codes for easy mobile sharing.</p>
                </div>
                <div class="feature-card">
                    <div class="feature-icon">🏠</div>
                    <h3>Self-Hosted</h3>
                    <p>Own your data. Deploy on your infrastructure with Docker.</p>
                </div>
                <div class="feature-card">
                    <div class="feature-icon">🦀</div>
                    <h3>Open Source</h3>
                    <p>Transparent, auditable code. Customize to your needs.</p>
                </div>
            </div>
        </section>
    </main>

    <footer class="footer">
        <p>Built with 🦀 Rust • <a href="https://github.com/your-repo">GitHub</a></p>
    </footer>

    <script src="/auth.js"></script>
    <script>
        // If user is authenticated, redirect to dashboard
        if (isAuthenticated()) {
            window.location.href = '/dashboard.html';
        }
    </script>
</body>
</html>
```

Add corresponding CSS to styles.css:

```css
/* Hero Section */
.hero {
  text-align: center;
  padding: var(--spacing-xl) 0;
  margin-bottom: var(--spacing-xl);
}

.hero h1 {
  font-size: 3rem;
  margin-bottom: var(--spacing-md);
}

.hero .highlight {
  color: var(--rust-orange);
}

.hero-subtitle {
  font-size: 1.25rem;
  color: var(--text-secondary);
  max-width: 600px;
  margin: 0 auto var(--spacing-lg);
}

.hero-actions {
  display: flex;
  gap: var(--spacing-md);
  justify-content: center;
}

.btn {
  display: inline-block;
  padding: var(--spacing-sm) var(--spacing-xl);
  border-radius: var(--radius-md);
  font-weight: 600;
  text-decoration: none;
  transition: all var(--transition-fast);
}

.btn-primary {
  background-color: var(--rust-orange);
  color: var(--text-primary);
}

.btn-primary:hover {
  background-color: var(--rust-orange-light);
  color: var(--text-primary);
  transform: translateY(-2px);
}

.btn-secondary {
  background-color: var(--rust-gray);
  color: var(--text-primary);
}

.btn-secondary:hover {
  background-color: var(--rust-light-gray);
  color: var(--text-primary);
}

/* Features Section */
.features {
  padding: var(--spacing-xl) 0;
}

.features h2 {
  text-align: center;
  margin-bottom: var(--spacing-xl);
}

.feature-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
  gap: var(--spacing-lg);
}

.feature-card {
  background-color: var(--rust-dark);
  padding: var(--spacing-lg);
  border-radius: var(--radius-lg);
  border: 1px solid var(--rust-gray);
  transition: transform var(--transition-fast), border-color var(--transition-fast);
}

.feature-card:hover {
  transform: translateY(-4px);
  border-color: var(--rust-orange);
}

.feature-icon {
  font-size: 2.5rem;
  margin-bottom: var(--spacing-md);
}

.feature-card h3 {
  color: var(--rust-orange);
  margin-bottom: var(--spacing-sm);
}

/* Footer */
.footer {
  text-align: center;
  padding: var(--spacing-lg);
  border-top: 1px solid var(--rust-gray);
  margin-top: var(--spacing-xl);
  color: var(--text-muted);
}

.footer a {
  color: var(--rust-orange);
}
```
```

## Expected Output
- Navbar with brand and auth links
- Hero section with CTA
- Feature grid showcasing capabilities
- Footer with attribution
- Auth redirect if logged in
- All using Rust theme colors
