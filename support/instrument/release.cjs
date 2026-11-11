// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// Report each actual close result; a rejected or timed-out close never passes.
async function release({context, browser, server}, timeoutMs = 10000) {
  const result = {schema:'instrument-release/v2', released_at:new Date().toISOString(),
    context_closed:false, browser_closed:false, http_closed:false, close_errors:[]};
  async function close(name, resource, operation, observed = () => true) {
    if (!resource) return;
    let timer;
    try {
      await Promise.race([Promise.resolve().then(operation), new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error('close timed out')), timeoutMs);
      })]);
      if (!observed()) throw new Error('resource remains open after close');
      result[name + '_closed'] = true;
    } catch (error) {
      result.close_errors.push({resource:name, error:String(error.message || error)});
    } finally { clearTimeout(timer); }
  }
  await close('context', context, () => context.close());
  await close('browser', browser, () => browser.close(), () => !browser.isConnected());
  await close('http', server, () => new Promise((resolve, reject) => {
    server.close(error => error ? reject(error) : resolve());
  }), () => !server.listening);
  return result;
}
module.exports = {release};
