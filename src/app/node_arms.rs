//! Node and view arms: what a node IS and how it is shown.
//!
//! Deleting stages into the recycle bin and recovery restores the ORIGINAL
//! id, which is the whole identity contract the bin exists for.

use uuid::Uuid;

use crate::action::{Action, Effect};
use crate::observe::AppEvent;
use crate::panes::PaneContent;
use crate::shell_services::{EntryPrivacy, ShellInput, ShellIntent, ShellOutcome};
use crate::surface::FocusTarget;
use crate::ui::{OmnibarState, Suggestion, normalize_address};

use super::App;

impl App {
    pub(super) fn delete_focused_node(&mut self) -> Vec<Effect> {
        // Build the bin record off the LIVING node (identity, url,
        // title, tags — everything recovery restores), then drop the
        // node and reap what hung off it: the live content session
        // and any workbench tile. The record stages through the bin
        // port (Effect::RecordDeleted); the actor answers with the
        // refreshed list, so `removed` mirrors the store, never a
        // hand-kept copy.
        let record = self.graph_runtimes.focused_member().and_then(|m| {
            let graph = self.graph_runtimes.graph();
            let (key, node) = graph.get_node_by_id(m)?;
            let title = node.title.trim();
            // The node's whole character rides the tombstone: its
            // borne world (by id) and its facet bundle, so recovery
            // restores residency/arrangement/web state, not just
            // identity.
            let facets = self.graph_runtimes.facets().facets_of(&m).map(|f| {
                serde_json::Value::Object(
                    f.iter()
                        .map(|(id, value)| (id.as_str().to_string(), value.clone()))
                        .collect(),
                )
            });
            Some(crate::action::RemovedRecord {
                node_id: node.id,
                url: node.url().to_string(),
                title: (!title.is_empty() && title != node.url()).then(|| title.to_string()),
                tags: graph
                    .node_tags(key)
                    .map(|t| t.iter().cloned().collect())
                    .unwrap_or_default(),
                deleted_at_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
                nested: node.nested.as_ref().map(|log| log.as_str().to_string()),
                facets,
            })
        });
        let Some(record) = record else {
            return vec![Effect::Redraw];
        };
        // Archive-never-orphan: the world's file moves to the archive
        // slot BEFORE the bearing node leaves; a failed archive
        // aborts the delete (the node stays, nothing is lost).
        if let Some(log_id) = &record.nested
            && let Err(err) = crate::denizen::archive_world(&self.session_dir(), log_id)
        {
            tracing::warn!(%err, log_id, "world archive failed; delete aborted");
            return vec![Effect::Redraw];
        }
        let Some(member) = self.graph_runtimes.remove_focused() else {
            // The node did not leave after all: put the world back.
            if let Some(log_id) = &record.nested {
                let _ = crate::denizen::unarchive_world(&self.session_dir(), log_id);
            }
            return vec![Effect::Redraw];
        };
        // The record is the archive now: the live facets go, and a
        // denizen's runtime entry goes with its node.
        if self.denizens.residents.remove(&member).is_some() {
            let sdir = self.session_dir();
            self.denizens = crate::denizen::rebuild(
                self.graph_runtimes.facets(),
                self.graph_runtimes.graph(),
                &sdir,
                self.identity.as_ref(),
            );
        }
        for runtime in self.forme_runtimes.iter_mut() {
            runtime.workbench.close_tile(member);
        }
        self.events.push(AppEvent::NodeRemoved(record.url.clone()));
        vec![
            Effect::RecordDeleted { record },
            Effect::CloseContent { node: member },
            Effect::SaveSession,
            Effect::Redraw,
        ]
    }

