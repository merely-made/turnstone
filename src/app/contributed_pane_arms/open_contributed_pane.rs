// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use crate::action::{Effect, SpaceRef};
use crate::panes::{PaneKindId, PaneSource, PaneSpec};

use super::App;

impl App {
    pub(in super::super) fn open_contributed_pane(
        &mut self,
        kind: PaneKindId,
        source: PaneSource,
    ) -> Vec<Effect> {
        let space = self
            .active_pane
            .and_then(|pane| self.space_of(pane))
            .unwrap_or(SpaceRef::Primary);
        let existing = self.space(space).and_then(|layout| {
            let frame = layout.outer_workspace().ok()?;
            layout.iter_leaves().find_map(|(pane, content, _)| {
                if content.kind_id() != kind {
                    return None;
                }
                let spec: PaneSpec =
                    serde_json::from_value(frame.descriptors.get(&pane.0)?.clone()).ok()?;
                (spec.source == source).then_some(pane)
            })
        });
        let effects = if let Some(pane) = existing {
            self.reveal_pane(pane);
            vec![Effect::Redraw]
        } else {
            self.summon_contributed_pane(kind.clone(), source)
        };
        if let Some(pane) = self.active_pane
            && self
                .pane_content(pane)
                .is_some_and(|content| content.kind_id() == kind)
        {
            self.focus = crate::surface::FocusTarget::Pane(pane);
        }
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;

    #[test]
    fn open_personae_reveals_the_existing_local_pane() {
        let mut app = App::test_stub();
        let action = Action::SummonContributedPane {
            kind: PaneKindId::new(crate::personae_surface::PANE_KIND),
            source: crate::personae_surface::source(),
        };
        app.update(action.clone());
        let pane = app.active_pane.expect("Personae pane");
        let kind = PaneKindId::new(crate::personae_surface::PANE_KIND);
        assert_eq!(app.pane_content(pane).unwrap().kind_id(), kind);
        let frame = app
            .space(app.space_of(pane).unwrap())
            .unwrap()
            .outer_workspace()
            .unwrap();
        let spec: PaneSpec = serde_json::from_value(frame.descriptors[&pane.0].clone()).unwrap();
        assert_eq!(spec.source, crate::personae_surface::source());
        app.update(action);
        assert_eq!(app.active_pane, Some(pane));
        assert!(matches!(app.focus, crate::surface::FocusTarget::Pane(id) if id == pane));
    }
}
