// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Projection-owned reading intent and temporary Find, separate from graph truth.
use crate::{
    action::{Action, Effect},
    app::App,
    panes::PaneId,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReadTarget {
    pub pane: PaneId,
    pub member: Uuid,
}
impl ReadTarget {
    pub(crate) fn appearance(self, window: u64) -> crate::surface::SurfaceId {
        crate::surface::SurfaceId::content_appearance(
            self.member,
            crate::surface::appearance::workbench_appearance_role(self.pane, self.member)
                ^ window.wrapping_mul(0x9e37_79b9_7f4a_7c15),
        )
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum ReadAction {
    Command(ReadTarget, cambium::ReadCommand),
    Query(ReadTarget, cambium::TextInput),
}
#[derive(Default)]
pub struct ReadingState {
    preferences: tabard::ReadingProjections<ReadTarget>,
    pub find: BTreeMap<ReadTarget, cambium::ReadFind>,
    pub load_error: Option<String>,
}
const FILE: &str = "reading-projections.json";
impl ReadingState {
    pub fn preferences(&self, target: ReadTarget) -> tabard::ReadingPreferences {
        self.preferences.get(&target)
    }
    pub(crate) fn for_surface(
        &self,
        surface: crate::surface::Surface,
        window: u64,
    ) -> tabard::ReadingPreferences {
        self.preferences
            .iter()
            .find_map(|(target, value)| (target.appearance(window) == surface.id).then_some(*value))
            .unwrap_or_default()
    }
    pub fn load(root: &Path) -> Self {
        let result = match std::fs::read(root.join(FILE)) {
            Ok(bytes) => tabard::ReadingProjections::from_json(&bytes)
                .and_then(|loaded| loaded.validate().map(|()| loaded)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Default::default()),
            Err(e) => Err(e.to_string()),
        };
        match result {
            Ok(preferences) => Self {
                preferences,
                ..Default::default()
            },
            Err(error) => Self {
                load_error: Some(error),
                ..Default::default()
            },
        }
    }
    pub(crate) fn save(&self, root: &Path) -> Result<(), String> {
        if let Some(error) = &self.load_error {
            return Err(format!("Reading settings were not loaded: {error}"));
        }
        let bytes = self.preferences.to_json()?;
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let temp = root.join(format!("reading-projections.{}.tmp", std::process::id()));
        std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(temp, root.join(FILE)).map_err(|e| e.to_string())
    }
}
impl App {
    pub(crate) fn has_reading_controls(&self, target: ReadTarget) -> bool {
        self.content.facts(target.member).is_some_and(|facts| {
            facts.engine == "genet.reader"
                || facts.engine.starts_with("nematic.")
                || facts.engine == "mere.document"
                || facts.engine == "fleece.retained-clip"
        })
    }
    pub(crate) fn focused_read_target(&self) -> Option<ReadTarget> {
        let (node, appearance) = match self.focus {
            crate::surface::FocusTarget::Content { node, appearance } => (node, appearance),
            crate::surface::FocusTarget::Pane(pane) if self.workbench_for_pane(pane).is_some() => {
                return self
                    .browser_command_target()
                    .map(|(member, _)| ReadTarget { pane, member });
            },
            _ => return None,
        };
        self.frisket
            .iter_leaves()
            .chain(
                self.lenses
                    .iter()
                    .flatten()
                    .flat_map(|layout| layout.iter_leaves()),
            )
            .filter(|(_, kind, _)| matches!(kind, crate::panes::PaneContent::Workbench))
            .find_map(|(pane, _, _)| {
                let target = ReadTarget { pane, member: node };
                (0..=self.lenses.len() as u64)
                    .any(|window| target.appearance(window) == appearance)
                    .then_some(target)
            })
    }
    pub(crate) fn read_find_changed(
        &mut self,
        target: ReadTarget,
        query: String,
        result: Result<inker::DocumentFindState, String>,
    ) -> Vec<Effect> {
        if let Some(ui) = self
            .reading
            .find
            .get_mut(&target)
            .filter(|ui| ui.open && ui.input.text() == query)
        {
            ui.status = match result {
                Ok(state) => cambium::find_status(query.is_empty(), false, state.count, state.current),
                Err(error) => cambium::find_failed(&error),
            };
        }
        vec![Effect::Redraw]
    }
    pub(crate) fn read_action(&mut self, action: ReadAction) -> Vec<Effect> {
        use cambium::ReadCommand as C;
        let (target, command) = match action {
            ReadAction::Query(target, input) => {
                let query = input.text().to_owned();
                if !self.reading.find.get(&target).is_some_and(|ui| ui.open) {
                    return Vec::new();
                }
                let ui = self.reading.find.get_mut(&target).unwrap();
                let changed = ui.input.text() != query;
                ui.input = input;
                if !changed {
                    return vec![Effect::Redraw];
                }
                return vec![
                    Effect::ReadFind {
                        target,
                        query,
                        step: None,
                        close: false,
                    },
                    Effect::Redraw,
                ];
            },
            ReadAction::Command(target, command) => (target, command),
        };
        if !self
            .workbench_for_pane(target.pane)
            .is_some_and(|w| w.has_tile(target.member))
        {
            return Vec::new();
        }
        if command == C::Find && !self.has_reading_controls(target) {
            return self.update(Action::OpenDocumentFind);
        }
        let mut preferences = self.reading.preferences(target);
        match command {
            C::Smaller => preferences.step_text(false),
            C::Larger => preferences.step_text(true),
            C::Narrower => preferences.step_column(false),
            C::Wider => preferences.step_column(true),
            C::Reset => preferences = Default::default(),
            C::Find | C::CloseFind | C::Next | C::Previous => {
                let ui = self.reading.find.entry(target).or_default();
                ui.open = command != C::CloseFind;
                let query = ui.input.text().to_owned();
                return vec![
                    Effect::ReadFind {
                        target,
                        query,
                        step: match command {
                            C::Next => Some(inker::DocumentFindDirection::Next),
                            C::Previous => Some(inker::DocumentFindDirection::Previous),
                            _ => None,
                        },
                        close: command == C::CloseFind,
                    },
                    Effect::Redraw,
                ];
            },
        }
        if self.reading.load_error.is_some() {
            return vec![Effect::Redraw];
        }
        self.reading.preferences.set(target, preferences);
        vec![Effect::SaveReadingPreferences, Effect::Redraw]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projections_with_shared_documents_keep_choices_across_cold_load() {
        let dir = tempfile::tempdir().unwrap();
        let a = ReadTarget {
            pane: PaneId(7),
            member: Uuid::from_u128(1),
        };
        let b = ReadTarget {
            pane: PaneId(8),
            ..a
        };
        let mut state = ReadingState::load(dir.path());
        let mut value = state.preferences(a);
        value.step_text(true);
        value.step_column(false);
        state.preferences.set(a, value);
        state.find.entry(a).or_default().open = true;
        state.save(dir.path()).unwrap();
        let cold = ReadingState::load(dir.path());
        assert_eq!(cold.preferences(a), value);
        assert_eq!(cold.preferences(b), Default::default());
        assert!(cold.find.is_empty());
        assert_ne!(a.appearance(0), b.appearance(0));
    }
    #[test]
    fn invalid_preferences_are_preserved_for_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reading-projections.json");
        std::fs::write(&path, b"not json").unwrap();
        let state = ReadingState::load(dir.path());
        assert!(state.save(dir.path()).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"not json");
    }
}
