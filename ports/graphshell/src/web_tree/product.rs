// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The bounded local saved-graph workflow on the retained tree.
use super::*;
use cambium::{FileEvent, SelectState, TextInput};
use graphshell::local_edit::{
    NodeMetadata, metadata, save_metadata, sync_canvas_metadata_from_graph,
};
use graphshell::local_intake::{create_address, create_file, persist_intake, sync_canvas_intake};
use mere::canvas::Role;
use mere::kernel::graph::CouplingId;
use muniment::IndexedDbBackend;
use uuid::Uuid;
mod commands;
mod detail;
mod fields;
mod find;
mod history;
mod projection;
mod promoted_links;
#[cfg(feature = "product")]
mod query;
mod scenes;
pub(super) use commands::menu;
use fields::TextField;
pub(super) use fields::focused_text;
pub(super) use find::section as find_section;
pub(super) use history::section as history_section;
pub(super) use projection::{editor as projection_editor, section as projection_section};
pub(super) use scenes::section as scenes_section;

type App = GraphshellApp<IndexedDbBackend>;
#[derive(Clone, Copy)]
enum CompletedEdit {
    Metadata,
    Intake,
    Detail(Option<Uuid>),
}
type Completion = (App, CompletedEdit, Result<NodeMetadata, String>);
type OperationCompletion = (App, Result<(), String>);
type CouplingCompletion = (App, CouplingId, Result<Option<String>, String>);
type LinkVisibilityCompletion = (
    App,
    Uuid,
    std::collections::BTreeSet<scenograph::link::LinkKey>,
    bool,
    Result<(), String>,
);
type AssertionVisibilityCompletion = (
    App,
    Uuid,
    std::collections::BTreeSet<String>,
    Result<(), String>,
);

pub(super) enum CouplingOperation {
    Apply(mere::coupling_edit::StrengthDraft),
    History(CouplingId, bool),
}

pub(super) struct SavedProduct {
    app: Option<App>,
    completed: Rc<RefCell<Option<Completion>>>,
    coupling_completed: Rc<RefCell<Option<CouplingCompletion>>>,
    pub(super) coupling_result: Option<Result<Option<String>, String>>,
    pub(super) selected: Option<Uuid>,
    pub(super) title: TextInput,
    pub(super) tags: TextInput,
    pub(super) address: String,
    pub(super) detail_open: bool,
    /// The picked item's own role: index 0 is "as recipe" (no override),
    /// then each role (F48).
    pub(super) role: SelectState,
    pub(super) saving: bool,
    /// Save feedback for the detail editor; empty until a save.
    pub(super) status: String,
    /// The store's open and persistence state, shown in Graph tools.
    pub(super) storage: String,
    pub(super) save_state: &'static str,
    pub(super) session: String,
    /// Read-only count of accepted actual codicil owner replacements.
    pub(super) observed_session_changes: u64,
    pub(super) reopened: bool,
    pub(super) inspection_fixture: bool,
    link_rest_fixture: bool,
    link_family_assertion_fixture: bool,
    link_history_fixture: bool,
    intake_open: bool,
    intake_address: TextInput,
    intake_title: TextInput,
    file_requested: bool,
    /// A created but unacknowledged member. Retrying stores this same object.
    pending_intake: Option<Uuid>,
    /// Suggested links the person kept, written one at a time as their own.
    kept_links: std::collections::VecDeque<mere::canvas::link_suggestions::KeptLink>,
    pending_detail: Option<CompletedEdit>,
    detail: detail::DetailControls,
    find: find::FindControls,
    scenes: scenes::SceneControls,
    projection: projection::ProjectionControls,
    history: history::HistoryControls,
    commands: commands::Commands,
    operation_completed: Rc<RefCell<Option<OperationCompletion>>>,
    operation_pending: bool,
    link_visibility_completed: Rc<RefCell<Option<LinkVisibilityCompletion>>>,
    link_visibility_pending: bool,
    link_visibility_uncertain_session: Option<Uuid>,
    link_visibility_desired: Option<std::collections::BTreeSet<scenograph::link::LinkKey>>,
    link_visibility_save_state: &'static str,
    link_visibility_status: String,
    assertion_visibility_completed: Rc<RefCell<Option<AssertionVisibilityCompletion>>>,
    assertion_visibility_pending: bool,
    assertion_visibility_uncertain_session: Option<Uuid>,
    assertion_visibility_desired: Option<std::collections::BTreeSet<String>>,
    assertion_visibility_save_state: &'static str,
    assertion_visibility_status: String,
    pub(super) forme_reconcile_session: Option<Uuid>,
    pub(super) forme_new_session: Option<Uuid>,
    operation_success: Option<String>,
    clip_member: Rc<Cell<Option<Uuid>>>,
}

/// The existing fixture and generated comparison routes remain unchanged.
/// `?app=local` is the opt-in migration route onto the existing browser store.
#[inline(never)]
pub(super) fn open(
    root: &Element,
) -> std::pin::Pin<Box<impl std::future::Future<Output = Result<Option<SavedProduct>, String>> + '_>>
{
    Box::pin(open_inner(root))
}

