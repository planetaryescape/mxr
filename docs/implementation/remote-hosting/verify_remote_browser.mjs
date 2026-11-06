#!/usr/bin/env node
// Owned HTTPS proxy and one browser journey; stdin carries credentials, stdout carries checks only.
import { createInterface } from 'node:readline';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import https from 'node:https';
import http from 'node:http';
import net from 'node:net';

const input = createInterface({ input: process.stdin })[Symbol.asyncIterator]();
const config = JSON.parse((await input.next()).value);
const emit = (value) => process.stdout.write(`${JSON.stringify(value)}\n`);

if (process.argv[2] === 'proxy') {
  const [host, rawPort] = config.upstream.split(':');
  const port = Number(rawPort);
  const server = https.createServer({ cert: readFileSync(config.certificate), key: readFileSync(config.key) }, (request, response) => {
    const upstream = http.request({ host, port, method: request.method, path: request.url, headers: request.headers }, (answer) => {
      if (answer.statusCode === 403 || request.url === '/api/v1/remote/browser-session') {
        process.stderr.write(`${JSON.stringify({ event: 'proxy-browser-policy', status: answer.statusCode, method: request.method, path: new URL(request.url, 'http://fixture').pathname, host: request.headers.host, origin: request.headers.origin ?? null, sec_fetch_site: request.headers['sec-fetch-site'] ?? null })}\n`);
      }
      response.writeHead(answer.statusCode, answer.headers);
      answer.pipe(response);
    });
    upstream.on('error', () => { if (!response.headersSent) response.writeHead(502); response.end(); });
    request.pipe(upstream);
  });
  server.on('upgrade', (request, socket, head) => {
    const upstream = net.connect({ host, port }, () => {
      upstream.write(`${request.method} ${request.url} HTTP/${request.httpVersion}\r\n${request.rawHeaders.reduce((lines, value, index, headers) => index % 2 ? lines : `${lines}${value}: ${headers[index + 1]}\r\n`, '')}\r\n`);
      if (head.length) upstream.write(head);
      socket.pipe(upstream).pipe(socket);
    });
    upstream.on('error', () => socket.destroy());
    socket.on('error', () => upstream.destroy());
    socket.on('close', () => upstream.destroy());
  });
  server.listen(config.listen, '127.0.0.1', () => emit({ event: 'ready' }));
} else {
  const require = createRequire(`${config.repo}/apps/web/package.json`);
  const { chromium, expect } = require('@playwright/test');
  const launchOptions = { headless: true };
  if (config.browser_channel === 'chrome') launchOptions.channel = 'chrome';
  const browser = await chromium.launch(launchOptions);
  let step = 'launch';
  const failures = [];
  const check = (name) => emit({ event: 'check', name });
  async function control(action) {
    emit({ action });
    const answer = await input.next();
    if (answer.done) throw new Error('Control ended');
    const response = JSON.parse(answer.value);
    if (!response.continue) throw new Error('Control ended');
    return response;
  }
  async function loginPage(page) {
    await page.goto(`${config.origin}/api/v1/remote/browser-launch`);
    if (config.diagnostic_same_origin_referrer) {
      await page.evaluate(() => {
        const meta = document.createElement('meta');
        meta.name = 'referrer';
        meta.content = 'same-origin';
        document.head.appendChild(meta);
      });
    }
  }
  try {
    const contexts = {}, pages = {};
    for (const name of ['a', 'b']) {
      step = `login-${name}`;
      contexts[name] = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1440, height: 900 } });
      pages[name] = await contexts[name].newPage();
      const page = pages[name];
      page.on('response', async (response) => {
        if (response.status() >= 400) {
          const failure = { path: new URL(response.url()).pathname, status: response.status() };
          if (failure.path === '/api/v1/platform/llm/status') {
            try {
              const body = await response.json();
              failure.error = body.error;
              failure.code = body.code;
            } catch { /* A proxy may return an empty error response. */ }
          }
          failures.push(failure);
          if (failures.length > 20) failures.shift();
        }
      });
      page.setDefaultTimeout(20000);
      await loginPage(page);
      await page.getByLabel('Connection code').fill(config.codes[name]);
      await page.getByRole('button', { name: 'Connect', exact: true }).click();
      await expect(page.getByLabel('Connection code')).toHaveCount(0);
      const cookie = (await contexts[name].cookies()).find((item) => item.name === '__Host-mxr-session');
      if (!cookie?.httpOnly || !cookie.secure || cookie.sameSite !== 'Strict') throw new Error('Cookie protections missing');
      const exposed = await page.evaluate(() => ({ cookies: document.cookie, storage: JSON.stringify(localStorage), url: location.href }));
      if (Object.values(exposed).some((value) => value.includes(cookie.value) || Object.values(config.codes).some((code) => value.includes(code)))) throw new Error('Browser credential exposure');
      const replay = await contexts[name].request.post(`${config.origin}/api/v1/remote/browser-session`, { headers: { origin: config.origin }, form: { code: config.codes[name] } });
      if (replay.status() !== 401) throw new Error('Launch code replay accepted');
      check(`Independent browser ${name} authenticates with protected cookie and single-use code`);
      if (config.diagnostic_owner_conflict) {
        const diagnostic = await page.evaluate(async () => {
          const response = await fetch('/api/v1/admin/status');
          const body = await response.json();
          return { status: response.status, error: body.error, code: body.code, details: body.details,
            request_owner: null,
            owner_storage: Object.fromEntries(Object.entries(localStorage).filter(([key]) => /workspace|owner/i.test(key))) };
        });
        process.stderr.write(`${JSON.stringify({ event: 'owner-conflict-diagnostic', ...diagnostic })}\n`);
        emit({ event: 'complete' });
        await browser.close();
        process.stdin.destroy();
        process.exit(0);
      }

    }
    const page = pages.a;
    step = 'relaunch-with-existing-cookie';
    const oldCookie = (await contexts.a.cookies()).find((item) => item.name === '__Host-mxr-session');
    const launch = await control('relaunch');
    await loginPage(page);
    await page.getByLabel('Connection code').fill(launch.browser_code);
    await page.getByRole('button', { name: 'Connect', exact: true }).click();
    await expect(page.getByLabel('Connection code')).toHaveCount(0);
    const replacedCookie = (await contexts.a.cookies()).find((item) => item.name === '__Host-mxr-session');
    if (replacedCookie?.value === oldCookie?.value) throw new Error('Relaunch kept the old child credential');
    const retired = await contexts.a.request.get(`${config.origin}/api/v1/auth/local-token`, { headers: { cookie: `${oldCookie.name}=${oldCookie.value}` } });
    if (retired.status() !== 401) throw new Error('Replaced child session remained usable');
    check('Repeated browser launch exchanges the selected code and retires its previous child session');
    step = 'remote-llm-status';
    const llm = await page.evaluate(async () => {
      const response = await fetch('/api/v1/platform/llm/status');
      return { http_status: response.status, payload: await response.json() };
    });
    if (llm.http_status !== 200) throw new Error('Remote model availability status is unavailable');
    const status = llm.payload.status;
    if (!status || status.enabled !== false || typeof status.model !== 'string'
        || typeof status.configured_model !== 'string') throw new Error('Fixture disabled-model state was not preserved');
    if (status.base_url != null || status.api_key_env != null
        || !Array.isArray(status.keys) || status.keys.length !== 0) throw new Error('Remote model status exposed host configuration');
    if (!Array.isArray(status.features) || status.features.some((feature) =>
      feature.base_url != null || (feature.endpoint != null && feature.endpoint.base_url !== ''))) throw new Error('Remote feature status exposed an endpoint address');
    if (typeof status.policy_key !== 'string' || !status.policy_key.length
        || status.policy_key.length > 256 || !/^[A-Za-z0-9:_-]+$/.test(status.policy_key)
        || /^(https?|file):/i.test(status.policy_key)) throw new Error('AI-approved status has no opaque policy identity');
    check('Remote model status preserves disabled availability and opaque policy identity without host URLs or key locations');
    step = 'search-and-read';
    await page.goto(`${config.origin}/search?q=${encodeURIComponent('subject:"Q1 Report review"')}`);
    const rows = page.getByTestId('mailbox-list').getByRole('option');
    await expect(rows.first()).toBeVisible();
    const selectedRow = rows.first();
    const rowId = await selectedRow.getAttribute('id');
    if (!rowId) throw new Error('Search row has no selection identity');
    await selectedRow.click();
    await expect(page.getByTestId('mailbox-list')).toHaveAttribute('aria-activedescendant', rowId);
    // Pointer-open previews keep the list active. Enter explicitly focuses the reader.
    await page.keyboard.press('Enter');
    await expect(page.getByRole('article', { name: /^Conversation:/ })).toHaveAttribute('data-active-pane', 'true');
    check('Remote SPA searches and reads hosted synthetic mail');
    step = 'archive-preview-and-submit';
    const archiveResponses = Promise.all([
      page.waitForResponse((r) => r.url().endsWith('/archive-operations/preview')),
      page.waitForResponse((r) => r.url().endsWith('/archive-operations/submit')),
    ]);
    await page.keyboard.press('e');
    const [preview, submit] = await archiveResponses;
    if (!preview.ok() || !submit.ok()) throw new Error('Archive operation rejected');
    const reviewed = (await preview.json()).operation;
    const submitted = submit.request().postDataJSON();
    if (JSON.stringify(reviewed.identity) !== JSON.stringify(submitted.identity)
        || reviewed.review_id !== submitted.review_id
        || JSON.stringify(reviewed.items.map((item) => item.message_id)) !== JSON.stringify(submitted.mutation.message_ids)) throw new Error('Archive selection changed');
    check('Remote archive commits exactly the daemon previewed selection');
    await page.keyboard.press('Escape');
    step = 'compose';
    await page.keyboard.press('c');
    const composer = page.getByRole('dialog', { name: 'New message' });
    await expect(composer).toBeVisible();
    await composer.getByRole('combobox', { name: 'To' }).fill('alice@example.com');
    await composer.getByRole('textbox', { name: 'Subject' }).fill('Synthetic remote browser send');
    await composer.locator('.cm-content').click();
    await page.keyboard.press('i');
    await page.keyboard.type('Synthetic remote browser message.');
    await page.keyboard.press('Escape');
    step = 'upload-and-save';
    const uploaded = page.waitForResponse((r) => r.url().endsWith('/compose/session/attachment') && r.request().postDataJSON()?.operation === 'finalize');
    await composer.getByLabel('Attach files').setInputFiles({ name: 'fixture.txt', mimeType: 'text/plain', buffer: Buffer.from('remote-browser-attachment\n') });
    if (!(await uploaded).ok()) throw new Error('Attachment upload rejected');
    await expect(composer.getByRole('button', { name: 'Remove fixture.txt' })).toBeVisible();
    await control('draft_saved');
    check('Remote browser uploads portable bytes and persists its draft');
    step = 'send';
    const sendPromise = page.waitForResponse((r) => r.url().endsWith('/compose/session/send'), { timeout: 40000 });
    await composer.getByRole('button', { name: /^Send (⌘|Ctrl)/ }).click();
    if (!(await sendPromise).ok()) throw new Error('Send rejected');
    await expect(composer).toHaveCount(0);
    await control('sent');
    check('Remote browser sends its uploaded attachment through the host');
    step = 'restart';
    for (const name of ['a', 'b']) {
      await pages[name].evaluate(() => {
        window.__fixtureReconnects = 0;
        const Native = window.WebSocket;
        window.WebSocket = new Proxy(Native, {
          construct(target, arguments_, newTarget) {
            const socket = Reflect.construct(target, arguments_, newTarget);
            socket.addEventListener('open', () => { window.__fixtureReconnects += 1; });
            return socket;
          },
        });
      });
    }
    await control('restart');
    for (const name of ['a', 'b']) {
      await expect.poll(() => pages[name].evaluate(() => window.__fixtureReconnects), { timeout: 30000 }).toBeGreaterThan(0);
      await expect.poll(() => pages[name].evaluate(async () => (await fetch('/api/v1/admin/status')).status), { timeout: 30000 }).toBe(200);
    }
    check('Both browser sessions reconnect their event streams after host restart without a reload');
    step = 'csrf-and-admin-boundaries';
    const csrf = await contexts.a.request.post(`${config.origin}/api/v1/mail/mutations/read`, { headers: { origin: 'https://unrelated.example.test' }, data: { message_ids: [config.message_id], read: true } });
    if (csrf.status() !== 403) throw new Error('Foreign Origin accepted');
    const privateStatus = await pages.a.evaluate(async () => (await fetch('/api/v1/admin/diagnostics')).status);
    if (privateStatus !== 403) throw new Error('Host diagnostics exposed');
    check('Remote browser rejects foreign Origin mutations and private host diagnostics');
    step = 'revoke-parent';
    await control('revoke');
    const revoked = await pages.a.evaluate(async () => (await fetch('/api/v1/mail/search?q=Q1')).status);
    if (revoked !== 401) throw new Error('Revoked child cookie retained access');
    await pages.b.goto(`${config.origin}/search?q=Q1`);
    await expect(pages.b.getByTestId('mailbox-list').getByRole('option').first()).toBeVisible();
    check('Parent revocation blocks its browser while the independent client continues');
    emit({ event: 'complete' });
  } catch (error) {
    // Response bodies, cookie values and Playwright locator dumps stay out of evidence.
    process.stderr.write(`${JSON.stringify({ step, error: error?.name ?? 'Error', api_failures: failures })}\n`);
    emit({ event: 'failed', step });
    process.exitCode = 1;
  } finally {
    await browser.close();
    process.stdin.destroy();
  }
}
