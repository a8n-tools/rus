# Basic Themes (Dark/Light Toggle) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a dark/light theme toggle across all pages, unifying the landing page's light theme and the site's dark theme into a single switchable system.

**Architecture:** Keep existing `styles.css` CSS variable names as the canonical namespace. Refactor `index.html` to use canonical names instead of `--l-*` prefixed variables. Add `[data-theme="light"]` overrides in `styles.css`. Create a tiny synchronous `theme.js` that reads OS preference / localStorage and sets `data-theme` before paint. Add a sun/moon toggle button to every navbar.

**Tech Stack:** Vanilla CSS custom properties, vanilla JS, Font Awesome 6.5.1 icons (already loaded on index.html, needs adding to other pages)

---

### Task 1: Create `theme.js` (theme detection and toggle)

**Files:**
- Create: `static/theme.js`

- [ ] **Step 1: Create theme.js**

```js
(function () {
  var STORAGE_KEY = 'rus_theme';
  var saved = localStorage.getItem(STORAGE_KEY);
  var prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
  var theme = saved || (prefersDark ? 'dark' : 'light');
  document.documentElement.setAttribute('data-theme', theme);

  window.__setTheme = function (t) {
    document.documentElement.setAttribute('data-theme', t);
    localStorage.setItem(STORAGE_KEY, t);
    updateToggleIcon();
  };

  window.__toggleTheme = function () {
    var current = document.documentElement.getAttribute('data-theme');
    window.__setTheme(current === 'dark' ? 'light' : 'dark');
  };

  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', function (e) {
    if (!localStorage.getItem(STORAGE_KEY)) {
      document.documentElement.setAttribute('data-theme', e.matches ? 'dark' : 'light');
      updateToggleIcon();
    }
  });

  function updateToggleIcon() {
    var btn = document.getElementById('themeToggle');
    if (!btn) return;
    var isDark = document.documentElement.getAttribute('data-theme') === 'dark';
    btn.innerHTML = isDark
      ? '<i class="fa-solid fa-sun"></i>'
      : '<i class="fa-solid fa-moon"></i>';
    btn.title = isDark ? 'Switch to light mode' : 'Switch to dark mode';
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', updateToggleIcon);
  } else {
    updateToggleIcon();
  }
})();
```

- [ ] **Step 2: Commit**

```bash
git add static/theme.js
git commit -m "feat: add theme.js for dark/light mode detection and toggle"
```

---

### Task 2: Add light-mode CSS variable overrides and toggle button styles to `styles.css`

**Files:**
- Modify: `static/styles.css`

This task adds three blocks to the end of `styles.css`:
1. Light-mode variable overrides (both `@media` and `[data-theme]` selectors)
2. New variables for hardcoded values that differ between themes (added to `:root`)
3. Theme toggle button styles

- [ ] **Step 1: Add new theme-aware variables to `:root`**

Add these variables inside the existing `:root` block in `styles.css`, after the `--teal` line (line 28):

```css
    /* Theme-aware UI values (dark defaults) */
    --navbar-bg: rgba(26, 24, 38, 0.8);
    --navbar-border: rgba(42, 38, 64, 0.6);
    --navbar-shadow: 0 1px 0 rgba(42, 38, 64, 0.5);
    --card-shadow: rgba(0, 0, 0, 0.3);
    --card-shadow-hover: rgba(0, 0, 0, 0.4);
    --heavy-shadow: rgba(0, 0, 0, 0.5);
    --focus-glow: rgba(247, 76, 0, 0.15);
    --btn-hover-shadow: rgba(247, 76, 0, 0.3);
```

- [ ] **Step 2: Replace hardcoded rgba values in existing rules**

Replace the following hardcoded values throughout `styles.css`:

In `.navbar` (line 41-50):
- `background: rgba(26, 24, 38, 0.8)` → `background: var(--navbar-bg)`
- `box-shadow: 0 1px 0 rgba(42, 38, 64, 0.5)` → `box-shadow: var(--navbar-shadow)`
- `border-bottom: 1px solid rgba(42, 38, 64, 0.6)` → `border-bottom: 1px solid var(--navbar-border)`

