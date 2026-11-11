// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// This particular std guest imports only CLI, clocks and output-stream WASI
// interfaces. No ambient environment, files, network, storage or user input
// is supplied. This is not a general WASI implementation or a timing API.
export function wasi(log) {
  class Pollable { block() { throw new Error('Blocking WASI poll unavailable'); } }
  class InputStream {}
  class OutputStream {
    checkWrite() { return 4096n; }
    write(bytes) { log(new TextDecoder().decode(bytes)); }
    blockingFlush() {}
    subscribe() { return new Pollable(); }
  }
  class TerminalInput {}
  class TerminalOutput {}
  class IoError { toDebugString() { return 'WASI I/O unavailable'; } }
  return {
    'wasi:cli/environment': { getEnvironment: () => [] },
    'wasi:cli/exit': { exit: () => { throw new Error('Guest requested exit'); } },
    'wasi:cli/stdin': { getStdin: () => new InputStream() },
    'wasi:cli/stdout': { getStdout: () => new OutputStream() },
    'wasi:cli/stderr': { getStderr: () => new OutputStream() },
    'wasi:cli/terminal-input': { TerminalInput },
    'wasi:cli/terminal-output': { TerminalOutput },
    'wasi:cli/terminal-stdin': { getTerminalStdin: () => undefined },
    'wasi:cli/terminal-stdout': { getTerminalStdout: () => undefined },
    'wasi:cli/terminal-stderr': { getTerminalStderr: () => undefined },
    'wasi:clocks/monotonic-clock': { subscribeDuration: () => { throw new Error('WASI timer unavailable'); } },
    'wasi:io/error': { Error: IoError },
    'wasi:io/poll': { Pollable, poll: () => { throw new Error('WASI poll unavailable'); } },
    'wasi:io/streams': { InputStream, OutputStream },
  };
}
