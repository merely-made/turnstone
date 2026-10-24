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
pub type FindUi = cambium::ReadFind;
#[derive(Default)]
pub struct ReadingState {
    preferences: tabard::ReadingProjections<ReadTarget>,
    pub find: BTreeMap<ReadTarget, FindUi>,
    pub anchors: BTreeMap<ReadTarget, tabard::ReadingAnchor>,
    pub trails: BTreeMap<PaneId, tabard::ReadingTrail<ReadVisit>>,
    pub excerpts: BTreeMap<Uuid, String>,
    pub scope: crate::scope_navigation::ScopeState,
    pub load_error: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadVisit {
    pub graph: crate::panes::GraphId,
    pub member: Uuid,
}
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum SavedReading {
    Current {
        choices: Vec<Choice>,
        trails: Vec<(PaneId, tabard::ReadingTrail<ReadVisit>)>,
        #[serde(default)]
        scope: crate::scope_navigation::ScopeState,
    },
    Legacy(Vec<Choice>),
}
#[derive(Serialize, Deserialize)]
struct Choice {
    target: ReadTarget,
    preferences: tabard::ReadingPreferences,
    #[serde(default)]
    anchor: Option<tabard::ReadingAnchor>,
}
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
        let result = (|| -> Result<Self, String> {
            let bytes = match std::fs::read(root.join("reading-projections.json")) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
                Err(e) => return Err(e.to_string()),
            };
            if bytes.len() > 2 * 1024 * 1024 {
                return Err("Reading state exceeds size limit".into());
            }
            let (choices, trails, scope) =
                match serde_json::from_slice::<SavedReading>(&bytes).map_err(|e| e.to_string())? {
                    SavedReading::Current {
                        choices,
                        trails,
                        scope,
                    } => (choices, trails, scope),
                    SavedReading::Legacy(choices) => (choices, Vec::new(), Default::default()),
                };
            if choices.len() > 1024 || trails.len() > 128 {
                return Err("Too many reading projections".into());
            }
            scope.navigation.validate(|place| place.valid(), |bookmark| bookmark.validate())?;
            let mut state = Self {
                scope,
                ..Default::default()
            };
            let mut seen = std::collections::BTreeSet::new();
            for choice in choices {
                choice.preferences.validate()?;
                if !seen.insert(choice.target) {
                    return Err("Duplicate reading projection".into());
                }
                state.preferences.set(choice.target, choice.preferences);
                if let Some(anchor) = choice.anchor {
                    anchor.validate()?;
                    state.anchors.insert(choice.target, anchor);
                }
            }
            for (pane, trail) in trails {
                trail.validate()?;
                if state.trails.insert(pane, trail).is_some() {
                    return Err("Duplicate reading trail".into());
                }
            }
            Ok(state)
        })();
        match result {
            Ok(state) => state,
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
        let targets: std::collections::BTreeSet<_> = self
            .preferences
            .keys()
            .chain(self.anchors.keys())
            .copied()
            .collect();
        let choices = targets
            .into_iter()
            .map(|target| Choice {
                target,
                preferences: self.preferences(target),
                anchor: self.anchors.get(&target).cloned(),
            })
            .collect();
        let saved = SavedReading::Current {
            choices,
            trails: self
                .trails
                .iter()
                .map(|(pane, trail)| (*pane, trail.clone()))
                .collect(),
            scope: self.scope.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&saved).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let temp = root.join(format!("reading-projections.{}.tmp", std::process::id()));
        std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(temp, root.join("reading-projections.json")).map_err(|e| e.to_string())
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
                Ok(state) => {
                    cambium::find_status(query.is_empty(), false, state.count, state.current)
                },
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
            C::Appearance => {
                let ui = self.reading.find.entry(target).or_default();
                ui.appearance_open = !ui.appearance_open;
                return vec![Effect::Redraw];
            },
            C::ShowGraph => return self.reveal_reading_on_graph(target),
            C::Back | C::Forward => {
                let Some(mut trail) = self.reading.trails.get(&target.pane).cloned() else {
                    return vec![Effect::Redraw];
                };
                let Some(visit) = trail.step(command == C::Forward) else {
                    return vec![Effect::Redraw];
                };
                if self.graph_for_pane(target.pane) != Some(visit.graph)
                    || !self
                        .graph_runtimes
                        .canvas(visit.graph)
                        .is_some_and(|canvas| canvas.graph().get_node_by_id(visit.member).is_some())
                {
                    return vec![Effect::Redraw];
                }
                let anchor = trail.current_anchor().cloned();
                self.reading.trails.insert(target.pane, trail);
                let mut effects = self.read_visit_in_pane(target.pane, visit.member);
                if let Some(anchor) = anchor {
                    let destination = ReadTarget {
                        pane: target.pane,
                        member: visit.member,
                    };
                    self.reading.anchors.insert(destination, anchor.clone());
                    effects.push(Effect::RestoreReadingAnchor {
                        target: destination,
                        anchor,
                    });
                }
                effects.push(Effect::SaveReadingPreferences);
                return effects;
            },
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
    fn anchored_trails_roundtrip_per_pane_and_legacy_preferences_load() {
        let root = tempfile::tempdir().unwrap();
        let target = ReadTarget {
            pane: PaneId(7),
            member: Uuid::from_u128(1),
        };
        std::fs::write(
            root.path().join("reading-projections.json"),
            serde_json::to_vec(&vec![Choice {
                target,
                preferences: Default::default(),
                anchor: None,
            }])
            .unwrap(),
        )
        .unwrap();
        let mut state = ReadingState::load(root.path());
        assert!(state.load_error.is_none());
        let anchor = tabard::ReadingAnchor {
            source: "https://example.test/a".into(),
            revision: 42,
            text: 3,
            offset: 0,
            exact: "A retained passage".into(),
            prefix: String::new(),
            suffix: String::new(),
            line_offset: 0.0,
            fallback_y: 600.0,
        };
        state.anchors.insert(target, anchor.clone());
        let visit = ReadVisit {
            graph: crate::panes::GraphId::from_uuid(Uuid::from_u128(2)),
            member: target.member,
        };
        let trail = state.trails.entry(target.pane).or_default();
        trail.visit(visit);
        trail.anchor_current(anchor.clone());
        trail.visit(ReadVisit {
            member: Uuid::from_u128(3),
            ..visit
        });
        state.save(root.path()).unwrap();
        let mut cold = ReadingState::load(root.path());
        assert_eq!(cold.anchors.get(&target), Some(&anchor));
        assert!(!cold.trails.contains_key(&PaneId(8)));
        let trail = cold.trails.get_mut(&target.pane).unwrap();
        assert_eq!(trail.step(false), Some(visit));
        assert_eq!(trail.current_anchor(), Some(&anchor));
    }

    #[test]
    fn reading_back_restores_the_visit_in_its_owner_and_refuses_a_stale_owner() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/a".into()));
        let a = app.graph_runtimes.focused_member().unwrap();
        app.update(Action::ReadGraphMember(a));
        let pane = app.workbench_owner_pane().unwrap();
        let graph = app.graph_for_pane(pane).unwrap();
        let anchor = tabard::ReadingAnchor {
            source: "https://example.test/a".into(),
            revision: 42,
            text: 3,
            offset: 0,
            exact: "A retained passage".into(),
            prefix: String::new(),
            suffix: String::new(),
            line_offset: 0.0,
            fallback_y: 600.0,
        };
        app.reading
            .trails
            .get_mut(&pane)
            .unwrap()
            .anchor_current(anchor.clone());
        app.update(Action::OpenAddress("https://example.test/b".into()));
        let b = app.graph_runtimes.focused_member().unwrap();
        app.update(Action::ReadGraphMember(b));
        let effects = app.read_action(ReadAction::Command(
            ReadTarget { pane, member: b },
            cambium::ReadCommand::Back,
        ));
        assert_eq!(
            app.reading.trails[&pane].current(),
            Some(&ReadVisit { graph, member: a })
        );
        assert_eq!(app.reading.trails[&pane].current_anchor(), Some(&anchor));
        assert!(effects.contains(&Effect::RestoreReadingAnchor {
            target: ReadTarget { pane, member: a },
            anchor
        }));
        assert_eq!(
            app.browser_command_target().map(|(member, _)| member),
            Some(a)
        );
        let mut trail = tabard::ReadingTrail::default();
        trail.visit(ReadVisit {
            graph: crate::panes::GraphId::nil(),
            member: b,
        });
        trail.visit(ReadVisit { graph, member: a });
        app.reading.trails.insert(pane, trail.clone());
        app.read_action(ReadAction::Command(
            ReadTarget { pane, member: a },
            cambium::ReadCommand::Back,
        ));
        assert_eq!(
            app.reading.trails[&pane], trail,
            "a stale graph owner cannot move the cursor"
        );
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