    pub(super) fn recover_deleted_node(&mut self, id: Uuid) -> Vec<Effect> {
        // Recover from the bin mirror BY IDENTITY: the node re-mints
        // under its ORIGINAL id with its recorded title/tags (the
        // canvas guards idempotency), gets selected + centered, joins
        // the visit history, and refetches. The bin record stays in
        // the store (append-only until athanor's pass); the Trail's
        // Removed section derives it away because the node is present
        // again.
        let Some(record) = self.removed.iter().find(|r| r.node_id == id).cloned() else {
            return vec![Effect::Redraw];
        };
        let member = self.graph_runtimes.recover_node(
            record.node_id,
            &record.url,
            record.title.as_deref(),
            &record.tags,
        );
        // Restore the node's character from the tombstone: the facet
        // bundle whole, then the borne world (file back to the live
        // slot, pointer re-borne through the spine), then the denizen
        // runtime so a recovered resident resides again.
        if let Some(serde_json::Value::Object(map)) = &record.facets {
            for (facet_id, value) in map {
                let _ = self.graph_runtimes.facets_mut().set(
                    member,
                    chartulary::FacetId::new(facet_id.as_str()),
                    value.clone(),
                    &chartulary::AcceptAll,
                );
            }
        }
        if let Some(log_id) = &record.nested {
            let sdir = self.session_dir();
            if let Err(err) = crate::denizen::unarchive_world(&sdir, log_id) {
                tracing::warn!(%err, log_id, "world unarchive failed; recovering empty");
            }
            let _ = self.graph_runtimes.set_node_nested_for(
                member,
                Some(mere::kernel::graph::LogId::new(log_id.clone())),
            );
            self.denizens = crate::denizen::rebuild(
                self.graph_runtimes.facets(),
                self.graph_runtimes.graph(),
                &sdir,
                self.identity.as_ref(),
            );
        }
        self.graph_runtimes.center_on_selected();
        self.history.visit(record.url.clone());
        self.events
            .push(AppEvent::NodeRecovered(record.url.clone()));
        let mut effects = vec![Effect::SaveSession, Effect::Redraw];
        if fetch::is_fetchable(&record.url) {
            effects.push(Effect::FetchPage {
                node: member,
                url: record.url.clone(),
            });
        }
        effects
    }

    pub(super) fn set_viewer_override(
        &mut self,
        member: Uuid,
        viewer: Option<String>,
    ) -> Vec<Effect> {
        self.browser.entry(member).viewer_override = viewer.clone();
        self.events.push(AppEvent::ViewerChanged {
            node: member,
            viewer: viewer.clone().unwrap_or_else(|| "auto".to_string()),
        });
        let mut effects = Vec::new();
        // Live (or in-flight) content respawns through the now-pinned
        // route, so the setting is seen applying (the Reload shape).
        if matches!(
            self.content.get(member),
            Some(crate::content::NodeContent::Live | crate::content::NodeContent::Requested)
        ) && let Some(url) = self
            .graph_runtimes
            .graph()
            .nodes()
            .find(|(_, n)| n.id == member)
            .map(|(_, n)| n.url().to_string())
        {
            self.content.note_requested(member);
            self.events.push(AppEvent::ContentState {
                node: member,
                state: "requested".to_string(),
            });
            effects.push(Effect::CloseContent { node: member });
            effects.push(Effect::SpawnContent { node: member, url });
        }
        effects.push(Effect::SaveSession);
        effects.push(Effect::Redraw);
        effects
    }

    pub(super) fn toggle_node_content(&mut self) -> Vec<Effect> {
        // The flip targets the focused node; no focus, no-op (the
        // caption chip tells the user what would flip).
        // Resolve the node by MEMBER, not by URL round-trip: two
        // nodes may share a URL (the sample graph + an open), and
        // get_node_by_url picks arbitrarily between them.
        let Some(target) = self
            .graph_runtimes
            .focused_member()
            .zip(self.graph_runtimes.focused_url().map(str::to_string))
        else {
            return Vec::new();
        };
        let (node, url) = target;
        if self.content.flip_spawns(node) {
            self.content.note_requested(node);
            self.events.push(AppEvent::ContentState {
                node,
                state: "requested".to_string(),
            });
            vec![Effect::SpawnContent { node, url }, Effect::Redraw]
        } else {
            self.content.note_closed(node);
            self.events.push(AppEvent::ContentState {
                node,
                state: "closed".to_string(),
            });
            vec![Effect::CloseContent { node }, Effect::Redraw]
        }
    }

