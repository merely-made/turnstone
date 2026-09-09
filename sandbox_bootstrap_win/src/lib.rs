// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! CEF's Windows sandbox bootstrap entry point for Turnstone's Weld renderer.
//!
//! `bootstrap.exe`, renamed `turnstone.exe` and bundled beside this crate's
//! `cdylib` artifact (renamed `turnstone.dll`) per
//! `design_docs/2026-08-03_turnstone_engine_adoption_plan.md`'s "E2-Weld
//! Windows sandbox bootstrap" bundle layout, loads this DLL and calls
//! [`RunWinMain`] instead of running an ordinary `main`. It hands over the
//! process instance and CEF's sandbox context, which
//! `turnstone::launch::run_bootstrap` borrows for the life of the call.
//! Off Windows this crate exports nothing (`turnstone` itself is a
//! Windows-only dependency here).

/// # Safety
///
/// Called only by CEF's matching `bootstrap.exe`. Its raw instance and
/// sandbox pointers must remain valid for the duration of the call.
#[cfg(windows)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn RunWinMain(
    instance: *mut std::ffi::c_void,
    _command_line: *const u8,
    _command_show: i32,
    sandbox_info: *mut u8,
) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        turnstone::launch::run_bootstrap(instance, sandbox_info)
    }))
    .unwrap_or_else(|_| {
        eprintln!("turnstone: panic escaped the sandboxed browser entry point");
        199
    })
}
