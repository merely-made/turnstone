// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The action catalog: what the app offers right now, and in what order.
//!
//! Contextual rows lead the static registry, because a pending grant review
//! must be the first thing an opened palette shows. One composition, read by
//! the `>` lane, the snapshot, and the automation runner alike — composing it
//! twice is how they come to disagree about what a label means.

use crate::action::{Action, Effect};
use crate::observe::AppEvent;
use crate::panes::PaneContent;

use super::{App, pane_label};

impl App {
    /// The dynamic switcher entries for the omnibar's `>` lane: a switch per
    /// OTHER session, most recently updated first ("New session" is a static
    /// palette entry).
    /// The denizen rows for the palette's actions lane: the pending
    /// install's visible review (the Confirm row IS the ask), then one Run
    /// row per resident (B1: the palette populated from denizen residency).
    /// Lower a denizen's emitted Actions through this same spine with the
    /// journal scoped to its subject, so every captured graph edit reads back
    /// attributed. Shared by both runnable lanes: piccolo returns Actions
    /// after evaluation, the component lane returns the ring-gate's accepted
    /// queue — by here, both are authorized.
    pub(super) fn lower_denizen_actions(
        &mut self,
        subject: servitor::Subject,
        label: String,
        actions: Vec<Action>,
    ) -> Vec<Effect> {
        if let Ok(mut journal) = self.journal.lock() {
            journal.set_author(subject.to_hex());
        }
        let mut effects = Vec::new();
        for action in actions {
            effects.extend(self.update(action));
        }
        if let Ok(mut journal) = self.journal.lock() {
            journal.set_author(mere::kernel::graph::USER_AUTHOR);
        }
        self.events.push(AppEvent::DenizenRan(label));
        effects.push(Effect::SaveSession);
        effects.push(Effect::Redraw);
        effects
    }

    pub fn denizen_actions(&self) -> Vec<(String, Action)> {
        let mut rows = Vec::new();
        if let Some(pending) = &self.pending_install {
            rows.push((
                crate::denizen::review_line(pending),
                Action::ConfirmInstallDenizen,
            ));
            rows.push((
                format!("Cancel install {}", pending.label),
                Action::CancelInstallDenizen,
            ));
        }
        let mut residents: Vec<_> = self.denizens.residents.iter().collect();
        residents.sort_by(|(_, a), (_, b)| a.label.cmp(&b.label));
        for (member, resident) in residents {
            rows.push((
                format!("Run {}", resident.label),
                Action::RunDenizen { member: *member },
            ));
            rows.push((
                format!("Uninstall {}", resident.label),
                Action::UninstallDenizen { member: *member },
            ));
        }
        rows
    }

    pub fn session_actions(&self) -> Vec<(String, Action)> {
        // Denizen rows lead: a pending install's review must be the first
        // thing the opened palette shows (B1's visible grant review).
        let mut rows = self.denizen_actions();
        let mut others: Vec<_> = self
            .sessions
            .iter()
            .filter(|(id, _)| *id != self.session_id)
            .collect();
        others.sort_by_key(|(_, m)| std::cmp::Reverse(m.updated_at));
        rows.extend(others.into_iter().map(|(id, _)| {
            (
                format!("Switch to session {}", self.session_label(id)),
                Action::SwitchSession(id),
            )
        }));
        rows.extend(self.pane_section_actions());
        rows
    }

