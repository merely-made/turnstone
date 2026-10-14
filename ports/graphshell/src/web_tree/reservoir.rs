// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! The ordinary shared mere component beside the selected resident graph.
use super::*;
pub(super) fn view(page: &TreePage) -> Option<Child> {
    let remote = page.shared.remote.borrow();
    let session = remote.reservoir.as_ref()?;
    let mut records: Vec<_> = session.records().iter().collect();
    records.sort_by(|(a_key, a), (b_key, b)| a.title.cmp(&b.title).then(a_key.cmp(b_key)));
    let focus = session.focus_key();
    let buttons: Vec<Child> = records
        .into_iter()
        .map(|(key, card)| {
            let key = key.clone();
            let selected = focus.as_ref() == Some(&key);
            let attribute_key = key.clone();
            Box::new(
                cambium::button(
                    format!("Read {}", card.title),
                    move |page: &mut TreePage, _| {
                        request(page, mere_view::MereViewRequest::Activate(key.clone()));
                    },
                )
                .attr("data-mere-node", attribute_key)
                .attr("aria-label", format!("Read {}", card.title))
                .attr("aria-pressed", if selected { "true" } else { "false" }),
            ) as Child
        })
        .collect();
    let record = focus.as_ref().and_then(|key| session.records().get(key));
    let values: Vec<Child> = record
        .into_iter()
        .flat_map(|card| card.values.iter())
        .map(|value| {
            Box::new(el::<_, TreePage, ()>(
                "p",
                format!("{}: {}", value.label, value.value),
            )) as Child
        })
        .collect();
    Some(Box::new(
        el(
            "section",
            (
                mere_view::mere_view(
                    mere_view::MereView {
                        model: session.model(),
                        state: &page.mere_view,
                        leaf_key: 0x6d657265,
                        width: 279,
                        height: 520,
                    },
                    |page: &mut TreePage, event| page.mere_view.apply(event),
                    |page: &mut TreePage, action| request(page, action),
                ),
                el("div", buttons).attr("aria-label", "Resident records"),
                el(
                    "section",
                    (
                        el(
                            "h3",
                            record
                                .map(|card| card.title.clone())
                                .unwrap_or_else(|| "Select a record".into()),
                        ),
                        values,
                        el(
                            "p",
                            if session.view_saved() {
                                "View saved"
                            } else {
                                "View not saved"
                            },
                        ),
                    ),
                )
                .attr("aria-label", "Selected resident record"),
            ),
        )
        .attr("class", "resident-mere-panel"),
    ))
}
fn request(page: &mut TreePage, request: mere_view::MereViewRequest) {
    let mut remote = page.shared.remote.borrow_mut();
    if let Some(session) = &mut remote.reservoir {
        if let Err(reason) = session.request(request) {
            session.show_notice(reason);
        }
    }
    remote.pump();
    page.shared.dirty.set(true);
}

pub(super) fn relayout(page: &mut TreePage) {
    let remote = page.shared.remote.borrow();
    let adopted = remote
        .reservoir
        .as_ref()
        .is_some_and(|session| session.primary().is_some());
    if let Some(session) = &remote.reservoir {
        page.mere_view.focused = session.focus_key();
        page.mere_view.lay_out(session.model(), 279, 520);
    }
    drop(remote);
    if adopted {
        page.session = remote::Session::Remote;
        page.shared.remote_shown.set(true);
        page.sections.remote.expanded = true;
        page.shared.dirty.set(true);
    }
}

pub(super) fn fields(page: &TreePage, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let remote = page.shared.remote.borrow();
    let Some(session) = &remote.reservoir else {
        return snapshot;
    };
    let focus = session.focus_key();
    let record = focus.as_ref().and_then(|key| session.records().get(key));
    snapshot
        .with_field("mere-status", session.status())
        .with_field(
            "mere-attached",
            session
                .model()
                .sessions
                .iter()
                .find(|entry| entry.attached)
                .map(|entry| entry.id.as_uuid().to_string())
                .unwrap_or_default(),
        )
        .with_field("mere-read-only", session.read_only().to_string())
        .with_field("mere-layout", session.model().layout.clone())
        .with_field("mere-view-saved", session.view_saved().to_string())
        .with_field(
            "mere-record",
            record.map(|card| card.title.clone()).unwrap_or_default(),
        )
        .with_field(
            "mere-record-data",
            record
                .map(|card| {
                    card.values
                        .iter()
                        .map(|value| format!("{}: {}", value.label, value.value))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default(),
        )
        .with_field("mere-records", session.records().len().to_string())
        .with_field("mere-sessions", session.model().sessions.len().to_string())
        .with_field(
            "mere-archives",
            session
                .model()
                .archive
                .as_ref()
                .map(|archive| archive.entries.len())
                .unwrap_or_default()
                .to_string(),
        )
}
