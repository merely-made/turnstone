// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// One owned headed Chrome profile, closed and reopened at the same loopback origin.
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const http = require('node:http');
const crypto = require('node:crypto');
const child = require('node:child_process');
const assert = require('node:assert/strict');
const {release} = require('../instrument/release.cjs');
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const write = (file, data) => fs.writeFileSync(file, JSON.stringify(data, null, 2) + '\n', {flag:'wx'});

function staticServer(packet) {
  const site = path.join(packet, 'site/ports/graphshell/web');
  const types = {'.html':'text/html', '.js':'text/javascript', '.css':'text/css',
    '.json':'application/json', '.wasm':'application/wasm', '.png':'image/png'};
  return http.createServer((request, response) => {
    try {
      assert.equal(request.method, 'GET');
      const name = decodeURIComponent(new URL(request.url, 'http://localhost').pathname);
      const root = name.startsWith('/pkg/') ? path.join(packet, 'pkg') : site;
      const relative = name.startsWith('/pkg/') ? name.slice(5) : name.slice(1);
      const file = path.resolve(root, relative || 'tree.html');
      assert(file.startsWith(root + path.sep));
      assert(fs.statSync(file).isFile());
      response.writeHead(200, {'Content-Type':types[path.extname(file)] || 'text/plain',
        'Cache-Control':'no-store', 'Cross-Origin-Opener-Policy':'same-origin',
        'Cross-Origin-Embedder-Policy':'require-corp'});
      fs.createReadStream(file).pipe(response);
    } catch { response.writeHead(404); response.end('not found'); }
  });
}

function processTable() {
  return child.execFileSync('ps', ['-axo', 'pid=,ppid='], {encoding:'utf8'}).trim().split('\n')
    .map(line => line.trim().split(/\s+/).map(Number));
}

function observeDescendants(owned) {
  const rows = processTable();
  let changed = true;
  while (changed) {
    changed = false;
    for (const [pid, parent] of rows) if (owned.has(parent) && !owned.has(pid)) {
      owned.add(pid); changed = true;
    }
  }
  const live = new Set(rows.map(row => row[0]));
  return [...owned].filter(pid => live.has(pid));
}

async function waitGone(owned) {
  for (let count = 0; count < 100; count++) {
    if (!observeDescendants(owned).length) return [];
    await sleep(100);
  }
  return observeDescendants(owned);
}

async function bounded(operation) {
  let timer;
  try {
    return await Promise.race([operation(), new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error('owned browser close timed out')), 15000);
    })]);
  } finally { clearTimeout(timer); }
}