    /// **The** action catalog offered right now: the contextual rows LEAD the
    /// static registry, because a pending denizen install's grant review must be
    /// the first thing an opened palette shows (participant gate B1) and the
    /// contextual rows outrank the fixed verbs generally.
    ///
    /// One composition, read by everything that offers or resolves an action:
    /// the omnibar's `>` lane filters it, the observation snapshot reports it,
    /// and the automation runner resolves a label through it. Composing it in
    /// more than one place is how the runner and the palette come to disagree
    /// about what a label means (they did: the runner resolved static-first
    /// while the palette showed dynamic-first, so a dynamic row that shadowed a
    /// static label would have acted as the wrong one).
    pub fn available_actions(&self) -> Vec<(String, Action)> {
        let mut rows = self.session_actions();
        rows.extend(self.place_founding_actions());
        rows.extend(self.place_member_actions());
        rows.extend(self.place_collection_actions());
        if let Some(member) = self.graph_runtimes.focused_member()
            && !self.node_is_kept(member)
        {
            rows.push(("Keep node".to_string(), Action::KeepNode { member }));
        }
        if self.graph_runtimes.focused_member().is_some_and(|member| {
            matches!(
                self.content.get(member),
                Some(crate::content::NodeContent::Live)
            ) && self
                .content
                .facts(member)
                .is_some_and(|facts| facts.capabilities.find_in_page.is_available())
        }) {
            rows.push(("Find in document".to_string(), Action::OpenDocumentFind));
        }
        // Page zoom is a document scale, so only a live engine that reports the
        // control offers it. An engine without it simply has no rows.
        if let Some(member) = self.graph_runtimes.focused_member()
            && self.page_zoom_offered(member)
        {
            rows.push(("Zoom in".to_string(), Action::PageZoomIn { member }));
            rows.push(("Zoom out".to_string(), Action::PageZoomOut { member }));
            rows.push(("Reset zoom".to_string(), Action::PageZoomReset { member }));
        }
        if self.focused_address().is_some_and(|address| {
            url::Url::parse(&address).is_ok_and(|url| matches!(url.scheme(), "titan" | "spartan"))
        }) {
            rows.push((
                "Compose smolweb submission".to_string(),
                Action::ComposeFocusedSmolwebSubmission,
            ));
        }
        if self
            .focused_address()
            .is_some_and(|address| crate::nomadnet::is_micron_address(&address))
        {
            rows.push((
                "Fill Micron form".to_string(),
                Action::ComposeFocusedMicronForm,
            ));
        }
        rows.extend(crate::action::palette_actions());
        rows
    }

    /// The founding half of the place vocabulary. Offered by situation: a
    /// personal session can found, join, or offer a pre-key; a session already
    /// in a place can export its card, invite, and speak. Offering a row that
    /// could only refuse would teach the palette to lie.
    pub(super) fn place_founding_actions(&self) -> Vec<(String, Action)> {
        let rows: &[(&str, Action)] = if self.place.binding().is_some() {
            &[
                ("Export place card", Action::BeginExportPlaceCard),
                ("Invite to place", Action::BeginInviteToPlace),
                (
                    "Invite to place as reader",
                    Action::BeginInviteToPlaceAsReader,
                ),
                ("Send place message", Action::BeginSendPlaceMessage),
                // Offered whatever is focused: the row is about the place,
                // and "no focused node to share" is the action's own answer.
                ("Share focused node", Action::ShareFocusedNode),
            ]
        } else {
            &[
                ("Found place", Action::BeginFoundPlace),
                ("Join place", Action::BeginJoinPlaceFile),
                ("Offer place pre-key", Action::BeginOfferPlacePrekey),
            ]
        };
        let mut rows: Vec<(String, Action)> = rows
            .iter()
            .map(|(label, action)| (label.to_string(), action.clone()))
            .collect();
        // The status row shows an elided ticket, so the copy row is the only
        // route to the whole one. Offered only where there IS one: a row that
        // could only refuse would teach the palette to lie.
        if !self.place.local_rendezvous().is_empty() {
            rows.push((
                "Copy local rendezvous".to_string(),
                Action::CopyLocalRendezvous,
            ));
        }
        rows
    }

