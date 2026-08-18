//! Feed subscription actions and update folding.

use crate::action::{Effect, FetchedPage};
use crate::feed::{FEED_ENTRY_TAG, FEED_TAG, FeedMemberInfo, KEEP_TAG, UNREAD_TAG};
use crate::observe::AppEvent;

use super::App;

impl App {
    pub(super) fn subscribe_focused_feed(&mut self, period: servitor::Period) -> Vec<Effect> {
        let Some(node) = self.graph_runtimes.focused_member() else {
            return vec![Effect::Redraw];
        };
        let Some(url) = self
            .graph_runtimes
            .graph_containing_member(node)
            .and_then(|graph| self.graph_runtimes.canvas(graph))
            .and_then(|canvas| {
                canvas
                    .graph()
                    .get_node_by_id(node)
                    .map(|(_, node)| node.url().to_string())
            })
        else {
            return vec![Effect::Redraw];
        };
        if !fetch::is_fetchable(&url) {
            self.events.push(AppEvent::FeedRefreshFailed {
                node,
                error: format!("{url} is not fetchable"),
            });
            return vec![Effect::Redraw];
        }

        if let Some(graph) = self.graph_runtimes.graph_containing_member(node)
            && let Some(canvas) = self.graph_runtimes.canvas_mut(graph)
        {
            canvas.tag_node(node, KEEP_TAG);
            canvas.tag_node(node, FEED_TAG);
        }
        self.feeds.subscribe(node, url, period);
        self.events.push(AppEvent::FeedSubscribed {
            node,
            period: period.as_str(),
        });
        let mut effects = vec![Effect::SaveSession, Effect::Redraw];
        if let Some(url) = self.feeds.start(node) {
            effects.insert(0, self.fetch_feed_effect(node, url));
        }
        effects
    }

