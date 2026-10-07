// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Host-neutral projection authoring state.
//!
//! The editor owns a draft and the validation needed to turn it into a
//! definition. Its panels are ordinary [`workbench::Tile`]s, so Graphshell can
//! split, stack, and tear them out through the same reusable workspace contract
//! as another Genet host. It deliberately has no graph, endpoint, or authority
//! handle. A host supplies [`ProjectionDefinitionSink`] when it elects to
//! persist a valid definition.

pub use scenograph::relationship::{
    RecipeEdit, RelationshipRecipe, RelationshipRecipeDraft, RelationshipRequirements,
    RelationshipSnapshot, relationship_recipe,
};
use std::collections::BTreeMap;
use std::mem::{Discriminant, discriminant};

use edit_history::History;

pub use scenograph::{
    Appearance, Arrangement, AuthoredDefinitionError, AuthoredProjectionDefinition, Channel,
    Encoding, Interaction, PROJECTION_DEFINITION_VERSION, ProjectionDefinition, ProjectionDraft,
    ProjectionInputBinding, ProjectionVariant, Provenance, PublicSourceRevision, Reading,
    RevisionEvidence, RuntimeProjectionBinding, RuntimeSourceBinding, SelectionMode, SourceBinding,
    ValidationIssue, ValidationSeverity,
};
use serde::{Deserialize, Serialize};
use workbench::{ContentSource, Tile, TileEvent, TileId, TileTree, Workbench, WorkbenchOutcome};

/// The plain panels a host may expose for projection authoring.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionPanel {
    #[default]
    Source,
    Reading,
    Encoding,
    Arrangement,
    Interaction,
    Preview,
    Provenance,
}

impl ProjectionPanel {
    pub const ALL: [Self; 7] = [
        Self::Source,
        Self::Reading,
        Self::Encoding,
        Self::Arrangement,
        Self::Interaction,
        Self::Preview,
        Self::Provenance,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Source => "Source",
            Self::Reading => "Reading",
            Self::Encoding => "Encoding",
            Self::Arrangement => "Arrangement",
            Self::Interaction => "Interaction",
            Self::Preview => "Preview",
            Self::Provenance => "Provenance",
        }
    }

    /// Stable Workbench identity for this editor tool.
    pub const fn tile_id(self) -> TileId {
        TileId(match self {
            Self::Source => 1,
            Self::Reading => 2,
            Self::Encoding => 3,
            Self::Arrangement => 4,
            Self::Interaction => 5,
            Self::Preview => 6,
            Self::Provenance => 7,
        })
    }

    /// Open content-lane id resolved by the Graphshell host.
    pub const fn content_id(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Reading => "reading",
            Self::Encoding => "encoding",
            Self::Arrangement => "arrangement",
            Self::Interaction => "interaction",
            Self::Preview => "preview",
            Self::Provenance => "provenance",
        }
    }

    /// Recover the editor tool represented by a Workbench tile.
    pub const fn from_tile_id(id: TileId) -> Option<Self> {
        match id.0 {
            1 => Some(Self::Source),
            2 => Some(Self::Reading),
            3 => Some(Self::Encoding),
            4 => Some(Self::Arrangement),
            5 => Some(Self::Interaction),
            6 => Some(Self::Preview),
            7 => Some(Self::Provenance),
            _ => None,
        }
    }
}

const PROJECTION_EDITOR_PANEL_KIND: &str = "graphshell.projection-editor.panel";

fn projection_editor_workbench() -> Workbench {
    let tabs = ProjectionPanel::ALL
        .into_iter()
        .map(|panel| Tile {
            id: panel.tile_id(),
            title: panel.label().to_owned(),
            content: ContentSource::Open {
                kind: PROJECTION_EDITOR_PANEL_KIND.to_owned(),
                id: panel.content_id().to_owned(),
            },
            accent: None,
        })
        .collect();
    Workbench::new(TileTree::stack(tabs, 0))
}