In `.auth-card` (line 146):
- `box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5)` → `box-shadow: 0 20px 60px var(--heavy-shadow)`

In `input:focus` (line 197):
- `box-shadow: 0 0 0 3px rgba(247, 76, 0, 0.15)` → `box-shadow: 0 0 0 3px var(--focus-glow)`

In `button[type="submit"]:hover` (line 230):
- `box-shadow: 0 10px 20px rgba(247, 76, 0, 0.3)` → `box-shadow: 0 10px 20px var(--btn-hover-shadow)`

In `.accent-card` (line 328):
- `box-shadow: 0 4px 15px rgba(0, 0, 0, 0.3)` → `box-shadow: 0 4px 15px var(--card-shadow)`

In `.url-card:hover` (line 449):
- `box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3)` → `box-shadow: 0 4px 16px var(--card-shadow)`

In `.modal-content` (line 710):
- `box-shadow: 0 10px 40px rgba(0, 0, 0, 0.5)` → `box-shadow: 0 10px 40px var(--heavy-shadow)`

In `.download-btn:hover` (line 810):
- `box-shadow: 0 4px 12px rgba(247, 76, 0, 0.3)` → `box-shadow: 0 4px 12px var(--btn-hover-shadow)`

- [ ] **Step 3: Append light-mode overrides and toggle button styles at end of file**

Add the following at the very end of `styles.css`, after the last `}`:

```css
/* ============================================================
   Light theme overrides
   ============================================================ */
@media (prefers-color-scheme: light) {
    :root:not([data-theme="dark"]) {
        --rust-orange: #f97316;
        --rust-orange-dark: #ea6b0a;
        --rust-orange-light: #fb923c;
        --bg-dark: hsl(240 20% 95%);
        --bg-darker: hsl(240 20% 98%);
        --bg-card: hsl(240 25% 100%);
        --bg-card-hover: hsl(240 20% 93%);
        --text-primary: hsl(240 24% 10%);
        --text-secondary: hsl(240 10% 46%);
        --text-muted: hsl(240 10% 60%);
        --border-color: hsl(240 15% 90%);
        --navbar-bg: rgba(255, 255, 255, 0.9);
        --navbar-border: hsl(240 15% 90%);
        --navbar-shadow: 0 1px 3px rgba(0, 0, 0, 0.06);
        --card-shadow: rgba(0, 0, 0, 0.06);
        --card-shadow-hover: rgba(0, 0, 0, 0.1);
        --heavy-shadow: rgba(0, 0, 0, 0.1);
        --focus-glow: rgba(249, 115, 22, 0.2);
        --btn-hover-shadow: rgba(249, 115, 22, 0.25);
    }
}

:root[data-theme="light"] {
    --rust-orange: #f97316;
    --rust-orange-dark: #ea6b0a;
    --rust-orange-light: #fb923c;
    --bg-dark: hsl(240 20% 95%);
    --bg-darker: hsl(240 20% 98%);
    --bg-card: hsl(240 25% 100%);
    --bg-card-hover: hsl(240 20% 93%);
    --text-primary: hsl(240 24% 10%);
    --text-secondary: hsl(240 10% 46%);
    --text-muted: hsl(240 10% 60%);
    --border-color: hsl(240 15% 90%);
    --navbar-bg: rgba(255, 255, 255, 0.9);
    --navbar-border: hsl(240 15% 90%);
    --navbar-shadow: 0 1px 3px rgba(0, 0, 0, 0.06);
    --card-shadow: rgba(0, 0, 0, 0.06);
    --card-shadow-hover: rgba(0, 0, 0, 0.1);
    --heavy-shadow: rgba(0, 0, 0, 0.1);
    --focus-glow: rgba(249, 115, 22, 0.2);
    --btn-hover-shadow: rgba(249, 115, 22, 0.25);
}

/* Theme toggle button */
.theme-toggle-btn {
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

.theme-toggle-btn:hover {
    background: var(--bg-card-hover);
    color: var(--text-primary);
    border-color: var(--text-muted);
}
```

- [ ] **Step 4: Commit**

```bash
git add static/styles.css
git commit -m "feat: add light-mode CSS variable overrides and theme toggle styles"
```

---

### Task 3: Refactor `index.html` to use canonical CSS variables

