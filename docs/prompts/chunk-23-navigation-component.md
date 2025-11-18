# Chunk 23: Uniform Navigation Component Styles

## Context
Building on global styles. Create consistent navigation that appears on all pages.

## Goal
Add CSS for a uniform navbar with Rust branding and responsive design.

## Prompt

```text
I have global styles. Now add navigation component styles.

In static/styles.css, add navbar styles:

```css
/* Navigation */
.navbar {
  background-color: var(--rust-dark);
  padding: var(--spacing-md) var(--spacing-lg);
  border-bottom: 2px solid var(--rust-orange);
  position: sticky;
  top: 0;
  z-index: 1000;
}

.navbar-container {
  max-width: 1200px;
  margin: 0 auto;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.navbar-brand {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  font-size: 1.5rem;
  font-weight: 700;
  color: var(--text-primary);
  text-decoration: none;
}

.navbar-brand:hover {
  color: var(--rust-orange);
}

.navbar-brand .logo {
  color: var(--rust-orange);
  font-size: 1.8rem;
}

.navbar-nav {
  display: flex;
  align-items: center;
  gap: var(--spacing-lg);
  list-style: none;
}

.navbar-nav a {
  color: var(--text-secondary);
  font-weight: 500;
  padding: var(--spacing-xs) var(--spacing-sm);
  border-radius: var(--radius-sm);
  transition: color var(--transition-fast), background-color var(--transition-fast);
}

.navbar-nav a:hover {
  color: var(--text-primary);
  background-color: var(--rust-gray);
}

.navbar-nav a.active {
  color: var(--rust-orange);
}

.navbar-btn {
  background-color: var(--rust-orange);
  color: var(--text-primary);
  padding: var(--spacing-xs) var(--spacing-md);
  border-radius: var(--radius-md);
  font-weight: 500;
}

.navbar-btn:hover {
  background-color: var(--rust-orange-light);
  color: var(--text-primary);
}

.navbar-user {
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
}

.navbar-username {
  color: var(--text-secondary);
  font-size: 0.9rem;
}
```

Navbar features:
- Sticky positioning (stays at top)
- Orange bottom border accent
- Flex layout for alignment
- Brand logo with Rust crab emoji or R
- Navigation links with hover states
- Active state highlighting
- User info display (when logged in)
- Consistent spacing

The navbar will be used on all pages with slight variations:
- Index: Shows Login/Signup buttons
- Auth pages: Shows minimal nav
- Dashboard: Shows username and logout

HTML structure (for reference):
```html
<nav class="navbar">
  <div class="navbar-container">
    <a href="/" class="navbar-brand">
      <span class="logo">🦀</span> RUS
    </a>
    <ul class="navbar-nav">
      <!-- Links vary by page -->
    </ul>
  </div>
</nav>
```
```

## Expected Output
- .navbar styles with sticky position
- .navbar-brand with logo styling
- .navbar-nav for links
- Hover and active states
- Orange accent border
- Flexible layout
- User display area
