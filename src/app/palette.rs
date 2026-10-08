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

/// The commands Turnstone keeps by default (Mark, 2026-10-08): the browsing
/// eight. A person can drop any of them and keep any other command.
pub(crate) const DEFAULT_KEPT_COMMANDS: [&str; 8] = [
    "Back",
    "Forward",
    "Reload",
    "Stop loading",
    "Toggle live content",
    "Open node in Workbench",
    "Fit view",
    "Save session",
];

/// The command category of the rows composed ahead of the static registry.
pub(crate) const CONTEXT_COMMANDS: &str = "context";
/// The command category of the static registry.
const STATIC_COMMANDS: &str = "turnstone";
/// The bare `>` lane's last row, which expands it to the whole catalog.
pub(crate) const ALL_COMMANDS_ROW: &str = "All commands\u{2026}";

/// Which labels are commands, and which of those the person keeps.
pub(crate) struct CommandKeepStates {
    registered: std::collections::HashSet<String>,
    kept: std::collections::HashSet<String>,
}

impl CommandKeepStates {
    pub(crate) fn of(&self, label: &str) -> Option<bool> {
        self.registered
            .contains(label)
            .then(|| self.kept.contains(label))
    }
}

impl App {
    /// The dynamic switcher entries for the omnibar's `>` lane: a switch per
    /// OTHER session, most recently updated first ("New session" is a static
    /// palette entry).
    /// The participant rows for the palette's actions lane: the pending
    /// install's visible review (the Confirm row IS the ask), then one Run
    /// row per resident (B1: the palette populated from participant residency).
    /// Lower a participant's emitted Actions through this same spine with the
    /// journal scoped to its subject, so every captured graph edit reads back
    /// attributed. Shared by both runnable lanes: piccolo returns Actions
    /// after evaluation, the component lane returns the ring-gate's accepted
    /// queue. Every action is revalidated against the live binding and grants.
    pub(super) fn lower_denizen_actions(
        &mut self,
        run_id: servitor::RunId,
        subject: servitor::Subject,
        label: String,
        actions: Vec<Action>,
    ) -> (Vec<Effect>, bool) {
        let session = self.session_id;
        let ticket = match self.resident_runs.reducer(run_id) {
            Ok(run) if run.ticket().binding.subject == subject => run.ticket().clone(),
            _ => {
                self.events.push(AppEvent::DenizenRefused(format!("{label}: missing or mismatched run ticket")));
                return (vec![Effect::Redraw], true);
            }
        };
        let member = uuid::Uuid::from_bytes(ticket.binding.id.0);
        // The admitted body supplies the version; the host supplies the route.
        // Both interpreted and component bodies are participant scripts.
        let author = mere::kernel::graph::Author::script(
            subject.to_hex(),
            blake3::Hash::from(ticket.binding.revision.0).to_hex().to_string(),
        )
        .via("turnstone");
        // `update` can synchronously drain another resident.  Preserve the
        // author that was in force when this lowering began so that the inner
        // run restores this run, rather than unconditionally restoring `user`.
        // The guard deliberately releases the journal lock before `update`:
        // graph capture takes that lock itself.
        let previous_author = match self.journal.lock() {
            Ok(mut journal) => {
                let previous = journal.author().to_owned();
                journal.set_author(author.clone());
                previous
            }
            Err(poisoned) => {
                let mut journal = poisoned.into_inner();
                let previous = journal.author().to_owned();
                journal.set_author(author);
                previous
            }
        };
        let _restore_author = JournalAuthorRestore {
            journal: self.journal.clone(),
            previous_author,
        };
        let mut effects = Vec::new();
        let mut refused = false;
        for action in actions {
            // Evaluation's capability view is advisory. Authority is checked
            // once more at every actual emission, because a cascade may have
            // changed live grants since the body was evaluated.
            if self.session_id != session {
                self.events.push(AppEvent::DenizenRefused(format!("{label}: session changed during run")));
                refused = true;
                break;
            }
            self.denizens.authority.set_now(crate::denizen::now_ms());
            let current = self.denizens.residents.get(&member).map(|resident| &resident.binding);
            let validation = current.ok_or_else(|| "resident disappeared".to_string())
                .and_then(|binding| servitor::revalidate(&ticket, binding, &self.denizens.authority, &[])
                    .map_err(|reason| format!("run invalidated: {reason:?}")));
            if let Err(reason) = validation {
                self.events.push(AppEvent::DenizenRefused(format!("{label}: {reason}")));
                refused = true;
                break;
            }
            if let Err(reason) = crate::ring::emit_allowed(&self.denizens.authority, subject, &action) {
                self.events.push(AppEvent::DenizenRefused(format!("{label}: action refused: {reason}")));
                refused = true;
                break;
            }
            effects.extend(self.update(action));
        }
        if !refused {
            self.events.push(AppEvent::DenizenRan(label));
        }
        effects.push(Effect::SaveSession);
        effects.push(Effect::Redraw);
        (effects, refused)
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
        // Participant rows lead: a pending install's review must be the first
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
    /// static registry, because a pending participant install's grant review must be
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
        if self.browser_command_target().is_some_and(|(member, _)| {
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
        if let Some((member, _)) = self.browser_command_target()
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

    /// The catalog read through Cambium's command set (Scenograph editor plan
    /// SE28 to SE32). Each row registers under its label, which is what the
    /// automation runner resolves; the rows `available_actions` puts ahead of
    /// the static registry form the `context` category, so they still lead.
    /// The first row wins a shared label, as it does for the runner.
    pub(crate) fn command_set(&self) -> (cambium::CommandSet, Vec<(String, Action)>) {
        let catalog = self.available_actions();
        let contextual = catalog
            .len()
            .saturating_sub(crate::action::palette_actions().len());
        let mut set = cambium::CommandSet::new().with_defaults(DEFAULT_KEPT_COMMANDS);
        for (index, (label, _)) in catalog.iter().enumerate() {
            if set.get(label).is_some() {
                continue;
            }
            let category = if index < contextual {
                CONTEXT_COMMANDS
            } else {
                STATIC_COMMANDS
            };
            set.register(cambium::Command::new(label.clone(), label.clone(), category));
        }
        (set, catalog)
    }

    /// What the `>` lane offers. A bare `>` shows the contextual rows, then the
    /// kept commands, then the recent ones, and ends with "All commands…",
    /// which expands to the whole catalog in its composed order. A query
    /// searches every command. `rows` bounds the lane so the expanding row is
    /// never truncated away.
    pub(crate) fn command_lane(&self, query: &str, all: bool, rows: usize) -> Vec<(String, Action)> {
        let (set, catalog) = self.command_set();
        if all {
            return catalog;
        }
        let action_for = |label: &str| {
            catalog
                .iter()
                .find(|(known, _)| known == label)
                .map(|(_, action)| action.clone())
        };
        let mut lane: Vec<(String, Action)> = set
            .menu(&self.command_choices, Some(CONTEXT_COMMANDS), query)
            .into_iter()
            .filter_map(|item| action_for(&item.id).map(|action| (item.label, action)))
            .collect();
        if query.trim().is_empty() {
            lane.truncate(rows.saturating_sub(1).max(1));
            lane.push((ALL_COMMANDS_ROW.to_string(), Action::OmnibarShowAllCommands));
        }
        lane
    }

    /// Whether the command `label` is kept, or `None` when it is not a command
    /// (an address row, a hint, or "All commands…" itself).
    pub(crate) fn command_keep_states(&self) -> CommandKeepStates {
        let (set, _) = self.command_set();
        CommandKeepStates {
            registered: set.commands().iter().map(|command| command.id.clone()).collect(),
            kept: set
                .kept(&self.command_choices)
                .into_iter()
                .map(|command| command.id.clone())
                .collect(),
        }
    }

    /// Keep `label` if it is not kept, drop it if it is. The choice is the
    /// person's and is saved with the session's view sidecar.
    pub(super) fn toggle_command_kept(&mut self, label: &str) -> Vec<Effect> {
        let (set, _) = self.command_set();
        if set.get(label).is_none() {
            return vec![Effect::Redraw];
        }
        let kept = set
            .kept(&self.command_choices)
            .iter()
            .any(|command| command.id == label);
        if kept {
            set.remove(&mut self.command_choices, label);
        } else {
            set.add(&mut self.command_choices, label);
        }
        self.events.push(AppEvent::CommandKept {
            label: label.to_string(),
            kept: !kept,
        });
        self.recompute_omnibar_suggestions();
        vec![Effect::SaveSession, Effect::Redraw]
    }

    /// Note that `label` ran from the palette. Recents ride the next session
    /// save rather than forcing one per command.
    pub(super) fn record_command_use(&mut self, label: &str) {
        let (set, _) = self.command_set();
        set.record_use(&mut self.command_choices, label);
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

/// Restores the graph journal's exact prior attribution after a synchronous
/// nested lowering or an early return. It never crosses an asynchronous wait.
struct JournalAuthorRestore {
    journal: std::sync::Arc<std::sync::Mutex<mere::kernel::graph::GraphJournal>>,
    previous_author: mere::kernel::graph::Author,
}

impl Drop for JournalAuthorRestore {
    fn drop(&mut self) {
        match self.journal.lock() {
            Ok(mut journal) => journal.set_author(self.previous_author.clone()),
            Err(poisoned) => poisoned.into_inner().set_author(self.previous_author.clone()),
        }
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

/// The `>` lane read through Cambium's command set (Scenograph editor plan
/// SE28 to SE32).
#[cfg(test)]
mod command_menu_tests {
    use super::{ALL_COMMANDS_ROW, DEFAULT_KEPT_COMMANDS};
    use crate::action::{Action, Effect};
    use crate::app::App;
    use crate::observe::AppEvent;
    use crate::ui::Suggestion;

    fn labels(rows: &[(String, Action)]) -> Vec<&str> {
        rows.iter().map(|(label, _)| label.as_str()).collect()
    }

    fn contextual(app: &App) -> Vec<String> {
        let catalog = app.available_actions();
        let statics = crate::action::palette_actions().len();
        catalog[..catalog.len() - statics]
            .iter()
            .map(|(label, _)| label.clone())
            .collect()
    }

    #[test]
    fn a_bare_lane_shows_context_then_kept_then_all_commands() {
        let app = App::test_stub();
        let lane = app.command_lane("", false, 100);
        let mut expected = contextual(&app);
        expected.extend(DEFAULT_KEPT_COMMANDS.iter().map(|label| label.to_string()));
        expected.push(ALL_COMMANDS_ROW.to_string());
        assert_eq!(labels(&lane), expected);
        assert!(
            !labels(&lane).contains(&"Reseed layout"),
            "a command that is not kept waits for search or All commands"
        );
        assert_eq!(lane.last().unwrap().1, Action::OmnibarShowAllCommands);
    }

    #[test]
    fn the_all_commands_row_survives_a_short_lane() {
        let app = App::test_stub();
        let lane = app.command_lane("", false, 4);
        assert_eq!(lane.len(), 4);
        assert_eq!(lane.last().unwrap().0, ALL_COMMANDS_ROW);
    }

    #[test]
    fn search_reaches_every_command() {
        let app = App::test_stub();
        let lane = app.command_lane("reseed", false, 100);
        assert_eq!(labels(&lane), ["Reseed layout"]);
        assert!(
            !labels(&lane).contains(&ALL_COMMANDS_ROW),
            "the expanding row belongs to the bare lane"
        );
    }

    #[test]
    fn keeping_and_dropping_change_the_lane_and_save() {
        let mut app = App::test_stub();
        let effects = app.toggle_command_kept("Reseed layout");
        assert!(effects.contains(&Effect::SaveSession));
        assert!(app.events.iter().any(|event| matches!(
            event,
            AppEvent::CommandKept { label, kept: true } if label == "Reseed layout"
        )));
        assert!(labels(&app.command_lane("", false, 100)).contains(&"Reseed layout"));

        app.toggle_command_kept("Fit view");
        let lane = app.command_lane("", false, 100);
        assert!(!labels(&lane).contains(&"Fit view"), "a default can be dropped");
        assert_eq!(app.command_choices.removed, ["Fit view"]);

        app.toggle_command_kept("Fit view");
        assert!(labels(&app.command_lane("", false, 100)).contains(&"Fit view"));
        assert!(app.command_choices.removed.is_empty());

        let before = app.command_choices.clone();
        assert_eq!(app.toggle_command_kept("Not a command"), vec![Effect::Redraw]);
        assert_eq!(app.command_choices, before, "only registered commands are kept");
    }

    #[test]
    fn a_command_run_from_the_palette_becomes_recent() {
        let mut app = App::test_stub();
        app.update(Action::OmnibarOpen { command: true });
        app.update(Action::OmnibarInsert("reseed".into()));
        let selected = app
            .omnibar
            .suggestions
            .iter()
            .position(|row| matches!(row, Suggestion::Act { label, .. } if label == "Reseed layout"))
            .expect("search finds it");
        app.update(Action::OmnibarCommitRow(selected));
        assert_eq!(app.command_choices.recent, ["Reseed layout"]);
        let lane = app.command_lane("", false, 100);
        let position = |label: &str| labels(&lane).iter().position(|row| *row == label);
        assert!(
            position("Reseed layout") > position("Save session"),
            "recent commands follow the kept ones"
        );
    }

    #[test]
    fn all_commands_expands_the_lane_and_keeps_the_palette_open() {
        let mut app = App::test_stub();
        app.update(Action::OmnibarOpen { command: true });
        let last = app.omnibar.suggestions.len() - 1;
        assert!(matches!(
            &app.omnibar.suggestions[last],
            Suggestion::Act { label, .. } if label == ALL_COMMANDS_ROW
        ));
        app.update(Action::OmnibarCommitRow(last));
        assert!(app.omnibar.open, "expanding is not a command");
        assert!(app.omnibar.all_commands);
        // The whole catalog in its composed order, bounded by the row limit as
        // the bare lane always was; typing narrows it.
        let shown: Vec<String> = app
            .omnibar
            .suggestions
            .iter()
            .map(|row| match row {
                Suggestion::Act { label, .. } => label.clone(),
                other => panic!("an expanded lane lists commands: {other:?}"),
            })
            .collect();
        let catalog: Vec<String> = app
            .available_actions()
            .into_iter()
            .map(|(label, _)| label)
            .take(shown.len())
            .collect();
        assert_eq!(shown, catalog);
        app.update(Action::OmnibarInsert("reseed".into()));
        assert!(app.omnibar.suggestions.iter().any(
            |row| matches!(row, Suggestion::Act { label, .. } if label == "Reseed layout")
        ));
        app.update(Action::OmnibarClose);
        app.update(Action::OmnibarOpen { command: true });
        assert!(!app.omnibar.all_commands, "closing returns to the person's commands");
    }

    #[test]
    fn the_highlighted_command_toggles_with_ctrl_d() {
        let mut app = App::test_stub();
        app.update(Action::OmnibarOpen { command: true });
        app.update(Action::OmnibarInsert("reseed".into()));
        app.omnibar.selected = app
            .omnibar
            .suggestions
            .iter()
            .position(|row| matches!(row, Suggestion::Act { label, .. } if label == "Reseed layout"))
            .unwrap();
        app.update(Action::OmnibarToggleKeepSelected);
        assert_eq!(app.command_choices.added, ["Reseed layout"]);
        assert!(app.omnibar.open, "keeping does not run or close");
    }

    /// The canvas, not Turnstone, decides what a right press was: a click
    /// within the slop asks for the menu (naming the node under it), and a
    /// right-drag past it selects and asks for nothing (SE26).
    #[test]
    fn a_right_click_asks_for_the_menu_and_a_right_drag_does_not() {
        use mere::canvas::PointerButton;
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("mere://alpha".into()));
        let pane = app.default_graph_pane();
        app.graph_pane_pointer_down(pane, PointerButton::Right, 40.0, 40.0);
        app.graph_pane_pointer_up(pane, PointerButton::Right, 41.0, 40.0);
        let request = app
            .graph_pane_take_context_request(pane)
            .expect("a right click within the slop asks for the menu");
        assert_eq!(request.at, (41.0, 40.0));
        assert!(app.graph_pane_take_context_request(pane).is_none(), "taken once");

        app.graph_pane_pointer_down(pane, PointerButton::Right, 40.0, 40.0);
        app.graph_pane_cursor_moved(pane, 140.0, 120.0);
        app.graph_pane_pointer_up(pane, PointerButton::Right, 140.0, 120.0);
        assert!(
            app.graph_pane_take_context_request(pane).is_none(),
            "a right-drag is a selection"
        );
    }

    #[test]
    fn palette_rows_and_their_keep_controls_are_accessible() {
        let mut app = App::test_stub();
        app.update(Action::OmnibarOpen { command: true });
        let lines = crate::a11y::tree_lines(&crate::a11y::project_app(&app));
        assert!(
            lines.iter().any(|line| line.contains("button") && line.contains("Drop Back")),
            "a kept command offers Drop: {lines:#?}"
        );
        assert!(lines.iter().any(|line| line.contains(ALL_COMMANDS_ROW)));

        let (_, routes) = crate::a11y::project_app_with_routes(&app);
        let back = app
            .omnibar
            .suggestions
            .iter()
            .position(|row| matches!(row, Suggestion::Act { label, .. } if label == "Back"))
            .unwrap();
        let target = uxtree::node_id_for_path(&format!("turnstone/chrome/omnibar/row/{back}/keep"));
        crate::a11y::apply_route(&mut app, routes.get(&target), target);
        assert_eq!(app.command_choices.removed, ["Back"], "the assistive action drops it");
    }
}
