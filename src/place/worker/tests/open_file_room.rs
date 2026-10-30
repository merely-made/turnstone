// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Raise this process's soft open-file limit toward its hard limit, once.
/// A refusal leaves the limit as it was.
pub(super) fn open_file_room() {
    static ROOM: std::sync::Once = std::sync::Once::new();
    ROOM.call_once(|| {
        #[cfg(unix)]
        // SAFETY: `rlimit` is plain data, and both calls only read or
        // write the struct passed to them.
        unsafe {
            let mut limit: libc::rlimit = std::mem::zeroed();
            if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) == 0 {
                let wanted = limit.rlim_max.min(65_536);
                if limit.rlim_cur < wanted {
                    limit.rlim_cur = wanted;
                    libc::setrlimit(libc::RLIMIT_NOFILE, &limit);
                }
            }
        }
    });
}
