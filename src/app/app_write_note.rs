// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    /// Consume one app intent. Never blocks; anything slow leaves as an effect.
    /// Write a note's body, minting the node if the address is new.
    ///
    /// Goes through `visit` and the kernel's body delta, so the write is
    /// journaled and attributed exactly like any other: a summary a behavior
    /// wrote reads back in the node's history under that behavior, not under
    /// the user.
    pub(super) fn write_note(&mut self, url: String, body: String) -> Vec<Effect> {
        let key = self.graph_runtimes.visit(&url);
        let Some(member) = self
            .graph_runtimes
            .graph()
            .get_node(key)
            .map(|node| node.id)
        else {
            return vec![Effect::Redraw];
        };
        let _ = self.graph_runtimes.set_node_body_for(member, Some(body));
        vec![Effect::SaveSession, Effect::Redraw]
    }

    /// Record a semantic event from outside the app module (the behavior
    /// drain). `events` stays private: the drain is the only outside writer,
    /// and naming it here keeps that true.
    pub(crate) fn record_event(&mut self, event: crate::observe::AppEvent) {
        self.events.push(event);
    }

    /// Apply an action, then run whatever behaviors its commits woke.
    ///
    /// The cascade runs *after* the action's own effects are decided, so a
    /// woken body sees the world the action left rather than the one it found,
    /// and its own effects join the same return.
    pub fn update(&mut self, action: Action) -> Vec<Effect> {
        if self.session_load_refused()
            && !matches!(
                &action,
                Action::NewSession
                    | Action::SwitchSession(_)
                    | Action::OmnibarOpen { .. }
                    | Action::OmnibarClose
                    | Action::OmnibarChar(_)
                    | Action::OmnibarInsert(_)
                    | Action::OmnibarBackspace
                    | Action::OmnibarDelete
                    | Action::OmnibarCaret(_)
                    | Action::OmnibarMove(_)
                    | Action::OmnibarCommitRow(_)
                    | Action::OmnibarShowAllCommands
                    | Action::OmnibarCommit
            )
        {
            return vec![Effect::Redraw];
        }
        let records_visit = matches!(&action, Action::OpenAddress(_));
        if records_visit || matches!(&action, Action::WorkbenchActivate(_)) {
            self.capture_scope_origin();
        }
        let mut effects = self.dispatch(action);
        if records_visit {
            self.record_scope_current();
            effects.push(Effect::SaveReadingPreferences);
        }
        if !self.session_load_refused() {
            effects.extend(crate::behaviors::drain(self));
        }
        effects.extend(self.invalidate_saved_readers());
        self.sync_research_context();
        effects.extend(self.invalidate_research_search());
        effects.extend(self.invalidate_research_clip_readers());
        effects
    }

    pub(super) fn dispatch(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::Reading(action) => self.read_action(action),
            Action::OpenThemeWorkshop => vec![Effect::OpenThemeWorkshop],
            Action::OpenAlembicWorkshop => vec![Effect::OpenAlembicWorkshop],
            Action::OpenAddress(url) => self.open_address(url),
            Action::RetainProducerSelection { choice, selection } => {
                self.retain_producer_selection(choice, selection)
            },
            Action::ChooseClipPackageFile => vec![Effect::ChooseClipPackageFile],
            Action::ImportClipPackage(path) => vec![Effect::ImportClipPackage(path)],
            Action::ChooseClipExportFile => self.choose_retained_clip_export(),
            Action::ExportClipPackage {
                clip,
                path,
                replace,
            } => {
                vec![Effect::ExportClipPackage {
                    clip,
                    path,
                    replace,
                }]
            },
            Action::ComposeFocusedSmolwebSubmission => self.compose_focused_smolweb_submission(),
            Action::ComposeFocusedMicronForm => self.compose_focused_micron_form(),
            Action::BeginSmolwebSubmission { source, target } => {
                self.begin_smolweb_submission(source, target)
            },
            Action::SmolwebSubmissionFile {
                name,
                bytes,
                suggested_mime,
            } => self.set_smolweb_submission_file(name, bytes, suggested_mime),
            Action::ContentNavigationCommitted { member, url } => {
                self.commit_content_navigation(member, url)
            },
            Action::ContentTitleChanged { member, title } => self.set_content_title(member, title),
            Action::OpenDocumentFind => self.open_document_find(),
            Action::CloseDocumentFind => self.close_document_find(),
            Action::EditDocumentFind(input) => self.edit_document_find(input),
            Action::StepDocumentFind(direction) => self.step_document_find(direction),
            Action::PageZoomIn { member } => self.page_zoom_in(member),
            Action::PageZoomOut { member } => self.page_zoom_out(member),
            Action::PageZoomReset { member } => self.page_zoom_reset(member),
            Action::CapturePage { member } => vec![Effect::CaptureContent { node: member }],
            Action::CaptureSourceDocument { target } => self.begin_source_capture(target),
            Action::RetrySourceDocumentCapture { request } => self.retry_source_capture(request),
            Action::SearchSavedSources => self.search_saved_sources(),
            Action::ResearchSearch(command) => self.research_search_command(command),
            Action::OpenSavedSource(target) => self.open_saved_source(target),
            Action::ChoosePermission { request, choice } => self.choose_permission(request, choice),
            Action::FocusAuthenticationField(field) => self.focus_authentication_field(field),
            Action::InsertAuthentication(text) => self.insert_authentication(text),
            Action::BackspaceAuthentication => self.backspace_authentication(),
            Action::ToggleAuthenticationMemory => self.toggle_authentication_memory(),
            Action::SubmitAuthentication { request } => self.submit_authentication(request),
            Action::CancelAuthentication { request } => self.cancel_authentication(request),
            Action::Scope(command) => self.scope_command(command),
            Action::FocusCard(command) => self.focus_card_command(command),
            Action::SelectScopeMere(id) => self.select_scope_mere(id),
            Action::NavBack => self.nav_back(),
            Action::NavForward => self.nav_forward(),
            Action::Reload => self.reload_focused(),
            Action::Stop => self.stop_focused(),
            Action::KeepNode { member } => self.keep_node(member),
            Action::SubscribeFocusedFeed { period } => self.subscribe_focused_feed(period),
            Action::UnsubscribeFocusedFeed => self.unsubscribe_focused_feed(),
            Action::RefreshFeeds => self.refresh_feeds(),
            Action::MarkFocusedFeedEntryRead => self.mark_focused_feed_entry_read(),
            action @ (Action::FoldSelection
            | Action::ExpandFold
            | Action::ExpandAllFolds
            | Action::UndoFold
            | Action::RedoFold) => self.edit_graph_folds(action),
            action @ (Action::ResetToSeed
            | Action::SetSeed
            | Action::RestoreRecipeSeed
            | Action::ToggleForegroundPin
            | Action::DismissRetained
            | Action::GrowTiles
            | Action::ShrinkTiles
            | Action::ToggleTerritory) => self.edit_graph_scene(action),
            Action::ReseedLayout => self.reseed_layout(),
            Action::SetLayoutStrategy(id) => self.set_layout_strategy(id),
            action @ (Action::SetPhysicsLaw(_)
            | Action::SetPhysicsOverlay(_, _)
            | Action::SetPhysicsKindSource(_)
            | Action::SetPhysicsMassSource(_)
            | Action::SetPhysicsDepthSource(_)
            | Action::ApplyPhysicsProfile(_)
            | Action::TogglePhysics) => self.edit_graph_physics(action),
            action @ (Action::SetPhysicsRole(_)
            | Action::SetPhysicsRoleGroups(_)
            | Action::ApplyPhysicsMix(_)
            | Action::ApplyPhysicsGrouping(_)
            | Action::ApplyPhysicsSchedule(_)
            | Action::UseSingleLaw
            | Action::SetPhysicsSpeed(_)) => self.compose_graph_physics(action),
            Action::ToggleIsometric => {
                let on = !self.graph_runtimes.is_isometric();
                self.graph_runtimes.set_isometric(on);
                vec![Effect::Redraw]
            },
            Action::OrbitBy(delta) => {
                self.graph_runtimes.orbit_by(delta);
                vec![Effect::Redraw]
            },
            Action::TiltBy(delta) => {
                let tilt = self.graph_runtimes.tilt();
                self.graph_runtimes.set_tilt(tilt + delta);
                vec![Effect::Redraw]
            },
            Action::ToggleHeightByDegree => {
                let on = !self.graph_runtimes.height_by_degree();
                self.graph_runtimes.set_height_by_degree(on);
                vec![Effect::Redraw]
            },
            Action::FitView => {
                if let Some(pane) = self.graph_command_pane() {
                    self.with_graph_pane(pane, |canvas| canvas.fit_visible());
                    self.graph_views.request_fit(pane);
                }
                vec![Effect::Redraw]
            },
            Action::ToggleSizeByRecency => self.toggle_size_by_recency(),
            Action::SaveSession => vec![Effect::SaveSession],
            Action::BeginFoundPlace => self.begin_place_prompt(crate::ui::PlacePrompt::FoundPlace),
            Action::FoundPlace { name } => self.found_place(name),
            Action::BeginExportPlaceCard => {
                self.begin_place_prompt(crate::ui::PlacePrompt::ExportCard)
            },
            Action::ExportPlaceCard { path } => self.export_place_card(path),
            Action::BeginRefreshPlaceContact => {
                self.begin_place_prompt(crate::ui::PlacePrompt::RefreshContact)
            },
            Action::RefreshPlaceContact { path } => self.refresh_place_contact(path),
            Action::ReconnectPlaceWithCard {
                session,
                generation,
                card,
            } => {
                if session != self.session_id || self.place.generation() != Some(generation) {
                    Vec::new()
                } else {
                    self.reconnect_place_with_contact(Some(card))
                }
            },
            Action::BeginOfferPlacePrekey => {
                self.begin_place_prompt(crate::ui::PlacePrompt::OfferPrekey)
            },
            Action::OfferPlacePrekey { path } => self.offer_place_prekey(path),
            Action::OfferPlacePrekeyForCard { card, out } => {
                self.offer_place_prekey_for_card(card, out)
            },
            Action::BeginInviteToPlace => self.begin_place_prompt(crate::ui::PlacePrompt::Invite),
            Action::BeginInviteToPlaceAsReader => {
                self.begin_place_prompt(crate::ui::PlacePrompt::InviteReader)
            },
            Action::InviteToPlace {
                path,
                access,
                lifetime_ms,
            } => self.invite_to_place(path, access, lifetime_ms),
            Action::InviteToPlaceWithPrekey {
                prekey,
                out,
                access,
                lifetime_ms,
            } => self.invite_to_place_with_prekey(prekey, out, access, lifetime_ms),
            Action::RevokePlaceMember { member } => self.revoke_place_member(member),
            Action::BeginJoinPlaceFile => self.begin_place_prompt(crate::ui::PlacePrompt::Join),
            Action::JoinPlaceFile { path } => self.join_place_file(path),
            Action::BeginSendPlaceMessage => {
                self.begin_place_prompt(crate::ui::PlacePrompt::SendMessage)
            },
            Action::JoinPlace(invite) => self.join_place(invite),
            Action::LeavePlace => self.leave_place(),
            Action::ReconnectPlace => self.reconnect_place(),
            Action::RejoinPlace => self.rejoin_place(),
            Action::ShowPlaceStatus => self.open_place_status(),
            Action::CopyLocalRendezvous => self.copy_local_rendezvous(),
            Action::SendPlaceMessage { channel, body } => {
                self.run_place_command(crate::place::worker::PlaceCommand::SendMessage {
                    channel,
                    body,
                })
            },
            Action::ShareFocusedNode => match self.focused_address() {
                Some(address) => self
                    .run_place_command(crate::place::worker::PlaceCommand::ShareNode { address }),
                None => {
                    self.events
                        .push(AppEvent::PlaceRefused("no focused node to share".into()));
                    vec![Effect::Redraw]
                },
            },
            Action::ResyncPlace => self.resync_place(),
            Action::SetPlaceCollection(selection) => self.set_place_collection(selection),
            Action::SearchCapturedPages => self.open_captured_pages(),
            // Multi-session (rung 6's second half). Both lower to the shell's
            // SwitchSession effect: the PORT saves the departing session and
            // tears down its live handles before the app adopts the target —
            // state here, ports there, ordering correct.
            Action::NewSession => {
                let id = Self::mint_session(&self.data_root, &mut self.sessions);
                self.enter_scope_session(id);
                vec![Effect::SwitchSession { id }]
            },
            Action::SwitchSession(id) => {
                if (id == self.session_id && !self.session_load_refused())
                    || (id != self.session_id && self.sessions.get(id).is_none())
                {
                    return vec![Effect::Redraw];
                }
                self.enter_scope_session(id);
                vec![Effect::SwitchSession { id }]
            },
            // ---- Participant residency (participant gate B1) ----
            Action::InstallDenizen { path } => self.install_denizen(path),
            Action::ConfirmInstallDenizen => self.confirm_install_denizen(),
            Action::CancelInstallDenizen => {
                if self.pending_install.take().is_some() {
                    self.events
                        .push(AppEvent::DenizenRefused("cancelled".into()));
                }
                self.omnibar = OmnibarState::default();
                self.shell.close_omnibar();
                vec![Effect::Redraw]
            },
            Action::UninstallDenizen { member } => self.uninstall_denizen(member),
            Action::RunDenizen { member } => self.run_denizen(member),
            Action::WriteNote { url, body } => self.write_note(url, body),
            Action::RecoverSession(id) => self.recover_session(id),
            Action::CloseSession => self.close_session(),
            Action::BeginRenameSession => self.begin_rename_session(),
            Action::RenameSession { id, name } => self.rename_session(id, name),
            Action::DeleteFocusedNode => self.delete_focused_node(),
            Action::RecoverDeletedNode(id) => self.recover_deleted_node(id),
            Action::EmptyRecycleBin => self.empty_recycle_bin(),
            Action::NewWindow => self.new_window(),
            Action::FloatActivePane => self.float_active_pane(),
            Action::DockActivePane => self.dock_active_pane(),
            // The tear-out trichotomy's LEAF arm: the active pane's frisket
            // leaf leaves this window's tree and joins the newest lens's
            // (spawning one when none is open). The pane's retained runner is
            // untouched — in the surface-compositor shape, identity across
            // windows is a property of the RUNNER staying put while the leaf
            // changes trees, which is exactly what the forest dom exists to
            // buy the one-shared-DOM shape.
            Action::TearOutActivePane => self.tear_out_active_pane(),
            Action::ReturnActivePaneToPrimary => self.return_active_pane_to_primary(),
            // The trichotomy's BRANCH arm, gesture-first: a workbench tab
            // dragged out of the pane. The tile leaves platen's tiling and
            // becomes a pinned Tile pane in a lens window; its live session
            // (if any) composites there as the pane's content surface.
            Action::TearOutTile { member } => self.tear_out_tile(member),
            // The trichotomy's FORK arm: snapshot the component into a fresh
            // session and switch to it (G4-R R2; the shell saves the donor on
            // the way out, as every switch does).
            Action::ForkNode { member } => self.fork_session_from(member),
            Action::ForkFocusedNode => match self.graph_runtimes.focused_member() {
                Some(member) => self.fork_session_from(member),
                None => Vec::new(),
            },
            Action::SetViewerOverride { member, viewer } => {
                self.set_viewer_override(member, viewer)
            },
            Action::SetNodeSprite {
                member,
                data_uri,
                hull,
            } => self.set_node_sprite(member, data_uri, hull),
            Action::ToggleNodeContent => self.toggle_node_content(),
            Action::OmnibarOpen { command } => self.open_omnibar(command),
            Action::OmnibarClose => self.close_omnibar(),
            Action::OmnibarChar(c) => self.omnibar_char(c),
            Action::OmnibarInsert(s) => self.omnibar_insert(s),
            Action::OmnibarBackspace => self.omnibar_backspace(),
            Action::OmnibarDelete => self.omnibar_delete(),
            Action::OmnibarCaret(m) => {
                // Caret motion never changes the text, so the suggestion
                // list (and the highlight) stays put.
                self.omnibar.move_caret(m);
                vec![Effect::Redraw]
            },
            Action::OmnibarMove(delta) => self.omnibar_move(delta),
            Action::OmnibarCommitRow(index) => self.omnibar_commit_row(index),
            Action::OmnibarShowAllCommands => self.show_all_commands(),
            Action::OmnibarToggleKeepRow(index) => self.toggle_keep_row(index),
            Action::OmnibarToggleKeepSelected => self.toggle_keep_row(self.omnibar.selected),
            Action::OmnibarCommit => self.commit_omnibar(),
            Action::RepeatShellEntry(id) => self.repeat_shell_entry(id),
            Action::OpenShellEntryTarget(id) => self.open_shell_entry_target(id),
            // Pane tree ops (rung 5 slice C). Each mutates the frisket layout and
            // persists it (SaveSession writes frame.json), so the arrangement
            // survives a restart. Maximize is view state, not persisted.
            Action::SummonPane(kind) => self.summon_pane(kind),
            Action::SetWorkspaceMap(map) => self.set_workspace_map(map),
            Action::ReadGraphMember(member) => self.read_graph_member(member),
            Action::NavigateRelationship(target) => self.navigate_relationship(target),
            Action::RevealRelationship(target) => self.reveal_relationship(target),
            Action::SetPaneBinding(intent) => self.apply_pane_binding(intent),
            Action::RevealGraphMember(member) => self.reveal_graph_member(member),
            Action::ToggleToolPane(kind) => self.toggle_tool_pane(kind),
            Action::SetChromeLayout(layout) => {
                let mut chrome = self.shell_chrome_config().clone();
                chrome.layout = layout;
                self.set_shell_chrome_config(chrome);
                vec![Effect::SaveChromeLayout(layout), Effect::Redraw]
            },
            Action::ChooseKnotDocumentFile { read_only } => {
                vec![Effect::ChooseKnotDocumentFile { read_only }]
            },
            Action::SetAnswerSelection {
                generation,
                expected,
                chosen,
            } => self.set_answer_selection(generation, expected, chosen),
            Action::OpenAnswers => self.open_answers(),
            Action::NewResearchNote => vec![Effect::CreateResearchNote {
                generation: self.source_capture_generation,
            }],
            Action::ChooseResearchNote { choice } => vec![Effect::ChooseResearchNote { choice }],
            Action::CommitResearchNote { choice } => self.commit_research_note(choice),
            Action::ResearchNoteCreated {
                generation,
                address,
            } => self.research_note_created(generation, address),
            Action::OpenResearchNote => self.open_research_note(),
            Action::SummonContributedPane { kind, source } => {
                self.summon_contributed_pane(kind, source)
            },
            Action::CloseActivePane => self.close_active_pane(),
            Action::OuterWorkspace { space, event } => self.outer_workspace_event(space, event),
            Action::SetSplitRatio { space, path, ratio } => {
                if let Some(layout) = self.space_mut(space) {
                    layout.set_split_ratio(&path, ratio);
                }
                vec![Effect::Redraw]
            },
            Action::SetActivePaneDivider(ratio) => self.set_active_pane_divider(ratio),
            Action::ToggleMaximizePane => self.toggle_maximize_pane(),
            Action::TogglePaneSection { pane, section } => self.toggle_pane_section(pane, section),
            Action::MovePaneSection {
                pane,
                section,
                delta,
            } => self.move_pane_section(pane, section, delta),
            // Workbench ops (rung 5 slice E). Platen owns the model and every
            // mutator; these arms lower intents onto it and persist. The
            // Workbench PANE (the frisket leaf) is where the tiling shows;
            // opening a tile summons it if absent, through the same summon
            // path as a palette summon (one spine, no side door).
            Action::OpenInWorkbench => self.open_in_workbench(),
            Action::WorkbenchActivate(member) => self.activate_workbench_member(member),
            Action::CloseWorkbenchTile => self.close_workbench_tile(),
            Action::WorkbenchStackOnto { dragged, target } => {
                self.workbench_stack_onto(dragged, target)
            },
            Action::WorkbenchSplitBeside {
                dragged,
                target,
                axis,
                after,
            } => self.workbench_split_beside(dragged, target, axis, after),
            Action::WorkbenchSplitOut {
                dragged,
                axis,
                after,
            } => self.workbench_split_out(dragged, axis, after),
            Action::WorkbenchSetFractions { path, fractions } => {
                if let Some(owner) = self.workbench_owner_pane()
                    && let Some(workbench) = self.workbench_for_pane_mut(owner)
                {
                    workbench.set_split_fractions(&path, &fractions);
                }
                vec![Effect::Redraw]
            },
        }
    }
}
