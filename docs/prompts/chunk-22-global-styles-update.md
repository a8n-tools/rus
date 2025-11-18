# Chunk 22: Global Styles with Rust Theme

## Context
Building on CSS variables. Apply the Rust theme to global styles.

## Goal
Update body, typography, and base element styles to use Rust theme.

## Prompt

```text
I have CSS variables defined. Now apply them globally.

In static/styles.css, update base styles (after :root variables):

```css
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
  background-color: var(--rust-black);
  color: var(--text-primary);
  line-height: 1.6;
  min-height: 100vh;
}

a {
  color: var(--rust-orange);
  text-decoration: none;
  transition: color var(--transition-fast);
}

a:hover {
  color: var(--rust-orange-light);
}

h1, h2, h3, h4, h5, h6 {
  color: var(--text-primary);
  margin-bottom: var(--spacing-md);
}

h1 {
  font-size: 2.5rem;
  font-weight: 700;
}

h2 {
  font-size: 2rem;
  font-weight: 600;
}

h3 {
  font-size: 1.5rem;
  font-weight: 600;
}

p {
  margin-bottom: var(--spacing-md);
  color: var(--text-secondary);
}

button {
  cursor: pointer;
  font-family: inherit;
  font-size: 1rem;
  padding: var(--spacing-sm) var(--spacing-lg);
  border: none;
  border-radius: var(--radius-md);
  background-color: var(--rust-orange);
  color: var(--text-primary);
  transition: background-color var(--transition-fast), transform var(--transition-fast);
}

button:hover {
  background-color: var(--rust-orange-light);
  transform: translateY(-1px);
}

button:active {
  background-color: var(--rust-orange-dark);
  transform: translateY(0);
}

button:disabled {
  background-color: var(--rust-gray);
  cursor: not-allowed;
  transform: none;
}

input, textarea {
  font-family: inherit;
  font-size: 1rem;
  padding: var(--spacing-sm) var(--spacing-md);
  border: 2px solid var(--rust-gray);
  border-radius: var(--radius-md);
  background-color: var(--rust-dark);
  color: var(--text-primary);
  transition: border-color var(--transition-fast);
}

input:focus, textarea:focus {
  outline: none;
  border-color: var(--rust-orange);
}

input::placeholder {
  color: var(--text-muted);
}

.container {
  max-width: 1200px;
  margin: 0 auto;
  padding: var(--spacing-lg);
}
```

Key changes from original:
- Black background instead of purple gradient
- Orange accent instead of purple
- Darker input fields
- Orange focus states
- Consistent spacing using variables
- Smooth transitions
- Better contrast for accessibility

Remove any existing conflicting global styles (like old gradient backgrounds).
```

## Expected Output
- CSS reset applied
- Body uses black background
- Links are orange with hover effects
- Buttons are orange with states
- Inputs have dark theme
- Orange focus indicators
- .container utility class
