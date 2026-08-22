// Minimal DOM, fetch and localStorage stubs plus a loader that evaluates a
// shipped page's real script tags in a vm context, so the tests drive the page
// as served instead of a copy of its logic.

import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

export const STATIC_DIR = fileURLToPath(new URL('../', import.meta.url));

// src="k9f3x2m7.js" is served from static/auth.js (see serve_auth_js in
// src/handlers/pages.rs), so the loader has to follow the same mapping.
const SERVED_AS = { 'k9f3x2m7.js': 'auth.js' };

const SCRIPT_TAG = /<script\b([^>]*)>([\s\S]*?)<\/script>/gi;
const SRC_ATTR = /\bsrc\s*=\s*"([^"]+)"/i;

/** Every page shipped in static/, newest listing straight off disk. */
export function pageFiles() {
  return readdirSync(STATIC_DIR)
    .filter((name) => name.endsWith('.html'))
    .sort();
}

/**
 * Every script a page runs, in document order. An external src that resolves to
 * no file throws: a silently dropped script would leave the page untested while
 * the suite still reported green.
 */
export function extractScripts(pageFile) {
  const html = readFileSync(path.join(STATIC_DIR, pageFile), 'utf8');
  const scripts = [];
  for (const [, attrs, body] of html.matchAll(SCRIPT_TAG)) {
    const src = SRC_ATTR.exec(attrs);
    if (!src) {
      scripts.push({ name: `${pageFile}#inline`, file: null, code: body });
      continue;
    }
    const file = SERVED_AS[src[1]] ?? src[1];
    const full = path.join(STATIC_DIR, file);
    let code;
    try {
      code = readFileSync(full, 'utf8');
    } catch (cause) {
      throw new Error(`${pageFile} loads "${src[1]}" but ${full} does not exist`, { cause });
    }
    scripts.push({ name: file, file, code });
  }
  return scripts;
}

class StubElement {
  constructor(tagName = 'div', id = '') {
    this.tagName = String(tagName).toUpperCase();
    this.id = id;
    this.style = {};
    this.value = '';
    this.checked = false;
    this.disabled = false;
    this.textContent = '';
    this.innerHTML = '';
    this.className = '';
    this.title = '';
    this.type = '';
    this.src = '';
    this.children = [];
    this.attributes = {};
    this.listeners = new Map();
    const classes = new Set();
    this.classes = classes;
    this.classList = {
      add: (...names) => names.forEach((n) => classes.add(n)),
      remove: (...names) => names.forEach((n) => classes.delete(n)),
      contains: (name) => classes.has(name),
      toggle: (name, force) => {
        const on = force === undefined ? !classes.has(name) : Boolean(force);
        if (on) classes.add(name);
        else classes.delete(name);
        return on;
      },
    };
  }

  addEventListener(type, fn) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push(fn);
  }

  removeEventListener(type, fn) {
    const list = this.listeners.get(type) ?? [];
    const at = list.indexOf(fn);
    if (at !== -1) list.splice(at, 1);
  }

  setAttribute(name, value) {
    this.attributes[name] = String(value);
  }

  getAttribute(name) {
    return Object.prototype.hasOwnProperty.call(this.attributes, name) ? this.attributes[name] : null;
  }

  appendChild(child) {
    this.children.push(child);
    return child;
  }

  removeChild(child) {
    const at = this.children.indexOf(child);
    if (at !== -1) this.children.splice(at, 1);
    return child;
  }

  remove() {}

  focus() {}

  select() {}

  querySelector() {
    return null;
  }

  querySelectorAll() {
    return [];
  }

  /** Fire every handler for `type` and await async ones, so tests can assert after. */
  async dispatch(type, extra = {}) {
    let defaultPrevented = false;
    const event = {
      type,
      target: this,
      currentTarget: this,
      preventDefault() {
        defaultPrevented = true;
      },
      stopPropagation() {},
      ...extra,
    };
    for (const fn of [...(this.listeners.get(type) ?? [])]) await fn.call(this, event);
    return { defaultPrevented };
  }
}

class StubDocument {
  constructor() {
    this.readyState = 'complete';
    this.documentElement = new StubElement('html');
    this.body = new StubElement('body');
    this.byId = new Map();
    this.bySelector = new Map();
    this.listeners = new Map();
  }

  // Auto-create: the tests care about the ids the logic touches, and an
  // unstubbed id returning null would crash the page instead of exercising it.
  getElementById(id) {
    if (!this.byId.has(id)) this.byId.set(id, new StubElement('div', id));
    return this.byId.get(id);
  }

  querySelector(selector) {
    if (!this.bySelector.has(selector)) this.bySelector.set(selector, new StubElement('div'));
    return this.bySelector.get(selector);
  }

  querySelectorAll() {
    return [];
  }