    /// One revoke row per other member, offered only to a profile that
    /// manages the open place.
    pub(super) fn place_member_actions(&self) -> Vec<(String, Action)> {
        use crate::place::{PlaceMemberAccess, PlaceState};
        let PlaceState::Offline { snapshot, .. } = &self.place else {
            return Vec::new();
        };
        let local = snapshot.personae_root;
        if !snapshot
            .members
            .iter()
            .any(|member| member.root == local && member.access == PlaceMemberAccess::Manage)
        {
            return Vec::new();
        }
        snapshot
            .members
            .iter()
            .filter(|member| member.root != local)
            .map(|member| {
                (
                    format!(
                        "Revoke place member {} ({})",
                        &crate::place::hex32(&member.root)[..8],
                        member.access.label()
                    ),
                    Action::RevokePlaceMember {
                        member: member.root,
                    },
                )
            })
            .collect()
    }

    /// Captured-page scope is a local reading choice. The worker supplies the
    /// currently named exact versions; the palette carries those opaque values
    /// back unchanged, rather than trying to reconstruct a collection from its
    /// label. A stale selection names its current authorized replacement,
    /// rather than suggesting that the unavailable historical version can be
    /// searched.
    fn place_collection_actions(&self) -> Vec<(String, Action)> {
        let crate::place::PlaceState::Offline { snapshot, .. } = &self.place else {
            return Vec::new();
        };

        let all_selected = matches!(
            &snapshot.captured_selection,
            crate::place::CapturedCollectionSelection::AllEffective
        );
        let mut rows = vec![
            (
                "Search captured pages".to_string(),
                Action::SearchCapturedPages,
            ),
            (
                format!(
                    "Use captured collection: all shared captures{}",
                    all_selected.then_some(" (selected)").unwrap_or_default()
                ),
                Action::SetPlaceCollection(None),
            ),
        ];
        let (selected, stale_current) = match &snapshot.captured_selection {
            crate::place::CapturedCollectionSelection::AllEffective => (None, None),
            crate::place::CapturedCollectionSelection::Collection { requested, status } => (
                Some(requested),
                match status {
                    crate::place::CapturedCollectionSelectionStatus::Stale { current } => {
                        Some(current)
                    },
                    _ => None,
                },
            ),
        };

        for choice in &snapshot.collection_choices {
            let same_name = snapshot
                .collection_choices
                .iter()
                .filter(|other| other.name == choice.name)
                .count()
                > 1;
            let suffix = if same_name {
                format!(
                    " ({})",
                    collection_id_suffix(
                        &choice.version,
                        &snapshot.collection_choices,
                        &choice.name
                    )
                )
            } else {
                String::new()
            };
            let state = if selected.is_some_and(|requested| requested == &choice.version) {
                " (selected)"
            } else if stale_current.is_some_and(|current| current == &choice.version) {
                " (use latest version)"
            } else {
                ""
            };
            rows.push((
                format!("Use captured collection: {}{suffix}{state}", choice.name),
                Action::SetPlaceCollection(Some(choice.version.clone())),
            ));
        }

        if let Some(current) = stale_current
            && !snapshot
                .collection_choices
                .iter()
                .any(|choice| &choice.version == current)
        {
            let name = snapshot
                .collection_choices
                .iter()
                .find(|choice| choice.version.collection == current.collection)
                .map(|choice| choice.name.as_str())
                .unwrap_or("selected collection");
            let has_named_choice = snapshot
                .collection_choices
                .iter()
                .filter(|choice| choice.name == name)
                .count()
                > 0;
            let suffix = has_named_choice
                .then(|| {
                    format!(
                        " ({})",
                        collection_id_suffix(current, &snapshot.collection_choices, name)
                    )
                })
                .unwrap_or_default();
            rows.push((
                format!("Use captured collection: {name}{suffix} (use latest version)"),
                Action::SetPlaceCollection(Some(current.clone())),
            ));
        }
        rows
    }

