// Drives the Account section of the shipped static/dashboard.html: painting from
// GET /api/me, the changed-fields-only PATCH, and the two auth modes.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { loadPage } from './dom.mjs';

const STANDALONE_CONFIG = { auth_mode: 'standalone', maintenance_mode: false };
const SAAS_CONFIG = {
  auth_mode: 'saas',
  maintenance_mode: false,
  login_url: '/oauth2/login',
  logout_url: '/oauth2/logout',
};

const STORED_EMAIL = 'alerts@example.test';

// A standalone dashboard already painted from GET /api/me. `patch` answers the
// save; it defaults to echoing the account back unchanged.
function standalone({ me = {}, patch } = {}) {
  const account = { username: 'nate', is_admin: false, email: STORED_EMAIL, notify_new_location: true, ...me };
  return loadPage('dashboard.html', {
    storage: { rus_token: 'tok-standalone', rus_username: 'nate' },
    routes: {
      'GET /api/config': STANDALONE_CONFIG,
      'GET /api/me': account,
      'GET /api/urls': [],
      'PATCH /api/me': patch ?? ((call) => ({ ...account, ...call.json })),
    },
  });
}

// saas answers GET /api/me without an `email` key: the address comes from the
// OIDC identity and PATCH /api/me ignores it.
function saas({ me = {}, patch } = {}) {
  const account = { username: 'nate', notify_new_location: false, ...me };
  return loadPage('dashboard.html', {
    storage: {},
    routes: {
      'GET /api/config': SAAS_CONFIG,
      'GET /api/me': account,
      'GET /api/urls': [],
      'PATCH /api/me': patch ?? ((call) => ({ ...account, ...call.json })),
    },
  });
}

test('standalone paints both account controls from GET /api/me', async () => {
  const page = await standalone();

  assert.equal(page.el('accountEmail').value, STORED_EMAIL);
  assert.equal(page.el('notifyNewLocation').checked, true);
  assert.equal(page.el('accountEmailGroup').style.display, 'block');
  assert.equal(page.el('accountEmailManaged').style.display, 'none');
  assert.equal(page.el('accountLoading').style.display, 'none');
  assert.equal(page.el('accountForm').style.display, 'block');
});

test('standalone reads the account with a bearer token and no cookie', async () => {
  const page = await standalone();
  const reads = page.callsTo('GET', '/api/me');

  assert.ok(reads.length > 0, 'the dashboard must read /api/me on load');
  for (const read of reads) {
    assert.equal(read.headers.Authorization, 'Bearer tok-standalone');
    assert.equal(read.credentials, undefined);
  }
});

test('saas paints the toggle and hides the address control', async () => {
  const page = await saas({ me: { notify_new_location: false } });

  assert.equal(page.el('notifyNewLocation').checked, false);
  assert.equal(page.el('accountEmail').value, '', 'saas returns no email, so the input stays untouched');
  assert.equal(page.el('accountEmailGroup').style.display, 'none');
  assert.equal(page.el('accountEmailManaged').style.display, 'block');
  assert.equal(page.el('accountForm').style.display, 'block');
});

test('saas reads the account with the session cookie and no bearer header', async () => {
  const page = await saas();
  const reads = page.callsTo('GET', '/api/me');

  assert.ok(reads.length > 0, 'the dashboard must read /api/me on load');
  for (const read of reads) {
    assert.equal(read.credentials, 'include');
    assert.equal(read.headers.Authorization, undefined);
  }
});

test('a toggle-only save submits notify_new_location and nothing else', async () => {
  const page = await standalone();
  page.el('notifyNewLocation').checked = false;

  await page.submit('accountForm');

  const writes = page.callsTo('PATCH', '/api/me');
  assert.equal(writes.length, 1);
  assert.deepEqual(writes[0].json, { notify_new_location: false });
  assert.deepEqual(Object.keys(writes[0].json), ['notify_new_location'], 'an untouched address must not be resent');
  assert.equal(page.el('accountSuccess').textContent, 'Account settings saved.');
  assert.equal(page.el('accountSuccess').classList.contains('show'), true);
});

test('an address-only save submits the trimmed address and nothing else', async () => {
  const page = await standalone();
  page.el('accountEmail').value = '  NEW@Example.Test  ';

  await page.submit('accountForm');

  const writes = page.callsTo('PATCH', '/api/me');
  assert.equal(writes.length, 1);
  assert.deepEqual(writes[0].json, { email: 'new@example.test' });
  assert.deepEqual(Object.keys(writes[0].json), ['email'], 'an untouched toggle must not be resent');
});

test('a blank address submits an empty string and clears the stored value', async () => {
  const page = await standalone({ patch: () => ({ email: null, notify_new_location: true }) });
  page.el('accountEmail').value = '';

  await page.submit('accountForm');

  const writes = page.callsTo('PATCH', '/api/me');
  assert.equal(writes.length, 1);
  assert.deepEqual(writes[0].json, { email: '' });
  assert.equal(page.el('accountEmail').value, '');

  // The cleared address is now the confirmed state, so resubmitting sends nothing.
  await page.submit('accountForm');
  assert.equal(page.callsTo('PATCH', '/api/me').length, 1);
});

test('an unchanged submit issues no request at all', async () => {
  const page = await standalone();
  const before = page.calls.length;

  await page.submit('accountForm');

  assert.equal(page.calls.length, before, 'a no-op save must not reach the API');
  assert.equal(page.callsTo('PATCH', '/api/me').length, 0);
  assert.equal(page.el('accountSuccess').textContent, 'No account changes to save.');
});

test('a 400 shows the API message, reverts the toggle and keeps the typed address', async () => {
  const page = await standalone({
    patch: () => ({ status: 400, body: { error: 'A valid email address is required.' } }),
  });
  page.el('accountEmail').value = 'not-an-email';
  page.el('notifyNewLocation').checked = false;

  await page.submit('accountForm');

  assert.equal(page.el('accountError').textContent, 'A valid email address is required.');
  assert.equal(page.el('accountError').classList.contains('show'), true);
  assert.equal(page.el('accountSuccess').classList.contains('show'), false);
  assert.equal(page.el('notifyNewLocation').checked, true, 'the toggle claims a stored state, so it reverts');
  assert.equal(page.el('accountEmail').value, 'not-an-email', 'the address is an edit buffer, so it is kept');
  assert.equal(page.el('accountSaveBtn').disabled, false);
  assert.equal(page.el('accountSaveBtn').textContent, 'Save Account Settings');
});

test('a save repaints from the response body, not from what was submitted', async () => {
  const page = await standalone({
    patch: () => ({ email: 'canonical@example.test', notify_new_location: false }),
  });
  page.el('notifyNewLocation').checked = false;

  await page.submit('accountForm');

  assert.equal(page.el('accountEmail').value, 'canonical@example.test');
  assert.equal(page.el('notifyNewLocation').checked, false);
});

test('saas never submits an address even when the input holds one', async () => {
  const page = await saas();
  page.el('accountEmail').value = 'sneaky@example.test';
  page.el('notifyNewLocation').checked = true;

  await page.submit('accountForm');

  const writes = page.callsTo('PATCH', '/api/me');
  assert.equal(writes.length, 1);
  assert.deepEqual(writes[0].json, { notify_new_location: true });
  assert.equal(writes[0].credentials, 'include');
});
