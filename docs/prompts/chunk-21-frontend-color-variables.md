# Chunk 21: Frontend CSS Color Variables

## Context
Backend is complete. Now updating frontend. Start with CSS custom properties for consistent theming.

## Goal
Define CSS variables for the Rust-themed color scheme (black and rusty orange).

## Prompt

```text
I have the backend complete. Now redesign the frontend with Rust theming.

Start with static/styles.css. At the very top, add CSS custom properties:

```css
:root {
  /* Rust Theme Colors */
  --rust-orange: #CE422B;
  --rust-orange-light: #E05A3A;
  --rust-orange-dark: #A33520;
  --rust-black: #0D0D0D;
  --rust-dark: #1A1A1A;
  --rust-gray: #2D2D2D;
  --rust-light-gray: #4A4A4A;

  /* Text Colors */
  --text-primary: #FFFFFF;
  --text-secondary: #CCCCCC;
  --text-muted: #888888;

  /* Status Colors */
  --success: #10B981;
  --error: #EF4444;
  --warning: #F59E0B;

  /* Spacing */
  --spacing-xs: 0.25rem;
  --spacing-sm: 0.5rem;
  --spacing-md: 1rem;
  --spacing-lg: 1.5rem;
  --spacing-xl: 2rem;

  /* Border Radius */
  --radius-sm: 4px;
  --radius-md: 8px;
  --radius-lg: 12px;

  /* Shadows */
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.3);
  --shadow-md: 0 4px 6px rgba(0, 0, 0, 0.4);
  --shadow-lg: 0 10px 15px rgba(0, 0, 0, 0.5);

  /* Transitions */
  --transition-fast: 150ms ease;
  --transition-normal: 300ms ease;
}
```

These variables:
- Rust orange: Official Rust brand color (#CE422B)
- Black theme: Professional, high contrast
- Consistent spacing: 0.25rem increments
- Shadows: Darker for black background
- Transitions: Smooth interactions

Benefits of CSS variables:
- Single source of truth for colors
- Easy theme adjustments
- Consistent look across all pages
- Can be overridden for specific components

The orange variants:
- rust-orange: Primary action color
- rust-orange-light: Hover states
- rust-orange-dark: Active/pressed states

This chunk only adds the variables. Next chunks will use them.
```

## Expected Output
- CSS custom properties in :root
- Rust brand colors defined
- Text colors for contrast
- Spacing scale established
- Shadow and transition utilities
- Foundation for consistent theming
