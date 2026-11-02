// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn close_if_transferred(texture: &NativeTextureHandle) {
    if let NativeTextureHandle::D3d12Shared { handle, ownership } = texture {
        close_transferred_handle(*handle, *ownership);
    }
}