    pub(super) fn reload_focused(&mut self) -> Vec<Effect> {
        let Some(target) = self
            .graph_runtimes
            .focused_member()
            .zip(self.graph_runtimes.focused_url().map(str::to_string))
        else {
            return vec![Effect::Redraw];
        };
        let (node, url) = target;
        self.events.push(AppEvent::Reloaded(url.clone()));
        let mut effects = Vec::new();
        if fetch::is_fetchable(&url) {
            effects.push(Effect::FetchPage {
                node,
                url: url.clone(),
            });
        }
        // A live (or in-flight) session respawns fresh; a node
        // without content stays without (reload is not a spawn).
        if matches!(
            self.content.get(node),
            Some(crate::content::NodeContent::Live | crate::content::NodeContent::Requested)
        ) {
            self.content.note_requested(node);
            self.events.push(AppEvent::ContentState {
                node,
                state: "requested".to_string(),
            });
            effects.push(Effect::CloseContent { node });
            effects.push(Effect::SpawnContent { node, url });
        }
        effects.push(Effect::Redraw);
        effects
    }

    pub(super) fn commit_omnibar(&mut self) -> Vec<Effect> {
        // Rename mode captures the whole line as the new name and
        // commits it, bypassing the find/go/actions lanes.
        if let crate::ui::OmnibarMode::RenameSession(id) = self.omnibar.mode {
            let name = self.omnibar.text.clone();
            let target = self.fallback_shell_context();
            let entry = self.shell.record_omnibar(
                ShellInput::Omnibar(name.clone()),
                ShellIntent::Command {
                    label: "Rename session".into(),
                    action: Action::RenameSession {
                        id,
                        name: name.clone(),
                    },
                },
                target,
                EntryPrivacy::Ordinary,
            );
            self.omnibar = OmnibarState::default();
            if self.focus == FocusTarget::Chrome {
                self.focus = FocusTarget::Graph(self.default_graph_pane());
            }
            let mut fx = self.update(Action::RenameSession { id, name });
            self.shell.complete(
                entry,
                ShellOutcome::Completed {
                    summary: "renamed session".into(),
                },
            );
            fx.push(Effect::Redraw);
            return fx;
        }
        // Commit always ends with the omnibar closed, so chrome hands
        // focus back to the canvas. (A committed OpenAddress may later
        // spawn content; routing focus onto it is slice B.)
        if self.focus == FocusTarget::Chrome {
            self.focus = FocusTarget::Graph(self.default_graph_pane());
        }
        let committed = self.omnibar.selection().cloned().or_else(|| {
            normalize_address(self.omnibar.text.trim()).map(|url| Suggestion::Go { url })
        });
        if let Some(s) = committed.as_ref() {
            self.events
                .push(AppEvent::OmnibarCommitted(crate::observe::suggestion_line(
                    s,
                )));
        }
        let mut effects = match committed {
            Some(Suggestion::Node { url, .. }) => {
                // Find lane: select the existing node; never refetch.
                let target = self.fallback_shell_context();
                let entry = self.shell.record_omnibar(
                    ShellInput::Omnibar(self.omnibar.text.clone()),
                    ShellIntent::SelectNode { url: url.clone() },
                    target,
                    EntryPrivacy::Ordinary,
                );
                self.graph_runtimes.select_by_url(&url);
                self.shell.complete(
                    entry,
                    ShellOutcome::Completed {
                        summary: format!("selected {url}"),
                    },
                );
                vec![Effect::Redraw]
            }
            Some(Suggestion::Go { url }) => {
                let target = self.fallback_shell_context();
                let entry = self.shell.record_omnibar(
                    ShellInput::Omnibar(self.omnibar.text.clone()),
                    ShellIntent::Navigate { url: url.clone() },
                    target,
                    EntryPrivacy::Ordinary,
                );
                self.omnibar = OmnibarState::default();
                return {
                    let mut fx = self.update(Action::OpenAddress(url));
                    self.shell.complete(
                        entry,
                        ShellOutcome::Completed {
                            summary: "opened address".into(),
                        },
                    );
                    fx.push(Effect::Redraw);
                    fx
                };
            }
            Some(Suggestion::Recall { url, .. }) => {
                // The recall lane: a page out of browsing memory opens
                // exactly as a typed address does. Its transcript intent is
                // Navigate for the same reason — where the row came from is
                // provenance, and the act is still opening an address.
                let target = self.fallback_shell_context();
                let entry = self.shell.record_omnibar(
                    ShellInput::Omnibar(self.omnibar.text.clone()),
                    ShellIntent::Navigate { url: url.clone() },
                    target,
                    EntryPrivacy::Ordinary,
                );
                self.omnibar = OmnibarState::default();
                return {
                    let mut fx = self.update(Action::OpenAddress(url));
                    self.shell.complete(
                        entry,
                        ShellOutcome::Completed {
                            summary: "opened a recalled address".into(),
                        },
                    );
                    fx.push(Effect::Redraw);
                    fx
                };
            }
            Some(Suggestion::Act { label, action }) => {
                // The actions lane: the committed registry entry is
                // an ordinary Action; lower it through the same
                // spine everything else uses.
                let target = self.fallback_shell_context();
                let entry = self.shell.record_omnibar(
                    ShellInput::Omnibar(self.omnibar.text.clone()),
                    ShellIntent::Command {
                        label: label.clone(),
                        action: action.clone(),
                    },
                    target,
                    EntryPrivacy::Ordinary,
                );
                self.omnibar = OmnibarState::default();
                return {
                    let mut fx = self.update(action);
                    self.shell.complete(
                        entry,
                        ShellOutcome::Completed {
                            summary: format!("ran {label}"),
                        },
                    );
                    fx.push(Effect::Redraw);
                    fx
                };
            }
            Some(Suggestion::Hint(_)) | None => vec![Effect::Redraw],
        };
        self.omnibar = OmnibarState::default();
        self.shell.close_omnibar();
        effects.push(Effect::Redraw);
        effects
    }