async fn open_inner(root: &Element) -> Result<Option<SavedProduct>, String> {
    let search = super::super::window()?
        .location()
        .search()
        .map_err(|_| "cannot read page options")?;
    let params =
        web_sys::UrlSearchParams::new_with_str(&search).map_err(|_| "invalid page options")?;
    let requested = params.get("app").as_deref() == Some("local");
    let public_page = root.tag_name().eq_ignore_ascii_case("graphshell-view");
    if !requested && (!public_page || web_graphs::requested().is_some()) {
        return Ok(None);
    }
    if web_graphs::requested().is_some() {
        return Err("app=local cannot also select a generated graph".into());
    }
    let inspection_fixture = params.get("fixture").as_deref() == Some("field-inspection");
    let link_history_fixture = params.get("fixture").as_deref() == Some("link-history");
    let link_rest_fixture = params.get("fixture").as_deref() == Some("link-rest");
    let link_family_assertion_fixture =
        params.get("fixture").as_deref() == Some("family-assertions");
    let promoted_link_fixture = params.get("fixture").as_deref() == Some("promoted-link");
    let query_fixture = params.get("fixture").as_deref() == Some("retained-content-query");
    let retained_fixture =
        query_fixture || params.get("fixture").as_deref() == Some("retained-content");
    let database = if promoted_link_fixture {
        "graphshell-promoted-link-p7-v1"
    } else if link_family_assertion_fixture {
        "graphshell-link-family-assertions-p6-v1"
    } else if link_rest_fixture {
        "graphshell-link-rest-p4-v1"
    } else if query_fixture {
        "graphshell-retained-content-query-acceptance"
    } else if retained_fixture {
        "graphshell-retained-content-acceptance"
    } else if inspection_fixture {
        "graphshell-field-inspection-draft-v1"
    } else if link_history_fixture {
        "graphshell-link-history-p2-v1"
    } else {
        "graphshell-reference-host-h5"
    };
    let backend = IndexedDbBackend::open(database, "muniment")
        .await
        .map_err(|error| error.to_string())?;
    let capture_store = backend.clone();
    let persona = SelectedPersonaRef {
        persona: FIXTURE_PERSONA_ADDRESS.into(),
        profile: "profile:graphshell-h3".into(),
    };
    let mut app = if inspection_fixture {
        GraphshellApp::open_or_field_inspection_fixture(backend, persona).await
    } else {
        GraphshellApp::open_or_fixture(backend, persona).await
    }
    .map_err(|error| error.to_string())?;
    let reopened = app.host.was_reopened();
    if link_history_fixture && !reopened {
        app.host
            .seed_link_history_fixture()
            .map_err(|error| error.to_string())?;
    }
    if link_rest_fixture && !reopened {
        app.host
            .seed_link_rest_fixture()
            .map_err(|error| error.to_string())?;
    }
    if link_family_assertion_fixture && !reopened {
        app.host
            .seed_link_family_assertion_fixture()
            .map_err(|error| error.to_string())?;
    }
    if promoted_link_fixture && !reopened {
        app.host.seed_link_promotion_fixture().map_err(|error| error.to_string())?;
    }
    let history = history::HistoryControls::load(&mut app, capture_store).await?;
    app.host
        .persist(now_secs())
        .await
        .map_err(|error| error.to_string())?;
    if retained_fixture && !reopened {
        graphshell::retained_content::fixture::seed(&mut app, super::reading::clip_limits()?)
            .await?;
    }
    if query_fixture && !reopened {
        graphshell::query_views::seed_fixture(&mut app).await?;
    }
    app.mount_local().map_err(|error| error.to_string())?;
    let persistence = super::super::resolve_storage_persistence().await;
    let storage = graphshell::browser_storage::status_line(
        if reopened {
            "IndexedDB reopened"
        } else {
            "IndexedDB seeded"
        },
        &persistence,
    );
    let session = app.host.graph_session().id().0.to_string();
    let commands = commands::Commands::load(&app);
    Ok(Some(SavedProduct {
        app: Some(app),
        completed: Rc::new(RefCell::new(None)),
        coupling_completed: Rc::new(RefCell::new(None)),
        coupling_result: None,
        selected: None,
        title: TextInput::new(""),
        tags: TextInput::new(""),
        address: String::new(),
        detail_open: false,
        role: SelectState::new(0).with_label("Item role"),
        saving: false,
        status: String::new(),
        storage,
        save_state: "idle",
        session,
        observed_session_changes: 0,
        reopened,
        inspection_fixture,
        link_rest_fixture,
        link_family_assertion_fixture: link_family_assertion_fixture || promoted_link_fixture,
        link_history_fixture,
        intake_open: false,
        intake_address: TextInput::new(""),
        intake_title: TextInput::new(""),
        file_requested: false,
        pending_intake: None,
        kept_links: Default::default(),
        pending_detail: None,
        detail: detail::DetailControls::default(),
        find: find::FindControls::default(),
        scenes: scenes::SceneControls::default(),
        projection: projection::ProjectionControls::default(),
        history,
        commands,
        operation_completed: Rc::new(RefCell::new(None)),
        operation_pending: false,
        link_visibility_completed: Rc::new(RefCell::new(None)),
        link_visibility_pending: false,
        link_visibility_uncertain_session: None,
        link_visibility_desired: None,
        link_visibility_save_state: "idle",
        link_visibility_status: String::new(),
        assertion_visibility_completed: Rc::new(RefCell::new(None)),
        assertion_visibility_pending: false,
        assertion_visibility_uncertain_session: None,
        assertion_visibility_desired: None,
        assertion_visibility_save_state: "idle",
        assertion_visibility_status: String::new(),
        forme_reconcile_session: None,
        forme_new_session: None,
        operation_success: None,
        clip_member: Rc::new(Cell::new(None)),
    }))
}

fn now_secs() -> u64 {
    (js_sys::Date::now() / 1_000.0) as u64
}

fn link_key_is_present(
    graph: &Graph,
    pair: (Uuid, Uuid),
    expected: &scenograph::link::LinkKey,
) -> bool {
    let source = scenograph::link::LinkSource::Unavailable {
        reason: "Graphshell is validating a local view target".into(),
    };
    mere::canvas::link_reading::read_link(graph, pair.0, pair.1, source)
        .is_ok_and(|reading| reading.links.iter().any(|link| &link.key == expected))
}

impl SavedProduct {
    pub(super) fn actual_session(&self) -> Option<Uuid> {
        self.app.as_ref().map(|app| app.host.graph_session().id().0)
    }
    pub(super) fn actual_graph_nodes(&self) -> Option<usize> {
        self.app
            .as_ref()
            .map(|app| app.host.graph().nodes().count())
    }
    pub(super) fn forme_source(&self) -> Option<(IndexedDbBackend, Uuid)> {
        let app = self.app.as_ref()?;
        Some((app.host.store(), app.host.graph_session().id().0))
    }
    pub(super) fn ready(&self) -> bool {
        self.forme_reconcile_session.is_some()
            || self.forme_new_session.is_some()
            || self.completed.borrow().is_some()
            || self.coupling_completed.borrow().is_some()
            || self.operation_completed.borrow().is_some()
            || self.link_visibility_completed.borrow().is_some()
            || self.assertion_visibility_completed.borrow().is_some()
            || self.projection.changed()
            || self.projection.needs_frame()
    }
    pub(super) fn selection_locked(&self) -> bool {
        self.saving
            || self.operation_pending
            || self.link_visibility_pending
            || self.link_visibility_uncertain_session.is_some()
            || self.assertion_visibility_pending
            || self.assertion_visibility_uncertain_session.is_some()
            || self.file_requested
            || self.pending_intake.is_some()
            || self.pending_detail.is_some()
    }
    pub(super) fn authority_nodes(&self) -> Option<usize> {
        self.app.as_ref().map(|app| app.host.graph().node_count())
    }
    pub(super) fn initial_fixture_positions(
        &self,
    ) -> Option<
        Vec<(
            mere::kernel::graph::NodeKey,
            mere::kernel::geometry::PortablePoint,
        )>,
    > {
        (self.link_rest_fixture || self.link_family_assertion_fixture)
            .then(|| {
                self.app
                    .as_ref()
                    .map(|app| app.host.link_rest_fixture_positions())
            })
            .flatten()
    }

    pub(super) fn graph(&self) -> Graph {
        self.app
            .as_ref()
            .expect("app present before a save")
            .host
            .graph()
            .clone()
    }

    /// Inspect the current source owner, never a cached Canvas or form draft.
    pub(super) fn inspect_access(
        &self,
        member: Uuid,
    ) -> Result<graphshell::access_inspection::AccessInspection, String> {
        let app = self
            .app
            .as_ref()
            .ok_or("Source inspection will resume when the current save finishes")?;
        graphshell::access_inspection::inspect_access(app.host.graph(), member)
    }

