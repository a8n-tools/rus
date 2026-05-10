# High Contrast Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a high contrast mode toggle that works independently alongside light/dark theme, with navbar layout changes on all pages.

**Architecture:** Contrast state is stored as `data-contrast` attribute on `<html>` and persisted in localStorage. CSS custom property overrides layer on top of existing dark/light themes. Every page gets a contrast toggle button in the navbar next to the theme toggle.

**Tech Stack:** Vanilla CSS custom properties, vanilla JS, Font Awesome icons

---

### Task 1: Add contrast toggle logic to theme.js

**Files:**
- Modify: `static/theme.js:1-43`

- [ ] **Step 1: Add contrast state initialization and toggle functions**

Replace the entire contents of `static/theme.js` with:

```js
(function () {
  var STORAGE_KEY = 'rus_theme';
  var CONTRAST_KEY = 'rus_contrast';

  // --- Theme (light/dark) ---
  var saved = localStorage.getItem(STORAGE_KEY);
  var prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
  var theme = saved || (prefersDark ? 'dark' : 'light');
  document.documentElement.setAttribute('data-theme', theme);

  // --- Contrast ---
  var savedContrast = localStorage.getItem(CONTRAST_KEY);
  if (savedContrast === 'high') {
    document.documentElement.setAttribute('data-contrast', 'high');
  }

  window.__setTheme = function (t) {
    document.documentElement.setAttribute('data-theme', t);
    localStorage.setItem(STORAGE_KEY, t);
    updateToggleIcon();
  };

  window.__toggleTheme = function () {
    var current = document.documentElement.getAttribute('data-theme');
    window.__setTheme(current === 'dark' ? 'light' : 'dark');
  };

  window.__toggleContrast = function () {
    var current = document.documentElement.getAttribute('data-contrast');
    var next = current === 'high' ? 'normal' : 'high';
    document.documentElement.setAttribute('data-contrast', next);
    localStorage.setItem(CONTRAST_KEY, next);
    updateContrastIcon();
  };

  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', function (e) {
    if (!localStorage.getItem(STORAGE_KEY)) {
      document.documentElement.setAttribute('data-theme', e.matches ? 'dark' : 'light');
      updateToggleIcon();
    }
  });

  window.__updateThemeIcon = updateToggleIcon;
  window.__updateContrastIcon = updateContrastIcon;

  function updateToggleIcon() {
    var btn = document.getElementById('themeToggle');
    if (!btn) return;
    var isDark = document.documentElement.getAttribute('data-theme') === 'dark';
    btn.innerHTML = isDark
      ? '<i class="fa-solid fa-sun"></i>'
      : '<i class="fa-solid fa-moon"></i>';
    btn.title = isDark ? 'Switch to light mode' : 'Switch to dark mode';
  }

  function updateContrastIcon() {
    var btn = document.getElementById('contrastToggle');
    if (!btn) return;
    var isHigh = document.documentElement.getAttribute('data-contrast') === 'high';
    btn.style.color = isHigh ? 'var(--rust-orange)' : '';
    btn.title = isHigh ? 'Switch to normal contrast' : 'Switch to high contrast';
  }

  function initIcons() {
    updateToggleIcon();
    updateContrastIcon();
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initIcons);
  } else {
    initIcons();
  }
})();
```

- [ ] **Step 2: Verify theme.js loads without errors**

Open any page in the browser and check the console for errors. The existing light/dark toggle should still work. The contrast toggle won't do anything visible yet (no CSS overrides).

- [ ] **Step 3: Commit**

```bash
git add static/theme.js
git commit -m "feat: add contrast toggle logic to theme.js"
```

---

### Task 2: Add high contrast CSS overrides to styles.css

**Files:**
- Modify: `static/styles.css` (append after line 907, the end of `.theme-toggle-btn:hover`)

- [ ] **Step 1: Add contrast toggle button style and high contrast dark overrides**

Append the following to the end of `static/styles.css`:

```css
/* Contrast toggle button (same style as theme toggle) */
.contrast-toggle-btn {
    background: transparent;
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    width: 36px;
    height: 36px;
    border-radius: 8px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 1rem;
    transition: background 0.2s, color 0.2s, border-color 0.2s;
    padding: 0;
    line-height: 1;
}

.contrast-toggle-btn:hover {
    background: var(--bg-card-hover);
    color: var(--text-primary);
    border-color: var(--text-muted);
}

/* ============================================================
   High-contrast dark overrides
   ============================================================ */
[data-contrast="high"] {
    --bg-dark: #0a0a0a;
    --bg-darker: #000000;
    --bg-card: #1a1a1a;
    --bg-card-hover: #2a2a2a;
    --text-primary: #ffffff;
    --text-secondary: #e0e0e0;
    --text-muted: #b0b0b0;
    --border-color: #ffffff;
    --navbar-bg: rgba(0, 0, 0, 0.95);
    --navbar-border: #ffffff;
    --navbar-shadow: 0 1px 0 #ffffff;
    --card-shadow: rgba(255, 255, 255, 0.1);
    --card-shadow-hover: rgba(255, 255, 255, 0.15);
    --heavy-shadow: rgba(255, 255, 255, 0.1);
    --focus-glow: rgba(247, 76, 0, 0.3);
    --btn-hover-shadow: rgba(247, 76, 0, 0.4);
}

/* ============================================================
   High-contrast light overrides
   ============================================================ */
[data-theme="light"][data-contrast="high"] {
    --rust-orange: #c53d00;
    --rust-orange-dark: #a83300;
    --rust-orange-light: #d94e10;
    --bg-dark: #f5f5f5;
    --bg-darker: #ffffff;
    --bg-card: #ffffff;
    --bg-card-hover: #eeeeee;
    --text-primary: #000000;
    --text-secondary: #1a1a1a;
    --text-muted: #333333;
    --border-color: #000000;
    --navbar-bg: rgba(255, 255, 255, 0.98);
    --navbar-border: #000000;
    --navbar-shadow: 0 1px 0 #000000;
    --card-shadow: rgba(0, 0, 0, 0.15);
    --card-shadow-hover: rgba(0, 0, 0, 0.2);
    --heavy-shadow: rgba(0, 0, 0, 0.2);
    --focus-glow: rgba(197, 61, 0, 0.3);
    --btn-hover-shadow: rgba(197, 61, 0, 0.35);
}

@media (prefers-color-scheme: light) {
    :root:not([data-theme="dark"])[data-contrast="high"] {
        --rust-orange: #c53d00;
        --rust-orange-dark: #a83300;
        --rust-orange-light: #d94e10;
        --bg-dark: #f5f5f5;
        --bg-darker: #ffffff;
        --bg-card: #ffffff;
        --bg-card-hover: #eeeeee;
        --text-primary: #000000;
        --text-secondary: #1a1a1a;
        --text-muted: #333333;
        --border-color: #000000;
        --navbar-bg: rgba(255, 255, 255, 0.98);
        --navbar-border: #000000;
        --navbar-shadow: 0 1px 0 #000000;
        --card-shadow: rgba(0, 0, 0, 0.15);
        --card-shadow-hover: rgba(0, 0, 0, 0.2);
        --heavy-shadow: rgba(0, 0, 0, 0.2);
        --focus-glow: rgba(197, 61, 0, 0.3);
        --btn-hover-shadow: rgba(197, 61, 0, 0.35);
    }
}
```

- [ ] **Step 2: Verify high contrast mode toggles visually**

Open any page in the browser. Open the browser console and run:
```js
__toggleContrast();
```
The page should switch to high contrast (black background, white borders). Run it again to toggle back. Also test with light mode:
```js
__setTheme('light');
__toggleContrast();
```

- [ ] **Step 3: Commit**

```bash
git add static/styles.css
git commit -m "feat: add high contrast CSS overrides for dark and light themes"
```

---

### Task 3: Restructure landing page navbar (index.html)

**Files:**
- Modify: `static/index.html:425-437` (static navbar HTML)
- Modify: `static/index.html:604-645` (updateUI() dynamic nav rewrites)

- [ ] **Step 1: Replace the static navbar HTML**

In `static/index.html`, replace lines 425-438 (the `<nav>` block):

```html
    <nav class="navbar">
      <div class="nav-content">
        <div style="display: flex; align-items: center; gap: 20px;">
          <a href="/" class="nav-brand">🦀 <span style="color: var(--rust-orange); font-weight: 700;">RUS</span></a>
          <a href="#features" style="color: var(--text-secondary); text-decoration: none; font-size: 0.9rem; font-weight: 500;">Features</a>
          <a href="#about" style="color: var(--text-secondary); text-decoration: none; font-size: 0.9rem; font-weight: 500;">About</a>
        </div>
        <div class="nav-links" id="navLinks">
          <button id="contrastToggle" class="contrast-toggle-btn" onclick="__toggleContrast()" aria-label="Toggle contrast">
            <i class="fa-solid fa-circle-half-stroke"></i>
          </button>
          <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
            <i class="fa-solid fa-moon"></i>
          </button>
          <a href="login.html">Log In</a>
          <a href="signup.html" class="nav-cta" id="signupLink">Sign Up</a>
        </div>
      </div>
    </nav>
```

