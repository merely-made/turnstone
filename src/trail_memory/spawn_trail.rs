// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Spawn the trail-memory actor over the session memory at `dir`, waking the
/// event loop on store activity like the bin does. W1 emits no updates:
/// failures warn and capture continues; the recall pane (W2) is the first
/// reader of what lands here.
pub fn spawn_trail(wake: Wake, dir: PathBuf) -> (ActorHandle<TrailCommand>, Receiver<Update>) {
    spawn_trail_with_capture_library(
        wake,
        dir,
        crate::place::captured_collection::LocalCaptureLibrary::default(),
    )
}