    /// The composed-section rows for the ACTIVE pane, when its content composes
    /// (a Gloss, an Overmap): one add/remove per registered provider, plus the
    /// reorder rows. Pane-scoped palette entries are how the gloss-composite
    /// design chose to expose composition (the right-click palette already
    /// selects the pane under the pointer), so no new chrome. Empty when the
    /// active pane is not a composable one.
    ///
    /// Written against `PaneContent::composition`, not a pane kind, so a pane
    /// that gains a composition gains this whole UI without touching it. The
    /// row's prefix is the pane's own tag, so it names itself too.
    fn pane_section_actions(&self) -> Vec<(String, Action)> {
        let Some(pane) = self.active_pane else {
            return Vec::new();
        };
        let Some(content) = self.pane_content(pane) else {
            return Vec::new();
        };
        let Some(cfg) = content.composition() else {
            return Vec::new();
        };
        let who = pane_label(content);
        let mut rows: Vec<(String, Action)> = crate::sections::ALL
            .iter()
            .map(|p| {
                let on = cfg.sections.iter().any(|id| id == p.id);
                let verb = if on { "remove" } else { "add" };
                (
                    format!("{who}: {verb} section — {}", p.title),
                    Action::TogglePaneSection {
                        pane,
                        section: p.id.to_string(),
                    },
                )
            })
            .collect();
        // Reorder rows only where a move would DO something: nothing to
        // reorder with one section, and no "up" on the first (the palette
        // should not offer a no-op).
        if cfg.sections.len() > 1 {
            for (i, id) in cfg.sections.iter().enumerate() {
                let Some(p) = crate::sections::by_id(id) else {
                    continue;
                };
                if i > 0 {
                    rows.push((
                        format!("{who}: move section up — {}", p.title),
                        Action::MovePaneSection {
                            pane,
                            section: id.clone(),
                            delta: -1,
                        },
                    ));
                }
                if i + 1 < cfg.sections.len() {
                    rows.push((
                        format!("{who}: move section down — {}", p.title),
                        Action::MovePaneSection {
                            pane,
                            section: id.clone(),
                            delta: 1,
                        },
                    ));
                }
            }
        }
        rows
    }
}

fn collection_id_suffix(
    version: &crate::place::PlaceCollectionVersion,
    choices: &[crate::place::PlaceCollectionChoice],
    name: &str,
) -> String {
    let hex = collection_id_hex(&version.collection);
    for length in (8..=hex.len()).step_by(2) {
        let prefix = &hex[..length];
        if choices
            .iter()
            .filter(|choice| choice.name == name)
            .all(|choice| {
                choice.version.collection == version.collection
                    || !collection_id_hex(&choice.version.collection).starts_with(prefix)
            })
        {
            return prefix.to_string();
        }
    }
    hex
}