    pub(super) fn linked_accesses(
        &self,
        member: Uuid,
    ) -> Result<Vec<graphshell::access_inspection::AccessSummary>, String> {
        let app = self
            .app
            .as_ref()
            .ok_or("Source inspection will resume when the current save finishes")?;
        graphshell::link_inspection::linked_accesses(app.host.graph(), member)
    }

    pub(super) fn inspect_link(
        &self,
        target: graphshell::link_inspection::LinkTarget,
    ) -> Result<graphshell::link_inspection::LinkInspection, String> {
        let app = self
            .app
            .as_ref()
            .ok_or("Source inspection will resume when the current save finishes")?;
        graphshell::link_inspection::inspect_link_with_source(
            app.host.graph(),
            target,
            app.host.link_source(),
        )
    }

    pub(super) fn opened_link(
        &self,
        target: graphshell::link_inspection::LinkTarget,
        history: bool,
    ) -> Result<scenograph::link::LinkPairReading, String> {
        let app = self
            .app
            .as_ref()
            .ok_or("Link reading will resume when the current save finishes")?;
        let read = if history {
            mere::canvas::link_reading::read_link_history
        } else {
            mere::canvas::link_reading::read_link
        };
        read(
            app.host.graph(),
            target.first,
            target.second,
            app.host.link_source(),
        )
    }

    pub(super) fn opened_link_time_ms(&self) -> u64 {
        // The isolated scenario's source clocks are 1000..3100. Its view
        // clock is explicit too; it is never written back as assertion time.
        if self.link_history_fixture {
            4000
        } else {
            js_sys::Date::now() as u64
        }
    }

    pub(super) fn inspect_link_history(
        &self,
        target: graphshell::link_inspection::LinkTarget,
    ) -> Result<graphshell::link_inspection::LinkHistoryInspection, String> {
        let app = self
            .app
            .as_ref()
            .ok_or("Link history inspection requires the local graph")?;
        graphshell::link_inspection::inspect_link_history_with_source(
            app.host.graph(),
            target,
            app.host.link_source(),
        )
    }

    pub(super) fn link_source(&self) -> Result<scenograph::link::LinkSource, String> {
        self.app
            .as_ref()
            .map(|app| app.host.link_source())
            .ok_or_else(|| "Source inspection will resume when the current save finishes".into())
    }

    pub(super) fn inspect_statement_at_owner(
        &self,
        statement_id: &str,
        stratum: mere::kernel::graph::GraphStratum,
        from: Uuid,
        to: Uuid,
    ) -> Result<graphshell::link_inspection::StatementInspection, String> {
        let app = self
            .app
            .as_ref()
            .ok_or("Assertion inspection requires the local graph")?;
        graphshell::link_inspection::inspect_statement_at_owner(
            app.host.graph(),
            statement_id,
            stratum,
            from,
            to,
        )
    }

    pub(super) fn inspect_statement(
        &self,
        statement: &str,
    ) -> Result<graphshell::link_inspection::StatementInspection, String> {
        let app = self
            .app
            .as_ref()
            .ok_or("Source inspection will resume when the current save finishes")?;
        graphshell::link_inspection::inspect_statement(app.host.graph(), statement)
    }

    pub(super) fn source_graph(&self) -> Result<&Graph, String> {
        self.app
            .as_ref()
            .map(|app| app.host.graph())
            .ok_or_else(|| "Source inspection will resume when the current save finishes".into())
    }

    /// Product-visible save state for whole-link view intent. `uncertain`
    /// means the backend did not confirm whether the last write committed;
    /// actions remain locked until this product is reopened.
    pub(super) fn link_visibility_state(&self) -> (&'static str, &str) {
        (
            self.link_visibility_save_state,
            self.link_visibility_status.as_str(),
        )
    }

    pub(super) fn assertion_visibility_state(&self) -> (&'static str, &str) {
        (
            self.assertion_visibility_save_state,
            self.assertion_visibility_status.as_str(),
        )
    }

    /// Restore canonical whole-link hides from this session's acknowledged
    /// Graphshell canvas view. Call after mounting the current product graph
    /// and again after an acknowledged owner/session switch.
    pub(super) fn restore_canvas_link_hides(&mut self, canvas: &mut Canvas) {
        if self.link_visibility_pending
            || self.link_visibility_completed.borrow().is_some()
            || self.assertion_visibility_pending
            || self.assertion_visibility_completed.borrow().is_some()
        {
            self.link_visibility_save_state = "pending";
            self.link_visibility_status = "Waiting for the link visibility save to finish".into();
            return;
        }
        let Some(app) = self.app.as_ref() else {
            self.link_visibility_save_state = "error";
            self.link_visibility_status = "The local graph is unavailable".into();
            return;
        };
        let owner = app.host.graph_session().id().0;
        if let Some(uncertain) = self.link_visibility_uncertain_session {
            if uncertain == owner {
                self.link_visibility_save_state = "uncertain";
                self.link_visibility_status =
                    "The previous save is unconfirmed; reopen this session before changing link visibility".into();
                return;
            }
            self.link_visibility_uncertain_session = None;
            self.link_visibility_desired = None;
        }
        if let Some(uncertain) = self.assertion_visibility_uncertain_session {
            if uncertain == owner {
                self.assertion_visibility_save_state = "uncertain";
                self.assertion_visibility_status =
                    "The previous assertion hide save is unconfirmed; reopen this session before changing it".into();
                return;
            }
            self.assertion_visibility_uncertain_session = None;
            self.assertion_visibility_desired = None;
        }
        if self.session.parse().ok() != Some(owner) {
            self.link_visibility_save_state = "error";
            self.link_visibility_status =
                "The product session has not reconciled to the current owner".into();
            return;
        }
        if canvas.graph().revision() != app.host.graph().revision() {
            self.link_visibility_save_state = "error";
            self.link_visibility_status =
                "The canvas graph has not reconciled to the current owner".into();
            return;
        }
        let hidden = app
            .host
            .view(graphshell::mere_host::GRAPHSHELL, "canvas")
            .map(|view| view.hidden_links.clone())
            .unwrap_or_else(std::collections::BTreeSet::new);
        let hidden_assertions = app
            .host
            .view(graphshell::mere_host::GRAPHSHELL, "canvas")
            .map(|view| view.hidden_assertions.clone())
            .unwrap_or_else(std::collections::BTreeSet::new);
        let count = hidden.len();
        let assertion_count = hidden_assertions.len();
        canvas.set_hidden_link_keys(hidden);
        canvas.set_hidden_assertion_ids(hidden_assertions);
        self.link_visibility_save_state = "saved";
        self.link_visibility_status = if count == 0 {
            "No saved whole-link hides".into()
        } else {
            format!("Restored {count} saved whole-link hide(s)")
        };
        self.assertion_visibility_save_state = "saved";
        self.assertion_visibility_status = if assertion_count == 0 {
            "No saved exact assertion hides".into()
        } else {
            format!("Restored {assertion_count} saved exact assertion hide(s)")
        };
    }

