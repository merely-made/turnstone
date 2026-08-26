use crate::action::{DocumentFindDirection, DocumentFindModel, Effect};

use super::App;

impl App {
    pub(super) fn open_document_find(&mut self) -> Vec<Effect> {
        let Some(target) = self.graph_runtimes.focused_member() else {
            return vec![Effect::Redraw];
        };
        if !matches!(
            self.content.get(target),
            Some(crate::content::NodeContent::Live)
        ) {
            return vec![Effect::Redraw];
        }
        let mut effects = if self.omnibar.open {
            self.close_omnibar()
        } else {
            Vec::new()
        };
        // Action-row commits clear the visible omnibar before lowering the
        // chosen Action. Clear its captured shell context here as well.
        self.shell.close_omnibar();
        if self.document_find.target != Some(target) {
            if let Some(previous) = self.document_find.target {
                effects.push(Effect::ClearContentFind { node: previous });
            }
            let request = self.document_find.request;
            self.document_find = crate::document_find::DocumentFindState {
                request,
                ..Default::default()
            };
        }
        self.document_find.open = true;
        self.document_find.target = Some(target);
        effects.push(Effect::Redraw);
        effects
    }

    pub(super) fn close_document_find(&mut self) -> Vec<Effect> {
        let target = self.document_find.target;
        self.document_find.request = self.document_find.request.wrapping_add(1).max(1);
        self.document_find.open = false;
        self.document_find.target = None;
        self.document_find.pending = false;
        self.document_find.model = DocumentFindModel::default();
        self.document_find.error = None;
        let mut effects = target
            .map(|node| Effect::ClearContentFind { node })
            .into_iter()
            .collect::<Vec<_>>();
        effects.push(Effect::Redraw);
        effects
    }

    pub(super) fn insert_document_find(&mut self, text: String) -> Vec<Effect> {
        if !self.document_find.open || text.is_empty() {
            return Vec::new();
        }
        self.document_find.query.push_str(&text);
        self.replace_document_find()
    }

    pub(super) fn backspace_document_find(&mut self) -> Vec<Effect> {
        if !self.document_find.open || self.document_find.query.pop().is_none() {
            return Vec::new();
        }
        self.replace_document_find()
    }

    fn replace_document_find(&mut self) -> Vec<Effect> {
        let Some(node) = self.document_find.target else {
            return vec![Effect::Redraw];
        };
        self.document_find.request = self.document_find.request.wrapping_add(1).max(1);
        self.document_find.pending = !self.document_find.query.is_empty();
        self.document_find.model = DocumentFindModel::default();
        self.document_find.error = None;
        if self.document_find.query.is_empty() {
            return vec![Effect::ClearContentFind { node }, Effect::Redraw];
        }
        vec![
            Effect::FindContent {
                node,
                request: self.document_find.request,
                query: self.document_find.query.clone(),
                direction: DocumentFindDirection::Next,
                find_next: false,
            },
            Effect::Redraw,
        ]
    }

    pub(super) fn step_document_find(&mut self, direction: DocumentFindDirection) -> Vec<Effect> {
        let Some(node) = self.document_find.target else {
            return Vec::new();
        };
        if !self.document_find.open || self.document_find.query.is_empty() {
            return Vec::new();
        }
        self.document_find.request = self.document_find.request.wrapping_add(1).max(1);
        self.document_find.pending = true;
        self.document_find.error = None;
        vec![Effect::FindContent {
            node,
            request: self.document_find.request,
            query: self.document_find.query.clone(),
            direction,
            find_next: true,
        }]
    }

    pub(super) fn apply_document_find_changed(
        &mut self,
        node: uuid::Uuid,
        request: u64,
        query: String,
        result: Result<DocumentFindModel, String>,
    ) -> Vec<Effect> {
        if !self.document_find.open
            || self.document_find.target != Some(node)
            || self.document_find.request != request
            || self.document_find.query != query
        {
            return Vec::new();
        }
        self.document_find.pending = result.as_ref().is_ok_and(|model| !model.complete);
        match result {
            Ok(model) => {
                self.document_find.model = model;
                self.document_find.error = None;
            }
            Err(error) => {
                self.document_find.model = DocumentFindModel::default();
                self.document_find.error = Some(error);
            }
        }
        vec![Effect::Redraw]
    }

