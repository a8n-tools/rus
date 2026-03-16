// Proactive token refresh for SaaS mode.
// Refreshes the access_token cookie every 13 minutes via the parent API.
(function () {
  'use strict';

  var REFRESH_INTERVAL_MS = 13 * 60 * 1000; // 13 minutes
  var RETRY_INTERVAL_MS = 30 * 1000; // 30 seconds on failure
  var refreshUrl = null;
  var timerId = null;

  function doRefresh() {
    if (!refreshUrl) return;
    fetch(refreshUrl, { method: 'POST', credentials: 'include' })
      .then(function (r) {
        if (r.ok) {
          schedule(REFRESH_INTERVAL_MS);
        } else {
          schedule(RETRY_INTERVAL_MS);
        }
      })
      .catch(function () {
        schedule(RETRY_INTERVAL_MS);
      });
  }

  function schedule(ms) {
    if (timerId) clearTimeout(timerId);
    timerId = setTimeout(doRefresh, ms);
  }

  // Refresh immediately when a backgrounded tab becomes visible
  document.addEventListener('visibilitychange', function () {
    if (document.visibilityState === 'visible' && refreshUrl) {
      doRefresh();
    }
  });

  // Fetch config to get refresh_url, then start the timer
  fetch('/api/config')
    .then(function (r) { return r.json(); })
    .then(function (config) {
      if (config.auth_mode === 'saas' && config.refresh_url) {
        refreshUrl = config.refresh_url;
        schedule(REFRESH_INTERVAL_MS);
      }
    })
    .catch(function (e) {
      console.error('saas-refresh: failed to load config', e);
    });
})();
