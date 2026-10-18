// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Product-neutral admission of provider-owned panes.

use crate::action::{Effect, SpaceRef};
use crate::panes::{GraphId, PaneContent, PaneId, PaneKindId, PaneSource, PaneSpec};

use super::App;

impl App {
    pub(super) fn summon_contributed_pane(
        &mut self,
        kind: PaneKindId,
        source: PaneSource,
    ) -> Vec<Effect> {
        let (space, anchor) = match self
            .active_pane
            .and_then(|active| self.space_of(active).map(|space| (space, active)))
        {
            Some((space, active)) => (space, Some(active)),
            None => (
                SpaceRef::Primary,
                self.frisket
                    .iter_leaves()
                    .find(|(_, content, _)| matches!(content, PaneContent::Orrery))
                    .map(|(id, _, _)| id),
            ),
        };
        let Some(anchor) = anchor else {
            return vec![Effect::Redraw];
        };
        if let Err(reason) = self.ensure_shared_space(space) {
            self.events
                .push(crate::observe::AppEvent::InteractionMissed {
                    what: "summon-pane",
                    target: reason,
                });
            return vec![Effect::Redraw];
        }
        let pane = PaneId(self.next_pane_id);
        let content = PaneContent::Registered(kind.clone());
        let spec = PaneSpec {
            id: pane,
            kind,
            source,
            context: crate::panes::ContextBinding::Own,
            config: crate::panes::PaneConfig::empty(format!(
                "{}.config",
                content.kind_id().as_str()
            )),
        };
        let Ok(descriptor) = serde_json::to_value(spec) else {
            return vec![Effect::Redraw];
        };
        let Some(frame) = self
            .space_mut(space)
            .and_then(|layout| layout.workspace_mut())
        else {
            return vec![Effect::Redraw];
        };
        let target = if frame
            .workspace
            .tiled()
            .find(workbench::TileId(anchor.0))
            .is_some()
        {
            anchor.0
        } else {
            frame
                .workspace
                .tiled()
                .tiles()
                .last()
                .map(|t| t.id.0)
                .unwrap_or(anchor.0)
        };
        if !frame.insert_beside(
            target,
            workbench::Edge::Right,
            pane.0,
            content.tag().into(),
            (content, GraphId::nil()),
        ) {
            return vec![Effect::Redraw];
        }
        frame.descriptors.insert(pane.0, descriptor);
        self.next_pane_id += 1;
        self.reveal_pane(pane);
        self.index_pane_spaces();
        vec![Effect::SaveSession, Effect::Redraw]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;

    fn external_source(tag: &str) -> PaneSource {
        PaneSource::Fixed(crate::panes::SourceRef::External {
            schema: crate::panes::SourceSchemaId::new("test.source"),
            payload: crate::panes::SerializedSource {
                version: 1,
                payload: serde_json::json!({ "tag": tag }),
            },
        })
    }

    #[test]
    fn contributed_summon_preserves_source_and_mints_one_identity() {
        let mut app = App::test_stub();
        let source = external_source("alpha");
        let previous = app.default_graph_pane();
        let effects = app.update(Action::SummonContributedPane {
            kind: PaneKindId::new("test.contributed"),
            source: source.clone(),
        });
        let pane = app.active_pane.expect("contributed pane is active");

        assert_ne!(pane, previous);
        assert_eq!(effects, vec![Effect::SaveSession, Effect::Redraw]);
        assert!(matches!(
            app.pane_content(pane),
            Some(PaneContent::Registered(kind)) if kind.as_str() == "test.contributed"
        ));
        assert!(app.blueprint_space(SpaceRef::Primary).is_none());
        let frame = app.frisket.outer_workspace().unwrap();
        let spec: PaneSpec = serde_json::from_value(frame.descriptors[&pane.0].clone()).unwrap();
        assert_eq!(spec.source, source);
        assert_eq!(
            frame.workspace.tiled().tiles().last().map(|t| t.id.0),
            Some(pane.0)
        );
    }

    #[test]
    fn knot_file_choices_are_palette_actions_lowered_to_shell_effects() {
        let rows = crate::action::palette_actions();
        assert!(rows.iter().any(|(label, action)| {
            label == "Open Knot document"
                && matches!(action, Action::ChooseKnotDocumentFile { read_only: false })
        }));
        assert!(rows.iter().any(|(label, action)| {
            label == "Open Knot document read-only"
                && matches!(action, Action::ChooseKnotDocumentFile { read_only: true })
        }));

        let mut app = App::test_stub();
        assert_eq!(
            app.update(Action::ChooseKnotDocumentFile { read_only: true }),
            vec![Effect::ChooseKnotDocumentFile { read_only: true }]
        );
    }

    #[test]
    fn contributed_summon_targets_the_active_lens() {
        let mut app = App::test_stub();
        app.update(Action::NewWindow);
        let lens_pane = app.lenses[0]
            .as_ref()
            .and_then(|lens| lens.iter_leaves().next())
            .map(|(id, _, _)| id)
            .expect("new lens has a pane");
        app.active_pane = Some(lens_pane);
        assert_eq!(
            app.space_of(app.active_pane.unwrap()),
            Some(SpaceRef::Lens(0))
        );

        app.update(Action::SummonContributedPane {
            kind: PaneKindId::new("test.lens-contributed"),
            source: external_source("lens"),
        });
        let pane = app.active_pane.expect("lens contribution is active");
        assert_eq!(app.space_of(pane), Some(SpaceRef::Lens(0)));
        assert!(
            app.space(SpaceRef::Lens(0))
                .and_then(|s| s.outer_workspace().ok())
                .is_some_and(|frame| frame.descriptors.contains_key(&pane.0))
        );
    }

    #[test]
    fn contributed_summon_rolls_back_when_blueprint_cannot_place_target() {
        let mut app = App::test_stub();
        let before_legacy = app.frisket.clone();
        let before_active = app.active_pane;
        let before_next = app.next_pane_id;
        let mut invalid = crate::panes::blueprint_from_frisket(&app.frisket);
        invalid.panes.clear();
        invalid.tiled = None;
        app.primary_blueprint = Some(invalid.clone());

        let effects = app.update(Action::SummonContributedPane {
            kind: PaneKindId::new("test.rollback"),
            source: external_source("rollback"),
        });

        assert_eq!(effects, vec![Effect::Redraw]);
        assert_eq!(app.frisket, before_legacy);
        assert_eq!(app.active_pane, before_active);
        assert_eq!(app.next_pane_id, before_next);
        assert_eq!(app.primary_blueprint, Some(invalid));
    }
}

impl App {
    pub(super) fn open_answers(&mut self) -> Vec<Effect> {
        // Re-enter the existing research view, retaining its exact choices.
        // Explicit generic pane creation still supports separate question views.
        let mut existing = Vec::new();
        for layout in std::iter::once(&self.frisket).chain(self.lenses.iter().flatten()) {
            let Ok(frame) = layout.outer_workspace() else {
                continue;
            };
            for (pane, content, _) in layout.iter_leaves() {
                if content.kind_id().as_str() != crate::inference_surface::PANE_KIND {
                    continue;
                }
                let Some(value) = frame.descriptors.get(&pane.0) else {
                    continue;
                };
                let Ok(spec) = serde_json::from_value::<PaneSpec>(value.clone()) else {
                    continue;
                };
                let PaneSource::Fixed(crate::panes::SourceRef::External { schema, payload }) =
                    spec.source
                else {
                    continue;
                };
                if schema.as_str() == crate::inference_surface::SOURCE_SCHEMA
                    && payload.version == 2
                    && serde_json::from_value::<crate::inference_surface::AnswerSourceV2>(
                        payload.payload,
                    )
                    .is_ok_and(|source| source.session == self.session_id.0)
                {
                    existing.push(pane);
                }
            }
        }
        if let Some(pane) = self
            .active_pane
            .filter(|pane| existing.contains(pane))
            .or_else(|| existing.first().copied())
        {
            self.reveal_pane(pane);
            self.focus = crate::surface::FocusTarget::Pane(pane);
            return vec![Effect::Redraw];
        }
        let pane = PaneId(self.next_pane_id);
        let effects = self.summon_contributed_pane(
            PaneKindId::new(crate::inference_surface::PANE_KIND),
            crate::inference_surface::answer_source(self.session_id.0),
        );
        if self.active_pane == Some(pane) {
            self.focus = crate::surface::FocusTarget::Pane(pane);
        }
        effects
    }
    pub(crate) fn sync_research_context(&self) {
        self.research.project(crate::research::Context {
            session: self.session_id,
            generation: self.source_capture_generation,
            admissions: self.saved_source_admissions(),
        });
    }
    pub(crate) fn set_answer_selection(
        &mut self,
        generation: uuid::Uuid,
        expected: crate::inference_surface::AnswerSourceV2,
        mut chosen: Vec<crate::saved_sources::Target>,
    ) -> Vec<Effect> {
        if generation != self.source_capture_generation
            || expected.session != *self.session_id.as_uuid()
            || chosen.iter().any(|target| {
                // Existing stale choices may remain while the person removes
                // them. Only additions need current admission; every ask still
                // validates the entire selected scope against exact content.
                !expected.chosen.contains(target)
                    && (target.selector.is_some() || !self.saved_source_admitted(&target.key))
            })
        {
            return self.saved_source_refusal(
                "The answer selection belongs to a stale view or unavailable saved source.".into(),
            );
        }
        let mut seen = std::collections::HashSet::new();
        chosen.retain(|target| seen.insert(target.clone()));
        let old = crate::inference_surface::answer_source_value(expected.clone());
        let next = crate::inference_surface::answer_source_value(
            crate::inference_surface::AnswerSourceV2 { chosen, ..expected },
        );
        let mut changed = false;
        for layout in std::iter::once(&mut self.frisket).chain(self.lenses.iter_mut().flatten()) {
            if let Some(frame) = layout.workspace_mut() {
                for value in frame.descriptors.values_mut() {
                    if let Ok(mut spec) = serde_json::from_value::<PaneSpec>(value.clone())
                        && spec.kind.as_str() == crate::inference_surface::PANE_KIND
                        && spec.source == old
                    {
                        spec.source = next.clone();
                        *value = serde_json::to_value(spec).expect("answer pane serialization");
                        changed = true;
                    }
                }
            }
        }
        if changed {
            vec![Effect::SaveSession, Effect::Redraw]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod research_view_tests {
    use super::*;
    use crate::action::Action;
    fn source(app: &App, pane: PaneId) -> crate::inference_surface::AnswerSourceV2 {
        let frame = app.frisket.outer_workspace().unwrap();
        let spec: PaneSpec = serde_json::from_value(frame.descriptors[&pane.0].clone()).unwrap();
        let PaneSource::Fixed(crate::panes::SourceRef::External { payload, .. }) = spec.source
        else {
            panic!("external source")
        };
        assert_eq!(payload.version, 2);
        serde_json::from_value(payload.payload).unwrap()
    }
    #[test]
    fn reopening_answers_reuses_the_saved_view_and_reveals_it_from_a_maximized_reader() {
        let mut app = App::test_stub();
        app.update(Action::OpenAnswers);
        let answers = app.active_pane.unwrap();
        assert_eq!(app.focus, crate::surface::FocusTarget::Pane(answers));
        let original = source(&app, answers);
        let count = app.frisket.iter_leaves().count();
        let graph = app.default_graph_pane();
        app.active_pane = Some(graph);
        app.focus = crate::surface::FocusTarget::Graph(graph);
        app.maximized = Some(graph);
        assert_eq!(app.update(Action::OpenAnswers), vec![Effect::Redraw]);
        assert_eq!(app.active_pane, Some(answers));
        assert_eq!(app.focus, crate::surface::FocusTarget::Pane(answers));
        assert_eq!(app.maximized, Some(answers));
        assert_eq!(source(&app, answers), original);
        assert_eq!(app.frisket.iter_leaves().count(), count);
        // The same lookup works after ordinary frame serialization/reopening.
        app.frisket = serde_json::from_slice(&serde_json::to_vec(&app.frisket).unwrap()).unwrap();
        app.active_pane = Some(graph);
        app.update(Action::OpenAnswers);
        assert_eq!(app.active_pane, Some(answers));
        assert_eq!(source(&app, answers), original);
        assert_eq!(app.frisket.iter_leaves().count(), count);
    }

    #[test]
    fn detached_choices_can_be_removed_incrementally_but_cannot_be_reintroduced() {
        let mut app = App::test_stub();
        for name in ["A", "B"] {
            let key = app
                .graph_runtimes
                .visit(&format!("https://gone.test/{name}"));
            let node = app.graph_runtimes.graph().get_node(key).unwrap().id;
            crate::source_capture::append_reference(
                app.graph_runtimes.facets_mut(),
                node,
                eidetic::ManifestId::of_blob(name.as_bytes()).to_string(),
            )
            .unwrap();
        }
        let chosen: Vec<_> = app
            .saved_source_admissions()
            .into_iter()
            .map(|admission| crate::saved_sources::Target {
                key: admission.key,
                text_hash: eidetic::Hash::of(b"saved text").to_string(),
                selector: None,
            })
            .collect();
        assert_eq!(chosen.len(), 2);
        app.update(Action::OpenAnswers);
        let pane = app.active_pane.unwrap();
        let generation = app.source_capture_generation;
        let select = |expected, chosen| Action::SetAnswerSelection {
            generation,
            expected,
            chosen,
        };
        app.update(select(source(&app, pane), chosen.clone()));
        let both = source(&app, pane);
        for target in &chosen {
            app.graph_runtimes.facets_mut().remove(
                &target.key.node,
                &chartulary::FacetId::new(crate::source_capture::SOURCE_CAPTURE_REFERENCES_FACET),
            );
        }
        assert!(app.saved_source_admissions().is_empty());
        let effects = app.update(select(both.clone(), vec![chosen[1].clone()]));
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::SaveSession))
        );
        let remaining = source(&app, pane);
        assert_eq!(remaining.chosen, vec![chosen[1].clone()]);
        assert!(
            crate::research::scope(
                &crate::research::namespace(app.session_id),
                &remaining.chosen,
                &[]
            )
            .is_err(),
            "editing stale choices never admits their bytes for an ask"
        );
        // An old control cannot erase a newer view, and retaining a stale ID
        // does not grant authority to add a detached ID back into the view.
        app.update(select(both, vec![]));
        assert_eq!(source(&app, pane), remaining);
        app.update(select(remaining.clone(), chosen));
        assert_eq!(source(&app, pane), remaining);
        app.update(select(remaining, vec![]));
        assert!(source(&app, pane).chosen.is_empty());
    }