    /// Persist selected canonical link hides. Canvas visibility is published
    /// only after `MereHost::set_view` confirms its IndexedDB write.
    pub(super) fn hide_selected_links(&mut self, canvas: &Canvas) -> Result<(), String> {
        if self.selection_locked() {
            return Err("Another save is pending or its outcome is unconfirmed".into());
        }
        let selected = canvas.selected_link_keys();
        if selected.is_empty() {
            return self.link_visibility_refusal("Select a link before hiding it".into());
        }
        let (owner, mut intent) = match self.link_visibility_intent_for_canvas(canvas) {
            Ok(state) => state,
            Err(error) => return self.link_visibility_refusal(error),
        };
        for key in selected {
            let Some(pair) = canvas.link_appearance_pair(&key) else {
                return self.link_visibility_refusal(format!(
                    "The selected link {key:?} has no current appearance"
                ));
            };
            let owner_has_key = match self.source_graph() {
                Ok(graph) => link_key_is_present(graph, pair, &key),
                Err(error) => return self.link_visibility_refusal(error),
            };
            if !link_key_is_present(canvas.graph(), pair, &key) || !owner_has_key {
                return self.link_visibility_refusal(format!(
                    "The selected link {key:?} does not belong to the current product graph"
                ));
            }
            intent.hidden_links.insert(key);
        }
        self.persist_link_visibility(owner, intent.hidden_links.clone(), true, intent)
    }

    /// Clear only canonical whole-link hides. Legacy exact-cell filters and
    /// every other field in the saved canvas view are preserved.
    pub(super) fn show_hidden_links(&mut self, canvas: &Canvas) -> Result<(), String> {
        if self.selection_locked() {
            return Err("Another save is pending or its outcome is unconfirmed".into());
        }
        let (owner, mut intent) = match self.link_visibility_intent_for_canvas(canvas) {
            Ok(state) => state,
            Err(error) => return self.link_visibility_refusal(error),
        };
        if intent.hidden_links.is_empty() {
            self.link_visibility_save_state = "saved";
            self.link_visibility_status = "No whole-link hides to show".into();
            return Ok(());
        }
        intent.hidden_links.clear();
        self.persist_link_visibility(owner, std::collections::BTreeSet::new(), false, intent)
    }

    /// Persist one exact source assertion hide. The stable ID is checked in
    /// both the mounted canvas graph and the acknowledged owner graph before
    /// changing only the view-intent set.
    pub(super) fn hide_assertion(
        &mut self,
        canvas: &Canvas,
        assertion_id: &str,
    ) -> Result<(), String> {
        if self.selection_locked() {
            return Err("Another save is pending or its outcome is unconfirmed".into());
        }
        let (owner, mut intent) = match self.link_visibility_intent_for_canvas(canvas) {
            Ok(state) => state,
            Err(error) => return self.assertion_visibility_refusal(error),
        };
        if canvas.graph().find_held_assertion(assertion_id).is_none() {
            return self.assertion_visibility_refusal(format!(
                "Assertion {assertion_id} is no longer held in the mounted graph"
            ));
        }
        let owner_has_assertion = match self.source_graph() {
            Ok(graph) => graph.find_held_assertion(assertion_id).is_some(),
            Err(error) => return self.assertion_visibility_refusal(error),
        };
        if !owner_has_assertion {
            return self.assertion_visibility_refusal(format!(
                "Assertion {assertion_id} does not belong to the current product graph"
            ));
        }
        intent.hidden_assertions.insert(assertion_id.to_owned());
        self.persist_assertion_visibility(owner, intent.hidden_assertions.clone(), intent)
    }

    /// Clear exact assertion hides while preserving whole-link and legacy
    /// selector visibility state.
    pub(super) fn show_hidden_assertions(&mut self, canvas: &Canvas) -> Result<(), String> {
        if self.selection_locked() {
            return Err("Another save is pending or its outcome is unconfirmed".into());
        }
        let (owner, mut intent) = match self.link_visibility_intent_for_canvas(canvas) {
            Ok(state) => state,
            Err(error) => return self.assertion_visibility_refusal(error),
        };
        if intent.hidden_assertions.is_empty() {
            self.assertion_visibility_save_state = "saved";
            self.assertion_visibility_status = "No exact assertion hides to show".into();
            return Ok(());
        }
        intent.hidden_assertions.clear();
        self.persist_assertion_visibility(owner, std::collections::BTreeSet::new(), intent)
    }

    fn assertion_visibility_refusal(&mut self, error: String) -> Result<(), String> {
        self.assertion_visibility_save_state = "error";
        self.assertion_visibility_status = error.clone();
        Err(error)
    }

