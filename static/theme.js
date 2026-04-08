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
    btn.classList.toggle('active', isHigh);
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