/// Chronicle's stable authored definition id.
pub const CHRONICLE_DEFINITION_ID: &str = "distillery.chronicle";
/// The arrangement-owned guide toggle used by Chronicle.
pub const CHRONICLE_ERA_BANDS_OPTION: &str = "era_bands";

/// Build the one Chronicle recipe used for its named Distillery and Djinn
/// inputs. The caller supplies source locators and authority-emitted
/// generations; this authoring seam never acquires either authority.
pub fn chronicle_definition(
    sources: BTreeMap<String, ProjectionInputBinding>,
    author: impl Into<String>,
    definition_revision: impl Into<String>,
) -> AuthoredProjectionDefinition {
    AuthoredProjectionDefinition {
        version: PROJECTION_DEFINITION_VERSION,
        id: CHRONICLE_DEFINITION_ID.to_owned(),
        label: "Job Chronicle".to_owned(),
        sources,
        reading: Reading {
            kind: "mesh.jobs".to_owned(),
            key: "job_id".to_owned(),
            value: Some("state".to_owned()),
        },
        encoding: Encoding {
            x: Channel::Field("observation_tick".to_owned()),
            y: Channel::Field("lease_epoch".to_owned()),
            color: Some(Channel::Field("state".to_owned())),
            label: Some(Channel::Field("job_id".to_owned())),
        },
        arrangement: Arrangement {
            kind: "timeline.default".to_owned(),
            direction: "observation_tick".to_owned(),
            spacing: 180,
            options: BTreeMap::from([(CHRONICLE_ERA_BANDS_OPTION.to_owned(), "true".to_owned())]),
        },
        interaction: Interaction::default(),
        appearance: Appearance {
            realization: "portable-card".to_owned(),
            title: "Job Chronicle".to_owned(),
            theme: "chronicle".to_owned(),
        },
        provenance: Provenance {
            author: author.into(),
            source_revision: Some(PublicSourceRevision::from(definition_revision.into())),
            revision_evidence: RevisionEvidence::PublicGeneration,
            note: "One read-only job-board recipe; source bindings carry authority generation expectations."
                .to_owned(),
        },
    }
}

/// The Chronicle variant that hides lease-epoch guide bands while preserving
/// the base definition and every non-arrangement field.
pub fn chronicle_era_bands_off_variant() -> ProjectionVariant {
    ProjectionVariant {
        definition_id: CHRONICLE_DEFINITION_ID.to_owned(),
        id: "era-bands-off".to_owned(),
        arrangement_options: BTreeMap::from([(
            CHRONICLE_ERA_BANDS_OPTION.to_owned(),
            "false".to_owned(),
        )]),
    }
}

/// Typed messages understood by the reducer. Hosts can map these to any UI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditorAction {
    SelectPanel(ProjectionPanel),
    SetId(String),
    SetLabel(String),
    SetSource(SourceBinding),
    SetReading(Reading),
    SetEncoding(Encoding),
    SetArrangement(Arrangement),
    SetInteraction(Interaction),
    SetAppearance(Appearance),
    SetProvenance(Provenance),
}

/// The result of reducing one editor action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReduceResult {
    Changed,
    PanelChanged,
}

/// How long edits to one field keep coalescing into one undo step, between
/// records, by the host's clock.
pub const COALESCE_WINDOW_MS: u64 = 400;

/// One undo step: the draft and the panel it was edited in. The workbench
/// layout is workspace, not document, and is never recorded.
type EditorStep = (ProjectionDraft, ProjectionPanel);

/// Stateful editor boundary, independent of a widget toolkit or host.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectionEditor {
    draft: ProjectionDraft,
    panel: ProjectionPanel,
    workspace: Workbench,
    history: History<EditorStep, Discriminant<EditorAction>>,
}

impl ProjectionEditor {
    pub fn new(draft: ProjectionDraft) -> Self {
        Self {
            draft,
            panel: ProjectionPanel::default(),
            workspace: projection_editor_workbench(),
            history: History::new().with_window(COALESCE_WINDOW_MS),
        }
    }