    fn persist_assertion_visibility(
        &mut self,
        owner: Uuid,
        desired: std::collections::BTreeSet<String>,
        intent: graphshell::mere_host::SessionViewIntent,
    ) -> Result<(), String> {
        let Some(mut app) = self.app.take() else {
            return Err("The local graph is unavailable".into());
        };
        self.saving = true;
        self.assertion_visibility_pending = true;
        self.assertion_visibility_desired = Some(desired.clone());
        self.assertion_visibility_save_state = "pending";
        self.assertion_visibility_status = if desired.is_empty() {
            "Saving exact assertion visibility…".into()
        } else {
            format!("Saving {} exact assertion hide(s)…", desired.len())
        };
        let completed = self.assertion_visibility_completed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = app
                .host
                .set_view(graphshell::mere_host::GRAPHSHELL, "canvas", intent)
                .await
                .map_err(|error| error.to_string());
            *completed.borrow_mut() = Some((app, owner, desired, result));
        });
        Ok(())
    }

    fn link_visibility_refusal(&mut self, error: String) -> Result<(), String> {
        self.link_visibility_save_state = "error";
        self.link_visibility_status = error.clone();
        Err(error)
    }

    fn link_visibility_intent_for_canvas(
        &self,
        canvas: &Canvas,
    ) -> Result<(Uuid, graphshell::mere_host::SessionViewIntent), String> {
        let app = self.app.as_ref().ok_or("The local graph is unavailable")?;
        let owner = app.host.graph_session().id().0;
        if self.session.parse().ok() != Some(owner) {
            return Err("The product is not reconciled to its current graph session".into());
        }
        if canvas.graph().revision() != app.host.graph().revision() {
            return Err("The canvas is stale relative to the current product graph".into());
        }
        let intent = app
            .host
            .view(graphshell::mere_host::GRAPHSHELL, "canvas")
            .cloned()
            .unwrap_or_default();
        let canvas_hidden: std::collections::BTreeSet<_> =
            canvas.hidden_link_keys().into_iter().collect();
        if canvas_hidden != intent.hidden_links {
            return Err(
                "Restore the current session's saved link visibility before changing it".into(),
            );
        }
        let canvas_hidden_assertions: std::collections::BTreeSet<_> =
            canvas.hidden_assertion_ids().into_iter().collect();
        if canvas_hidden_assertions != intent.hidden_assertions {
            return Err(
                "Restore the current session's saved assertion visibility before changing it"
                    .into(),
            );
        }
        Ok((owner, intent))
    }

    fn persist_link_visibility(
        &mut self,
        owner: Uuid,
        desired: std::collections::BTreeSet<scenograph::link::LinkKey>,
        clear_selected_links: bool,
        intent: graphshell::mere_host::SessionViewIntent,
    ) -> Result<(), String> {
        let Some(mut app) = self.app.take() else {
            return Err("The local graph is unavailable".into());
        };
        self.saving = true;
        self.link_visibility_pending = true;
        self.link_visibility_desired = Some(desired.clone());
        self.link_visibility_save_state = "pending";
        self.link_visibility_status = if desired.is_empty() {
            "Saving whole-link visibility…".into()
        } else {
            format!("Saving {} whole-link hide(s)…", desired.len())
        };
        let completed = self.link_visibility_completed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = app
                .host
                .set_view(graphshell::mere_host::GRAPHSHELL, "canvas", intent)
                .await
                .map_err(|error| error.to_string());
            *completed.borrow_mut() = Some((app, owner, desired, clear_selected_links, result));
        });
        Ok(())
    }

    pub(super) fn source_session_id(&self) -> Result<Uuid, String> {
        self.app
            .as_ref()
            .map(|app| app.host.graph_session().id().0)
            .ok_or_else(|| "Source inspection will resume when the current save finishes".into())
    }

    /// Selected file facts from the acknowledged local owner, for receipts.
    pub(super) fn saved_file(&self) -> Option<serde_json::Value> {
        if self.saving || self.pending_intake.is_some() {
            return None;
        }
        let app = self.app.as_ref()?;
        let (_, node) = app.host.graph().get_node_by_id(self.selected?)?;
        let content = app
            .host
            .facet_value(node.url(), graphshell::product::CONTENT_FACET)?;
        let local = app
            .host
            .facet_value(node.url(), graphshell::product::LOCAL_FILE_FACET)?;
        Some(serde_json::json!({ "content": content, "local": local }))
    }

    pub(super) fn select(&mut self, selected: Option<Uuid>) {
        if self.selection_locked() || self.selected == selected {
            return;
        }
        self.selected = selected;
        self.detail_open = false;
        self.save_state = "idle";
        let data = selected
            .and_then(|member| self.app.as_ref().and_then(|app| metadata(app, member).ok()));
        if let Some(data) = data {
            self.title = TextInput::new(data.title);
            self.tags = TextInput::new(data.tags.join(", "));
            self.address = data.address;
        } else {
            self.selected = None;
            self.title = TextInput::new("");
            self.tags = TextInput::new("");
            self.address.clear();
        }
    }

    /// Open the detail editor, its item role read from the canvas.
    pub(super) fn open_detail(&mut self, canvas: &Canvas) {
        // Keep the open_file view's registered target alive until the host
        // finishes reading the chosen bytes and dispatches its answer.
        if self.selection_locked() {
            return;
        }
        self.intake_open = false;
        self.detail_open = true;
        self.role.selected =
            item_role_index(self.selected.and_then(|member| canvas.member_role(member)));
        self.detail.face.selected = detail::face_index(canvas, self.selected);
    }

    pub(super) fn save(&mut self) {
        if self.selection_locked() {
            return;
        }
        if self.pending_intake.is_some() {
            self.status = "Retry intake before saving other changes".into();
            return;
        }
        let Some(member) = self.selected else {
            self.status = "Select an object before saving".into();
            return;
        };
        let Some(mut app) = self.app.take() else {
            return;
        };
        let title = self.title.text().to_string();
        let tags = self.tags.text().to_string();
        self.saving = true;
        self.save_state = "saving";
        self.status = "Saving changes…".into();
        let completed = self.completed.clone();
        // The task owns the app. No RefCell borrow crosses the IndexedDB await.
        wasm_bindgen_futures::spawn_local(async move {
            let result = save_metadata(&mut app, member, &title, &tags, now_secs()).await;
            *completed.borrow_mut() = Some((app, CompletedEdit::Metadata, result));
        });
    }

    pub(super) fn save_coupling(&mut self, operation: CouplingOperation) -> Result<(), String> {
        if self.selection_locked() {
            return Err("Another graph save or intake is pending".into());
        }
        let Some(mut app) = self.app.take() else {
            return Err("Local source is unavailable".into());
        };
        self.saving = true;
        self.save_state = "saving";
        let completed = self.coupling_completed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let (id, result) = match operation {
                CouplingOperation::Apply(draft) => {
                    let id = draft.baseline.id;
                    (
                        id,
                        app.host
                            .save_coupling_strength(&draft, now_secs())
                            .await
                            .map(|_| ()),
                    )
                },
                CouplingOperation::History(id, redo) => {
                    let result = app
                        .host
                        .coupling_history(id, redo, now_secs())
                        .await
                        .and_then(|changed| {
                            if changed {
                                Ok(())
                            } else {
                                Err("No saved coupling step is available".into())
                            }
                        });
                    (id, result)
                },
            };
            // Keep the portable local client current as well as the Canvas.
            // A refresh error after acknowledgement must not be called a
            // failed storage write; failed writes still display the old graph.
            let warning = app
                .mount_local()
                .err()
                .map(|error| format!("Local projection refresh needs retry: {error}"));
            *completed.borrow_mut() = Some((app, id, result.map(|_| warning)));
        });
        Ok(())
    }

    fn add_address(&mut self) {
        if self.selection_locked() {
            return;
        }
        let Some(app) = self.app.as_mut() else {
            return;
        };
        match create_address(app, self.intake_address.text(), self.intake_title.text()) {
            Ok(member) => self.store_intake(member),
            Err(error) => {
                self.status = error;
                self.save_state = "error";
            },
        }
    }

    pub(super) fn retained_store(&self) -> Option<IndexedDbBackend> {
        self.app.as_ref().map(|app| app.host.store())
    }
    pub(super) fn retained_scope(&self) -> Option<(Uuid, String)> {
        let app = self.app.as_ref()?;
        Some((
            app.host.graph_session().id().0,
            app.host.selected_persona().persona.clone(),
        ))
    }
    fn store_clip_file(&mut self, bytes: Vec<u8>) {
        let limits = match super::reading::clip_limits() {
            Ok(limits) => limits,
            Err(error) => {
                self.status = error;
                return;
            },
        };
        let Some(mut app) = self.app.take() else {
            return;
        };
        self.saving = true;
        self.save_state = "saving";
        self.status = "Importing selected retained clip".into();
        let completed = self.completed.clone();
        let clip_member = self.clip_member.clone();
        let context = fleece::portable_clip::CaptureContext {
            source: eidetic::clip_profile::ClipSourceV1 {
                owner: Some(app.host.selected_persona().persona.clone()),
                entity: "selected-package-import".into(),
                revision: None,
            },
            capture_id: Uuid::new_v4().to_string(),
            provenance: eidetic::ProvenanceRecord::self_imported("explicit selected clip package"),
            trust: eidetic::TrustEnvelope::self_asserted(),
            captured_at: eidetic::Timestamp(now_secs() * 1000),
        };
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let member =
                    graphshell::retained_content::import_clip(&mut app, &bytes, limits, &context)
                        .await?;
                clip_member.set(Some(member));
                persist_intake(&mut app, member, now_secs()).await
            }
            .await;
            *completed.borrow_mut() = Some((app, CompletedEdit::Intake, result));
        });
    }
    fn receive_file(&mut self, event: FileEvent) {
        self.file_requested = false;
        if self.saving || self.pending_intake.is_some() || self.pending_detail.is_some() {
            return;
        }
        let Some(file) = event.files.into_iter().next() else {
            self.status = "File choice canceled".into();
            return;
        };
        if file.name.to_ascii_lowercase().ends_with(".mereclip") {
            self.store_clip_file(file.bytes);
            return;
        }
        let Some(app) = self.app.as_mut() else {
            return;
        };
        match create_file(
            app,
            &file.name,
            file.media_type.as_deref(),
            file.last_modified_ms,
            &file.bytes,
        ) {
            Ok(member) => self.store_intake(member),
            Err(error) => {
                self.status = format!("File intake refused · {error}");
                self.save_state = "error";
            },
        }
    }

    /// Write a kept suggestion as the person's own object, through the same
    /// intake as an added address; the acknowledged graph then replaces the
    /// canvas's derived member.
    fn keep_links(&mut self, canvas: &mut Canvas) {
        self.kept_links.extend(canvas.take_kept_links());
        if self.saving
            || self.pending_intake.is_some()
            || self.pending_detail.is_some()
            || self.selection_locked()
        {
            return;
        }
        let Some(kept) = self.kept_links.pop_front() else {
            return;
        };
        let Some(app) = self.app.as_mut() else {
            return;
        };
        match create_address(app, &kept.url, &kept.url) {
            Ok(member) => self.store_intake(member),
            Err(error) => {
                self.status = error;
                self.save_state = "error";
            },
        }
    }

    fn store_intake(&mut self, member: Uuid) {
        if self.saving {
            return;
        }
        let Some(mut app) = self.app.take() else {
            return;
        };
        self.pending_intake = Some(member);
        self.saving = true;
        self.save_state = "saving";
        self.status = "Saving new object…".into();
        let completed = self.completed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = persist_intake(&mut app, member, now_secs()).await;
            *completed.borrow_mut() = Some((app, CompletedEdit::Intake, result));
        });
    }

    /// Persist the current typed owner transaction without holding a borrow across await.
    fn persist_operation(&mut self) {
        if self.saving {
            return;
        }
        let Some(mut app) = self.app.take() else {
            return;
        };
        self.operation_pending = true;
        self.status = "Saving changes…".into();
        self.saving = true;
        self.save_state = "saving";
        let completed = self.operation_completed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = app
                .host
                .persist(now_secs())
                .await
                .map_err(|error| error.to_string());
            *completed.borrow_mut() = Some((app, result));
        });
    }

    pub(super) fn poll(&mut self, canvas: &mut Canvas) -> bool {
        self.history.poll();
        self.keep_links(canvas);
        let projection_advanced = self.projection.advance();
        let projection_changed = self.projection.changed();
        if projection_changed {
            self.projection.apply_fields();
        }
        let coupling = self.coupling_completed.borrow_mut().take();
        if let Some((app, id, result)) = coupling {
            let owner = app.host.graph_session().id().0;
            let previous_owner = self.session.parse().ok();
            let owner_changed = previous_owner != Some(owner);
            // Reconciliation installs the complete returned owner graph. Capture
            // an acknowledged coupling change first: its new force inputs must
            // restart settling, unlike an ordinary metadata-only completion.
            let coupling_changed = result.is_ok()
                && previous_owner == Some(owner)
                && app
                    .host
                    .graph()
                    .couplings()
                    .find(|coupling| coupling.id == id)
                    .is_some_and(|coupling| {
                        !canvas.graph().couplings().any(|held| held == coupling)
                    });
            graphshell::local_scene::reconcile_completed_graph(
                canvas,
                previous_owner,
                owner,
                app.host.graph(),
            );
            if coupling_changed {
                // The graph is already coherent; retain the coupling editor's
                // deliberate structural-refresh/settle boundary for changed
                // force inputs without restarting failed writes or new owners.
                canvas.ingest_graph(|graph| {
                    *graph = app.host.graph().clone();
                    true
                });
            }
            self.session = owner.to_string();
            self.forme_reconcile_session = Some(owner);
            self.app = Some(app);
            if owner_changed {
                self.restore_canvas_link_hides(canvas);
            }
            self.saving = false;
            self.save_state = if result.is_ok() { "saved" } else { "error" };
            self.coupling_result = Some(result);
            return true;
        }

        let link_visibility = self.link_visibility_completed.borrow_mut().take();
        if let Some((app, requested_owner, desired, clear_selected_links, result)) = link_visibility
        {
            let owner = app.host.graph_session().id().0;
            let owner_matches =
                owner == requested_owner && self.session.parse().ok() == Some(owner);
            let persisted = app
                .host
                .view(graphshell::mere_host::GRAPHSHELL, "canvas")
                .map(|view| view.hidden_links.clone())
                .unwrap_or_default();
            let confirmed = result.is_ok() && owner_matches && persisted == desired;
            self.session = owner.to_string();
            self.forme_reconcile_session = Some(owner);
            self.app = Some(app);
            self.saving = false;
            self.link_visibility_pending = false;
            if confirmed {
                canvas.set_hidden_link_keys(desired.iter().cloned());
                if clear_selected_links
                    && canvas
                        .selected_link_keys()
                        .iter()
                        .all(|key| desired.contains(key))
                {
                    canvas.hide_selected_links();
                }
                self.link_visibility_desired = None;
                self.link_visibility_uncertain_session = None;
                self.link_visibility_save_state = "saved";
                self.link_visibility_status = if desired.is_empty() {
                    "All whole-link hides are cleared".into()
                } else {
                    format!("Saved {} whole-link hide(s)", desired.len())
                };
            } else {
                let cause = match result {
                    Err(error) => format!("{error}; the backend did not confirm the write"),
                    Ok(()) if !owner_matches => {
                        "the owning session changed before the saved view could be applied".into()
                    },
                    Ok(()) => "the acknowledged view did not contain the requested hide set".into(),
                };
                self.link_visibility_desired = Some(desired);
                self.link_visibility_uncertain_session = Some(owner);
                self.link_visibility_save_state = "uncertain";
                self.link_visibility_status = format!(
                    "Link visibility save is unconfirmed ({cause}); reopen this session before retrying"
                );
            }
            return true;
        }

        let assertion_visibility = self.assertion_visibility_completed.borrow_mut().take();
        if let Some((app, requested_owner, desired, result)) = assertion_visibility {
            let owner = app.host.graph_session().id().0;
            let owner_matches =
                owner == requested_owner && self.session.parse().ok() == Some(owner);
            let persisted = app
                .host
                .view(graphshell::mere_host::GRAPHSHELL, "canvas")
                .map(|view| view.hidden_assertions.clone())
                .unwrap_or_default();
            let confirmed = result.is_ok() && owner_matches && persisted == desired;
            self.session = owner.to_string();
            self.forme_reconcile_session = Some(owner);
            self.app = Some(app);
            self.saving = false;
            self.assertion_visibility_pending = false;
            if confirmed {
                canvas.set_hidden_assertion_ids(desired.iter().cloned());
                self.assertion_visibility_desired = None;
                self.assertion_visibility_uncertain_session = None;
                self.assertion_visibility_save_state = "saved";
                self.assertion_visibility_status = if desired.is_empty() {
                    "All exact assertion hides are cleared".into()
                } else {
                    format!("Saved {} exact assertion hide(s)", desired.len())
                };
            } else {
                let cause = match result {
                    Err(error) => format!("{error}; the backend did not confirm the write"),
                    Ok(()) if !owner_matches => {
                        "the owning session changed before the saved view could be applied".into()
                    },
                    Ok(()) => {
                        "the acknowledged view did not contain the requested assertion hide set"
                            .into()
                    },
                };
                self.assertion_visibility_desired = Some(desired);
                self.assertion_visibility_uncertain_session = Some(owner);
                self.assertion_visibility_save_state = "uncertain";
                self.assertion_visibility_status = format!(
                    "Assertion visibility save is unconfirmed ({cause}); reopen this session before retrying"
                );
            }
            return true;
        }

        let operation = self.operation_completed.borrow_mut().take();
        if let Some((app, result)) = operation {
            let owner = app.host.graph_session().id().0;
            let owner_changed = self.session.parse().ok() != Some(owner);
            graphshell::local_scene::reconcile_completed_graph(
                canvas,
                self.session.parse().ok(),
                owner,
                app.host.graph(),
            );
            self.session = owner.to_string();
            self.forme_reconcile_session = Some(owner);
            self.app = Some(app);
            if owner_changed {
                self.restore_canvas_link_hides(canvas);
            }
            self.saving = false;
            match result {
                Ok(()) => {
                    self.operation_pending = false;
                    self.projection.saved();
                    self.save_state = "saved";
                    self.status = self
                        .operation_success
                        .take()
                        .unwrap_or_else(|| "Changes saved".into());
                },
                Err(error) => {
                    self.save_state = "error";
                    self.status =
                        format!("Save failed: {error}. Retry to persist the pending changes");
                },
            }
            return true;
        }

        let Some((app, edit, result)) = self.completed.borrow_mut().take() else {
            return projection_changed || projection_advanced;
        };
        let owner = app.host.graph_session().id().0;
        let owner_changed = self.session.parse().ok() != Some(owner);
        graphshell::local_scene::reconcile_completed_graph(
            canvas,
            self.session.parse().ok(),
            owner,
            app.host.graph(),
        );
        self.session = owner.to_string();
        self.forme_reconcile_session = Some(owner);
        if matches!(edit, CompletedEdit::Intake) && self.pending_intake.is_none() {
            self.pending_intake = self.clip_member.take();
        }
        self.app = Some(app);
        if owner_changed {
            self.restore_canvas_link_hides(canvas);
        }
        self.saving = false;
        match result {
            Ok(saved) => {
                let refresh = match edit {
                    CompletedEdit::Metadata => sync_canvas_metadata_from_graph(
                        canvas,
                        self.app.as_ref().expect("completed app").host.graph(),
                        &saved,
                    ),
                    CompletedEdit::Intake => {
                        let refresh = sync_canvas_intake(
                            canvas,
                            self.app.as_ref().expect("completed app").host.graph(),
                            saved.member,
                        );
                        self.pending_intake = None;
                        if refresh.is_ok() {
                            self.select(Some(saved.member));
                            self.open_detail(canvas);
                            self.intake_address = TextInput::new("");
                            self.intake_title = TextInput::new("");
                        }
                        refresh
                    },
                    CompletedEdit::Detail(target) => {
                        let refresh = sync_canvas_intake(
                            canvas,
                            self.app.as_ref().expect("completed app").host.graph(),
                            saved.member,
                        );
                        if refresh.is_ok() {
                            if let Some(target) = target {
                                canvas.set_selected_members(&[saved.member, target]);
                            }
                        }
                        self.pending_detail = None;
                        refresh
                    },
                };
                if matches!(edit, CompletedEdit::Metadata) || refresh.is_ok() {
                    self.title = TextInput::new(saved.title);
                    self.tags = TextInput::new(saved.tags.join(", "));
                }
                self.status = match refresh {
                    Ok(()) => match edit {
                        CompletedEdit::Metadata | CompletedEdit::Detail(_) => {
                            "Changes saved".into()
                        },
                        CompletedEdit::Intake => "Object added and saved".into(),
                    },
                    Err(error) => match edit {
                        CompletedEdit::Metadata | CompletedEdit::Detail(_) => {
                            format!("Changes saved · canvas refresh failed: {error}")
                        },
                        CompletedEdit::Intake => format!(
                            "Object saved · canvas refresh failed: {error} · reload to restore the local view"
                        ),
                    },
                };
                self.save_state = "saved";
            },
            Err(error) => {
                self.status = match edit {
                    CompletedEdit::Metadata => {
                        format!("Save failed · {error} · changes remain in memory")
                    },
                    CompletedEdit::Intake => format!(
                        "Intake save failed · {error} · new object remains in memory; retry intake to save it"
                    ),
                    CompletedEdit::Detail(_) => {
                        format!("Detail save failed · {error} · retry to save the pending edit")
                    },
                };
                self.save_state = "error";
            },
        }
        true
    }
}