    #[test]
    fn chosen_capture_intent_persists_in_the_pane_and_stale_controls_cannot_edit_it() {
        let mut app = App::test_stub();
        let key = app.graph_runtimes.visit("https://gone.test/A");
        let node = app.graph_runtimes.graph().get_node(key).unwrap().id;
        crate::source_capture::append_reference(
            app.graph_runtimes.facets_mut(),
            node,
            eidetic::ManifestId::of_blob(b"A1").to_string(),
        )
        .unwrap();
        app.update(Action::OpenAnswers);
        let pane = app.active_pane.unwrap();
        let initial = source(&app, pane);
        assert!(initial.chosen.is_empty());
        let target = crate::saved_sources::Target {
            key: app.saved_source_admissions()[0].key.clone(),
            text_hash: eidetic::Hash::of(b"saved text").to_string(),
            selector: None,
        };
        let generation = app.source_capture_generation;
        let select = |expected, chosen| Action::SetAnswerSelection {
            generation,
            expected,
            chosen,
        };
        let effects = app.update(select(initial.clone(), vec![target.clone()]));
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::SaveSession))
        );
        let chosen = source(&app, pane);
        assert_eq!(chosen.chosen, vec![target.clone()]);
        // frame.json's ordinary serialized pane authority carries identities only.
        let serialized = serde_json::to_vec(&app.frisket).unwrap();
        let reopened: crate::panes::FrisketLayout = serde_json::from_slice(&serialized).unwrap();
        let frame = reopened.outer_workspace().unwrap();
        let spec: PaneSpec = serde_json::from_value(frame.descriptors[&pane.0].clone()).unwrap();
        assert_eq!(
            spec.source,
            crate::inference_surface::answer_source_value(chosen.clone())
        );
        assert!(
            !String::from_utf8(serialized)
                .unwrap()
                .contains("saved text")
        );
        assert!(
            app.update(select(initial, vec![])).is_empty(),
            "an old UI control cannot replace a newer choice"
        );
        assert_eq!(source(&app, pane), chosen);
        app.update(Action::SetAnswerSelection {
            generation: uuid::Uuid::new_v4(),
            expected: chosen.clone(),
            chosen: vec![],
        });
        assert_eq!(source(&app, pane), chosen);
        let mut foreign = target;
        foreign.key.session = crate::panes::SessionId::new();
        app.update(select(chosen.clone(), vec![foreign]));
        assert_eq!(source(&app, pane), chosen);
        app.update(select(chosen, vec![]));
        assert!(
            source(&app, pane).chosen.is_empty(),
            "clearing scope persists without selecting every capture"
        );
    }
}