    pub fn draft(&self) -> &ProjectionDraft {
        &self.draft
    }

    pub fn panel(&self) -> ProjectionPanel {
        self.panel
    }

    /// The reusable split/tab arrangement for the editor's tool panels.
    pub fn workspace(&self) -> &Workbench {
        &self.workspace
    }

    /// Apply a Workbench gesture without granting graph or persistence authority.
    ///
    /// A valid outside drop is returned as a typed host effect and retains the
    /// panel until Graphshell accepts tearout custody.
    pub fn apply_workspace(&mut self, event: &TileEvent) -> WorkbenchOutcome {
        let outcome = self.workspace.apply(event);
        if outcome.changed() {
            if let TileEvent::Activated(id) = event
                && let Some(panel) = ProjectionPanel::from_tile_id(*id)
            {
                self.panel = panel;
            }
            if self.workspace.tree().find(self.panel.tile_id()).is_none()
                && let Some(panel) = first_active_panel(self.workspace.tree())
            {
                self.panel = panel;
            }
        }
        outcome
    }

    /// Apply one action. A draft edit that changes the draft records an undo
    /// step first, coalescing with the run in progress when it sets the same
    /// field within [`COALESCE_WINDOW_MS`] of `now_ms`, the host's clock.
    pub fn reduce(&mut self, action: EditorAction, now_ms: u64) -> ReduceResult {
        if let EditorAction::SelectPanel(panel) = action {
            self.show_panel(panel);
            return ReduceResult::PanelChanged;
        }
        let before = (self.draft.clone(), self.panel);
        let key = discriminant(&action);
        let result = self.apply_draft(action);
        if self.draft != before.0 {
            self.history.record(before, Some(key), now_ms);
        }
        result
    }

    /// Restore the draft before the last undo step and return to the panel it
    /// was edited in. `false` when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        let current = (self.draft.clone(), self.panel);
        self.history
            .undo(current)
            .map(|step| self.restore(step))
            .is_some()
    }

    /// Re-apply the last undone step. `false` when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        let current = (self.draft.clone(), self.panel);
        self.history
            .redo(current)
            .map(|step| self.restore(step))
            .is_some()
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Whether the draft differs from its last save (or from where it opened).
    pub fn is_dirty(&self) -> bool {
        self.history.is_dirty()
    }

    /// End the coalescing run, so the next edit is its own undo step (the end
    /// of a drag, focus leaving a field).
    pub fn break_run(&mut self) {
        self.history.break_run();
    }

    fn restore(&mut self, (draft, panel): EditorStep) {
        self.draft = draft;
        self.show_panel(panel);
    }

    fn show_panel(&mut self, panel: ProjectionPanel) {
        let _ = self.workspace.apply(&TileEvent::Activated(panel.tile_id()));
        self.panel = panel;
    }

    fn apply_draft(&mut self, action: EditorAction) -> ReduceResult {
        match action {
            EditorAction::SelectPanel(panel) => {
                self.show_panel(panel);
                ReduceResult::PanelChanged
            },
            EditorAction::SetId(value) => {
                self.draft.id = value;
                ReduceResult::Changed
            },
            EditorAction::SetLabel(value) => {
                self.draft.label = value;
                ReduceResult::Changed
            },
            EditorAction::SetSource(value) => {
                self.draft.source = value;
                ReduceResult::Changed
            },
            EditorAction::SetReading(value) => {
                self.draft.reading = value;
                ReduceResult::Changed
            },
            EditorAction::SetEncoding(value) => {
                self.draft.encoding = value;
                ReduceResult::Changed
            },
            EditorAction::SetArrangement(value) => {
                self.draft.arrangement = value;
                ReduceResult::Changed
            },
            EditorAction::SetInteraction(value) => {
                self.draft.interaction = value;
                ReduceResult::Changed
            },
            EditorAction::SetAppearance(value) => {
                self.draft.appearance = value;
                ReduceResult::Changed
            },
            EditorAction::SetProvenance(value) => {
                self.draft.provenance = value;
                ReduceResult::Changed
            },
        }
    }

    pub fn validate(&self) -> Result<(), Vec<ValidationIssue>> {
        self.draft.validate()
    }

    /// Persist only a validated, immutable definition through host policy.
    /// A successful save marks the draft clean.
    pub fn save<S: ProjectionDefinitionSink>(
        &mut self,
        sink: &mut S,
    ) -> Result<(), SaveError<S::Error>> {
        let definition = self.draft.to_definition().map_err(SaveError::Invalid)?;
        sink.save(&definition).map_err(SaveError::Sink)?;
        self.history.mark_saved();
        Ok(())
    }

    /// Take `draft`, read back from the store after an undo or redo of a save,
    /// as one undo step on the draft's own history, and read clean against
    /// the store (SE20).
    pub fn load_stored(&mut self, draft: ProjectionDraft) {
        if draft != self.draft {
            self.history
                .record((self.draft.clone(), self.panel), None, 0);
            self.draft = draft;
        }
        self.history.mark_saved();
    }

    /// The store no longer holds this draft's definition: it reads unsaved.
    pub fn forget_saved(&mut self) {
        self.history.forget_saved();
    }

    /// Mark the draft clean after the host persisted it by another path.
    pub fn mark_saved(&mut self) {
        self.history.mark_saved();
    }
}

