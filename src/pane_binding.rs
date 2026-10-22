// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Visible source bindings. Workspace PaneSpec descriptors own the durable
//! choice; ContextIndex resolves live publishers; Cambium only presents it.
use crate::{
    action::Effect,
    app::App,
    panes::{
        ContextBinding, GraphId, PaneConfig, PaneContent, PaneContext, PaneId, PaneSource,
        PaneSpec, SourceRef, SourceSelector,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingChoice {
    FollowActive,
    FollowPane(PaneId),
    Pin {
        graph: GraphId,
        member: Option<uuid::Uuid>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindingIntent {
    pub pane: PaneId,
    pub admission: uuid::Uuid,
    pub choice: BindingChoice,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BindingSnapshot {
    pub mode: String,
    pub source: String,
    pub available: bool,
    pub choices: Vec<(String, BindingIntent, bool)>,
}

#[derive(Default)]
pub(crate) struct BindingViewState {
    snapshot: Option<BindingSnapshot>,
    pub(crate) expanded: bool,
}

impl BindingViewState {
    pub(crate) fn sync(&mut self, app: &App, pane: PaneId) {
        self.snapshot = app.pane_binding_snapshot(pane);
    }
}

pub(crate) fn binding_view<S: 'static, A: 'static>(
    state: &BindingViewState,
    access: fn(&mut S) -> &mut BindingViewState,
    emit: fn(BindingIntent) -> A,
) -> cambium::ShellSlot<S, A>
where
    A: cambium::Action,
{
    let Some(snapshot) = &state.snapshot else {
        return Box::new(cambium::el("div", ()));
    };
    let choices = snapshot
        .choices
        .iter()
        .map(|(label, intent, selected)| {
            let intent = *intent;
            cambium::shell_control(
                label.clone(),
                label.clone(),
                cambium::ShellControl {
                    pressed: Some(*selected),
                    ..Default::default()
                },
                move |state: &mut S, _| {
                    access(state).expanded = false;
                    emit(intent)
                },
            )
        })
        .collect();
    cambium::shell_source_binding(
        snapshot.mode.clone(),
        snapshot.source.clone(),
        snapshot.available,
        state.expanded,
        choices,
        move |state: &mut S, _| {
            let view = access(state);
            view.expanded = !view.expanded;
        },
    )
}

impl App {
    pub(crate) fn has_pane_binding(&self, pane: PaneId) -> bool {
        matches!(
            self.pane_content(pane),
            Some(PaneContent::Inspector | PaneContent::Roster)
        )
    }

    pub(crate) fn pane_binding_spec(&self, pane: PaneId) -> Option<PaneSpec> {
        if !self.has_pane_binding(pane) {
            return None;
        }
        let space = self.space_of(pane)?;
        if let Some(spec) = self
            .blueprint_space(space)
            .and_then(|space| space.pane(pane))
        {
            // Older station imports synthesized fixed graph sources for
            // Inspector before member bindings were live. Keep its historical
            // following behavior when that unsupported shape is encountered.
            if !matches!(self.pane_content(pane), Some(PaneContent::Inspector))
                || !matches!(spec.source, PaneSource::Fixed(SourceRef::Graph(_)))
            {
                return Some(spec.clone());
            }
        }
        let layout = self.space(space)?;
        if let crate::panes::PaneNode::Workspace(frame) = &layout.root
            && let Some(value) = frame.descriptors.get(&pane.0)
        {
            let spec: PaneSpec = serde_json::from_value(value.clone()).ok()?;
            if !matches!(self.pane_content(pane), Some(PaneContent::Inspector))
                || !matches!(spec.source, PaneSource::Fixed(SourceRef::Graph(_)))
            {
                return Some(spec);
            }
        }
        let kind = self.pane_content(pane)?.kind_id();
        Some(PaneSpec {
            id: pane,
            config: PaneConfig::empty(format!("{}.config", kind.as_str())),
            kind,
            source: PaneSource::FromContext(
                if matches!(self.pane_content(pane), Some(PaneContent::Inspector)) {
                    SourceSelector::Member
                } else {
                    SourceSelector::Graph
                },
            ),
            context: ContextBinding::FocusedInOwnSpace,
        })
    }

    pub(crate) fn pane_bound_context(&self, pane: PaneId) -> Option<PaneContext> {
        let spec = self.pane_binding_spec(pane)?;
        let context = match &spec.source {
            PaneSource::Fixed(source) => source.context(),
            PaneSource::FromContext(_) => {
                // A graph publisher with no selected member is still the
                // current source. Do not skip it and resurrect an older reader.
                if let ContextBinding::Follow(target) = spec.context {
                    self.pane_content(target)?;
                }
                self.pane_context
                    .resolve_context(pane, &spec.context, SourceSelector::Graph)?
            },
        };
        let graph = context.graph?;
        let canvas = self.graph_runtimes.canvas(graph)?;
        if let Some(member) = context.member {
            canvas.graph().get_node_by_id(member)?;
        }
        Some(context)
    }

    fn source_name(&self, context: Option<PaneContext>, member_scope: bool) -> (String, bool) {
        let Some(context) = context else {
            return ("Source unavailable".into(), false);
        };
        if member_scope {
            let Some((canvas, member)) = context
                .graph
                .and_then(|g| self.graph_runtimes.canvas(g))
                .zip(context.member)
            else {
                return ("No document selected".into(), false);
            };
            let Some((key, _)) = canvas.graph().get_node_by_id(member) else {
                return ("Document unavailable".into(), false);
            };
            (canvas.graph().node_display_label(key), true)
        } else {
            (
                context
                    .graph
                    .map(|g| {
                        self.sessions
                            .iter()
                            .find(|(_, manifest)| manifest.root_graph_id == g)
                            .and_then(|(_, manifest)| manifest.display_name.clone())
                            .unwrap_or_else(|| format!("Graph {}", &g.0.to_string()[..8]))
                    })
                    .unwrap_or_else(|| "Graph unavailable".into()),
                context.graph.is_some(),
            )
        }
    }

    pub(crate) fn pane_binding_snapshot(&self, pane: PaneId) -> Option<BindingSnapshot> {
        let spec = self.pane_binding_spec(pane)?;
        let member_scope = matches!(self.pane_content(pane), Some(PaneContent::Inspector));
        let context = self.pane_bound_context(pane);
        let (source, available) = self.source_name(context, member_scope);
        let mode = match (&spec.source, &spec.context) {
            (PaneSource::Fixed(_), _) => "Pinned".into(),
            (_, ContextBinding::Follow(target)) => self
                .pane_content(*target)
                .map(|content| format!("Following {}", self.publisher_name(*target, content)))
                .unwrap_or_else(|| "Following closed pane".into()),
            _ => "Following active pane".into(),
        };
        let intent = |choice| BindingIntent {
            pane,
            admission: self.pane_context.admission(),
            choice,
        };
        let mut choices = vec![(
            "Follow active pane".into(),
            intent(BindingChoice::FollowActive),
            matches!(spec.source, PaneSource::FromContext(_))
                && spec.context == ContextBinding::FocusedInOwnSpace,
        )];
        if available && let Some(context) = context {
            choices.push((
                if member_scope {
                    "Pin this document"
                } else {
                    "Pin this graph"
                }
                .into(),
                intent(BindingChoice::Pin {
                    graph: context.graph?,
                    member: if member_scope { context.member } else { None },
                }),
                matches!(spec.source, PaneSource::Fixed(_)),
            ));
        }
        for (target, content, _) in self
            .frisket
            .iter_leaves()
            .chain(self.lenses.iter().flatten().flat_map(|s| s.iter_leaves()))
        {
            if target != pane && is_publisher(content) {
                choices.push((
                    format!("Follow {}", self.publisher_name(target, content)),
                    intent(BindingChoice::FollowPane(target)),
                    spec.context == ContextBinding::Follow(target),
                ));
            }
        }
        Some(BindingSnapshot {
            mode,
            source,
            available,
            choices,
        })
    }

    fn publisher_name(&self, pane: PaneId, content: &PaneContent) -> String {
        let peers = self
            .frisket
            .iter_leaves()
            .chain(
                self.lenses
                    .iter()
                    .flatten()
                    .flat_map(|space| space.iter_leaves()),
            )
            .filter(|(_, candidate, _)| candidate.kind_id() == content.kind_id())
            .map(|(id, _, _)| id)
            .collect::<Vec<_>>();
        if peers.len() > 1 {
            format!(
                "{} {}",
                publisher_label(content),
                peers.iter().position(|id| *id == pane).unwrap_or(0) + 1
            )
        } else {
            publisher_label(content).into()
        }
    }

    pub(crate) fn apply_pane_binding(&mut self, intent: BindingIntent) -> Vec<Effect> {
        let refuse = || vec![Effect::Redraw];
        if intent.admission != self.pane_context.admission() {
            return refuse();
        }
        let Some(mut spec) = self.pane_binding_spec(intent.pane) else {
            return refuse();
        };
        let member_scope = matches!(self.pane_content(intent.pane), Some(PaneContent::Inspector));
        match intent.choice {
            BindingChoice::FollowActive => {
                spec.source = PaneSource::FromContext(if member_scope {
                    SourceSelector::Member
                } else {
                    SourceSelector::Graph
                });
                spec.context = ContextBinding::FocusedInOwnSpace;
            },
            BindingChoice::FollowPane(target) => {
                if target == intent.pane || !self.pane_content(target).is_some_and(is_publisher) {
                    return refuse();
                }
                spec.source = PaneSource::FromContext(if member_scope {
                    SourceSelector::Member
                } else {
                    SourceSelector::Graph
                });
                spec.context = ContextBinding::Follow(target);
            },
            BindingChoice::Pin { graph, member } => {
                let Some(context) = self.pane_bound_context(intent.pane) else {
                    return refuse();
                };
                if context.graph != Some(graph)
                    || (member_scope && (member.is_none() || context.member != member))
                    || (!member_scope && member.is_some())
                {
                    return refuse();
                }
                spec.pin(if let Some(member) = member {
                    SourceRef::Member { graph, member }
                } else {
                    SourceRef::Graph(graph)
                });
            },
        }
        let Some(space) = self.space_of(intent.pane) else {
            return refuse();
        };
        if self.ensure_shared_space(space).is_err() {
            return refuse();
        }
        let Ok(value) = serde_json::to_value(spec) else {
            return refuse();
        };
        let Some(frame) = self.space_mut(space).and_then(|s| s.workspace_mut()) else {
            return refuse();
        };
        frame.descriptors.insert(intent.pane.0, value);
        self.pane_context.invalidate();
        vec![Effect::SaveSession, Effect::Redraw]
    }
}

fn is_publisher(content: &PaneContent) -> bool {
    matches!(
        content,
        PaneContent::Orrery | PaneContent::Workbench | PaneContent::Tile(_)
    )
}
fn publisher_label(content: &PaneContent) -> &'static str {
    match content {
        PaneContent::Orrery => "Map",
        PaneContent::Workbench => "Read",
        _ => "Document",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;

    fn setup() -> (App, PaneId, PaneId, uuid::Uuid) {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/source-a".into()));
        let map = app.default_graph_pane();
        let member = app.graph_pane_focused_member(map).unwrap();
        app.update(Action::SummonPane(crate::panes::PaneKindId::new(
            crate::panes::kind::INSPECTOR,
        )));
        let inspector = app
            .frisket
            .iter_leaves()
            .find(|(_, content, _)| matches!(content, PaneContent::Inspector))
            .unwrap()
            .0;
        (app, map, inspector, member)
    }
    fn choose(app: &mut App, pane: PaneId, label: &str) {
        let snapshot = app.pane_binding_snapshot(pane).unwrap();
        let intent = snapshot
            .choices
            .iter()
            .find(|(text, _, _)| text == label)
            .unwrap()
            .1;
        assert!(
            app.update(Action::SetPaneBinding(intent))
                .contains(&Effect::SaveSession)
        );
    }

    #[test]
    fn pinned_inspector_keeps_its_document_when_read_focus_changes_and_follows_again() {
        let (mut app, map, inspector, a) = setup();
        choose(&mut app, inspector, "Pin this document");
        app.active_pane = Some(map);
        app.update(Action::OpenAddress("https://example.test/source-b".into()));
        let b = app.graph_pane_focused_member(map).unwrap();
        assert_ne!(a, b);
        assert_eq!(
            crate::inspector_view::inspector_member(&app, inspector),
            Some(a)
        );
        assert_eq!(app.pane_binding_snapshot(inspector).unwrap().mode, "Pinned");
        choose(&mut app, inspector, "Follow active pane");
        assert_eq!(
            crate::inspector_view::inspector_member(&app, inspector),
            Some(b)
        );
    }

    #[test]
    fn explicit_follow_stays_with_its_publisher_and_closed_publisher_is_unavailable() {
        let (mut app, map, inspector, a) = setup();
        choose(&mut app, inspector, "Follow Map");
        app.active_pane = Some(map);
        app.update(Action::ReadGraphMember(a));
        let read = app
            .frisket
            .iter_leaves()
            .find(|(_, c, _)| matches!(c, PaneContent::Workbench))
            .unwrap()
            .0;
        let b = app
            .with_graph_pane(map, |canvas| {
                canvas.open_member_as_new_node(None, "https://example.test/other")
            })
            .unwrap();
        app.graph_pane_select_member(map, a);
        app.active_pane = Some(read);
        app.publish_member_context(read, Some(b));
        assert_eq!(
            crate::inspector_view::inspector_member(&app, inspector),
            Some(a)
        );
        assert!(app.frisket.close_pane(map));
        app.index_pane_spaces();
        let snapshot = app.pane_binding_snapshot(inspector).unwrap();
        assert!(!snapshot.available);
        assert!(snapshot.mode.contains("closed pane"));
        assert_eq!(
            crate::inspector_view::inspector_member(&app, inspector),
            None
        );
    }

    #[test]
    fn pin_is_in_existing_descriptor_and_survives_disk_roundtrip_and_float_move() {
        let (mut app, _, inspector, a) = setup();
        choose(&mut app, inspector, "Pin this document");
        let expected = app.pane_binding_spec(inspector).unwrap();
        app.active_pane = Some(inspector);
        app.update(Action::FloatActivePane);
        assert_eq!(app.pane_binding_spec(inspector), Some(expected.clone()));
        let root = tempfile::tempdir().unwrap();
        crate::session::save_frisket_layout(root.path(), &app.frisket);
        let layout = crate::session::load_frisket_layout(root.path()).unwrap();
        app.frisket = layout;
        app.index_pane_spaces();
        assert_eq!(app.pane_binding_spec(inspector), Some(expected));
        assert_eq!(
            crate::inspector_view::inspector_member(&app, inspector),
            Some(a)
        );
    }

    #[test]
    fn stale_binding_control_cannot_pin_a_new_subject_or_replay_after_a_change() {
        let (mut app, map, inspector, _) = setup();
        let snapshot = app.pane_binding_snapshot(inspector).unwrap();
        let old = snapshot
            .choices
            .iter()
            .find(|(t, _, _)| t == "Pin this document")
            .unwrap()
            .1;
        app.active_pane = Some(map);
        app.update(Action::OpenAddress("https://example.test/new".into()));
        assert!(
            !app.update(Action::SetPaneBinding(old))
                .contains(&Effect::SaveSession)
        );
        assert!(matches!(
            app.pane_binding_spec(inspector).unwrap().source,
            PaneSource::FromContext(_)
        ));
        choose(&mut app, inspector, "Pin this document");
        let before = app.pane_binding_spec(inspector);
        assert!(
            !app.update(Action::SetPaneBinding(old))
                .contains(&Effect::SaveSession)
        );
        assert_eq!(app.pane_binding_spec(inspector), before);
    }

    #[test]
    fn a_removed_pinned_member_does_not_fall_back_to_the_legacy_canvas_focus() {
        let (mut app, map, inspector, _) = setup();
        choose(&mut app, inspector, "Pin this document");
        app.with_graph_pane(map, |canvas| {
            canvas.set_graph(mere::kernel::graph::Graph::new())
        });
        app.active_pane = Some(map);
        app.update(Action::OpenAddress(
            "https://example.test/replacement".into(),
        ));
        assert_eq!(
            crate::inspector_view::inspector_member(&app, inspector),
            None
        );
        assert!(!app.pane_binding_snapshot(inspector).unwrap().available);
        assert!(
            app.pane_binding_snapshot(inspector)
                .unwrap()
                .mode
                .contains("Pinned")
        );
    }

    #[test]
    fn roster_pins_graph_and_does_not_accept_a_member_binding() {
        let (mut app, map, _, a) = setup();
        app.active_pane = Some(map);
        app.update(Action::SummonPane(crate::panes::PaneKindId::new(
            crate::panes::kind::ROSTER,
        )));
        let roster = app
            .frisket
            .iter_leaves()
            .find(|(_, c, _)| matches!(c, PaneContent::Roster))
            .unwrap()
            .0;
        let graph = app.graph_for_pane(map).unwrap();
        let invalid = BindingIntent {
            pane: roster,
            admission: app.pane_context.admission(),
            choice: BindingChoice::Pin {
                graph,
                member: Some(a),
            },
        };
        assert!(
            !app.update(Action::SetPaneBinding(invalid))
                .contains(&Effect::SaveSession)
        );
        choose(&mut app, roster, "Pin this graph");
        assert_eq!(app.follower_context(roster).unwrap().member, None);
        assert_eq!(
            app.pane_binding_spec(roster).unwrap().source,
            PaneSource::Fixed(SourceRef::Graph(graph))
        );
    }
}
