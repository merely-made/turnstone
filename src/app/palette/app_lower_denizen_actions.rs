// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
    pub(in super::super) fn lower_denizen_actions(
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
                self.events.push(AppEvent::DenizenRefused(format!(
                    "{label}: missing or mismatched run ticket"
                )));
                return (vec![Effect::Redraw], true);
            },
        };
        let member = uuid::Uuid::from_bytes(ticket.binding.id.0);
        // The admitted body supplies the version; the host supplies the route.
        // Both interpreted and component bodies are participant scripts.
        let author = mere::kernel::graph::Author::script(
            subject.to_hex(),
            blake3::Hash::from(ticket.binding.revision.0)
                .to_hex()
                .to_string(),
        )
        .via("turnstone");

        let origin = self.behavior_execution_origin();
        if origin.is_none() {
            self.refuse_behavior("participant lowering has no accepted graph binding".into());
            return (vec![Effect::Redraw], true);
        }

        let _restore_author =
            crate::host_journal::AuthorRestore::enter(self.journal.clone(), author);
        let mut effects = Vec::new();
        let mut refused = false;
        for action in actions {
            // Evaluation's capability view is advisory. Authority is checked
            // once more at every actual emission, because a cascade may have
            // changed live grants since the body was evaluated.
            if self.session_id != session || self.behavior_execution_origin() != origin {
                self.events.push(AppEvent::DenizenRefused(format!(
                    "{label}: session or graph runtime changed during run"
                )));
                refused = true;
                break;
            }
            self.denizens.authority.set_now(crate::denizen::now_ms());
            let current = self
                .denizens
                .residents
                .get(&member)
                .map(|resident| &resident.binding);
            let validation = current
                .ok_or_else(|| "resident disappeared".to_string())
                .and_then(|binding| {
                    servitor::revalidate(&ticket, binding, &self.denizens.authority, &[])
                        .map_err(|reason| format!("run invalidated: {reason:?}"))
                });
            if let Err(reason) = validation {
                self.events
                    .push(AppEvent::DenizenRefused(format!("{label}: {reason}")));
                refused = true;
                break;
            }
            if let Err(reason) =
                crate::ring::emit_allowed(&self.denizens.authority, subject, &action)
            {
                self.events.push(AppEvent::DenizenRefused(format!(
                    "{label}: action refused: {reason}"
                )));
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
        if self.session_load_refused() {
            let mut rows = vec![
                (
                    "Retry session".into(),
                    Action::SwitchSession(self.session_id),
                ),
                ("New session".into(), Action::NewSession),
            ];
            rows.extend(
                self.sessions
                    .iter()
                    .filter(|(id, _)| *id != self.session_id)
                    .map(|(id, _)| {
                        (
                            format!("Switch to session {}", self.session_label(id)),
                            Action::SwitchSession(id),
                        )
                    }),
            );
            return rows;
        }
        let mut rows = self.session_actions();
        if let Some(target) = self.forme_card_target() {
            rows.push((
                "Preview reading layout".into(),
                Action::FocusCard(crate::focus_card::Command::Peek(target.clone())),
            ));
            rows.push((
                "Enter reading layout".into(),
                Action::FocusCard(crate::focus_card::Command::EnterLayout(target)),
            ));
        }
        if let Some(action) = self.producer_retention_action() {
            rows.push(action);
        }
        if let Some((member, _)) = self.browser_command_target()
            && let Ok(target) = self.source_capture_target(member)
        {
            rows.push((
                "Save source and keep".into(),
                Action::CaptureSourceDocument { target },
            ));
        }
        let mut retries: Vec<_> = self
            .source_captures
            .values()
            .filter(|attempt| {
                attempt.request.session == self.session_id
                    && attempt.admitted
                    && matches!(
                        attempt.phase,
                        crate::source_capture::CapturePhase::Failed
                            | crate::source_capture::CapturePhase::Recoverable
                    )
            })
            .collect();
        retries.sort_by_key(|attempt| {
            (
                attempt
                    .request
                    .source
                    .as_ref()
                    .map(|source| source.acquired_at_ms),
                attempt.request.id,
            )
        });
        rows.extend(retries.into_iter().enumerate().map(|(index, attempt)| {
            (
                format!("Retry source save {}: {}", index + 1, attempt.request.url),
                Action::RetrySourceDocumentCapture {
                    request: attempt.request.id,
                },
            )
        }));
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
            ) && self.content.offers(member, inker::webcue::Feature::Find)
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
        if self.clip_export_target().is_ok() {
            rows.push(("Export retained clip".into(), Action::ChooseClipExportFile));
        }
        if let Some(choice) = self.offered_research_note() {
            rows.push((choice.use_label(), Action::ChooseResearchNote { choice }));
        }
        rows.extend(crate::action::palette_actions());
        rows
    }

    /// The catalog read through Cambium's command set (Scenograph editor plan
    /// SE28 to SE32, SE45 to SE50). A shared verb registers under its
    /// catalogue id; every other row under its label, which is what the
    /// automation runner resolves. The rows `available_actions` puts ahead of
    /// the static registry form the `context` category, so they still lead.
    pub(crate) fn command_catalog(&self) -> CommandCatalog {
        let catalog = self.available_actions();
        let contextual = if self.session_load_refused() {
            catalog.len()
        } else {
            catalog
                .len()
                .saturating_sub(crate::action::palette_actions().len())
        };
        let mut set = cambium::CommandSet::new().with_defaults(DEFAULT_KEPT_COMMANDS);
        let mut actions = std::collections::HashMap::new();
        let mut ids = std::collections::HashMap::new();
        for (index, (label, action)) in catalog.into_iter().enumerate() {
            if ids.contains_key(&label) {
                continue;
            }
            let shared = (index >= contextual)
                .then(|| crate::action::shared_command_id(&action))
                .flatten();
            let (effect, destructive) = crate::action::command_metadata(&action);
            let mut command = match shared.and_then(cambium::catalogue::command) {
                Some(command) => command,
                None => {
                    let category = if index < contextual {
                        CONTEXT_COMMANDS
                    } else {
                        STATIC_COMMANDS
                    };
                    cambium::Command::new(label.clone(), label.clone(), category, effect)
                },
            };
            command.effect = effect;
            command.destructive = destructive;
            if set.get(&command.id).is_some() {
                continue;
            }
            ids.insert(label, command.id.clone());
            actions.insert(command.id.clone(), action);
            set.register(command);
        }
        CommandCatalog { set, actions, ids }
    }

    /// What the `>` lane offers, read through the omnibar's menu session. A
    /// bare `>` shows the contextual rows, then the kept commands, then the
    /// recent ones, and ends with "All commands…", which expands to every
    /// command in the catalog's composed order. A query searches every
    /// command. `rows` bounds the lane so the expanding row is never
    /// truncated away.
    pub(crate) fn command_lane(
        &self,
        menu: &cambium::MenuSession,
        rows: usize,
    ) -> Vec<(String, Action)> {
        let catalog = self.command_catalog();
        let mut lane: Vec<(String, Action)> = menu
            .rows(&catalog.set, &self.command_choices)
            .into_iter()
            .filter_map(|command| {
                catalog
                    .action(&command.id)
                    .map(|action| (command.label.clone(), action.clone()))
            })
            .collect();
        if menu.query.trim().is_empty() && !menu.all {
            lane.truncate(rows.saturating_sub(1).max(1));
            lane.push((ALL_COMMANDS_ROW.to_string(), Action::OmnibarShowAllCommands));
        }
        lane
    }

    /// Whether the command shown as `label` is kept, or `None` when it is not
    /// a command (an address row, a hint, or "All commands…" itself).
    pub(crate) fn command_keep_states(&self) -> CommandKeepStates {
        let catalog = self.command_catalog();
        let kept: std::collections::HashSet<&str> = catalog
            .set
            .kept(&self.command_choices)
            .into_iter()
            .map(|command| command.id.as_str())
            .collect();
        CommandKeepStates {
            registered: catalog
                .set
                .commands()
                .iter()
                .map(|command| command.label.clone())
                .collect(),
            kept: catalog
                .set
                .commands()
                .iter()
                .filter(|command| kept.contains(command.id.as_str()))
                .map(|command| command.label.clone())
                .collect(),
        }
    }

    /// Keep the command shown as `label` if it is not kept, drop it if it is.
    /// The choice is the person's, stored by id, and is saved with the
    /// session's view sidecar.
    pub(in super::super) fn toggle_command_kept(&mut self, label: &str) -> Vec<Effect> {
        let catalog = self.command_catalog();
        let Some(id) = catalog.id_of(label) else {
            return vec![Effect::Redraw];
        };
        let kept = catalog
            .set
            .kept(&self.command_choices)
            .iter()
            .any(|command| command.id == id);
        if kept {
            catalog.set.remove(&mut self.command_choices, id);
        } else {
            catalog.set.add(&mut self.command_choices, id);
        }
        self.events.push(AppEvent::CommandKept {
            label: label.to_string(),
            kept: !kept,
        });
        self.recompute_omnibar_suggestions();
        vec![Effect::SaveSession, Effect::Redraw]
    }

    /// Note that the command shown as `label` ran from the palette. Recents
    /// ride the next session save rather than forcing one per command.
    pub(in super::super) fn record_command_use(&mut self, label: &str) {
        let catalog = self.command_catalog();
        if let Some(id) = catalog.id_of(label) {
            catalog.set.record_use(&mut self.command_choices, id);
        }
    }

    /// The founding half of the place vocabulary. Offered by situation: a
    /// personal session can found, join, or offer a pre-key; a session already
    /// in a place can export its card, invite, and speak. Offering a row that
    /// could only refuse would teach the palette to lie.
    pub(in super::super) fn place_founding_actions(&self) -> Vec<(String, Action)> {
        let rows: &[(&str, Action)] = if self.place.binding().is_some() {
            &[
                ("Export place card", Action::BeginExportPlaceCard),
                ("Refresh place contact", Action::BeginRefreshPlaceContact),
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
    pub(in super::super) fn place_member_actions(&self) -> Vec<(String, Action)> {
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
}
