use super::*;

impl App {
    pub(crate) fn scope_label(&self, place: &ScopePlace<GraphId, Uuid>) -> String {
        let member = place.focus.and_then(|id| {
            self.graph_runtimes.canvas(place.graph).and_then(|c| {
                c.graph()
                    .get_node_by_id(id)
                    .map(|(k, _)| c.graph().node_display_label(k))
            })
        });
        match place.level {
            ScopeLevel::Document => member.unwrap_or_else(|| "Document".into()),
            ScopeLevel::Neighborhood => format!(
                "Neighborhood of {}",
                place
                    .root
                    .and_then(|id| self.graph_runtimes.canvas(place.graph).and_then(|c| c
                        .graph()
                        .get_node_by_id(id)
                        .map(|(k, _)| c.graph().node_display_label(k))))
                    .unwrap_or_else(|| "document".into())
            ),
            ScopeLevel::Graph => self
                .sessions
                .iter()
                .find(|(_, m)| m.root_graph_id == place.graph)
                .map(|(id, _)| self.session_label(id))
                .unwrap_or_else(|| "Document graph".into()),
            ScopeLevel::Overmap => "Meres".into(),
        }
    }
    pub(crate) fn scope_controls(&self) -> ScopeControls {
        let current = self.scope_place();
        let nav = &self.reading.scope.navigation;
        let native = current.level == ScopeLevel::Document
            && current.focus.is_some_and(|m| self.node_uses_web_surface(m));
        ScopeControls {
            back: if native && self.focused_can_back() {
                Some("Previous page".into())
            } else {
                nav.destination(false)
                    .map(|v| self.scope_label(&v.place))
                    .or_else(|| self.focused_can_back().then(|| "Previous page".into()))
            },
            forward: if native && self.focused_can_forward() {
                Some("Next page".into())
            } else {
                nav.destination(true)
                    .map(|v| self.scope_label(&v.place))
                    .or_else(|| self.focused_can_forward().then(|| "Next page".into()))
            },
            up: current.up().map(|p| self.scope_label(&p)),
            down: current.down().map(|p| self.scope_label(&p)),
            hint: self.reading.scope.hint,
        }
    }
    pub(crate) fn scope_crumbs(&self) -> Vec<ScopeCrumb> {
        let place = self.scope_place();
        [
            ScopeLevel::Overmap,
            ScopeLevel::Graph,
            ScopeLevel::Neighborhood,
            ScopeLevel::Document,
        ]
        .into_iter()
        .filter(|l| *l as u8 >= place.level as u8)
        .map(|level| {
            let label = if level == ScopeLevel::Neighborhood {
                "Neighborhood".into()
            } else {
                self.scope_label(&ScopePlace {
                    level,
                    ..place.clone()
                })
            };
            ScopeCrumb {
                label,
                level,
                current: level == place.level,
            }
        })
        .collect()
    }
}