- [ ] **Step 2: Update the `updateUI()` SaaS mode nav rewrite**

In `static/index.html`, find the SaaS mode `navLinks.innerHTML` assignment (inside the `if (config.auth_mode === 'saas')` block) and replace it with:

```js
          navLinks.innerHTML = `
            <button id="contrastToggle" class="contrast-toggle-btn" onclick="__toggleContrast()" aria-label="Toggle contrast">
              <i class="fa-solid fa-circle-half-stroke"></i>
            </button>
            <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
              <i class="fa-solid fa-moon"></i>
            </button>
            <a href="dashboard.html">Dashboard</a>
            <a href="${loginUrl}">Log In</a>
          `;
          __updateThemeIcon();
          __updateContrastIcon();
```

- [ ] **Step 3: Update the `updateUI()` standalone logged-in nav rewrite**

Find the standalone logged-in `navLinks.innerHTML` assignment (inside the `else if (isAuthenticated())` block) and replace it with:

```js
          navLinks.innerHTML = `
            <button id="contrastToggle" class="contrast-toggle-btn" onclick="__toggleContrast()" aria-label="Toggle contrast">
              <i class="fa-solid fa-circle-half-stroke"></i>
            </button>
            <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
              <i class="fa-solid fa-moon"></i>
            </button>
            <a href="dashboard.html">Dashboard</a>
            <button class="logout-btn" onclick="handleLogout()">Log Out</button>
          `;
          __updateThemeIcon();
          __updateContrastIcon();
```

- [ ] **Step 4: Verify landing page navbar layout**

Open `index.html` in the browser. Confirm:
- Left side: RUS logo, Features, About
- Right side: contrast toggle (half-circle), theme toggle (moon/sun), Log In, Sign Up
- Both toggles work independently
- Smooth scroll for Features/About still works

- [ ] **Step 5: Commit**

```bash
git add static/index.html
git commit -m "feat: restructure landing page navbar with contrast toggle"
```

---

### Task 4: Add contrast toggle to dashboard navbar

**Files:**
- Modify: `static/dashboard.html:15-23`

- [ ] **Step 1: Replace the nav-links div**

In `static/dashboard.html`, replace lines 15-23 (the `.nav-links` div and its contents):

```html
        <div class="nav-links">
          <span id="maintenanceBadge" class="maintenance-badge" style="display: none">Maintenance Mode</span>
          <a href="admin.html" id="adminLink" style="display: none" class="admin-link">⚙️ Admin</a>
          <button id="contrastToggle" class="contrast-toggle-btn" onclick="__toggleContrast()" aria-label="Toggle contrast">
            <i class="fa-solid fa-circle-half-stroke"></i>
          </button>
          <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
            <i class="fa-solid fa-moon"></i>
          </button>
          <span class="user-info">👤 <span id="username"></span></span>
          <button id="logoutBtn" class="logout-btn">Log Out</button>
        </div>
```

- [ ] **Step 2: Verify dashboard navbar**

Open `dashboard.html`. Confirm: contrast toggle, theme toggle, username, Log Out (in that order, right side).

- [ ] **Step 3: Commit**

```bash
git add static/dashboard.html
git commit -m "feat: add contrast toggle to dashboard navbar"
```

---

### Task 5: Add contrast toggle to login page navbar

**Files:**
- Modify: `static/login.html:15-21`

- [ ] **Step 1: Replace the nav-links div**

In `static/login.html`, replace lines 15-21 (the `.nav-links` div):

```html
            <div class="nav-links" id="navLinks">
                <button id="contrastToggle" class="contrast-toggle-btn" onclick="__toggleContrast()" aria-label="Toggle contrast">
                    <i class="fa-solid fa-circle-half-stroke"></i>
                </button>
                <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
                    <i class="fa-solid fa-moon"></i>
                </button>
                <a href="/">Home</a>
                <a href="signup.html" id="signupLink">Sign Up</a>
            </div>
```

- [ ] **Step 2: Commit**

```bash
git add static/login.html
git commit -m "feat: add contrast toggle to login page navbar"
```

---

### Task 6: Add contrast toggle to signup page navbar

**Files:**
- Modify: `static/signup.html:15-20`

- [ ] **Step 1: Replace the nav-links div**

In `static/signup.html`, replace lines 15-20 (the `.nav-links` div):

```html
            <div class="nav-links">
                <button id="contrastToggle" class="contrast-toggle-btn" onclick="__toggleContrast()" aria-label="Toggle contrast">
                    <i class="fa-solid fa-circle-half-stroke"></i>
                </button>
                <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
                    <i class="fa-solid fa-moon"></i>
                </button>
                <a href="/">Home</a>
                <a href="login.html">Log In</a>
            </div>
```