    pub(super) fn open_address(&mut self, url: String) -> Vec<Effect> {
        self.events.push(AppEvent::AddressOpened(url.clone()));
        // A graph pane owns its selection. Visiting through the compatibility
        // canvas cursor selected the node only until the next render installed
        // that pane's saved selection, at which point an address opened from
        // the omnibar became unfocused and could not spawn or retarget content.
        let pane = self
            .focused_graph_pane()
            .unwrap_or_else(|| self.default_graph_pane());
        let Some(key) = self.with_graph_pane(pane, |canvas| canvas.visit(&url)) else {
            return vec![Effect::Redraw];
        };
        self.history.visit(url.clone());
        let mut effects = vec![Effect::Redraw];
        if fetch::is_fetchable(&url)
            && let Some(node) = self.graph_runtimes.graph().get_node(key).map(|n| n.id)
        {
            effects.push(Effect::FetchPage { node, url });
        }
        effects
    }

    pub(super) fn commit_content_navigation(&mut self, member: Uuid, url: String) -> Vec<Effect> {
        let Some(graph) = self.graph_runtimes.graph_containing_member(member) else {
            tracing::warn!(%member, %url, "content navigation named an unknown member");
            return Vec::new();
        };
        let Some(canvas) = self.graph_runtimes.canvas_mut(graph) else {
            return Vec::new();
        };
        let already_current = canvas
            .graph()
            .get_node_by_id(member)
            .is_some_and(|(_, node)| node.url() == url);
        if already_current || !canvas.navigate_member(member, &url) {
            return Vec::new();
        }
        self.events
            .push(AppEvent::ContentNavigated { node: member, url });
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(super) fn set_content_title(&mut self, member: Uuid, title: String) -> Vec<Effect> {
        let Some(graph) = self.graph_runtimes.graph_containing_member(member) else {
            tracing::warn!(%member, "content title named an unknown member");
            return Vec::new();
        };
        let changed = self
            .graph_runtimes
            .canvas_mut(graph)
            .is_some_and(|canvas| canvas.set_node_title_for(member, title.clone()));
        if !changed {
            return Vec::new();
        }
        self.events.push(AppEvent::ContentTitleChanged {
            node: member,
            title,
        });
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(super) fn nav_back(&mut self) -> Vec<Effect> {
        let Some(url) = self.history.back().map(str::to_string) else {
            return vec![Effect::Redraw];
        };
        self.events.push(AppEvent::NavigatedBack(url.clone()));
        if !url.is_empty() {
            // Navigation is a revisit even when its node already
            // exists, so P3's recency-derived score remains honest.
            let pane = self
                .focused_graph_pane()
                .unwrap_or_else(|| self.default_graph_pane());
            let _ = self.with_graph_pane(pane, |canvas| canvas.visit(&url));
        }
        vec![Effect::Redraw]
    }

    pub(super) fn nav_forward(&mut self) -> Vec<Effect> {
        let Some(url) = self.history.forward().map(str::to_string) else {
            return vec![Effect::Redraw];
        };
        self.events.push(AppEvent::NavigatedForward(url.clone()));
        let pane = self
            .focused_graph_pane()
            .unwrap_or_else(|| self.default_graph_pane());
        let _ = self.with_graph_pane(pane, |canvas| canvas.visit(&url));
        vec![Effect::Redraw]
    }

    pub(super) fn reseed_layout(&mut self) -> Vec<Effect> {
        if self.graph_runtimes.reseed() {
            self.events.push(AppEvent::LayoutReseeded);
            vec![Effect::Redraw]
        } else {
            Vec::new()
        }
    }

    pub(super) fn set_layout_strategy(&mut self, id: Option<&'static str>) -> Vec<Effect> {
        self.graph_runtimes
            .set_layout_strategy(id.map(str::to_string));
        if id != Some("phyllotaxis.default") {
            self.graph_runtimes.set_projection_score(None);
        }
        // The projection itself is computed on the next frame by
        // `drive_layout_strategy` (it needs the surface viewport).
        vec![Effect::Redraw]
    }

    pub(super) fn toggle_size_by_recency(&mut self) -> Vec<Effect> {
        let on = !self.graph_runtimes.size_by_recency();
        self.graph_runtimes.set_size_by_recency(on);
        // A size change moves extents and the recency ordering, so the
        // active analytic layout must recompute; re-selecting the same
        // strategy drops its input cache (last_strategy_inputs = None).
        let active = self.graph_runtimes.layout_strategy().map(str::to_string);
        self.graph_runtimes.set_layout_strategy(active);
        vec![Effect::Redraw]
    }

    pub(super) fn set_node_sprite(
        &mut self,
        member: Uuid,
        data_uri: String,
        hull: Vec<(f32, f32)>,
    ) -> Vec<Effect> {
        self.graph_runtimes.set_node_sprite(member, data_uri);
        // The traced collider: the node collides at its picture. Under
        // 3 points the tracer found no opaque region — keep the
        // silhouette collider rather than installing a degenerate one.
        if hull.len() >= 3 {
            self.graph_runtimes.set_node_sprite_hull(member, hull);
        }
        self.events.push(AppEvent::NodeSpriteSet(member));
        vec![Effect::SaveSession, Effect::Redraw]
    }
}

#[cfg(test)]
mod tests {
    use crate::action::{Action, Effect};
    use crate::app::App;

    #[test]
    fn opening_an_address_keeps_the_new_selection_through_pane_rendering() {
        let mut app = App::test_stub();
        let pane = app.default_graph_pane();

        app.update(Action::OpenAddress("https://example.com/".into()));
        let selected = app
            .graph_runtimes
            .focused_member()
            .expect("opening an address selects its node");

        // A graph frame restores pane-local state. The selection must survive
        // that restore or the next content action has no node to target.
        app.graph_pane_frame(pane, 800, 600)
            .expect("the default pane owns the active graph");
        assert_eq!(app.graph_pane_focused_member(pane), Some(selected));
        assert_eq!(app.graph_runtimes.focused_member(), Some(selected));
    }

    #[test]
    fn committed_content_navigation_grows_the_same_members_lineage() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.com/".into()));
        let member = app.graph_runtimes.focused_member().unwrap();
        let before = app.graph_runtimes.graph().node_count();

        let effects = app.update(Action::ContentNavigationCommitted {
            member,
            url: "https://www.iana.org/help/example-domains".into(),
        });

        assert_eq!(app.graph_runtimes.graph().node_count(), before);
        assert_eq!(
            app.graph_runtimes
                .graph()
                .get_node_by_id(member)
                .map(|(_, node)| node.url()),
            Some("https://www.iana.org/help/example-domains")
        );
        let key = app
            .graph_runtimes
            .graph()
            .get_node_by_id(member)
            .unwrap()
            .0;
        assert_eq!(
            app.graph_runtimes
                .graph()
                .node_history_projection(key)
                .entries,
            vec![
                "https://example.com/".to_string(),
                "https://www.iana.org/help/example-domains".to_string(),
            ]
        );
        assert!(effects.contains(&Effect::SaveSession));
    }
}
