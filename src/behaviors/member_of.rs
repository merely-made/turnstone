// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Which resident node holds `subject`.
///
/// The mapping is deliberately unique. A duplicate subject is malformed
/// session state, not an invitation to let hash iteration choose which body
/// receives a journal payload. Read authority is checked before that payload
/// is constructed, then the run lane adds world-write admission.
pub(super) fn member_of(
    app: &App,
    subject: Subject,
    scoped_reads: &[(servitor::Cap, Mode)],
) -> Option<uuid::Uuid> {
    let members: Vec<_> = app
        .denizens
        .residents
        .iter()
        .filter(|(_, resident)| resident.subject == subject)
        .map(|(member, _)| *member)
        .collect();
    let readable = app
        .denizens
        .authority
        .covers(subject, &crate::denizen::read_cap(), Mode::Read)
        && scoped_reads
            .iter()
            .all(|(cap, mode)| app.denizens.authority.covers(subject, cap, *mode));
    if members.len() != 1 || !readable {
        tracing::warn!(
            subject = %subject.to_hex(),
            count = members.len(),
            "behavior subject routing or live read authority refused"
        );
        return None;
    }
    members.into_iter().next()
}
