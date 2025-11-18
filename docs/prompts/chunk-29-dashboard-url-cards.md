# Chunk 29: Dashboard URL Card Styling

## Context
Building on dashboard structure. Add proper styling for URL cards and forms.

## Goal
Add CSS for dashboard layout, URL cards, and form styling.

## Prompt

```text
I have dashboard structure. Now add dashboard-specific styles.

Add to static/styles.css:

```css
/* Dashboard */
.dashboard-header {
  margin-bottom: var(--spacing-lg);
}

.dashboard-header h1 {
  margin-bottom: 0;
}

/* URL Form Section */
.url-form-section {
  margin-bottom: var(--spacing-xl);
}

.card {
  background-color: var(--rust-dark);
  padding: var(--spacing-lg);
  border-radius: var(--radius-lg);
  border: 1px solid var(--rust-gray);
}

.card h3 {
  margin-bottom: var(--spacing-md);
  color: var(--rust-orange);
}

.shorten-form .form-row {
  display: flex;
  gap: var(--spacing-md);
}

.shorten-form input {
  flex: 1;
}

.shorten-form button {
  white-space: nowrap;
}

.shorten-result {
  margin-top: var(--spacing-md);
  padding: var(--spacing-md);
  background-color: rgba(16, 185, 129, 0.1);
  border: 1px solid var(--success);
  border-radius: var(--radius-md);
  color: var(--success);
}

.shorten-result a {
  color: var(--success);
  font-weight: 600;
}

/* URLs List Section */
.urls-section {
  margin-bottom: var(--spacing-xl);
}

.urls-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: var(--spacing-lg);
}

.urls-header h3 {
  margin-bottom: 0;
}

.btn-sm {
  padding: var(--spacing-xs) var(--spacing-md);
  font-size: 0.9rem;
}

.btn-secondary {
  background-color: var(--rust-gray);
}

.btn-secondary:hover {
  background-color: var(--rust-light-gray);
}

.btn-logout {
  background-color: transparent;
  border: 1px solid var(--rust-gray);
  color: var(--text-secondary);
  padding: var(--spacing-xs) var(--spacing-md);
}

.btn-logout:hover {
  background-color: var(--rust-gray);
  color: var(--text-primary);
}

.loading {
  text-align: center;
  padding: var(--spacing-xl);
  color: var(--text-muted);
}

.empty-state {
  text-align: center;
  padding: var(--spacing-xl);
  background-color: var(--rust-dark);
  border-radius: var(--radius-lg);
  border: 1px dashed var(--rust-gray);
}

.empty-state p {
  color: var(--text-muted);
}

.urls-list {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
}

/* URL Card */
.url-card {
  background-color: var(--rust-dark);
  padding: var(--spacing-lg);
  border-radius: var(--radius-lg);
  border: 1px solid var(--rust-gray);
  transition: border-color var(--transition-fast);
}

.url-card:hover {
  border-color: var(--rust-orange);
}

.url-card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: var(--spacing-sm);
}

.url-name {
  font-weight: 600;
  font-size: 1.1rem;
  color: var(--text-primary);
}

.url-clicks {
  background-color: var(--rust-orange);
  color: var(--text-primary);
  padding: var(--spacing-xs) var(--spacing-sm);
  border-radius: var(--radius-sm);
  font-size: 0.85rem;
  font-weight: 500;
}

.url-short {
  margin-bottom: var(--spacing-sm);
}

.url-short a {
  font-size: 1rem;
  color: var(--rust-orange);
  font-weight: 500;
}

.url-original {
  color: var(--text-muted);
  font-size: 0.9rem;
  word-break: break-all;
  margin-bottom: var(--spacing-sm);
}

.url-created {
  color: var(--text-muted);
  font-size: 0.8rem;
}
```

These styles:
- Form with input and button in row
- Success result with green styling
- URL cards with hover effects
- Click count badge
- Responsive text handling (word-break)
- Empty state with dashed border
- Loading state centered
```

## Expected Output
- .card component styled
- Shorten form with flex row
- Success result styling
- URL cards with header, body, footer
- Click count badge
- Empty and loading states
- Hover effects on cards
