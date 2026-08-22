#!/usr/bin/env node
// Entry point for the static/ page tests, used by `just test-js`, by the eighth
// pre-commit step and by .forgejo/workflows/check.yml.
//
// It wraps node's built-in runner because `node --test` exits 0 when its
// arguments match no file: the run would report green having asserted nothing.
// Discovering the files here and holding the pass count to a floor makes a
// vanished or unwired test a build failure instead.

import { spawn } from 'node:child_process';
import { readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const TESTS_DIR = fileURLToPath(new URL('.', import.meta.url));
const MIN_FILES = 3;
const MIN_TESTS = 14;

const files = readdirSync(TESTS_DIR)
  .filter((name) => name.endsWith('.test.mjs'))
  .sort()
  .map((name) => path.join(TESTS_DIR, name));

if (files.length < MIN_FILES) {
  console.error(`static/tests: found ${files.length} test files, expected at least ${MIN_FILES}`);
  process.exit(1);
}

const child = spawn(process.execPath, ['--test', '--test-reporter=tap', ...files], {
  stdio: ['ignore', 'pipe', 'inherit'],
});

let output = '';
child.stdout.on('data', (chunk) => {
  output += chunk;
  process.stdout.write(chunk);
});

child.on('error', (error) => {
  console.error(`static/tests: could not start the test runner: ${error.message}`);
  process.exit(1);
});

child.on('close', (code) => {
  const count = (label) => {
    const found = output.match(new RegExp(`^# ${label} (\\d+)$`, 'm'));
    return found ? Number(found[1]) : Number.NaN;
  };
  const pass = count('pass');
  const fail = count('fail');

  if (Number.isNaN(pass) || Number.isNaN(fail)) {
    console.error('static/tests: the runner printed no summary, so nothing can be trusted as run');
    process.exit(1);
  }
  if (code !== 0 || fail > 0) {
    console.error(`static/tests: ${fail} failing test(s) across ${files.length} files`);
    process.exit(code === 0 ? 1 : code);
  }
  if (pass < MIN_TESTS) {
    console.error(`static/tests: only ${pass} tests ran, expected at least ${MIN_TESTS}`);
    process.exit(1);
  }
  console.log(`static/tests: ${pass} tests passed across ${files.length} files`);
});
