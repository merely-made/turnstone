// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
// Explicit local acceptance host, not an implicit peer or application authority.
export async function mountResourceOwner(wasm, endpoint) {
  const readUrl = new URL(endpoint, location.href);
  if (readUrl.origin !== location.origin) throw new Error('Resource owner fixture must share this origin');
  const read = async () => {
    const response = await fetch(readUrl, {cache:'no-store'});
    if (!response.ok) throw new Error('Resource owner read failed');
    return response.json();
  };
  const initial = await read();
  for (const {reading,snapshot} of initial.snapshots) wasm.moot_resource_read(reading, JSON.stringify(snapshot));
  let busy = false;
  const timer = setInterval(async () => {
    if (busy) return;
    busy = true;
    try {
      const commands = JSON.parse(wasm.moot_resource_commands());
      for (const command of commands) {
        try {
        const current = (await read()).snapshots.find(item => item.reading === command.reading);
        if (!current) throw new Error('Reading no longer disclosed');
        try { wasm.moot_resource_prepare_dispatch(command.reading,command.token,JSON.stringify(current.snapshot)); }
        catch (_) { continue; }
        if (command.target.refresh) {
          wasm.moot_resource_refreshed(command.reading,command.token,JSON.stringify(current.snapshot));
          continue;
        }
        const response = await fetch(new URL('action',readUrl), {method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(command)});
        if (!response.ok) {
          wasm.moot_resource_complete(command.reading,command.token,JSON.stringify({Refused:{reason:'Action owner request failed'}}));
          continue;
        }
        const result = await response.json();
        if (typeof result.text === 'string') wasm.moot_resource_opened(command.reading,command.token,JSON.stringify(result.outcome),result.text);
        else wasm.moot_resource_complete(command.reading,command.token,JSON.stringify(result.outcome));
        } catch (_) {
          wasm.moot_resource_failed(command.reading,command.token,'Resource owner read or transport unavailable');
        }
      }
    } catch (error) { console.error('Resource owner bridge:',error); }
    finally { busy = false; }
  },100);
  addEventListener('pagehide',()=>clearInterval(timer),{once:true});
}
