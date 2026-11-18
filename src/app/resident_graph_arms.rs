// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use crate::action::Effect;
use crate::panes::PaneKindId;

use super::App;

impl App {
    pub(crate) fn open_resident_graph(&mut self, domain: &str) -> Vec<Effect> {
        let kind = PaneKindId::new(crate::resident_graph::PANE_KIND);
        let source = crate::resident_graph::source(domain);
        self.open_contributed_pane(kind, source)
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
