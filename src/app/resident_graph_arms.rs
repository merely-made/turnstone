// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use crate::action::{Effect, SpaceRef};
use crate::panes::{PaneKindId, PaneSpec};

use super::App;

impl App {
    pub(crate) fn open_resident_graph(&mut self, domain: &str) -> Vec<Effect> {
        let kind = PaneKindId::new(crate::resident_graph::PANE_KIND);
        let source = crate::resident_graph::source(domain);
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
            self.summon_contributed_pane(kind, source)
        };
        if let Some(pane) = self.active_pane
            && self.pane_content(pane).is_some_and(|content| {
                content.kind_id().as_str() == crate::resident_graph::PANE_KIND
            })
        {
            self.focus = crate::surface::FocusTarget::Pane(pane);
        }
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::App;
    use crate::action::Action;

    #[test]
    fn reopening_a_resident_graph_reuses_the_pane_for_that_domain() {
        let mut app = App::test_stub();
        let open = |app: &mut App, domain: &str| {
            app.update(Action::OpenAddress(format!(
                "{}{domain}",
                crate::resident_graph::ADDRESS_PREFIX
            )));
            app.active_pane.unwrap()
        };
        let notes = open(&mut app, "notes");
        let field = open(&mut app, "field");
        assert_ne!(notes, field);
        assert_eq!(open(&mut app, "notes"), notes);
        assert!(matches!(app.focus, crate::surface::FocusTarget::Pane(pane) if pane == notes));
    }
}
