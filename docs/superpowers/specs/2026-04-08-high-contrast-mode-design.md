# High Contrast Mode Design

**Date:** 2026-04-08
**Status:** Approved

## Overview

Add a high contrast mode that works independently alongside the existing light/dark theme toggle. Users get 4 combinations: dark, dark+high-contrast, light, light+high-contrast. A new toggle button (half-circle icon) appears on every page's navbar.

## How It Works

- New `data-contrast="high"` attribute on `<html>`, independent of `data-theme`
- Persisted in `localStorage` key `rus_contrast`
- High contrast CSS layers on top of existing light/dark variables — no changes to existing theme values

## High Contrast Dark Palette

```
--bg-darker: #000000
--bg-dark: #0a0a0a
--bg-card: #1a1a1a
--bg-card-hover: #2a2a2a
--text-primary: #ffffff
--text-secondary: #e0e0e0
--text-muted: #b0b0b0
--border-color: #ffffff
--navbar-bg: rgba(0, 0, 0, 0.95)
--navbar-border: #ffffff
--card-shadow: rgba(255, 255, 255, 0.1)
--heavy-shadow: rgba(255, 255, 255, 0.1)
```

Orange accent stays as-is. CTA buttons use black text on orange for maximum contrast.

## High Contrast Light Palette

```
--bg-darker: #ffffff
--bg-dark: #f5f5f5
--bg-card: #ffffff
--bg-card-hover: #eeeeee
--text-primary: #000000
--text-secondary: #1a1a1a
--text-muted: #333333
--border-color: #000000
--navbar-bg: rgba(255, 255, 255, 0.98)
--navbar-border: #000000
--card-shadow: rgba(0, 0, 0, 0.15)
--heavy-shadow: rgba(0, 0, 0, 0.2)
```

Darker orange (`#c53d00`) for better contrast on white backgrounds.

## CSS Approach

Add two override blocks in `styles.css`:

1. `[data-contrast="high"]` — dark high-contrast (default dark theme + contrast boost)
2. `[data-theme="light"][data-contrast="high"]` — light high-contrast

These override only the CSS custom properties. No structural CSS changes needed — the existing component styles consume the variables unchanged.

Also add a `.contrast-toggle-btn` class (same dimensions/style as `.theme-toggle-btn`).

## theme.js Changes

Add to the existing IIFE:

- `CONTRAST_KEY = 'rus_contrast'` — localStorage key
- Read saved contrast on load, apply `data-contrast` attribute
- `window.__toggleContrast()` — toggles `data-contrast` between `"high"` and `"normal"`
- `window.__updateContrastIcon()` — updates the button icon (filled vs outline half-circle)
- Call `__updateContrastIcon()` on DOMContentLoaded

## Navbar Layout Changes

### Landing page (`index.html`)

**Left side:** RUS logo, Features, About (Features/About move from right to left)

**Right side (not logged in):** contrast toggle, theme toggle, Log In, Sign Up

**Right side (logged in, standalone):** contrast toggle, theme toggle, Dashboard, Log Out

**Right side (SaaS mode):** contrast toggle, theme toggle, Dashboard, Log In

The `updateUI()` function's innerHTML rewrites must include both toggle buttons and call both `__updateThemeIcon()` and `__updateContrastIcon()`.

### Dashboard (`dashboard.html`)

**Right side:** contrast toggle, theme toggle, username, Log Out

### Login (`login.html`)

**Right side:** contrast toggle, theme toggle, Home, Sign Up

### Signup (`signup.html`)

**Right side:** contrast toggle, theme toggle, Home, Log In

### All other pages (admin, report, setup, maintenance, 404)

Add contrast toggle button to the left of the existing theme toggle button.

## Toggle Button

- Icon: `fa-circle-half-stroke` (Font Awesome)
- Same dimensions as theme toggle: 36x36px, 8px border-radius
- Uses `.contrast-toggle-btn` class (identical styling to `.theme-toggle-btn`)
- Active state: button gets `color: var(--rust-orange)` when high contrast is on, reverts to `var(--text-secondary)` when off

## Files Changed

| File | Change |
|---|---|
| `static/styles.css` | High contrast variable overrides, `.contrast-toggle-btn` |
| `static/theme.js` | Contrast toggle logic, persistence, icon updates |
| `static/index.html` | Navbar restructure (left/right split) + contrast button in static HTML and `updateUI()` |
| `static/dashboard.html` | Add contrast button, reorder nav items |
| `static/login.html` | Add contrast button, reorder nav items |
| `static/signup.html` | Add contrast button, reorder nav items |
| `static/admin.html` | Add contrast button, reorder nav items |
| `static/report.html` | Add contrast button, reorder nav items |
| `static/setup.html` | Add contrast button, reorder nav items |
| `static/maintenance.html` | Add contrast button, reorder nav items |
| `static/404.html` | Add contrast button, reorder nav items |
