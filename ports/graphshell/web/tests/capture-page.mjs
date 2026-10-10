// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
// Read the actual headed page after its scenario completes. This captures the
// outer embedding page as well as the retained surface, without UI mutation.
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';

const [port, origin, output] = process.argv.slice(2);
if (!port || !origin || !output) throw new Error('Expected port, origin and output directory');
const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = pages.find(page => page.type === 'page' && page.url.startsWith(origin));
if (!page) throw new Error('The headed receipt page is absent');
const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.addEventListener('open', resolve, {once: true});
  socket.addEventListener('error', reject, {once: true});
});
let sequence = 0;
const pending = new Map();
socket.addEventListener('message', event => {
  const reply = JSON.parse(event.data);
  const slot = pending.get(reply.id);
  if (!slot) return;
  pending.delete(reply.id);
  clearTimeout(slot.timeout);
  reply.error ? slot.reject(new Error(JSON.stringify(reply.error))) : slot.resolve(reply.result);
});
function read(method, params = {}) {
  const id = ++sequence;
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`${method} timed out`));
    }, 15000);
    pending.set(id, {resolve, reject, timeout});
    socket.send(JSON.stringify({id, method, params}));
  });
}
try {
  const screenshot = await read('Page.captureScreenshot', {
    format: 'png', fromSurface: true, captureBeyondViewport: false,
  });
  await writeFile(join(output, 'whole-frame.png'), Buffer.from(screenshot.data, 'base64'));
  const accessibility = await read('Accessibility.getFullAXTree');
  await writeFile(join(output, 'whole-page-accessibility.json'), JSON.stringify(accessibility, null, 2));
  const layout = await read('Page.getLayoutMetrics');
  const browser = await read('Browser.getVersion');
  await writeFile(join(output, 'whole-frame-metadata.json'), JSON.stringify({
    url: page.url, capturedAt: new Date().toISOString(), layout, browser,
    headed: true, captureBeyondViewport: false,
  }, null, 2));
} finally {
  socket.close();
}
