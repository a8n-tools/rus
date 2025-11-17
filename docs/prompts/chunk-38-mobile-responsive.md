# Chunk 38: Mobile Responsive Layout

## Context
Building on QR modal. Need to make entire app mobile-friendly.

## Goal
Add responsive CSS for mobile and tablet viewports.

## Prompt

```text
I have QR code modal working. Now add mobile responsiveness.

Add media queries to styles.css at the end:

```css
/* Mobile Responsive */
@media (max-width: 768px) {
  /* Typography */
  h1 {
    font-size: 2rem;
  }

  h2 {
    font-size: 1.5rem;
  }

  .hero h1 {
    font-size: 2rem;
  }

  .hero-subtitle {
    font-size: 1rem;
  }

  /* Container */
  .container {
    padding: var(--spacing-md);
  }

  /* Navbar */
  .navbar {
    padding: var(--spacing-sm) var(--spacing-md);
  }

  .navbar-container {
    flex-direction: column;
    gap: var(--spacing-sm);
  }

  .navbar-brand {
    font-size: 1.25rem;
  }

  .navbar-nav {
    gap: var(--spacing-sm);
  }

  .navbar-user {
    flex-direction: column;
    gap: var(--spacing-xs);
    text-align: center;
  }

  /* Hero */
  .hero-actions {
    flex-direction: column;
    align-items: center;
  }

  .btn {
    width: 100%;
    max-width: 250px;
    text-align: center;
  }

  /* Features */
  .feature-grid {
    grid-template-columns: 1fr;
  }

  /* Auth Forms */
  .auth-card {
    padding: var(--spacing-md);
    margin: var(--spacing-md);
  }

  /* Dashboard */
  .shorten-form .form-row {
    flex-direction: column;
  }

  .shorten-form button {
    width: 100%;
  }

  .urls-header {
    flex-direction: column;
    align-items: stretch;
    gap: var(--spacing-md);
  }

  .urls-controls {
    flex-direction: column;
  }

  #filter-input {
    width: 100%;
  }

  select {
    width: 100%;
  }

  /* URL Cards */
  .url-card {
    padding: var(--spacing-md);
  }

  .url-card-header {
    flex-direction: column;
    align-items: flex-start;
    gap: var(--spacing-xs);
  }

  .url-clicks {
    align-self: flex-start;
  }

  .url-actions {
    flex-wrap: wrap;
  }

  .btn-action {
    flex: 1;
    min-width: calc(50% - var(--spacing-xs));
    text-align: center;
    font-size: 0.8rem;
    padding: var(--spacing-sm);
  }

  /* Modals */
  .modal-content {
    width: 95%;
    margin: var(--spacing-sm);
    max-height: 95vh;
  }

  .modal-header {
    padding: var(--spacing-md);
  }

  .modal-body {
    padding: var(--spacing-md);
  }

  .chart-tabs {
    flex-wrap: wrap;
  }

  .tab-btn {
    flex: 1;
    text-align: center;
  }

  .analytics-summary {
    flex-direction: column;
  }

  .qr-actions {
    flex-direction: column;
  }

  .qr-actions button {
    width: 100%;
  }
}

@media (max-width: 480px) {
  /* Extra small screens */
  .navbar-brand .logo {
    font-size: 1.5rem;
  }

  .hero h1 {
    font-size: 1.75rem;
  }

  .feature-icon {
    font-size: 2rem;
  }

  .url-actions {
    flex-direction: column;
  }

  .btn-action {
    width: 100%;
    min-width: 100%;
  }

  .chart-container {
    height: 250px;
  }
}

/* Touch-friendly improvements */
@media (hover: none) {
  /* Remove hover effects on touch devices */
  .url-card:hover {
    border-color: var(--rust-gray);
    transform: none;
  }

  .feature-card:hover {
    transform: none;
    border-color: var(--rust-gray);
  }

  /* Larger touch targets */
  button, .btn, .btn-action {
    min-height: 44px;
  }

  input, select, textarea {
    min-height: 44px;
  }
}
```

Key responsive features:
- Stacked layouts on mobile
- Full-width buttons
- Smaller typography
- Reduced padding
- Wrapping action buttons
- Collapsible navbar
- Touch-friendly 44px minimum targets
- Disabled hover on touch devices

The app should now work well on:
- Desktop (1200px+)
- Tablet (768px-1199px)
- Mobile (480px-767px)
- Small mobile (<480px)
```

## Expected Output
- Media queries for 768px and 480px breakpoints
- Stacked layouts for mobile
- Full-width form elements
- Responsive typography
- Touch-friendly sizing (44px min)
- Disabled hover on touch devices
- Flexible action buttons
- Modal fits mobile screens
