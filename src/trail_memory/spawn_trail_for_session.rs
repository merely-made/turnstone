// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn spawn_trail_for_session(
    wake: Wake,
    dir: Option<PathBuf>,
    capture_library: crate::place::captured_collection::LocalCaptureLibrary,
    capture_generation: uuid::Uuid,
) -> (ActorHandle<TrailCommand>, Receiver<Update>) {
    spawn_named("trail-memory", wake, move |commands, out| {
        super::worker::run(commands, out, dir, capture_library, capture_generation)
    })
}
