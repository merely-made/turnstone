// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
import { blake3 } from './noble/blake3.js';

export const hex = bytes => Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
export const digest = bytes => hex(blake3(bytes));
function unhex(value, length) {
  if (typeof value !== 'string' || value.length !== length * 2 || !/^[0-9a-f]+$/.test(value)) throw new Error('Malformed signing binding');
  return Uint8Array.from(value.match(/../g), b => parseInt(b, 16));
}

// Mirrors pack.rs v1's struct-order serde encoding, including the part schema.
// Do not treat arbitrary incoming property order as canonical.
export function canonical(manifest) {
  return new TextEncoder().encode(JSON.stringify({
    name: manifest.name, version: manifest.version, author: manifest.author,
    requested_scopes: manifest.requested_scopes,
    parts: manifest.parts.map(p => ({ name: p.name, role: p.role, blob: p.blob, bytes: p.bytes })),
  }));
}
export async function verifyPack(pack, contributor = pack.manifest.author) {
  if (pack.manifest.author !== contributor) throw new Error('Contributor is not the pack author');
  for (const binding of pack.trust.signatures) {
    if (!binding.startsWith('personae:ed25519:')) continue;
    const [author, signature, extra] = binding.slice('personae:ed25519:'.length).split(':');
    if (extra !== undefined || author !== pack.manifest.author) throw new Error('Broken signing binding');
    const key = await crypto.subtle.importKey('raw', unhex(author, 32), 'Ed25519', false, ['verify']);
    if (!await crypto.subtle.verify('Ed25519', key, unhex(signature, 64), canonical(pack.manifest))) throw new Error('Broken pack signature');
    return;
  }
  throw new Error('No verified personae signature');
}
export function verifyPart(part, bytes) {
  if (bytes.length !== part.bytes || `blake3:${digest(bytes)}` !== part.blob) throw new Error('Part hash or size mismatch');
}
export async function fetchBytes(url) {
  const response = await fetch(url, { cache: 'no-store' });
  if (!response.ok) throw new Error(`Fetch refused: ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
}
