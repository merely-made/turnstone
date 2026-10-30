// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A session's explicitly chosen note is an ordinary owned graph member.
use super::App;
use crate::action::{Action, Effect};
use crate::panes::{GraphId, PaneContent, SessionId};

pub(crate) const NOTE_FACET: &str = "turnstone.research-note.v1";

/// A captured choice, not an endpoint write grant. The shell checks the live
/// Knot session before committing this intent or offering insertion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteChoice {
    pub generation: uuid::Uuid,
    pub context_admission: uuid::Uuid,
    pub session: SessionId,
    pub graph: GraphId,
    pub member: uuid::Uuid,
    pub address: String,
}

impl NoteChoice {
    pub(crate) fn use_label(&self) -> String {
        let format = url::Url::parse(&self.address).ok().and_then(|url| {
            knot_document::DocumentFormat::from_path(std::path::Path::new(url.path()))
        });
        match format {
            Some(format) => format!(
                "Use this {} document as research note",
                knot_document::knot_document_format_label(format)
            ),
            None => "Use this document as research note".into(),
        }
    }
}

impl App {
    pub(crate) fn note_choice(&self, member: uuid::Uuid) -> Option<NoteChoice> {
        let graph = self.graph_runtimes.graph_containing_member(member)?;
        let runtime = self.graph_runtimes.get(graph)?;
        if runtime.session != Some(self.session_id) {
            return None;
        }
        let (_, node) = runtime.canvas.graph().get_node_by_id(member)?;
        Some(NoteChoice {
            generation: self.source_capture_generation,
            context_admission: self.pane_context.admission(),
            session: self.session_id,
            graph,
            member,
            address: node.url().to_string(),
        })
    }

    pub(crate) fn research_note(&self) -> Result<Option<NoteChoice>, String> {
        let mut notes = Vec::new();
        for runtime in self
            .graph_runtimes
            .iter()
            .filter(|r| r.session == Some(self.session_id))
        {
            for (_, node) in runtime.canvas.graph().nodes() {
                if let Some(value) = runtime
                    .canvas
                    .facets()
                    .get(&node.id, &chartulary::FacetId::new(NOTE_FACET))
                {
                    let address = value
                        .as_str()
                        .ok_or("The research note binding is unreadable; choose a note again.")?;
                    if node.url() != address {
                        return Err("The chosen note member now names another document; choose a note again.".into());
                    }
                    if let Some(note) = self.note_choice(node.id) {
                        notes.push(note);
                    }
                }
            }
        }
        match notes.len() {
            0 => Ok(None),
            1 => Ok(notes.pop()),
            _ => Err("This session has several research note choices; choose a note again.".into()),
        }
    }

    pub(crate) fn offered_research_note(&self) -> Option<NoteChoice> {
        let (member, address) = self.browser_command_target()?;
        if !self
            .content
            .facts(member)
            .is_some_and(|facts| facts.engine == crate::knot_authoring::ENGINE_ID)
            || !self
                .knot_documents()
                .iter()
                .any(|doc| doc.address == address && doc.status == "local")
        {
            return None;
        }
        self.note_choice(member)
    }

