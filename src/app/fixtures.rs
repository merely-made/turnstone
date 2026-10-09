// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Headless App constructors for tests and the projection host: an app with
//! no ports and a scratch data root, so a test drives the spine without a
//! window or a profile.

use std::path::PathBuf;

use mere::canvas::Canvas;

use crate::content::ContentStates;
use crate::panes::{FrisketLayout, GraphId, SessionId};
use crate::surface::FocusTarget;
use crate::ui::OmnibarState;

use super::App;

impl App {
    fn isolated(data_root: PathBuf) -> Self {
        let identity = match crate::identity::load_root(&data_root, &data_root.join("personae-vault")) {
            crate::identity::RootLoad::Ready(root) => root,
            crate::identity::RootLoad::Locked => unreachable!("a fresh fixture vault is never locked"),
        };
        let root = identity::IdentityProvider::master_public_key(identity.as_ref()).to_bytes();
        let session_id = SessionId::new();
        let graph_id = GraphId::from_uuid(*session_id.as_uuid());
        let mut frisket = FrisketLayout::default();
        frisket.retag_graph_bound(graph_id);
        mere::kernel::graph::set_captured_delta_hook(None);
        let journal = crate::host_journal::shared_journal();
        let mut graph_runtimes = super::GraphRuntimePool::with_journal(graph_id, Some(session_id), Canvas::new(),
            journal.clone(),
        );
        graph_runtimes.adopt_graph(mere::kernel::graph::Graph::new(),
            crate::session_persistence::SessionPersistence::writable(
                crate::session::session_dir(&data_root, session_id), None,
            ),
        );
        let behavior_binding = graph_runtimes
            .get(graph_id)
            .and_then(|runtime| runtime.origin());
        Self {
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
            graph_runtimes,
            graph_views: super::GraphPaneViews::default(),
            forme_runtimes: super::FormeRuntimePool::default(),
            pane_context: crate::panes::ContextIndex::default(),
            omnibar: OmnibarState::default(),
            command_choices: cambium::CommandChoices::default(),
            document_find: crate::document_find::DocumentFindState::default(),
            user_agent_decision: crate::user_agent_decision::UserAgentDecisionState::default(),
            frame_timings: crate::frame_timing::FrameTimings::default(),
            diagnostic_inspection: None,
            shell: crate::shell_services::ShellServices::default(),
            data_root,
            sessions: pandect::ManifestStore::new(),
            session_id,
            content: ContentStates::default(),
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
            frisket,
            history: chrome::nav::History::new(""),
            active_pane: None,
            browser: pandect::browser_node_state::BrowserNodeStates::new(),
            physics_damping: pandect::DEFAULT_PHYSICS_DAMPING,
            physics_refusal: None,
            maximized: None,
            window_count: 1,
            viewport: crate::app::DEFAULT_VIEWPORT,
            lenses: Vec::new(),
            primary_blueprint: None,
            lens_blueprints: Vec::new(),
            roster_tab: 0,
            removed: Vec::new(),
            recall: Vec::new(),
            recall_query: String::new(),
            trash: Vec::new(),
            pending_install: None,
            denizens: crate::denizen::Denizens::new(root),
            resident_runs: crate::resident_runs::ResidentRuns::default(),
            resident_run_error: None,
            resident_run_effect_policy: crate::resident_runs::ExternalEffectPolicy::default(),
            resident_run_limits: servitor::RunLimits { decisions: crate::denizen::RUN_BUDGET as u64, tool_calls: 0, tokens: 0, elapsed_ms: 30_000, consecutive_failures: 1 },
            resident_run_storage_limits: crate::resident_runs::StorageLimits::default(),
            gemini_identities: crate::gemini_identity::GeminiIdentityBindings::default(),
            identity: Some(identity),
            journal,
            behavior_binding,
            behavior_refusal: None,
            next_pane_id: 1,
            events: Vec::new(),
            knot_documents: Vec::new(),
        }
    }

    /// Deterministic live graph truth for Graphshell's headed G3 receipt.
    pub(crate) fn projection_fixture() -> Self {
        use mere::kernel::geometry::PortablePoint;
        use mere::kernel::graph::apply::{add_node, assert_relation};
        use mere::kernel::graph::{EdgeAssertion, Graph, SemanticSubKind};

        let mut app = Self::isolated(std::env::temp_dir().join("turnstone-graphshell-g3"));
        let mut graph = Graph::new();
        let notes = add_node(
            &mut graph,
            Some(uuid::Uuid::from_u128(0x101)),
            "mere://field-notes".to_string(),
            PortablePoint::zero(),
        );
        let radios = add_node(
            &mut graph,
            Some(uuid::Uuid::from_u128(0x102)),
            "mere://radio-map".to_string(),
            PortablePoint::zero(),
        );
        let harmony = add_node(
            &mut graph,
            Some(uuid::Uuid::from_u128(0x103)),
            "mere://harmony-map".to_string(),
            PortablePoint::zero(),
        );
        let relation = || EdgeAssertion::Semantic {
            sub_kind: SemanticSubKind::Hyperlink,
            label: None,
            decay_progress: None,
        };
        let _ = assert_relation(&mut graph, notes, radios, relation());
        let _ = assert_relation(&mut graph, notes, harmony, relation());
        app.graph_runtimes.set_graph(graph);
        app.bind_behavior_journal().expect("fixture graph binding");
        let _ = app
            .graph_runtimes
            .set_node_title_for(uuid::Uuid::from_u128(0x101), "Field notes".into());
        let _ = app
            .graph_runtimes
            .set_node_title_for(uuid::Uuid::from_u128(0x102), "Radio map".into());
        let _ = app
            .graph_runtimes
            .set_node_title_for(uuid::Uuid::from_u128(0x103), "Harmony map".into());
        app
    }

    #[cfg(test)]
    pub(crate) fn test_stub() -> Self {
        Self::test_stub_at(std::env::temp_dir().join("turnstone-app-test"))
    }

    #[cfg(test)]
    pub(crate) fn test_stub_at(data_root: PathBuf) -> Self {
        let mut app = Self::isolated(data_root);
        app.engine_inventory = crate::shell::project_engine_inventory(
            &crate::shell::standard_content_engines(),
            &inker::SurfaceEngineRegistry::new(),
        );
        app
    }
}