**Files:**
- Modify: `static/index.html`

This task removes the `--l-*` variable namespace from `index.html` and replaces all references with the canonical variable names from `styles.css`. It also adds `theme.js` and the toggle button.

- [ ] **Step 1: Add theme.js to `<head>`**

Add this line in the `<head>` section, after the Font Awesome `<link>` tag (after line 8) and before the `<style>` tag:

```html
    <script src="theme.js"></script>
```

- [ ] **Step 2: Remove the `:root` block with `--l-*` variables**

Delete lines 13-25 of the inline `<style>` (the entire `:root { ... }` block containing `--l-*` variables).

- [ ] **Step 3: Replace all `var(--l-*)` references with canonical names**

Perform the following find-and-replace operations throughout the inline `<style>` block in `index.html`:

| Find | Replace with |
|------|-------------|
| `var(--l-bg)` | `var(--bg-darker)` |
| `var(--l-bg-sec)` | `var(--bg-dark)` |
| `var(--l-fg)` | `var(--text-primary)` |
| `var(--l-card)` | `var(--bg-card)` |
| `var(--l-muted)` | `var(--text-secondary)` |
| `var(--l-border)` | `var(--border-color)` |
| `var(--l-primary)` | `var(--rust-orange)` |
| `var(--l-primary-d)` | `var(--rust-orange-dark)` |
| `var(--l-indigo)` | `var(--indigo)` |
| `var(--l-cyan)` | `var(--teal)` |
| `var(--l-radius)` | `0.5rem` |

After this step, there should be zero remaining `--l-` references in the file.

- [ ] **Step 4: Fix the `.hero-badge` hardcoded dark background**

The `.hero-badge` has a hardcoded dark background (`hsl(240 24% 10%)`) that works on a light page but won't adapt to dark mode. Replace it with theme-aware variables.

Change the `.hero-badge` rule from:
```css
      .hero-badge {
        display: inline-flex;
        align-items: center;
        gap: 8px;
        background: hsl(240 24% 10%);
        color: hsl(240 10% 75%);
        border-radius: 50px;
        padding: 6px 18px;
        margin-bottom: 32px;
        font-size: 0.82rem;
        font-weight: 500;
        letter-spacing: 0.01em;
      }
```

To:
```css
      .hero-badge {
        display: inline-flex;
        align-items: center;
        gap: 8px;
        background: var(--bg-card);
        color: var(--text-secondary);
        border: 1px solid var(--border-color);
        border-radius: 50px;
        padding: 6px 18px;
        margin-bottom: 32px;
        font-size: 0.82rem;
        font-weight: 500;
        letter-spacing: 0.01em;
      }
```

- [ ] **Step 5: Fix the inline `style="color: var(--l-primary);"` in HTML body**

In the hero title (around line 459), replace:
```html
<span style="color: var(--l-primary);">RUS</span>
```
With:
```html
<span style="color: var(--rust-orange);">RUS</span>
```

Also fix the nav brand (around line 442), replace:
```html
<span style="color: #f97316; font-weight: 700;">RUS</span>
```
With:
```html
<span style="color: var(--rust-orange); font-weight: 700;">RUS</span>
```

- [ ] **Step 6: Add theme toggle button to navbar**

In the nav-links div (around line 443-448), add the theme toggle button. The nav currently is:
```html
        <div class="nav-links" id="navLinks">
          <a href="#features">Features</a>
          <a href="#about">About</a>
          <a href="login.html">Log In</a>
          <a href="signup.html" class="nav-cta" id="signupLink">Sign Up</a>
        </div>
```

Add the toggle button before the closing `</div>`:
```html
        <div class="nav-links" id="navLinks">
          <a href="#features">Features</a>
          <a href="#about">About</a>
          <a href="login.html">Log In</a>
          <a href="signup.html" class="nav-cta" id="signupLink">Sign Up</a>
          <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
            <i class="fa-solid fa-moon"></i>
          </button>
        </div>
```

- [ ] **Step 7: Update the `updateUI()` function to preserve theme toggle**

The `updateUI()` function dynamically replaces the `navLinks` innerHTML for different auth states. Each replacement must include the theme toggle button. Update the three innerHTML assignments:

In the SaaS mode block (around line 620-625), change the innerHTML to:
```js
          navLinks.innerHTML = `
            <a href="#features">Features</a>
            <a href="#about">About</a>
            <a href="dashboard.html">Dashboard</a>
            <a href="${loginUrl}">Log In</a>
            <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
              <i class="fa-solid fa-moon"></i>
            </button>
          `;
```

In the standalone logged-in block (around line 632-636), change to:
```js
          navLinks.innerHTML = `
            <a href="#features">Features</a>
            <a href="#about">About</a>
            <a href="dashboard.html">Dashboard</a>
            <button class="logout-btn" onclick="handleLogout()">Log Out</button>
            <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
              <i class="fa-solid fa-moon"></i>
            </button>
          `;
```

After each innerHTML assignment that replaces navLinks, call the icon updater so the correct icon (sun/moon) is shown:
```js
          if (typeof updateToggleIcon === 'undefined') {
            // theme.js exposes updateToggleIcon via DOMContentLoaded; re-trigger it
            var tb = document.getElementById('themeToggle');
            if (tb) {
              var isDark = document.documentElement.getAttribute('data-theme') === 'dark';
              tb.innerHTML = isDark ? '<i class="fa-solid fa-sun"></i>' : '<i class="fa-solid fa-moon"></i>';
              tb.title = isDark ? 'Switch to light mode' : 'Switch to dark mode';
            }
          }
```

**Note:** Since `updateToggleIcon` is scoped inside the IIFE in `theme.js`, we need to either expose it or inline the icon update. The simplest approach: after each `navLinks.innerHTML = ...` assignment, add:
```js
          // Update theme toggle icon after nav rebuild
          (function() {
            var tb = document.getElementById('themeToggle');
            if (tb) {
              var isDark = document.documentElement.getAttribute('data-theme') === 'dark';
              tb.innerHTML = isDark ? '<i class="fa-solid fa-sun"></i>' : '<i class="fa-solid fa-moon"></i>';
              tb.title = isDark ? 'Switch to light mode' : 'Switch to dark mode';
            }
          })();
```

- [ ] **Step 8: Commit**

```bash
git add static/index.html
git commit -m "feat: refactor index.html to use canonical CSS variables and add theme toggle"
```

---

### Task 4: Add theme toggle and theme.js to `dashboard.html`

**Files:**
- Modify: `static/dashboard.html`

- [ ] **Step 1: Add Font Awesome and theme.js to `<head>`**

After the `<link rel="stylesheet" href="styles.css?v=3" />` line (line 7), add:

```html
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css" />
    <script src="theme.js"></script>
```

- [ ] **Step 2: Add theme toggle button to navbar**

In the `.nav-links` div (line 13-18), add the toggle button before the closing `</div>`. The current nav-links are:

```html
        <div class="nav-links">
          <span id="maintenanceBadge" class="maintenance-badge" style="display: none">Maintenance Mode</span>
          <a href="admin.html" id="adminLink" style="display: none" class="admin-link">⚙️ Admin</a>
          <span class="user-info">👤 <span id="username"></span></span>
          <button id="logoutBtn" class="logout-btn">Log Out</button>
        </div>
```

Change to:

```html
        <div class="nav-links">
          <span id="maintenanceBadge" class="maintenance-badge" style="display: none">Maintenance Mode</span>
          <a href="admin.html" id="adminLink" style="display: none" class="admin-link">⚙️ Admin</a>
          <span class="user-info">👤 <span id="username"></span></span>
          <button id="logoutBtn" class="logout-btn">Log Out</button>
          <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
            <i class="fa-solid fa-moon"></i>
          </button>
        </div>
```

- [ ] **Step 3: Commit**

```bash
git add static/dashboard.html
git commit -m "feat: add theme toggle to dashboard"
```

---

### Task 5: Add theme toggle and theme.js to `login.html`

**Files:**
- Modify: `static/login.html`

- [ ] **Step 1: Add Font Awesome and theme.js to `<head>`**

After the `<link rel="stylesheet" href="styles.css?v=2">` line (line 7), add:

```html
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css" />
    <script src="theme.js"></script>
```