async function main(args) {
  assert.equal(args.length, 4, 'usage: fresh_browser.cjs --packet PATH --playwright MODULE');
  assert.equal(args[0], '--packet'); assert.equal(args[2], '--playwright');
  const packet = fs.realpathSync(args[1]);
  const spec = JSON.parse(fs.readFileSync(path.join(packet, 'fresh.json')));
  assert.equal(spec.schema, 'fresh-link-family/v1');
  const output = path.join(packet, 'browser-qualified');
  fs.mkdirSync(output); // Refuse a rerun over any previous evidence.
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'mere-link-qualification-'));
  const token = crypto.randomUUID();
  const marker = path.join(profile, 'qualification-owner.json');
  write(marker, {token, packet, driver_pid:process.pid});
  const nativeChildren = [];
  const owned = new Set();
  const spawn = child.spawn;
  child.spawn = function(command, ...rest) {
    const result = spawn.call(this, command, ...rest);
    if (/Google Chrome\.app\/.*Google Chrome$/.test(String(command))) {
      nativeChildren.push(result); owned.add(result.pid);
    }
    return result;
  };
  const {chromium} = require(args[3]);
  const server = staticServer(packet);
  let context, browser, tracking, error, adapter, port, httpClosed = false, rebound = false;
  const observations = [], phases = [], failures = [];
  const versions = {node:process.version, playwright:require(path.join(args[3], 'package.json')).version};
  try {
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    port = server.address().port;
    tracking = setInterval(() => observeDescendants(owned), 500);
    for (const scenario of spec.scenarios) {
      const launchCount = nativeChildren.length;
      context = await chromium.launchPersistentContext(profile, {
        executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
        headless:false, viewport:{width:1600, height:1000}, deviceScaleFactor:1,
        args:['--enable-unsafe-webgpu', '--no-first-run', '--no-default-browser-check']});
      browser = context.browser();
      assert.equal(nativeChildren.length, launchCount + 1, 'one owned Chrome must be observed');
      const chrome = nativeChildren.at(-1);
      versions.chrome = browser.version();
      observations.push({phase:scenario.phase + '-before-navigation', chrome_pids:[chrome.pid]});
      const page = context.pages()[0] || await context.newPage();
      const pageErrors = [];
      page.on('pageerror', value => pageErrors.push(String(value)));
      const query = new URLSearchParams({app:'local', fixture:scenario.fixture,
        scenario:'scenarios/' + scenario.name + '.scn', render_backend:'webgpu'});
      await page.goto(`http://127.0.0.1:${port}/tree.html?${query}`, {timeout:120000});
      await page.bringToFront();
      await page.waitForFunction(() => ['ok', 'fail'].includes(document.body.dataset.scenario), null, {timeout:300000});
      const evidence = await page.evaluate(async () => {
        const root = document.querySelector('graphshell-tree');
        const gpu = await navigator.gpu.requestAdapter();
        const info = gpu?.info;
        return {scenario:window.graphshellScenario(), receipt:window.graphshellReceipt(),
          root:Object.fromEntries([...root.attributes].map(a => [a.name, a.value])),
          runtime:{visibility:document.visibilityState, viewport:[innerWidth,innerHeight],
            adapter:info && {vendor:info.vendor, architecture:info.architecture,
              device:info.device, description:info.description}},
          captures:[...document.querySelectorAll('#scenario-captures img')].map(img => ({name:img.dataset.capture, data:img.src}))};
      });
      const folder = path.join(output, scenario.name);
      fs.mkdirSync(folder);
      const captures = evidence.captures;
      delete evidence.captures;
      evidence.page_errors = pageErrors;
      write(path.join(folder, 'result.json'), evidence);
      for (const capture of captures) {
        assert(/^[a-z0-9_]+$/.test(capture.name));
        assert(capture.data.startsWith('data:image/png;base64,'));
        fs.writeFileSync(path.join(folder, capture.name + '.png'), Buffer.from(capture.data.split(',')[1], 'base64'), {flag:'wx'});
      }
      assert.deepEqual(pageErrors, []);
      assert.equal(evidence.scenario.state, 'ok');
      assert.deepEqual(evidence.scenario.errors, []);
      assert.deepEqual(evidence.scenario.gate_failures, []);
      assert.equal(evidence.scenario.result.result, 'ok');
      assert.deepEqual(captures.map(c => c.name).sort(), [...scenario.captures].sort());
      assert.equal(evidence.runtime.visibility, 'visible');
      assert.equal(evidence.root['data-selected-render-backend'], 'webgpu');
      assert.match(JSON.stringify(evidence.runtime.adapter), /amd|ati|radeon/i);
      assert.doesNotMatch(JSON.stringify(evidence.runtime.adapter), /swiftshader|llvmpipe/i);
      adapter = evidence.runtime.adapter;
      observeDescendants(owned);
      await bounded(() => context.close());
      assert(!browser.isConnected());
      context = null; browser = null;
      const remaining = await waitGone(owned);
      assert.deepEqual(remaining, [], 'all owned Chrome processes must exit');
      observations.push({phase:scenario.phase + '-exit', chrome_pids:[]});
      phases.push({phase:scenario.phase, context_closed:true, browser_closed:true,
        owned_pids_remaining:remaining, http_retained:server.listening, profile_retained:fs.existsSync(profile)});
    }
  } catch (value) { error = value; }
  finally {
    clearInterval(tracking);
    const closed = await release({context, browser, server}, 15000);
    failures.push(...closed.close_errors);
    httpClosed = closed.http_closed;
    for (const process of nativeChildren) if (process.exitCode === null && process.signalCode === null) process.kill('SIGTERM');
    const remaining = await waitGone(owned);
    if (port && !server.listening) {
      const probe = http.createServer();
      try {
        await new Promise((resolve, reject) => { probe.once('error', reject); probe.listen(port, '127.0.0.1', resolve); });
        await new Promise((resolve, reject) => probe.close(e => e ? reject(e) : resolve()));
        rebound = true;
      } catch (value) { failures.push({resource:'port-rebind', error:String(value)}); }
    }
    const ownership = JSON.parse(fs.readFileSync(marker));
    assert.equal(ownership.token, token); assert.equal(ownership.packet, packet);
    if (!remaining.length) fs.rmSync(profile, {recursive:true});
    const runtime = {source_revision:spec.source_revision, headed:true, render_backend:'webgpu',
      adapter, versions, phases, owned_pids:[...owned], owned_pids_remaining:remaining,
      profile_removed:!fs.existsSync(profile), http_closed:httpClosed, http_port_rebound:rebound,
      close_errors:failures, error:error && String(error), driver_sha256:sha(fs.readFileSync(__filename))};
    write(path.join(packet, 'browser-runtime.json'), runtime);
    write(path.join(packet, 'browser-processes.json'), {source_revision:spec.source_revision, observations});
    child.spawn = spawn;
    if (failures.length || remaining.length || !runtime.profile_removed || !httpClosed || !rebound)
      error ||= new Error('owned-resource cleanup was not confirmed');
  }
  if (error) throw error;
  console.log(JSON.stringify({family:spec.family, scenarios:phases.length, cleanup:true}));
}

if (require.main === module) main(process.argv.slice(2)).catch(error => { console.error(error); process.exitCode = 1; });
module.exports = {staticServer, observeDescendants};