fn first_active_panel(tree: &TileTree) -> Option<ProjectionPanel> {
    match tree {
        TileTree::Stack(stack) => stack
            .tabs
            .get(stack.active)
            .and_then(|tile| ProjectionPanel::from_tile_id(tile.id)),
        TileTree::Split { children, .. } => children
            .iter()
            .find_map(|branch| first_active_panel(&branch.tree)),
    }
}

/// Host persistence is the only effect exposed by the editor.
pub trait ProjectionDefinitionSink {
    type Error;

    fn save(&mut self, definition: &ProjectionDefinition) -> Result<(), Self::Error>;
}

#[derive(Debug, Eq, PartialEq)]
pub enum SaveError<E> {
    Invalid(Vec<ValidationIssue>),
    Sink(E),
}

#[cfg(test)]
mod tests {
    use super::*;
    use workbench::{DropTarget, WorkbenchEffect};

    fn valid_draft() -> ProjectionDraft {
        ProjectionDraft {
            version: PROJECTION_DEFINITION_VERSION,
            id: "notes-by-topic".into(),
            label: "Notes by topic".into(),
            source: SourceBinding {
                authority: "local-device".into(),
                domain: "notes".into(),
                resource: "graph:notes".into(),
            },
            reading: Reading {
                kind: "nodes".into(),
                key: "topic".into(),
                value: None,
            },
            encoding: Encoding {
                x: Channel::Field("topic_x".into()),
                y: Channel::Field("topic_y".into()),
                color: Some(Channel::Field("kind".into())),
                label: Some(Channel::Field("title".into())),
            },
            arrangement: Arrangement::default(),
            interaction: Interaction::default(),
            appearance: Appearance {
                realization: "canvas".into(),
                title: "Notes by topic".into(),
                theme: "light".into(),
            },
            provenance: Provenance {
                author: "mark".into(),
                source_revision: Some("rev-7".into()),
                revision_evidence: RevisionEvidence::PublicGeneration,
                note: "fixture".into(),
            },
        }
    }