    pub(crate) fn commit_research_note(&mut self, choice: NoteChoice) -> Vec<Effect> {
        if self.note_choice(choice.member).as_ref() != Some(&choice) {
            return self.saved_source_refusal(
                "The research note choice belongs to a stale session or member.".into(),
            );
        }
        let graphs: Vec<_> = self
            .graph_runtimes
            .iter()
            .filter(|r| r.session == Some(self.session_id))
            .map(|r| r.graph)
            .collect();
        for graph in graphs {
            let canvas = self.graph_runtimes.canvas_mut(graph).unwrap();
            let members: Vec<_> = canvas.graph().nodes().map(|(_, n)| n.id).collect();
            if let Err(error) = crate::graph_facets::edit_canvas_facets(canvas, |facets| {
                for member in members {
                    facets.remove(&member, &chartulary::FacetId::new(NOTE_FACET));
                }
            }) {
                return self.saved_source_refusal(format!("Research note choice failed: {error}"));
            }
        }
        let Some(canvas) = self.graph_runtimes.canvas_mut(choice.graph) else {
            return vec![];
        };
        if let Err(error) = crate::graph_facets::edit_canvas_facets(canvas, |facets| {
            facets.set(
                choice.member,
                chartulary::FacetId::new(NOTE_FACET),
                serde_json::Value::String(choice.address.clone()),
                &chartulary::AcceptAll,
            )
        }) {
            return self.saved_source_refusal(format!("Research note choice failed: {error}"));
        }
        self.keep_node(choice.member);
        self.pane_context.invalidate();
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(super) fn research_note_created(
        &mut self,
        generation: uuid::Uuid,
        address: String,
    ) -> Vec<Effect> {
        if generation != self.source_capture_generation {
            return vec![];
        }
        let pane = self
            .focused_graph_pane()
            .unwrap_or_else(|| self.default_graph_pane());
        self.reveal_pane(pane);
        self.focus = crate::surface::FocusTarget::Graph(pane);
        let mut effects = self.open_address(address.clone());
        if let Some((member, opened)) = self.browser_command_target()
            && opened == address
            && let Some(choice) = self.note_choice(member)
        {
            effects.extend(self.commit_research_note(choice));
            if self.content.flip_spawns(member) {
                effects.extend(self.toggle_node_content());
            }
        }
        effects
    }

    pub(super) fn open_research_note(&mut self) -> Vec<Effect> {
        let note = match self.research_note() {
            Ok(Some(note)) => note,
            Ok(None) => {
                return self.saved_source_refusal("Create or choose a research note first.".into());
            },
            Err(reason) => return self.saved_source_refusal(reason),
        };
        let pane = self.frisket.iter_leaves().find_map(|(pane, content, _)| {
            (matches!(content, PaneContent::Orrery | PaneContent::Workbench)
                && self.graph_for_pane(pane) == Some(note.graph))
            .then_some(pane)
        });
        let Some(pane) = pane else {
            return self.saved_source_refusal(
                "Reveal the graph that owns the chosen research note first.".into(),
            );
        };
        self.reveal_pane(pane);
        self.focus = crate::surface::FocusTarget::Graph(pane);
        self.graph_pane_select_member(pane, note.member);
        self.publish_member_context(pane, Some(note.member));
        // Opening the owned member uses the ordinary address/content path.
        let mut effects = self.update(Action::OpenAddress(note.address));
        if self.content.flip_spawns(note.member) {
            effects.extend(self.toggle_node_content());
        }
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn research_note_choice_is_owned_address_bound_and_rejects_stale_generation() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("file:///notes/A.knot".into()));
        let first = app.graph_runtimes.focused_member().unwrap();
        let a = app.note_choice(first).unwrap();
        assert!(
            app.commit_research_note(a.clone())
                .contains(&Effect::SaveSession)
        );
        app.update(Action::OpenAddress("file:///notes/B.knot".into()));
        let second = app.graph_runtimes.focused_member().unwrap();
        let b = app.note_choice(second).unwrap();
        app.commit_research_note(b.clone());
        assert_eq!(app.research_note().unwrap().unwrap().member, b.member);
        let mut stale = a.clone();
        stale.generation = uuid::Uuid::new_v4();
        app.commit_research_note(stale);
        assert_eq!(app.research_note().unwrap().unwrap().member, b.member);
        let graph = app.graph_runtimes.canvas_mut(b.graph).unwrap();
        crate::graph_facets::edit_canvas_facets(graph, |facets| {
            facets.set(
                b.member,
                chartulary::FacetId::new(NOTE_FACET),
                serde_json::Value::String(a.address.clone()),
                &chartulary::AcceptAll,
            )
        })
        .unwrap()
        .unwrap();
        assert!(
            app.research_note().is_err(),
            "retargeting the chosen member cannot redirect future clips"
        );
        let b = app.note_choice(second).unwrap();
        app.commit_research_note(b.clone());
        app.source_capture_generation = uuid::Uuid::new_v4();
        app.commit_research_note(a);
        assert_eq!(app.research_note().unwrap().unwrap().member, second);
        app.session_id = SessionId::new();
        assert!(
            app.research_note().unwrap().is_none(),
            "a different session cannot borrow the note"
        );
    }
}