fn collection_id_hex(collection: &crate::place::PlaceCollectionId) -> String {
    collection
        .0
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(seed: u8) -> crate::place::PlaceCollectionVersion {
        crate::place::PlaceCollectionVersion {
            moot: crate::place::PlaceId([1; 32]),
            collection: crate::place::PlaceCollectionId([seed; 32]),
            frontier: vec![[seed.wrapping_add(1); 32]],
            membership_commitment: [seed.wrapping_add(2); 32],
        }
    }

    fn offline(snapshot: crate::place::OfflinePlaceSnapshot) -> crate::place::PlaceState {
        crate::place::PlaceState::Offline {
            binding: crate::place::PlaceBindingV1::new(
                crate::place::PlaceId([1; 32]),
                crate::place::SharedContainerId([2; 32]),
                crate::place::ChatSpaceId([3; 32]),
                "hall",
            )
            .unwrap(),
            generation: 1,
            snapshot,
        }
    }

    #[test]
    fn captured_collection_rows_preserve_worker_versions_and_mark_selection() {
        let first = version(10);
        let second = version(20);
        let mut app = App::test_stub();
        app.place = offline(crate::place::OfflinePlaceSnapshot {
            captured_selection: crate::place::CapturedCollectionSelection::Collection {
                requested: first.clone(),
                status: crate::place::CapturedCollectionSelectionStatus::Ready {
                    name: "field notes".into(),
                    effective_contributions: 1,
                    pending_facts: 0,
                },
            },
            collection_choices: vec![
                crate::place::PlaceCollectionChoice {
                    name: "field notes".into(),
                    version: first.clone(),
                },
                crate::place::PlaceCollectionChoice {
                    name: "research".into(),
                    version: second.clone(),
                },
            ],
            ..Default::default()
        });

        let rows = app.available_actions();
        assert!(rows.contains(&("Search captured pages".into(), Action::SearchCapturedPages,)));
        assert!(rows.contains(&(
            "Use captured collection: all shared captures".into(),
            Action::SetPlaceCollection(None),
        )));
        assert!(rows.contains(&(
            "Use captured collection: field notes (selected)".into(),
            Action::SetPlaceCollection(Some(first)),
        )));
        assert!(rows.contains(&(
            "Use captured collection: research".into(),
            Action::SetPlaceCollection(Some(second)),
        )));
    }

    #[test]
    fn stale_captured_collection_selection_offers_the_latest_version() {
        let requested = version(30);
        let mut current = requested.clone();
        current.frontier = vec![[33; 32]];
        current.membership_commitment = [34; 32];
        let mut app = App::test_stub();
        app.place = offline(crate::place::OfflinePlaceSnapshot {
            captured_selection: crate::place::CapturedCollectionSelection::Collection {
                requested: requested.clone(),
                status: crate::place::CapturedCollectionSelectionStatus::Stale {
                    current: current.clone(),
                },
            },
            collection_choices: vec![crate::place::PlaceCollectionChoice {
                name: "field notes".into(),
                version: current.clone(),
            }],
            ..Default::default()
        });

        assert!(app.available_actions().contains(&(
            "Use captured collection: field notes (use latest version)".into(),
            Action::SetPlaceCollection(Some(current)),
        )));
    }

    #[test]
    fn duplicate_collection_names_have_stable_distinct_palette_labels() {
        let first = version(10);
        let mut second = version(10);
        second.collection.0[4] = 11;
        let mut app = App::test_stub();
        app.place = offline(crate::place::OfflinePlaceSnapshot {
            collection_choices: vec![
                crate::place::PlaceCollectionChoice {
                    name: "notes".into(),
                    version: first.clone(),
                },
                crate::place::PlaceCollectionChoice {
                    name: "notes".into(),
                    version: second.clone(),
                },
            ],
            ..Default::default()
        });

        assert!(app.available_actions().contains(&(
            "Use captured collection: all shared captures (selected)".into(),
            Action::SetPlaceCollection(None),
        )));
        assert!(app.available_actions().contains(&(
            "Use captured collection: notes (0a0a0a0a0a)".into(),
            Action::SetPlaceCollection(Some(first)),
        )));
        assert!(app.available_actions().contains(&(
            "Use captured collection: notes (0a0a0a0a0b)".into(),
            Action::SetPlaceCollection(Some(second)),
        )));
    }

    #[test]
    fn committing_a_captured_collection_row_emits_its_exact_version() {
        let selected = version(10);
        let mut app = App::test_stub();
        app.place = offline(crate::place::OfflinePlaceSnapshot {
            collection_choices: vec![crate::place::PlaceCollectionChoice {
                name: "field notes".into(),
                version: selected.clone(),
            }],
            ..Default::default()
        });

        app.update(Action::OmnibarOpen { command: true });
        app.update(Action::OmnibarInsert("field notes".into()));
        assert!(app.omnibar.suggestions.iter().any(|row| {
            matches!(
                row,
                crate::ui::Suggestion::Act { label, action }
                    if label == "Use captured collection: field notes"
                        && action == &Action::SetPlaceCollection(Some(selected.clone()))
            )
        }));
        let effects = app.update(Action::OmnibarCommit);

        assert!(effects.contains(&Effect::SetPlaceCollection {
            session: app.session_id,
            generation: 1,
            selection: Some(selected),
        }));
        assert!(!app.omnibar.open, "the palette closes on commit");
    }
}