    pub(crate) fn invalidate_document_find_for(&mut self, node: uuid::Uuid) -> Vec<Effect> {
        if self.document_find.target == Some(node) {
            self.close_document_find()
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Action, DocumentFindMatch, Update};

    #[test]
    fn captured_target_and_request_drop_late_find_results() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("gemini://example.test/find".into()));
        let node = app.graph_runtimes.focused_member().expect("focused member");
        app.content.note_live(node, None);

        app.update(Action::OpenDocumentFind);
        let effects = app.update(Action::InsertDocumentFind("needle".into()));
        let (request, query) = effects
            .iter()
            .find_map(|effect| match effect {
                Effect::FindContent { request, query, .. } => Some((*request, query.clone())),
                _ => None,
            })
            .expect("typed find effect");
        app.update(Action::InsertDocumentFind("s".into()));

        let effects = app.apply_update(Update::DocumentFindChanged {
            node,
            request,
            query,
            result: Ok(DocumentFindModel {
                count: 1,
                matches: vec![DocumentFindMatch {
                    label: "late".into(),
                    role: Some("paragraph".into()),
                }],
                current: Some(0),
                complete: true,
            }),
        });
        assert!(effects.is_empty());
        assert!(app.document_find.model.matches.is_empty());

        let effects = app.invalidate_document_find_for(node);
        assert!(effects.contains(&Effect::ClearContentFind { node }));
        assert!(!app.document_find.open);
    }

    #[test]
    fn current_match_is_observable_and_accessible() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("gemini://example.test/find".into()));
        let node = app.graph_runtimes.focused_member().expect("focused member");
        app.content.note_live(node, None);
        app.update(Action::OpenDocumentFind);
        let effects = app.update(Action::InsertDocumentFind("needle".into()));
        let request = effects
            .iter()
            .find_map(|effect| match effect {
                Effect::FindContent { request, .. } => Some(*request),
                _ => None,
            })
            .expect("find request");
        app.apply_update(Update::DocumentFindChanged {
            node,
            request,
            query: "needle".into(),
            result: Ok(DocumentFindModel {
                count: 2,
                matches: vec![
                    DocumentFindMatch {
                        label: "Needle heading".into(),
                        role: Some("heading".into()),
                    },
                    DocumentFindMatch {
                        label: "needle paragraph".into(),
                        role: Some("paragraph".into()),
                    },
                ],
                current: Some(0),
                complete: true,
            }),
        });

        let view = crate::observe::snapshot(&app)
            .document_find
            .expect("find is observed");
        assert_eq!(view.target, node);
        assert_eq!(view.count, 2);
        assert_eq!(view.current, Some(0));
        assert_eq!(view.current_role.as_deref(), Some("heading"));
        assert_eq!(view.current_label.as_deref(), Some("Needle heading"));
        assert_eq!(view.status, "1 of 2 · heading: Needle heading");
        let lines = crate::a11y::a11y_lines(&app);
        assert!(
            lines
                .iter()
                .any(|line| line == "searchinput: Find in document = needle")
        );
        assert!(
            lines
                .iter()
                .any(|line| { line == "status: 1 of 2 · heading: Needle heading" })
        );
    }

    #[test]
    fn engine_managed_count_is_observed_without_synthetic_matches() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/find".into()));
        let node = app.graph_runtimes.focused_member().expect("focused member");
        app.content.note_live(node, None);
        app.update(Action::OpenDocumentFind);
        let effects = app.update(Action::InsertDocumentFind("needle".into()));
        let request = effects
            .iter()
            .find_map(|effect| match effect {
                Effect::FindContent { request, .. } => Some(*request),
                _ => None,
            })
            .expect("find request");
        app.apply_update(Update::DocumentFindChanged {
            node,
            request,
            query: "needle".into(),
            result: Ok(DocumentFindModel {
                count: 3,
                matches: Vec::new(),
                current: Some(1),
                complete: true,
            }),
        });

        let view = crate::observe::snapshot(&app)
            .document_find
            .expect("find is observed");
        assert_eq!(view.count, 3);
        assert_eq!(view.current, Some(1));
        assert_eq!(view.status, "2 of 3");
        assert_eq!(view.current_role, None);
        assert_eq!(view.current_label, None);
    }
}
