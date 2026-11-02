// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Spawn the trail actor with the capture library consumed by the place
/// worker. Only plain records cross this seam; the Fjall handle stays here.
pub(crate) fn spawn_trail_with_capture_library(
    wake: Wake,
    dir: PathBuf,
    capture_library: crate::place::captured_collection::LocalCaptureLibrary,
) -> (ActorHandle<TrailCommand>, Receiver<Update>) {
    spawn_trail_for_session(wake, Some(dir), capture_library, uuid::Uuid::nil())
}
