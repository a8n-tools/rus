// Guards the harness itself: every page's script tags must still resolve, so a
// renamed or newly added asset fails here instead of quietly going untested.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { extractScripts, pageFiles } from './dom.mjs';

test('every page resolves each script tag it loads', () => {
  const pages = pageFiles();
  assert.ok(pages.length >= 9, `expected the shipped page set, found ${pages.length}`);

  for (const page of pages) {
    // extractScripts throws on a src with no file behind it.
    const scripts = extractScripts(page);
    for (const script of scripts) {
      assert.ok(script.code.length > 0, `${page} loads an empty script (${script.name})`);
    }
  }
});

test('the driven pages each ship exactly one inline script block', () => {
  for (const page of ['dashboard.html', 'signup.html']) {
    const inline = extractScripts(page).filter((script) => script.file === null);
    assert.equal(inline.length, 1, `${page} should keep its logic in one inline block`);
  }
});