- [ ] **Step 2: Commit**

```bash
git add static/signup.html
git commit -m "feat: add contrast toggle to signup page navbar"
```

---

### Task 7: Add contrast toggle to admin page navbar

**Files:**
- Modify: `static/admin.html:15-22`

- [ ] **Step 1: Replace the nav-links div**

In `static/admin.html`, replace lines 15-22 (the `.nav-links` div):

```html
            <div class="nav-links">
                <a href="dashboard.html">My Dashboard</a>
                <button id="contrastToggle" class="contrast-toggle-btn" onclick="__toggleContrast()" aria-label="Toggle contrast">
                    <i class="fa-solid fa-circle-half-stroke"></i>
                </button>
                <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
                    <i class="fa-solid fa-moon"></i>
                </button>
                <span class="user-info">👤 <span id="username"></span> (Admin)</span>
                <button id="logoutBtn" class="logout-btn">Log Out</button>
            </div>
```

- [ ] **Step 2: Commit**

```bash
git add static/admin.html
git commit -m "feat: add contrast toggle to admin page navbar"
```

---

### Task 8: Add contrast toggle to report page navbar

**Files:**
- Modify: `static/report.html:15-22`

- [ ] **Step 1: Replace the nav-links div**

In `static/report.html`, replace lines 15-22 (the `.nav-links` div):

```html
        <div class="nav-links" id="navLinks">
          <button id="contrastToggle" class="contrast-toggle-btn" onclick="__toggleContrast()" aria-label="Toggle contrast">
            <i class="fa-solid fa-circle-half-stroke"></i>
          </button>
          <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
            <i class="fa-solid fa-moon"></i>
          </button>
          <a href="/">Home</a>
          <a href="login.html">Log In</a>
          <a href="signup.html" id="signupLink">Sign Up</a>
        </div>
```

- [ ] **Step 2: Commit**

```bash
git add static/report.html
git commit -m "feat: add contrast toggle to report page navbar"
```

---

### Task 9: Add contrast toggle to setup page navbar

**Files:**
- Modify: `static/setup.html:15-19`

- [ ] **Step 1: Replace the nav-links div**

In `static/setup.html`, replace lines 15-19 (the `.nav-links` div):

```html
            <div class="nav-links">
                <button id="contrastToggle" class="contrast-toggle-btn" onclick="__toggleContrast()" aria-label="Toggle contrast">
                    <i class="fa-solid fa-circle-half-stroke"></i>
                </button>
                <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
                    <i class="fa-solid fa-moon"></i>
                </button>
            </div>
```

- [ ] **Step 2: Commit**

```bash
git add static/setup.html
git commit -m "feat: add contrast toggle to setup page navbar"
```

---

### Task 10: Add contrast toggle to 404 page navbar

**Files:**
- Modify: `static/404.html:124-131`

- [ ] **Step 1: Replace the nav-links div**

In `static/404.html`, replace lines 124-131 (the `.nav-links` div):

```html
            <div class="nav-links" id="navLinks">
                <button id="contrastToggle" class="contrast-toggle-btn" onclick="__toggleContrast()" aria-label="Toggle contrast">
                    <i class="fa-solid fa-circle-half-stroke"></i>
                </button>
                <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
                    <i class="fa-solid fa-moon"></i>
                </button>
                <a href="/">Home</a>
                <a href="login.html">Log In</a>
                <a href="signup.html">Sign Up</a>
            </div>
```

- [ ] **Step 2: Commit**

```bash
git add static/404.html
git commit -m "feat: add contrast toggle to 404 page navbar"
```

---

### Task 11: Final verification across all 4 theme combinations

- [ ] **Step 1: Test all combinations**

Open the app and cycle through all 4 states on the landing page, dashboard, and login page:
1. Dark + normal contrast
2. Dark + high contrast
3. Light + normal contrast
4. Light + high contrast

Verify for each:
- Text is readable
- Borders are visible
- Buttons have sufficient contrast
- Both toggles work and persist across page navigation
- Contrast icon turns orange when active

- [ ] **Step 2: Test persistence**

Toggle high contrast on, navigate to a different page, confirm it stays active. Refresh the page, confirm it stays active.

- [ ] **Step 3: Test landing page navbar layout**

Confirm Features/About are on the left next to RUS logo. Confirm contrast and theme toggles are to the left of Log In / Sign Up.

Note: `maintenance.html` has no navbar — it is a standalone error page with no navigation, so it does not get a contrast toggle button. It will still benefit from high contrast mode through the CSS variable overrides if contrast was toggled on a previous page.
