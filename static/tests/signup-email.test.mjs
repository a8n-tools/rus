// Drives the shipped static/signup.html: the optional security-alert address is
// omitted entirely when blank, so registration never stores a placeholder.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { loadPage } from './dom.mjs';

function signupPage({ register } = {}) {
  return loadPage('signup.html', {
    storage: {},
    routes: {
      'GET /api/setup/required': { setup_required: false },
      'GET /api/config': { allow_registration: true, auth_mode: 'standalone' },
      'POST /api/register': register ?? { token: 'tok', username: 'nate', refresh_token: 'ref' },
    },
  });
}

function fill(page, email) {
  page.el('username').value = 'nate';
  page.el('password').value = 'password123';
  page.el('confirmPassword').value = 'password123';
  page.el('email').value = email;
}

test('a blank address posts no email key', async () => {
  const page = await signupPage();
  fill(page, '   ');

  await page.submit('signupForm');

  const posts = page.callsTo('POST', '/api/register');
  assert.equal(posts.length, 1);
  assert.deepEqual(posts[0].json, { username: 'nate', password: 'password123' });
  assert.equal('email' in posts[0].json, false, 'a blank address must not reach the API');
  assert.equal(page.localStorage.getItem('rus_token'), 'tok');
  assert.deepEqual(page.navigations, ['dashboard.html']);
});

test('a filled address posts the trimmed value', async () => {
  const page = await signupPage();
  fill(page, '  alerts@example.test  ');

  await page.submit('signupForm');

  const posts = page.callsTo('POST', '/api/register');
  assert.equal(posts.length, 1);
  assert.deepEqual(posts[0].json, {
    username: 'nate',
    password: 'password123',
    email: 'alerts@example.test',
  });
});

test('a rejected registration surfaces the API message and re-enables the button', async () => {
  const page = await signupPage({ register: { status: 400, body: { error: 'Username already taken' } } });
  fill(page, 'alerts@example.test');

  await page.submit('signupForm');

  assert.equal(page.el('error').textContent, 'Username already taken');
  assert.equal(page.el('error').classList.contains('show'), true);
  assert.equal(page.el('signupBtn').disabled, false);
  assert.deepEqual(page.navigations, [], 'a failed registration must not navigate');
});