- [ ] **Step 2: Add theme toggle button to navbar**

Change the `.nav-links` div (lines 13-16) from:

```html
            <div class="nav-links" id="navLinks">
                <a href="/">Home</a>
                <a href="signup.html" id="signupLink">Sign Up</a>
            </div>
```

To:

```html
            <div class="nav-links" id="navLinks">
                <a href="/">Home</a>
                <a href="signup.html" id="signupLink">Sign Up</a>
                <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
                    <i class="fa-solid fa-moon"></i>
                </button>
            </div>
```

- [ ] **Step 3: Commit**

```bash
git add static/login.html
git commit -m "feat: add theme toggle to login page"
```

---

### Task 6: Add theme toggle and theme.js to `signup.html`

**Files:**
- Modify: `static/signup.html`

- [ ] **Step 1: Add Font Awesome and theme.js to `<head>`**

After the `<link rel="stylesheet" href="styles.css?v=2">` line (line 7), add:

```html
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css" />
    <script src="theme.js"></script>
```

- [ ] **Step 2: Add theme toggle button to navbar**

Change the `.nav-links` div (lines 13-16) from:

```html
            <div class="nav-links">
                <a href="/">Home</a>
                <a href="login.html">Log In</a>
            </div>
```

To:

```html
            <div class="nav-links">
                <a href="/">Home</a>
                <a href="login.html">Log In</a>
                <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
                    <i class="fa-solid fa-moon"></i>
                </button>
            </div>
```

- [ ] **Step 3: Commit**

```bash
git add static/signup.html
git commit -m "feat: add theme toggle to signup page"
```

---

### Task 7: Add theme toggle and theme.js to `admin.html`

**Files:**
- Modify: `static/admin.html`

- [ ] **Step 1: Add Font Awesome and theme.js to `<head>`**

After the `<link rel="stylesheet" href="styles.css?v=2">` line (line 7), add:

```html
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css" />
    <script src="theme.js"></script>
```

- [ ] **Step 2: Add theme toggle button to navbar**

Change the `.nav-links` div (lines 13-17) from:

```html
            <div class="nav-links">
                <a href="dashboard.html">My Dashboard</a>
                <span class="user-info">👤 <span id="username"></span> (Admin)</span>
                <button id="logoutBtn" class="logout-btn">Log Out</button>
            </div>
```

To:

```html
            <div class="nav-links">
                <a href="dashboard.html">My Dashboard</a>
                <span class="user-info">👤 <span id="username"></span> (Admin)</span>
                <button id="logoutBtn" class="logout-btn">Log Out</button>
                <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
                    <i class="fa-solid fa-moon"></i>
                </button>
            </div>
```

- [ ] **Step 3: Fix hardcoded shadow values in inline `<style>`**

In admin.html's inline `<style>` block, replace hardcoded shadow values:

`.stat-card` (line 615): `box-shadow: 0 4px 15px rgba(0, 0, 0, 0.3)` → `box-shadow: 0 4px 15px var(--card-shadow)`

`.stat-card:hover` (line 624): `box-shadow: 0 6px 20px rgba(0, 0, 0, 0.4)` → `box-shadow: 0 6px 20px var(--card-shadow-hover)`

`.users-section` (line 647): `box-shadow: 0 4px 15px rgba(0, 0, 0, 0.3)` → `box-shadow: 0 4px 15px var(--card-shadow)`

`.promote-user-btn:hover` (line 691): `box-shadow: 0 4px 12px rgba(247, 76, 0, 0.5)` → `box-shadow: 0 4px 12px var(--btn-hover-shadow)`

- [ ] **Step 4: Fix `.info-box` on setup.html-style elements if present**

The admin page's inline styles don't have an `.info-box` but do have hardcoded colors for `.text-muted` — this already uses `var(--text-muted)` so it's fine.

- [ ] **Step 5: Commit**

```bash
git add static/admin.html
git commit -m "feat: add theme toggle to admin page and fix hardcoded shadows"
```

---

### Task 8: Add theme toggle and theme.js to `404.html`

**Files:**
- Modify: `static/404.html`

- [ ] **Step 1: Add Font Awesome and theme.js to `<head>`**