/// The item role select's index for an override (0 is "as recipe").
fn item_role_index(role: Option<Role>) -> usize {
    role.and_then(|role| Role::ALL.iter().position(|r| *r == role))
        .map_or(0, |index| index + 1)
}

/// Set or clear the picked item's own role (F48). It is view state, saved
/// with the scene, not graph truth.
fn apply_item_role(page: &mut TreePage) {
    let Some(product) = &mut page.product else {
        return;
    };
    let Some(member) = product.selected else {
        product.status = "Select an object first".into();
        return;
    };
    let role = product
        .role
        .selected
        .checked_sub(1)
        .and_then(|i| Role::ALL.get(i).copied());
    let set = page
        .shared
        .canvas
        .borrow_mut()
        .set_member_role(member, role);
    product.status = match role {
        _ if !set => "Item role refused: the item does not permit it".into(),
        Some(role) => format!("Item role set to {}", role.id()),
        None => "Item role follows the recipe".into(),
    };
    page.shared.dirty.set(true);
}

fn intake_controls(page: &TreePage) -> Child {
    use cambium::{FileFilter, button, el, lens, open_file, text_field_typed};
    let product = page.product.as_ref().expect("local intake");
    let mut children: Vec<Child> = vec![Box::new(
        button("Add an object", |page: &mut TreePage, _| {
            if let Some(product) = &mut page.product
                && !product.saving
                && !product.file_requested
                && product.pending_detail.is_none()
            {
                product.intake_open = !product.intake_open;
                if product.intake_open {
                    product.detail_open = false;
                }
            }
        })
        .attr(
            "aria-expanded",
            if product.intake_open { "true" } else { "false" },
        ),
    )];
    if product.intake_open {
        let mut fields: Vec<Child> = Vec::new();
        if product.saving {
            fields.push(Box::new(el("p", "Saving new object…")));
        } else if let Some(member) = product.pending_intake {
            fields.push(Box::new(button(
                "Retry intake",
                move |page: &mut TreePage, _| {
                    if let Some(product) = &mut page.product {
                        product.store_intake(member);
                    }
                },
            )));
        } else {
            fields.push(Box::new(el(
                "label",
                (
                    "Address",
                    lens(
                        |input: &mut TextInput| {
                            text_field_typed(input).attr("aria-label", "Address")
                        },
                        |page: &mut TreePage| {
                            &mut page.product.as_mut().expect("local intake").intake_address
                        },
                    ),
                ),
            )));
            fields.push(Box::new(el(
                "label",
                (
                    "New object title",
                    lens(
                        |input: &mut TextInput| {
                            text_field_typed(input).attr("aria-label", "New object title")
                        },
                        |page: &mut TreePage| {
                            &mut page.product.as_mut().expect("local intake").intake_title
                        },
                    ),
                ),
            )));
            fields.push(Box::new(button(
                cambium::catalogue::label(cambium::catalogue::ids::NODE_NEW)
                    .expect("shared New node label"),
                |page: &mut TreePage, _| {
                    if let Some(product) = &mut page.product {
                        product.add_address();
                    }
                },
            )));
            fields.push(Box::new(open_file(
                button("Choose a file", |page: &mut TreePage, _| {
                    if let Some(product) = &mut page.product
                        && !product.saving
                        && product.pending_intake.is_none()
                    {
                        product.file_requested = true;
                    }
                }),
                product.file_requested,
                FileFilter::default(),
                |page: &mut TreePage, event: FileEvent| {
                    if let Some(product) = &mut page.product {
                        product.receive_file(event);
                    }
                },
            )));
        }
        children.push(Box::new(
            el("section", fields)
                .attr("class", "tree-detail tree-intake")
                .attr("aria-label", "Add an object"),
        ));
    }
    Box::new(el("div", children))
}