  createElement(tagName) {
    return new StubElement(tagName);
  }

  createTextNode(text) {
    const node = new StubElement('#text');
    node.textContent = text;
    return node;
  }

  addEventListener(type, fn) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push(fn);
  }

  removeEventListener() {}

  execCommand() {
    return true;
  }
}

function makeStorage(seed = {}) {
  const map = new Map(Object.entries(seed));
  return {
    getItem: (key) => (map.has(key) ? map.get(key) : null),
    setItem: (key, value) => map.set(key, String(value)),
    removeItem: (key) => map.delete(key),
    clear: () => map.clear(),
    snapshot: () => Object.fromEntries(map),
  };
}

function makeResponse({ status = 200, body = {} }) {
  const response = {
    status,
    ok: status >= 200 && status < 300,
    headers: { get: () => null },
    json: async () => structuredClone(body),
    text: async () => JSON.stringify(body),
    blob: async () => ({ size: 0 }),
    clone: () => response,
  };
  return response;
}

/**
 * fetch stub over a `"METHOD /path"` route table. Records every call so a test
 * can assert the payload shape and the auth headers, not just that a request
 * happened. An unrouted request throws rather than resolving to a default.
 */
function makeFetch(routes, calls) {
  return async function fetch(url, options = {}) {
    const method = String(options.method ?? 'GET').toUpperCase();
    const call = {
      url,
      method,
      headers: { ...(options.headers ?? {}) },
      credentials: options.credentials,
      body: options.body,
      json: options.body === undefined ? undefined : JSON.parse(options.body),
    };
    calls.push(call);
    const route = routes[`${method} ${url}`] ?? routes[url];
    if (route === undefined) throw new Error(`no stub route for ${method} ${url}`);
    const result = typeof route === 'function' ? await route(call) : route;
    // A route returns either a bare body or a `{ status, body }` descriptor.
    const descriptor =
      result !== null && typeof result === 'object' && typeof result.status === 'number' && 'body' in result;
    return makeResponse(descriptor ? result : { body: result });
  };
}

/**
 * Evaluate every script tag of `pageFile` in document order inside one vm
 * context wired to the stubs, then let the page's own async boot settle.
 */
export async function loadPage(pageFile, { routes = {}, storage = {}, origin = 'https://rus.test' } = {}) {
  const calls = [];
  const navigations = [];
  const logs = [];
  const timers = [];
  const document = new StubDocument();
  const localStorage = makeStorage(storage);

  const location = {
    origin,
    pathname: `/${pageFile}`,
    search: '',
    _href: `${origin}/${pageFile}`,
    get href() {
      return this._href;
    },
    set href(value) {
      this._href = value;
      navigations.push(value);
    },
    assign(value) {
      this.href = value;
    },
    reload() {},
  };

  const sandbox = {
    document,
    localStorage,
    location,
    fetch: makeFetch(routes, calls),
    navigator: { clipboard: { writeText: async () => {} }, userAgent: 'rus-tests' },
    isSecureContext: true,
    matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
    // Timers are recorded, never fired: the only use is a delayed banner reset,
    // and a live timer would either hang the run or race the assertions.
    setTimeout: (fn, ms) => timers.push({ fn, ms }) - 1,
    clearTimeout: () => {},
    setInterval: () => 0,
    clearInterval: () => {},
    requestAnimationFrame: (fn) => timers.push({ fn, ms: 0 }) - 1,
    URL: { createObjectURL: () => 'blob:stub', revokeObjectURL: () => {} },
    alert: (message) => logs.push(['alert', message]),
    confirm: () => true,
    console: {
      log: (...args) => logs.push(['log', ...args]),
      warn: (...args) => logs.push(['warn', ...args]),
      error: (...args) => logs.push(['error', ...args]),
    },
  };
  vm.createContext(sandbox);
  sandbox.window = sandbox;
  sandbox.globalThis = sandbox;
  sandbox.self = sandbox;

  for (const script of extractScripts(pageFile)) {
    vm.runInContext(script.code, sandbox, { filename: script.name });
  }

  const harness = {
    window: sandbox,
    document,
    localStorage,
    calls,
    navigations,
    logs,
    timers,
    el: (id) => document.getElementById(id),
    settle,
    /** Calls matching a method and url, in order. */
    callsTo(method, url) {
      return calls.filter((call) => call.method === method.toUpperCase() && call.url === url);
    },
    /** Fire a form's submit handlers and wait for the async ones to finish. */
    async submit(formId) {
      await document.getElementById(formId).dispatch('submit');
      await settle();
    },
  };
  await settle();
  return harness;
}

/** Drain the microtask and immediate queues so the page's async boot finishes. */
export async function settle(ticks = 40) {
  for (let i = 0; i < ticks; i += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
}
