// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The app events the drain has not considered, as cascade inputs.
///
/// Their sequence is the queue ordinal, a different counter from the journal's,
/// which is why the app tier has its own [`WatchTable`](servitor::WatchTable).
/// `author_from` marks where a woken body's own events begin, so a behavior
/// cannot be woken by the events it just caused.
pub(super) fn app_entries(app: &App, author_from: Option<(usize, &str)>) -> Vec<CommittedEntry> {
    let base = app.events_len() - app.unseen_events().len();
    app.unseen_events()
        .iter()
        .enumerate()
        .filter_map(|(offset, event)| {
            let index = base + offset;
            let author = match author_from {
                Some((from, subject)) if index >= from => subject,
                _ => mere::kernel::graph::USER_AUTHOR,
            };
            let scope = app_event_scope(event)?;
            let ordinal = app.events_base() + index as u64 + 1;
            Some(CommittedEntry::new(ordinal, author, vec![scope]))
        })
        .collect()
}
