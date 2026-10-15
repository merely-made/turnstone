// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Executable projection editing without a browser or widget owner.
use crate::projection_compile::{
    CompiledProjection, ProjectionDataset, ProjectionSnapshot, default_definition,
    practice_compiler,
};
use scenograph::{ProjectionDefinition, ProjectionDraft, ValidationIssue};
use scenomise::authoring::{EditorAction, ProjectionEditor};

pub fn draft_from_definition(definition: &ProjectionDefinition) -> ProjectionDraft {
    ProjectionDraft {
        dynamics: definition.dynamics.clone(),
        version: definition.version,
        id: definition.id.clone(),
        label: definition.label.clone(),
        source: definition.source.clone(),
        reading: definition.reading.clone(),
        encoding: definition.encoding.clone(),
        arrangement: definition.arrangement.clone(),
        interaction: definition.interaction.clone(),
        appearance: definition.appearance.clone(),
        provenance: definition.provenance.clone(),
    }
}

fn issues(issues: Vec<ValidationIssue>) -> String {
    issues
        .iter()
        .map(|issue| format!("{}: {}", issue.field, issue.message))
        .collect::<Vec<_>>()
        .join("; ")
}

fn compile_issues(issues: Vec<crate::projection_compile::CompileIssue>) -> String {
    issues
        .iter()
        .map(|issue| format!("{}: {}", issue.field, issue.message))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Compile or refresh through the existing compiler. Appearance-only edits keep
/// solved placement, and selection is always resolved by occurrence identity.
pub fn compile_editor(
    editor: &ProjectionEditor,
    dataset: &ProjectionDataset,
    previous: Option<&CompiledProjection>,
    selected: Option<&str>,
) -> Result<CompiledProjection, String> {
    let definition = editor.draft().to_definition().map_err(issues)?;
    let mut compiled = match previous {
        Some(previous) => practice_compiler().refresh(previous, &definition, dataset),
        None => practice_compiler().compile(&definition, dataset),
    }
    .map_err(compile_issues)?;
    compiled.selected = selected
        .and_then(|id| compiled.instance_by_occurrence.get(id))
        .copied();
    Ok(compiled)
}

/// A compiler-validated save payload. Persistence remains the MereHost's job.
pub fn snapshot_editor(
    editor: &ProjectionEditor,
    dataset: &ProjectionDataset,
    selected: Option<&str>,
) -> Result<ProjectionSnapshot, String> {
    let definition = editor.draft().to_definition().map_err(issues)?;
    let snapshot = ProjectionSnapshot {
        definition,
        selected_occurrence: selected.map(str::to_owned),
    };
    practice_compiler()
        .compile_snapshot(&snapshot, dataset)
        .map_err(compile_issues)?;
    Ok(snapshot)
}

pub struct ProjectionSession {
    editor: ProjectionEditor,
    dataset: ProjectionDataset,
    compiled: Option<CompiledProjection>,
    selected: Option<String>,
    error: String,
    solves: u64,
    placement_reuses: u64,
}

impl ProjectionSession {
    /// Parse and validate supplied facts before replacing any existing session.
    pub fn load(dataset: &str, definition: Option<&str>) -> Result<Self, String> {
        let dataset: ProjectionDataset = serde_json::from_str(dataset)
            .map_err(|error| format!("Source load failed: {error}"))?;
        let definition = definition
            .map(serde_json::from_str)
            .transpose()
            .map_err(|error| format!("Recipe load failed: {error}"))?
            .unwrap_or_else(|| default_definition(&dataset));
        let editor = ProjectionEditor::new(draft_from_definition(&definition));
        let compiled = compile_editor(&editor, &dataset, None, None)?;
        Ok(Self {
            editor,
            dataset,
            compiled: Some(compiled),
            selected: None,
            error: String::new(),
            solves: 1,
            placement_reuses: 0,
        })
    }

    pub fn editor(&self) -> &ProjectionEditor {
        &self.editor
    }
    pub fn dataset(&self) -> &ProjectionDataset {
        &self.dataset
    }
    pub fn compiled(&self) -> Option<&CompiledProjection> {
        self.compiled.as_ref()
    }
    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }
    pub fn error(&self) -> &str {
        &self.error
    }
    pub fn solves(&self) -> u64 {
        self.solves
    }
    pub fn placement_reuses(&self) -> u64 {
        self.placement_reuses
    }

    pub fn edit(&mut self, action: EditorAction, now_ms: u64) {
        self.editor.reduce(action, now_ms);
        self.refresh();
    }

    pub fn undo(&mut self) -> bool {
        let changed = self.editor.undo();
        if changed {
            self.refresh();
        }
        changed
    }

    pub fn redo(&mut self) -> bool {
        let changed = self.editor.redo();
        if changed {
            self.refresh();
        }
        changed
    }

    pub fn mark_saved(&mut self) {
        self.editor.mark_saved();
    }

    pub fn refresh(&mut self) {
        match compile_editor(
            &self.editor,
            &self.dataset,
            self.compiled.as_ref(),
            self.selected(),
        ) {
            Ok(compiled) => {
                if compiled.placement_reused {
                    self.placement_reuses += 1;
                } else {
                    self.solves += 1;
                }
                self.compiled = Some(compiled);
                self.error.clear();
            },
            Err(error) => {
                self.compiled = None;
                self.error = error;
            },
        }
    }

    pub fn select(&mut self, occurrence: &str) -> Result<(), String> {
        let compiled = self.compiled.as_mut().ok_or("No executable projection")?;
        let instance = compiled
            .instance_by_occurrence
            .get(occurrence)
            .copied()
            .ok_or_else(|| format!("Unknown occurrence {occurrence}"))?;
        compiled.selected = Some(instance);
        self.selected = Some(occurrence.into());
        Ok(())
    }

    pub fn snapshot(&self) -> Result<ProjectionSnapshot, String> {
        if self.compiled.is_none() {
            return Err(self.error.clone());
        }
        snapshot_editor(&self.editor, &self.dataset, self.selected())
    }

    /// Refused snapshots leave the currently editable and executable state whole.
    pub fn reopen(&mut self, snapshot: &ProjectionSnapshot) -> Result<(), String> {
        let compiled = practice_compiler()
            .compile_snapshot(snapshot, &self.dataset)
            .map_err(compile_issues)?;
        self.editor = ProjectionEditor::new(draft_from_definition(&snapshot.definition));
        self.selected = snapshot.selected_occurrence.clone();
        self.compiled = Some(compiled);
        self.error.clear();
        self.solves += 1;
        Ok(())
    }
}