pub(super) fn controls(page: &TreePage) -> Child {
    use cambium::{DetailRow, DetailSection, button, detail_panel, el, lens, text_field_typed};
    let Some(product) = &page.product else {
        return Box::new(el("div", ()));
    };
    let mut children: Vec<Child> = Vec::new();
    if product.inspection_fixture {
        children.push(Box::new(
            el("p", "Field inspection fixture · separate browser store").attr("role", "status"),
        ));
    }
    if !product.status.is_empty() {
        children.push(Box::new(
            el("p", product.status.clone())
                .attr("role", "status")
                .attr("aria-label", "Graph status"),
        ));
    }
    if product.operation_pending && !product.saving {
        children.push(Box::new(button("Retry save", |page: &mut TreePage, _| {
            if let Some(product) = &mut page.product {
                product.persist_operation();
            }
        })));
    }
    for (label, id) in [
        ("Undo session", "session:undo"),
        ("Redo session", "session:redo"),
    ] {
        children.push(Box::new(button(label, move |page: &mut TreePage, _| {
            page.command(id);
        })));
    }
    children.push(intake_controls(page));
    children.push(super::inspection::open_button(page));
    children.push(super::field_inspection::open_button(page));
    children.push(Box::new(button(
        if product.detail_open {
            "Close details"
        } else {
            "Open details"
        },
        |page: &mut TreePage, _| {
            if page
                .product
                .as_ref()
                .is_some_and(|product| product.detail_open)
            {
                if let Some(product) = &mut page.product {
                    product.detail_open = false;
                }
                page.shared
                    .events
                    .borrow_mut()
                    .push("command close-detail".into());
            } else {
                page.command("node:edit");
            }
        },
    )));
    if product.detail_open {
        let mut fields: Vec<Child> = vec![Box::new(detail_panel(&[DetailSection::new(
            "Object details",
            vec![DetailRow::new("Address", &product.address)],
        )]))];
        if product.saving {
            fields.push(Box::new(el("p", "Saving changes…")));
        } else if product.pending_detail.is_some() {
            fields.push(Box::new(button(
                "Retry detail save",
                |page: &mut TreePage, _| {
                    if let Some(product) = &mut page.product {
                        product.retry_detail();
                    }
                },
            )));
        } else {
            fields.push(Box::new(el(
                "label",
                (
                    "Title",
                    lens(
                        |input: &mut TextInput| text_field_typed(input).attr("aria-label", "Title"),
                        |page: &mut TreePage| {
                            &mut page.product.as_mut().expect("local detail").title
                        },
                    ),
                ),
            )));
            fields.push(Box::new(el(
                "label",
                (
                    "Tags",
                    lens(
                        |input: &mut TextInput| text_field_typed(input).attr("aria-label", "Tags"),
                        |page: &mut TreePage| {
                            &mut page.product.as_mut().expect("local detail").tags
                        },
                    ),
                ),
            )));
            fields.push(Box::new(button(
                "Save changes",
                |page: &mut TreePage, _| {
                    if let Some(product) = &mut page.product {
                        product.save();
                    }
                },
            )));
            let options: Vec<&'static str> = std::iter::once("As recipe")
                .chain(Role::ALL.iter().map(|role| role.label()))
                .collect();
            fields.push(Box::new(el(
                "label",
                (
                    "Item role",
                    lens(
                        move |state: &mut SelectState| cambium::select(state, &options),
                        |page: &mut TreePage| {
                            &mut page.product.as_mut().expect("local detail").role
                        },
                    ),
                ),
            )));
            fields.push(Box::new(button(
                "Apply item role",
                |page: &mut TreePage, _| apply_item_role(page),
            )));
            fields.push(detail::controls(page));
        }
        children.push(Box::new(
            el("section", fields)
                .attr("class", "tree-detail")
                .attr("aria-label", "Object details"),
        ));
    }
    Box::new(
        el("aside", children)
            .attr(
                "class",
                if page.forme.as_ref().is_some_and(|pane| pane.workbench) {
                    "tree-product tree-product-workbench workspace-controls"
                } else {
                    "tree-product workspace-controls"
                },
            )
            .attr("aria-label", "Saved graph"),
    )
}
