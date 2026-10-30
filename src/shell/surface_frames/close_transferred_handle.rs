// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn close_transferred_handle(handle: u64, ownership: FrameHandleOwnership) {
    if ownership != FrameHandleOwnership::Transferred || handle == 0 {
        return;
    }
    unsafe {
        let _ = windows::Win32::Foundation::CloseHandle(windows::Win32::Foundation::HANDLE(
            handle as *mut std::ffi::c_void,
        ));
    }
}