After the `<link rel="stylesheet" href="styles.css?v=2">` line (line 7), add:

```html
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css" />
    <script src="theme.js"></script>
```

- [ ] **Step 2: Add theme toggle button to navbar**

Change the `.nav-links` div (lines 122-126) from:

```html
            <div class="nav-links" id="navLinks">
                <a href="/">Home</a>
                <a href="login.html">Log In</a>
                <a href="signup.html">Sign Up</a>
            </div>
```

To:

```html
            <div class="nav-links" id="navLinks">
                <a href="/">Home</a>
                <a href="login.html">Log In</a>
                <a href="signup.html">Sign Up</a>
                <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
                    <i class="fa-solid fa-moon"></i>
                </button>
            </div>
```

- [ ] **Step 3: Update the dynamic nav innerHTML to include toggle**

In the script at the bottom (around line 158), the nav is dynamically replaced for authenticated users:

```js
        if (typeof isAuthenticated === 'function' && isAuthenticated()) {
            navLinks.innerHTML = `
                <a href="/">Home</a>
                <a href="dashboard.html">Dashboard</a>
                <button class="logout-btn" onclick="handleLogout()">Log Out</button>
            `;
        }
```

Change to:

```js
        if (typeof isAuthenticated === 'function' && isAuthenticated()) {
            navLinks.innerHTML = `
                <a href="/">Home</a>
                <a href="dashboard.html">Dashboard</a>
                <button class="logout-btn" onclick="handleLogout()">Log Out</button>
                <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
                    <i class="fa-solid fa-moon"></i>
                </button>
            `;
        }
        // Update theme toggle icon after nav rebuild
        (function() {
            var tb = document.getElementById('themeToggle');
            if (tb) {
                var isDark = document.documentElement.getAttribute('data-theme') === 'dark';
                tb.innerHTML = isDark ? '<i class="fa-solid fa-sun"></i>' : '<i class="fa-solid fa-moon"></i>';
                tb.title = isDark ? 'Switch to light mode' : 'Switch to dark mode';
            }
        })();
```

- [ ] **Step 4: Fix hardcoded shadow in inline `<style>`**

In `.error-container` (line 12): `box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5)` → `box-shadow: 0 20px 60px var(--heavy-shadow)`

- [ ] **Step 5: Commit**

```bash
git add static/404.html
git commit -m "feat: add theme toggle to 404 page"
```

---

### Task 9: Add theme.js to `maintenance.html`

**Files:**
- Modify: `static/maintenance.html`

Note: `maintenance.html` has no navbar (just a centered message), so we only add `theme.js` for consistent theming, no toggle button.

- [ ] **Step 1: Add Font Awesome and theme.js to `<head>`**

After the `<link rel="stylesheet" href="styles.css?v=3" />` line (line 7), add:

```html
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css" />
    <script src="theme.js"></script>
```

- [ ] **Step 2: Commit**

```bash
git add static/maintenance.html
git commit -m "feat: add theme support to maintenance page"
```

---

### Task 10: Add theme toggle and theme.js to `setup.html`

**Files:**
- Modify: `static/setup.html`

- [ ] **Step 1: Add Font Awesome and theme.js to `<head>`**

After the `<link rel="stylesheet" href="styles.css?v=2">` line (line 7), add:

```html
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css" />
    <script src="theme.js"></script>
```

- [ ] **Step 2: Add theme toggle button to navbar**

The setup.html navbar has no `.nav-links` div. The current nav is:

```html
    <nav class="navbar">
        <div class="nav-content">
            <a href="/" class="nav-brand">🦀 Rust URL Shortener</a>
        </div>
    </nav>
```

Change to:

```html
    <nav class="navbar">
        <div class="nav-content">
            <a href="/" class="nav-brand">🦀 Rust URL Shortener</a>
            <div class="nav-links">
                <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
                    <i class="fa-solid fa-moon"></i>
                </button>
            </div>
        </div>
    </nav>
```

- [ ] **Step 3: Fix hardcoded `.info-box` colors for theme compatibility**

The `.info-box` in setup.html's inline `<style>` has hardcoded light-mode colors:

