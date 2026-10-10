// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// Worker containment for this app-core adapter. Every turn has a deadline;
// stopping rejects all pending calls so late work cannot become host actions.
export class Runtime {
  constructor(url = new URL('./worker.mjs', import.meta.url)) {
    this.worker = new Worker(url, { type: 'module' });
    this.pending = new Map();
    this.next = 0;
    this.closed = false;
    this.worker.onmessage = ({ data }) => {
      const request = this.pending.get(data.id);
      if (!request) return;
      this.pending.delete(data.id);
      clearTimeout(request.timer);
      if (data.error) request.reject(new Error(data.error)); else request.resolve(data);
    };
    this.worker.onerror = event => this.close(new Error(event.message));
  }
  call(command, data, timeout = 2500) {
    if (this.closed) return Promise.reject(new Error('Applet stopped'));
    return new Promise((resolve, reject) => {
      const id = ++this.next;
      const timer = setTimeout(() => this.close(new Error('Applet interrupted: turn exceeded its time limit')), timeout);
      this.pending.set(id, { resolve, reject, timer });
      this.worker.postMessage({ id, command, data });
    });
  }
  close(error = new Error('Applet stopped')) {
    this.closed = true;
    this.worker.terminate();
    for (const request of this.pending.values()) { clearTimeout(request.timer); request.reject(error); }
    this.pending.clear();
  }
}