    fn chronicle_sources() -> BTreeMap<String, ProjectionInputBinding> {
        BTreeMap::from([
            (
                "distillery".to_owned(),
                ProjectionInputBinding {
                    source: SourceBinding {
                        authority: "distillery.endpoint".to_owned(),
                        domain: "mesh.job-board".to_owned(),
                        resource: "distillery.chronicle".to_owned(),
                    },
                    expects_generation: Some("generation:distillery-fixture".into()),
                    revision_evidence: RevisionEvidence::PublicGeneration,
                },
            ),
            (
                "djinn".to_owned(),
                ProjectionInputBinding {
                    source: SourceBinding {
                        authority: "djinn.resident".to_owned(),
                        domain: "mesh.job-board".to_owned(),
                        resource: "distillery.chronicle".to_owned(),
                    },
                    expects_generation: Some("generation:djinn-fixture".into()),
                    revision_evidence: RevisionEvidence::PublicGeneration,
                },
            ),
        ])
    }

    fn label(editor: &ProjectionEditor) -> &str {
        &editor.draft().label
    }

    #[test]
    fn undo_restores_the_draft_and_returns_to_its_panel() {
        let mut editor = ProjectionEditor::new(valid_draft());
        editor.reduce(EditorAction::SelectPanel(ProjectionPanel::Provenance), 0);
        editor.reduce(EditorAction::SetLabel("Edited".into()), 0);
        editor.reduce(EditorAction::SelectPanel(ProjectionPanel::Preview), 10);
        assert!(editor.undo());
        assert_eq!(label(&editor), "Notes by topic");
        assert_eq!(editor.panel(), ProjectionPanel::Provenance);
        assert!(!editor.can_undo(), "panel changes record nothing");
        assert!(editor.redo());
        assert_eq!(label(&editor), "Edited");
        assert!(editor.can_undo() && !editor.can_redo());
    }

    #[test]
    fn edits_to_one_field_coalesce_within_the_window() {
        let mut editor = ProjectionEditor::new(valid_draft());
        for (at, text) in [(1_000, "N"), (1_200, "Ne"), (1_500, "New")] {
            editor.reduce(EditorAction::SetLabel(text.into()), at);
        }
        editor.reduce(
            EditorAction::SetLabel("New!".into()),
            1_500 + COALESCE_WINDOW_MS + 1,
        );
        assert!(editor.undo());
        assert_eq!(label(&editor), "New", "the lapsed edit is its own step");
        assert!(editor.undo());
        assert_eq!(
            label(&editor),
            "Notes by topic",
            "the typed run is one step"
        );
        assert!(!editor.can_undo());
    }

    #[test]
    fn another_field_or_a_broken_run_starts_a_step() {
        let mut editor = ProjectionEditor::new(valid_draft());
        editor.reduce(EditorAction::SetLabel("A".into()), 0);
        editor.reduce(EditorAction::SetId("a".into()), 1);
        editor.break_run();
        editor.reduce(EditorAction::SetId("ab".into()), 2);
        assert!(editor.undo());
        assert_eq!(editor.draft().id, "a");
        assert!(editor.undo());
        assert_eq!(editor.draft().id, "notes-by-topic");
        assert!(editor.undo());
        assert_eq!(label(&editor), "Notes by topic");
    }

    #[test]
    fn an_unchanged_value_records_nothing() {
        let mut editor = ProjectionEditor::new(valid_draft());
        editor.reduce(EditorAction::SetLabel("Notes by topic".into()), 0);
        assert!(!editor.can_undo());
        assert!(!editor.is_dirty());
    }

    #[test]
    fn a_save_marks_clean_and_undoing_past_it_reads_dirty() {
        let mut editor = ProjectionEditor::new(valid_draft());
        let mut sink = FixtureSink { saved: Vec::new() };
        assert!(!editor.is_dirty());
        editor.reduce(EditorAction::SetLabel("Saved".into()), 0);
        assert!(editor.is_dirty());
        editor.save(&mut sink).expect("valid draft saves");
        assert!(!editor.is_dirty());
        editor.reduce(EditorAction::SetLabel("After".into()), 10_000);
        assert!(editor.is_dirty());
        editor.undo();
        assert!(!editor.is_dirty(), "back at the save");
        editor.undo();
        assert!(editor.is_dirty(), "before the save");
    }