```css
        .info-box {
            background: #e3f2fd;
            border-left: 4px solid #2196f3;
            padding: 1rem;
            margin-bottom: 1.5rem;
            border-radius: 4px;
        }

        .info-box p {
            margin: 0;
            color: #1565c0;
            font-size: 0.9rem;
        }

        small {
            display: block;
            margin-top: 0.25rem;
            color: #666;
            font-size: 0.85rem;
        }
```

Replace with theme-aware values:

```css
        .info-box {
            background: rgba(33, 150, 243, 0.1);
            border-left: 4px solid #2196f3;
            padding: 1rem;
            margin-bottom: 1.5rem;
            border-radius: 4px;
        }

        .info-box p {
            margin: 0;
            color: #42a5f5;
            font-size: 0.9rem;
        }

        small {
            display: block;
            margin-top: 0.25rem;
            color: var(--text-muted);
            font-size: 0.85rem;
        }
```

- [ ] **Step 4: Commit**

```bash
git add static/setup.html
git commit -m "feat: add theme toggle to setup page"
```

---

### Task 11: Add theme toggle and theme.js to `report.html`

**Files:**
- Modify: `static/report.html`

- [ ] **Step 1: Add Font Awesome and theme.js to `<head>`**

After the `<link rel="stylesheet" href="styles.css?v=3" />` line (line 7), add:

```html
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css" />
    <script src="theme.js"></script>
```

- [ ] **Step 2: Add theme toggle button to navbar**

Change the `.nav-links` div (lines 13-17) from:

```html
        <div class="nav-links" id="navLinks">
          <a href="/">Home</a>
          <a href="login.html">Log In</a>
          <a href="signup.html" id="signupLink">Sign Up</a>
        </div>
```

To:

```html
        <div class="nav-links" id="navLinks">
          <a href="/">Home</a>
          <a href="login.html">Log In</a>
          <a href="signup.html" id="signupLink">Sign Up</a>
          <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
            <i class="fa-solid fa-moon"></i>
          </button>
        </div>
```

- [ ] **Step 3: Update the dynamic nav innerHTML to include toggle**

In the `updateNavLinks()` function (around line 113), the nav is dynamically replaced for authenticated users:

```js
          navLinks.innerHTML = `
            <a href="/">Home</a>
            <a href="dashboard.html">Dashboard</a>
            <button class="logout-btn" onclick="handleLogout()">Log Out</button>
          `;
```

Change to:

```js
          navLinks.innerHTML = `
            <a href="/">Home</a>
            <a href="dashboard.html">Dashboard</a>
            <button class="logout-btn" onclick="handleLogout()">Log Out</button>
            <button id="themeToggle" class="theme-toggle-btn" onclick="__toggleTheme()" aria-label="Toggle theme">
              <i class="fa-solid fa-moon"></i>
            </button>
          `;
          // Update theme toggle icon after nav rebuild
          (function() {
            var tb = document.getElementById('themeToggle');
            if (tb) {
              var isDark = document.documentElement.getAttribute('data-theme') === 'dark';
              tb.innerHTML = isDark ? '<i class="fa-solid fa-sun"></i>' : '<i class="fa-solid fa-moon"></i>';
              tb.title = isDark ? 'Switch to light mode' : 'Switch to dark mode';
            }
          })();
```

- [ ] **Step 4: Commit**

```bash
git add static/report.html
git commit -m "feat: add theme toggle to report page"
```

---

### Task 12: Build and test

**Files:** None (verification only)

- [ ] **Step 1: Build the project**

Run: `cargo build`
Expected: Compiles successfully (frontend changes don't affect Rust build, but confirms nothing is broken)

- [ ] **Step 2: Run tests**

Run: `cargo test`
Expected: All existing tests pass

- [ ] **Step 3: Verify no remaining `--l-` references**

Run: `grep -r '\-\-l-' static/`
Expected: No output (all `--l-*` variables have been replaced)

- [ ] **Step 4: Verify theme.js is referenced in all HTML files**

Run: `grep -L 'theme.js' static/*.html`
Expected: No output (all HTML files include theme.js)

- [ ] **Step 5: Verify Font Awesome is referenced in all HTML files**

Run: `grep -L 'font-awesome' static/*.html`
Expected: No output (all HTML files include Font Awesome)
