// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    /// Boot the app state (rung 6's second half: multi-session): load the
    /// manifest set, migrate the flat single-session layout if this profile
    /// predates `sessions/`, pick the session to open (recorded current,
    /// else most recent, else mint one), adopt it wholesale, then layer the
    /// launch address or the first-run sample graph on top. Returns the
    /// state plus the boot effects.
    pub fn boot(address: Option<&str>) -> (Self, Vec<Effect>) {
        let data_root = session::default_turnstone_root();
        let _ = std::fs::create_dir_all(&data_root);
        // Identity moved to the family-shared root. Turnstone held the personas
        // before the split, so it is the one that has to hand them over, and it
        // does so on first boot rather than as a migration step someone has to
        // remember to run.
        let shared = pandect::shared_root::shared_root();
        match pandect::shared_root::adopt_legacy_identity(&shared, &data_root) {
            Ok(true) => eprintln!("[turnstone] adopted this device's identity into {shared:?}"),
            Ok(false) => {},
            Err(error) => eprintln!("[turnstone] could not adopt the legacy identity: {error}"),
        }

        let journal = crate::host_journal::shared_journal();
        // Each installed graph records once. Scratch/migration graphs are quiet.

        mere::kernel::graph::set_captured_delta_hook(None);
        let mut sessions = session::load_manifests(&data_root);
        // Pre-overmap manifests minted nil root_graph_ids; the container id
        // must be real (scene.* facet key + overmap identity), so heal at boot.
        session::heal_nil_graph_ids(&mut sessions);
        let migrated = session::migrate_flat_layout(&data_root, &mut sessions);
        let picked = migrated.or_else(|| session::pick_session(&data_root, &sessions));
        let (session_id, minted) = match picked {
            Some(id) => (id, false),
            None => (Self::mint_session(&data_root, &mut sessions), true),
        };
        let initial_graph = sessions
            .get(session_id)
            .map(|manifest| manifest.root_graph_id)
            .unwrap_or_else(GraphId::nil);
        // djinn holds the profile identity; pending (None) when it is absent
        // or Locked, with no fallback key (dramatis D12).
        let identity: Option<std::sync::Arc<crate::identity::RootIdentity>> = None;
        let root = identity
            .as_deref()
            .map(|identity| identity::IdentityProvider::master_public_key(identity).to_bytes());
        let gemini_identities = crate::gemini_identity::GeminiIdentityBindings::load(&data_root);
        let mut app = Self {
            watches: servitor::WatchTable::new(),
            app_watches: servitor::WatchTable::new(),
            events_seen: 0,
            events_base: 0,
            draining: false,
            time_watches: servitor::TimeWatchTable::new(),
            deadbands: servitor::DeadbandTable::new(),
            now_ms: None,
            behavior_cursor: 0,
            cascade_budget: servitor::cascade::CascadeBudget::DEFAULT.rounds(),
            graph_runtimes: super::super::GraphRuntimePool::with_journal(
                initial_graph,
                Some(session_id),
                Canvas::new(),
                journal.clone(),
            ),
            graph_views: super::super::GraphPaneViews::default(),
            forme_runtimes: super::super::FormeRuntimePool::default(),
            pane_context: crate::panes::ContextIndex::default(),
            omnibar: OmnibarState::default(),
            reading: Default::default(),
            command_choices: cambium::CommandChoices::default(),
            document_find: crate::document_find::DocumentFindState::default(),
            user_agent_decision: crate::user_agent_decision::UserAgentDecisionState::default(),
            frame_timings: crate::frame_timing::FrameTimings::default(),
            map_theme: Default::default(),
            diagnostic_inspection: None,
            shell: crate::shell_services::ShellServices::default(),
            data_root,
            sessions,
            session_id,
            content: ContentStates::default(),
            source_captures: Default::default(),
            source_capture_generation: uuid::Uuid::new_v4(),
            saved_sources: Default::default(),
            research: Default::default(),
            research_search: Default::default(),
            saved_readers: Default::default(),
            research_clip_readers: Default::default(),
            engine_inventory: Vec::new(),
            feeds: crate::feed::FeedSubscriptions::default(),
            redshank: crate::redshank_host::RedshankHost::default(),
            redshank_members: std::collections::BTreeMap::new(),
            place: crate::place::PlaceState::default(),
            pending_place_artifact: None,
            next_place_generation: 0,
            next_place_request: 0,
            next_smolweb_submission: 0,
            active_smolweb_submission: None,
            micron_submission_sources: std::collections::HashMap::new(),
            focus: FocusTarget::Graph(crate::panes::PaneId(0)),
            link_preview: None,
            frisket: FrisketLayout::default(),
            history: chrome::nav::History::new(String::new()),
            active_pane: None,
            browser: pandect::browser_node_state::BrowserNodeStates::new(),
            physics_refusal: None,
            maximized: None,
            window_count: 1,
            viewport: super::super::DEFAULT_VIEWPORT,
            lenses: Vec::new(),
            primary_blueprint: None,
            lens_blueprints: Vec::new(),
            roster_tab: 0,
            removed: Vec::new(),
            trash: Vec::new(),
            recall: Vec::new(),
            recall_query: String::new(),
            pending_install: None,
            denizens: root.map_or_else(
                crate::denizen::Denizens::pending,
                crate::denizen::Denizens::new,
            ),
            resident_runs: crate::resident_runs::ResidentRuns::default(),
            resident_run_error: None,
            resident_run_effect_policy: crate::resident_runs::ExternalEffectPolicy::default(),
            resident_run_limits: servitor::RunLimits {
                decisions: crate::denizen::RUN_BUDGET as u64,
                tool_calls: 0,
                tokens: 0,
                elapsed_ms: 30_000,
                consecutive_failures: 1,
            },
            resident_run_storage_limits: crate::resident_runs::StorageLimits::default(),
            gemini_identities,
            identity,
            journal,
            behavior_binding: None,
            behavior_refusal: None,
            next_pane_id: 1,
            events: Vec::new(),
            knot_documents: Vec::new(),
        };
        if app.identity.is_none() {
            app.events.push(AppEvent::ProfileIdentityPending);
        }
        let mut effects = app.adopt_session(session_id);
        if !app.session_load_refused()
            && let Some(url) = address
        {
            app.apply_launch_address(url, &mut effects);
        } else if !app.session_load_refused()
            && minted
            && app.graph_runtimes.graph().nodes().count() == 0
        {
            // A bare FIRST launch: the sample graph, with the omnibar open by
            // itself so the app is discoverable without documentation. A bare
            // relaunch restores the canvas quietly (Ctrl+L / Ctrl+K summon).
            tracing::info!("no session graph; starting on the sample graph");
            app.graph_runtimes
                .replace_active_canvas(Canvas::with_sample_graph());
            if let Err(error) = app.bind_behavior_journal() {
                app.refuse_behavior(error);
            }
            app.omnibar.open = true;
            let context = app.fallback_shell_context();
            app.shell.begin_omnibar(context);
            app.focus = FocusTarget::Chrome;
            app.recompute_omnibar_suggestions();
        }
        (app, effects)
    }

    /// Layer a command-line address over restored state. Kept separate from
    /// credential and profile bootstrapping so its effect ordering is covered
    /// without touching a person's profile during tests.
    pub(in super::super) fn apply_launch_address(&mut self, url: &str, effects: &mut Vec<Effect>) {
        // The launch address is a pane-facing visit just like an omnibar
        // address. Saving it only on Canvas would be erased on the first pane
        // render when its local selection is restored.
        let pane = self.default_graph_pane();
        let key = self
            .with_graph_pane(pane, |canvas| canvas.visit(url))
            .expect("the default graph pane must resolve during boot");
        if crate::browse::is_fetchable(url)
            && let Some(node) = self.graph_runtimes.graph().get_node(key).map(|n| n.id)
        {
            // A restored content-on node already has a SpawnContent effect
            // from adopt_session. The fetch must precede that spawn so the
            // shell records one request before it decides whether a body is
            // pending. Appending it would start a request in SpawnContent,
            // then immediately supersede it with this launch request.
            let fetch = self.fetch_page_effect(node, url.to_string(), url.to_string());
            if let Some(index) = effects.iter().position(|effect| {
                matches!(effect, Effect::SpawnContent { node: actual, url: actual_url }
                    if *actual == node && actual_url == url)
            }) {
                effects.insert(index, fetch);
            } else {
                effects.push(fetch);
            }
        }
    }

    /// Mint a fresh session: a new manifest under `sessions/<id>/`, written
    /// through the store. Returns the id.
    pub(in super::super) fn mint_session(
        data_root: &std::path::Path,
        sessions: &mut pandect::ManifestStore,
    ) -> crate::panes::SessionId {
        let id = crate::panes::SessionId::new();
        // A REAL GraphId from birth: the root graph is the session's container
        // node (the one-node model), so its id keys the scene.* facets and is
        // the session's identity in the overmap. (Pre-overmap sessions minted
        // nil; `session::heal_nil_graph_ids` repairs those at boot.)
        let mut manifest = pandect::GraphSessionManifest::new(id, GraphId::new());
        manifest.storage_path = Some(session::session_dir(data_root, id));
        sessions.insert(manifest);
        if let Err(err) = sessions.flush_dirty() {
            tracing::warn!(%err, "failed to write the new session's manifest");
        }
        id
    }

    /// The live session's directory — where every save and load targets.
    pub fn session_dir(&self) -> PathBuf {
        session::session_dir(&self.data_root, self.session_id)
    }

    /// Move `closing`'s whole directory to the manifest trash and refresh the
    /// removed-sessions cache (overmap O3). The shell calls this AFTER
    /// releasing the bin store (open files block the rename on Windows) and
    /// BEFORE adopting the next session. Returns whether the trash move ran.
    pub fn apply_trash(&mut self, closing: crate::panes::SessionId) -> bool {
        match self.sessions.move_to_trash(closing) {
            Ok(true) => {
                self.trash = self.sessions.list_trash();
                self.events.push(AppEvent::SessionClosed);
                true
            },
            Ok(false) => {
                tracing::warn!(session = %closing.as_uuid(), "close: nothing to trash");
                false
            },
            Err(err) => {
                tracing::warn!(%err, "failed to trash the closed session");
                false
            },
        }
    }
}