    #[test]
    fn reducer_edits_typed_draft_and_panel() {
        let mut editor = ProjectionEditor::new(valid_draft());
        assert_eq!(
            editor.reduce(EditorAction::SetLabel("Edited".into()), 0),
            ReduceResult::Changed
        );
        assert_eq!(
            editor.reduce(EditorAction::SelectPanel(ProjectionPanel::Preview), 0),
            ReduceResult::PanelChanged
        );
        assert_eq!(editor.draft().label, "Edited");
        assert_eq!(editor.panel(), ProjectionPanel::Preview);
    }

    #[test]
    fn editor_panels_are_open_workbench_tiles() {
        let editor = ProjectionEditor::new(valid_draft());
        let tiles = editor.workspace().tree().tiles();
        assert_eq!(tiles.len(), ProjectionPanel::ALL.len());
        for panel in ProjectionPanel::ALL {
            let tile = editor
                .workspace()
                .tree()
                .find(panel.tile_id())
                .expect("every editor panel has a tile");
            assert_eq!(tile.title, panel.label());
            assert_eq!(
                tile.content,
                ContentSource::Open {
                    kind: PROJECTION_EDITOR_PANEL_KIND.to_owned(),
                    id: panel.content_id().to_owned(),
                }
            );
        }
    }

    #[test]
    fn workspace_activation_selects_panel_and_tearout_preserves_it() {
        let mut editor = ProjectionEditor::new(valid_draft());
        assert_eq!(
            editor.apply_workspace(&TileEvent::Activated(ProjectionPanel::Preview.tile_id())),
            WorkbenchOutcome::Applied
        );
        assert_eq!(editor.panel(), ProjectionPanel::Preview);

        let before = editor.workspace().tree().clone();
        assert_eq!(
            editor.apply_workspace(&TileEvent::Dragged {
                tile: ProjectionPanel::Preview.tile_id(),
                to: DropTarget::Outside,
            }),
            WorkbenchOutcome::Effect(WorkbenchEffect::TearOut {
                tile: ProjectionPanel::Preview.tile_id(),
            })
        );
        assert_eq!(editor.workspace().tree(), &before);
        assert_eq!(editor.panel(), ProjectionPanel::Preview);
    }

