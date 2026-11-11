// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// The viewer's physics Worker (a module Worker; the host plan, T7). It loads
// `graphshell_physics.js`/`_bg.wasm` from beside it, then runs the loop the
// actor thread runs natively: apply every message, answer with an update, then
// one interval of ticks per 60 Hz timer while there is work, parking when
// there is none. Messages are postcard bytes (`seiche::wire`), posted as
// `Uint8Array`s whose buffers are transferred.
//
// Messages that arrive while the wasm loads wait, in order.

import init, { PhysicsWorker } from "./graphshell_physics.js";

const FRAME_MS = 1000 / 60;
let core = null;
let timer = null;
const waiting = [];

const post = (bytes) => {
  if (bytes) postMessage(bytes, [bytes.buffer]);
};

const tick = () => {
  timer = null;
  const started = performance.now();
  post(core.interval());
  if (core.is_busy()) schedule(Math.max(0, FRAME_MS - (performance.now() - started)));
};

// One timer at a time: a burst of messages shares the interval that follows it.
const schedule = (delay) => {
  if (timer === null) timer = setTimeout(tick, delay);
};

const handle = (data) => {
  post(core.receive(new Uint8Array(data.buffer ?? data)));
  schedule(0);
};

self.onmessage = (event) => {
  if (core) handle(event.data);
  else waiting.push(event.data);
};

// A wasm that does not load is reported to the page as a string (the page reads
// every other message as bytes), so it can say its physics is unavailable.
try {
  await init();
  core = new PhysicsWorker();
} catch (error) {
  postMessage(`physics worker wasm did not load: ${error?.message ?? error}`);
  throw error;
}
for (const data of waiting.splice(0)) handle(data);
