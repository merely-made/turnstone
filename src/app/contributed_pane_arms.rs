//! Product-neutral admission of provider-owned panes.

use crate::action::{Effect, SpaceRef};
use crate::panes::{
    GraphId, InsertSide, PaneContent, PaneId, PaneKindId, PaneNode, PaneSource, PaneSpec, SplitAxis,
};

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
        let Some(legacy) = self.space(space).cloned() else {
            return vec![Effect::Redraw];
        };
        let Some(anchor_path) = crate::pane::path_of(&legacy, anchor) else {
            return vec![Effect::Redraw];
        };
        let pane = PaneId(self.next_pane_id);
        let content = PaneContent::Registered(kind.clone());
        let mut next_legacy = legacy;
        if !next_legacy.summon_leaf(
            &anchor_path,
            InsertSide::Right,
            PaneNode::Leaf {
                pane_id: pane,
                content: content.clone(),
                graph_id: GraphId::nil(),
            },
        ) {
            return vec![Effect::Redraw];
        }

        let mut blueprint = self
            .blueprint_space(space)
            .cloned()
            .unwrap_or_else(|| crate::panes::blueprint_from_frisket(self.space(space).unwrap()));
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
        if blueprint
            .insert_tiled_beside(spec, anchor, SplitAxis::Horizontal, true)
            .is_err()
        {
            return vec![Effect::Redraw];
        }

        match space {
            SpaceRef::Primary => self.frisket = next_legacy,
            SpaceRef::Lens(ordinal) => {
                let Some(slot) = self.lenses.get_mut(ordinal) else {
                    return vec![Effect::Redraw];
                };
                let Some(slot) = slot.as_mut() else {
                    return vec![Effect::Redraw];
                };
                *slot = next_legacy;
            }
        }
        self.restore_blueprint_space(space, blueprint);
        self.next_pane_id += 1;
        self.active_pane = Some(pane);
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
        let space = app
            .blueprint_space(SpaceRef::Primary)
            .expect("summon promotes the active space");
        let spec = space.pane(pane).expect("matching durable spec");
        assert_eq!(spec.source, source);
        assert_eq!(space.tiled_panes().last(), Some(&pane));
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
            app.blueprint_space(SpaceRef::Lens(0))
                .is_some_and(|space| space.pane(pane).is_some())
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