    #[test]
    fn validation_reports_useful_fields() {
        let mut draft = valid_draft();
        draft.source.domain.clear();
        draft.arrangement.spacing = 0;
        draft
            .provenance
            .source_revision
            .as_mut()
            .expect("fixture has source revision")
            .clear();
        let issues = draft.validate().expect_err("invalid draft");
        assert!(issues.iter().any(|issue| issue.field == "source.domain"));
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "arrangement.spacing")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "provenance.source_revision")
        );
    }

    #[test]
    fn unsupported_version_cannot_be_promoted_or_saved() {
        let mut draft = valid_draft();
        draft.version += 1;
        let issues = draft.to_definition().expect_err("version is invalid");
        assert!(issues.iter().any(|issue| issue.field == "version"));
    }

    struct FixtureSink {
        saved: Vec<ProjectionDefinition>,
    }

    impl ProjectionDefinitionSink for FixtureSink {
        type Error = &'static str;

        fn save(&mut self, definition: &ProjectionDefinition) -> Result<(), Self::Error> {
            self.saved.push(definition.clone());
            Ok(())
        }
    }

    #[test]
    fn save_uses_fixture_sink_and_serialization_is_repeatable() {
        let mut editor = ProjectionEditor::new(valid_draft());
        let mut sink = FixtureSink { saved: Vec::new() };
        editor.save(&mut sink).expect("valid draft saves");
        assert_eq!(sink.saved[0].id, valid_draft().id);
        let definition = editor.draft().to_definition().expect("valid definition");
        let first = definition.to_json_bytes().expect("definition serializes");
        let second = definition.to_json_bytes().expect("definition serializes");
        assert_eq!(first, second);
    }

    #[test]
    fn invalid_draft_never_reaches_sink() {
        let mut draft = valid_draft();
        draft.id.clear();
        let mut editor = ProjectionEditor::new(draft);
        let mut sink = FixtureSink { saved: Vec::new() };
        let error = editor
            .save(&mut sink)
            .expect_err("invalid draft is refused");
        assert!(matches!(error, SaveError::Invalid(_)));
        assert!(sink.saved.is_empty());
    }

    #[test]
    fn chronicle_is_one_recipe_bound_to_two_checkable_sources() {
        let authored = chronicle_definition(
            chronicle_sources(),
            "Distillery W2",
            "chronicle-definition-v1",
        );
        authored.validate().expect("complete authored definition");
        let distillery = authored.bind("distillery", None).expect("distillery bind");
        let djinn = authored.bind("djinn", None).expect("djinn bind");

        assert_eq!(authored.id, CHRONICLE_DEFINITION_ID);
        assert_eq!(distillery.id, authored.id);
        assert_eq!(djinn.id, authored.id);
        assert_ne!(distillery.source, djinn.source);
        assert_eq!(distillery.reading, djinn.reading);
        assert_eq!(distillery.encoding, djinn.encoding);
        assert_eq!(distillery.arrangement, djinn.arrangement);
        assert_eq!(distillery.interaction, djinn.interaction);
        assert_eq!(distillery.appearance, djinn.appearance);
        assert_eq!(
            distillery.provenance.source_revision,
            Some(PublicSourceRevision::from("generation:distillery-fixture"))
        );
        assert_eq!(
            djinn.provenance.source_revision,
            Some(PublicSourceRevision::from("generation:djinn-fixture"))
        );

        let first = authored.to_json_bytes().expect("authoring bytes");
        let second = authored.to_json_bytes().expect("authoring bytes");
        assert_eq!(first, second);
        assert_eq!(
            serde_json::from_slice::<AuthoredProjectionDefinition>(&first).expect("authoring wire"),
            authored
        );
    }

    #[test]
    fn chronicle_era_bands_off_is_a_variant_not_another_recipe() {
        let authored = chronicle_definition(
            chronicle_sources(),
            "Distillery W2",
            "chronicle-definition-v1",
        );
        let base = authored.bind("distillery", None).expect("base bind");
        let variant = chronicle_era_bands_off_variant();
        let without_bands = authored
            .bind("distillery", Some(&variant))
            .expect("variant bind");

        assert_eq!(variant.definition_id, authored.id);
        assert_eq!(base.id, without_bands.id);
        assert_eq!(base.source, without_bands.source);
        assert_eq!(base.reading, without_bands.reading);
        assert_eq!(base.encoding, without_bands.encoding);
        assert_eq!(base.interaction, without_bands.interaction);
        assert_eq!(base.appearance, without_bands.appearance);
        assert_eq!(base.provenance, without_bands.provenance);
        assert_eq!(
            base.arrangement.options.get(CHRONICLE_ERA_BANDS_OPTION),
            Some(&"true".to_owned())
        );
        assert_eq!(
            without_bands
                .arrangement
                .options
                .get(CHRONICLE_ERA_BANDS_OPTION),
            Some(&"false".to_owned())
        );
    }

    #[test]
    fn authored_definition_refuses_a_variant_for_another_recipe() {
        let authored = chronicle_definition(
            chronicle_sources(),
            "Distillery W2",
            "chronicle-definition-v1",
        );
        let variant = ProjectionVariant {
            definition_id: "other.recipe".to_owned(),
            id: "era-bands-off".to_owned(),
            arrangement_options: BTreeMap::new(),
        };
        assert_eq!(
            authored.bind("distillery", Some(&variant)),
            Err(AuthoredDefinitionError::VariantForAnotherDefinition(
                "other.recipe".to_owned()
            ))
        );
    }
}