    pub(super) fn unsubscribe_focused_feed(&mut self) -> Vec<Effect> {
        let Some(node) = self.graph_runtimes.focused_member() else {
            return vec![Effect::Redraw];
        };
        let entries = self.feeds.entry_members(node);
        if !self.feeds.unsubscribe(node) {
            return vec![Effect::Redraw];
        }
        if let Some(graph) = self.graph_runtimes.graph_containing_member(node)
            && let Some(canvas) = self.graph_runtimes.canvas_mut(graph)
        {
            canvas.untag_node(node, FEED_TAG);
        }
        for entry in entries {
            if let Some(graph) = self.graph_runtimes.graph_containing_member(entry)
                && let Some(canvas) = self.graph_runtimes.canvas_mut(graph)
            {
                canvas.untag_node(entry, UNREAD_TAG);
            }
        }
        self.events.push(AppEvent::FeedUnsubscribed(node));
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(super) fn refresh_feeds(&mut self) -> Vec<Effect> {
        let requests = self.feeds.start_all();
        self.feed_fetch_effects(requests)
    }

    pub(super) fn mark_focused_feed_entry_read(&mut self) -> Vec<Effect> {
        let Some(node) = self.graph_runtimes.focused_member() else {
            return vec![Effect::Redraw];
        };
        if !self.feeds.mark_read(node) {
            return vec![Effect::Redraw];
        }
        if let Some(graph) = self.graph_runtimes.graph_containing_member(node)
            && let Some(canvas) = self.graph_runtimes.canvas_mut(graph)
        {
            canvas.untag_node(node, UNREAD_TAG);
        }
        self.events.push(AppEvent::FeedEntryRead(node));
        vec![Effect::SaveSession, Effect::Redraw]
    }

    /// Supply the host clock to both W4 behaviors and feed schedules.
    pub fn tick(&mut self, now_ms: u64) -> Vec<Effect> {
        self.now_ms = Some(now_ms);
        let requests = self.feeds.start_due(now_ms);
        let mut effects = self.feed_fetch_effects(requests);
        effects.extend(crate::behaviors::drain(self));
        effects
    }

    fn feed_fetch_effects(&mut self, requests: Vec<(uuid::Uuid, String)>) -> Vec<Effect> {
        let mut effects = Vec::new();
        let mut changed = false;
        for (node, url) in requests {
            let current = self
                .graph_runtimes
                .graph_containing_member(node)
                .and_then(|graph| self.graph_runtimes.canvas(graph))
                .is_some_and(|canvas| crate::browse::still_current(canvas, node, &url));
            if current {
                effects.push(self.fetch_feed_effect(node, url));
            } else {
                self.feeds.unsubscribe(node);
                self.events.push(AppEvent::FeedRefreshFailed {
                    node,
                    error: "source node is gone or has navigated".to_string(),
                });
                changed = true;
            }
        }
        if changed {
            effects.push(Effect::SaveSession);
        }
        if !effects.is_empty() {
            effects.push(Effect::Redraw);
        }
        effects
    }

    fn fetch_feed_effect(&self, node: uuid::Uuid, url: String) -> Effect {
        let identity = match self
            .gemini_identities
            .identity_for(self.identity.as_ref(), &url)
        {
            Ok(identity) => identity,
            Err(error) => {
                tracing::warn!(%error, "failed to project Gemini client identity for feed");
                None
            }
        };
        Effect::FetchFeed {
            node,
            url,
            identity,
        }
    }

    pub(super) fn apply_feed_fetched(
        &mut self,
        node: uuid::Uuid,
        url: String,
        result: Result<FetchedPage, String>,
    ) -> Vec<Effect> {
        if !self.feeds.is_current(node, &url) {
            return Vec::new();
        }
        let current = self
            .graph_runtimes
            .graph_containing_member(node)
            .and_then(|graph| self.graph_runtimes.canvas(graph))
            .is_some_and(|canvas| crate::browse::still_current(canvas, node, &url));
        if !current {
            self.feeds.unsubscribe(node);
            return vec![Effect::SaveSession];
        }
        let now_ms = self.now_ms.unwrap_or_else(crate::denizen::now_ms);
        let fetched = match result {
            Ok(fetched) => fetched,
            Err(error) => return self.fail_feed_fetch(node, error, now_ms),
        };
        let parsed = match crate::feed::parse_document(&url, &fetched) {
            Ok(parsed) => parsed,
            Err(error) => return self.fail_feed_fetch(node, error, now_ms),
        };
        let Some(merge) = self.feeds.merge(node, parsed, now_ms) else {
            return Vec::new();
        };
        let changed = merge.entries.len();
        let graph = self
            .graph_runtimes
            .graph_containing_member(node)
            .expect("the current source has a graph runtime");
        if let Some(canvas) = self.graph_runtimes.canvas_mut(graph) {
            if let Some(title) = merge.title.filter(|title| !title.trim().is_empty()) {
                canvas.set_node_title_for(node, title);
            }
            for projection in merge.entries {
                let member = projection
                    .member
                    .filter(|member| {
                        canvas
                            .graph()
                            .get_node_by_id(*member)
                            .is_some_and(|(_, entry)| entry.url() == projection.entry.url)
                    })
                    .or_else(|| {
                        canvas
                            .graph()
                            .get_node_by_url(&projection.entry.url)
                            .map(|(_, entry)| entry.id)
                    })
                    .unwrap_or_else(|| {
                        let selected = canvas.selected_members();
                        let member =
                            canvas.open_member_as_new_node(Some(node), &projection.entry.url);
                        canvas.set_selected_members(&selected);
                        member
                    });
                canvas.assert_relation_between_members(
                    node,
                    member,
                    mere::kernel::graph::SemanticSubKind::Hyperlink,
                );
                canvas.set_node_title_for(member, projection.entry.title.clone());
                canvas.set_node_body_for(member, projection.entry.summary.clone());
                canvas.tag_node(member, FEED_ENTRY_TAG);
                canvas.tag_node(member, UNREAD_TAG);
                self.feeds.bind_entry(node, &projection.entry.url, member);
            }
        }
        let unread = self.feeds.unread_count();
        self.events.push(AppEvent::FeedRefreshed {
            node,
            changed,
            unread,
        });
        vec![Effect::SaveSession, Effect::Redraw]
    }

    fn fail_feed_fetch(&mut self, node: uuid::Uuid, error: String, now_ms: u64) -> Vec<Effect> {
        if self.feeds.fetched_error(node, error.clone(), now_ms) {
            self.events
                .push(AppEvent::FeedRefreshFailed { node, error });
            vec![Effect::SaveSession, Effect::Redraw]
        } else {
            Vec::new()
        }
    }

    pub(super) fn reconcile_feed_tags(&mut self) {
        let members: Vec<uuid::Uuid> = self
            .graph_runtimes
            .graph()
            .nodes()
            .map(|(_, node)| node.id)
            .collect();
        for member in members {
            let Some(info) = self.feeds.member_info(member) else {
                continue;
            };
            let Some(graph) = self.graph_runtimes.graph_containing_member(member) else {
                continue;
            };
            let Some(canvas) = self.graph_runtimes.canvas_mut(graph) else {
                continue;
            };
            match info {
                FeedMemberInfo::Source { .. } => {
                    canvas.tag_node(member, KEEP_TAG);
                    canvas.tag_node(member, FEED_TAG);
                }
                FeedMemberInfo::Entry { unread, .. } => {
                    canvas.tag_node(member, FEED_ENTRY_TAG);
                    if unread {
                        canvas.tag_node(member, UNREAD_TAG);
                    } else {
                        canvas.untag_node(member, UNREAD_TAG);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Action, Update};

    #[test]
    fn subscription_refresh_projects_unread_entries_once() {
        let mut app = App::test_stub();
        app.now_ms = Some(1_000);
        app.update(Action::OpenAddress(
            "gemini://capsule.test/feed.gmi".to_string(),
        ));
        let source = app.graph_runtimes.focused_member().unwrap();
        let effects = app.update(Action::SubscribeFocusedFeed {
            period: servitor::Period::Hour,
        });
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::FetchFeed { node, .. } if *node == source
        )));
        let source_key = app.graph_runtimes.graph().get_node_by_id(source).unwrap().0;
        let source_tags = app.graph_runtimes.graph().node_tags(source_key).unwrap();
        assert!(source_tags.contains(KEEP_TAG) && source_tags.contains(FEED_TAG));

        let fetched = FetchedPage {
            content_type: Some("text/gemini".into()),
            body: "# Log\n=> /one.gmi 2026-08-17 One\n=> /two.gmi 2026-08-18 Two\n".into(),
        };
        app.apply_update(Update::FeedFetched {
            node: source,
            url: "gemini://capsule.test/feed.gmi".into(),
            result: Ok(fetched.clone()),
        });
        assert_eq!(app.graph_runtimes.graph().nodes().count(), 3);
        assert_eq!(app.feeds.unread_count(), 2);
        let one = app
            .graph_runtimes
            .graph()
            .get_node_by_url("gemini://capsule.test/one.gmi")
            .unwrap()
            .1
            .id;
        let one_key = app.graph_runtimes.graph().get_node_by_id(one).unwrap().0;
        let tags = app.graph_runtimes.graph().node_tags(one_key).unwrap();
        assert!(tags.contains(FEED_ENTRY_TAG) && tags.contains(UNREAD_TAG));

        app.apply_update(Update::FeedFetched {
            node: source,
            url: "gemini://capsule.test/feed.gmi".into(),
            result: Ok(fetched),
        });
        assert_eq!(app.graph_runtimes.graph().nodes().count(), 3);
        assert_eq!(app.feeds.unread_count(), 2);
    }

    #[test]
    fn host_tick_runs_the_persisted_cadence() {
        let mut app = App::test_stub();
        app.now_ms = Some(0);
        app.update(Action::OpenAddress(
            "gemini://capsule.test/feed.gmi".to_string(),
        ));
        let source = app.graph_runtimes.focused_member().unwrap();
        app.update(Action::SubscribeFocusedFeed {
            period: servitor::Period::Hour,
        });
        app.apply_update(Update::FeedFetched {
            node: source,
            url: "gemini://capsule.test/feed.gmi".into(),
            result: Ok(FetchedPage {
                content_type: Some("text/gemini".into()),
                body: "=> /one 2026-08-18 One".into(),
            }),
        });

        assert!(
            !app.tick(3_599_999)
                .iter()
                .any(|effect| matches!(effect, Effect::FetchFeed { .. }))
        );
        assert!(app.tick(3_600_000).iter().any(|effect| matches!(
            effect,
            Effect::FetchFeed { node, .. } if *node == source
        )));
    }
}
