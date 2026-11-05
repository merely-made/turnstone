// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {staticServer} = require('./fresh_browser.cjs');

test('frozen site and explicit package are the only served roots', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'fresh-link-server-'));
  fs.mkdirSync(path.join(root, 'site/ports/graphshell/web'), {recursive:true});
  fs.mkdirSync(path.join(root, 'pkg'));
  fs.writeFileSync(path.join(root, 'site/ports/graphshell/web/tree.html'), 'frozen page');
  fs.writeFileSync(path.join(root, 'pkg/app.wasm'), 'frozen package');
  fs.writeFileSync(path.join(root, 'outside.txt'), 'private');
  const server = staticServer(root);
  try {
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    const origin = 'http://127.0.0.1:' + server.address().port;
    assert.equal(await (await fetch(origin + '/tree.html')).text(), 'frozen page');
    const wasm = await fetch(origin + '/pkg/app.wasm');
    assert.equal(wasm.headers.get('content-type'), 'application/wasm');
    assert.equal(wasm.headers.get('cache-control'), 'no-store');
    assert.equal(await wasm.text(), 'frozen package');
    for (const resource of ['/outside.txt', '/pkg/..%2foutside.txt', '/%2e%2e%2foutside.txt'])
      assert.equal((await fetch(origin + resource)).status, 404);
  } finally {
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    fs.rmSync(root, {recursive:true});
  }
});
