// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Ask a member's worker to prepare a visit to the founder's document.
/// The prepared dial is dropped unopened: only the answer matters here.
pub(super) fn visit(
    worker: &armillary::ActorHandle<PlaceWorkerCommand>,
    updates: &std::sync::mpsc::Receiver<Update>,
    session: SessionId,
    directory: &Path,
    holder_root: [u8; 32],
    request: u64,
) -> Result<(), String> {
    worker.command(PlaceWorkerCommand::VisitDocument {
        session,
        generation: 1,
        directory: directory.to_path_buf(),
        holder_root,
        path: "field.knot".into(),
        request,
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Ok(Update::PlaceDocumentVisit {
            request: got,
            result,
            ..
        }) = updates.recv_timeout(Duration::from_secs(10))
            && got == request
        {
            return result.map(|_| ());
        }
    }
    panic!("visit {request} never answered");
}
