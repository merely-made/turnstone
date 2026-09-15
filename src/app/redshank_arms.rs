// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Redshank's episode tiles, where they meet Turnstone's graph.
//!
//! The listening authority itself is [`crate::redshank_host`]; this file owns
//! only what belongs to `App`: routing an enclosure to a tile, applying the
//! dock's commands, and projecting one library item, its progress, and its
//! notes into the graph so they reopen after a restart.

use chartulary::FacetId;
use redshank_model::ItemId;
use redshank_surfaces::{CompactCommand, CompactPlayerState};
use serde_json::json;
use uuid::Uuid;

use crate::action::Effect;
use crate::content_classes::{REDSHANK_NOTE_FACET, REDSHANK_PROGRESS_FACET};
use crate::redshank_episode_surface::{PANE_KIND, RedshankEpisodeSourceV1, episode_source};
use crate::redshank_host::HostOutcome;

use super::App;

/// The tag an episode note node carries, beside `feed-entry` and `unread` on
/// the episode itself.
pub const REDSHANK_NOTE_TAG: &str = "redshank-note";

/// One note's stable graph address. The node is durable session-graph state,
/// so a restart finds it without reattaching anything.
pub fn note_address(id: &redshank_model::AnnotationId) -> String {
    format!("mere://redshank/note/{}", id.0)
}

impl App {
    /// Open one podcast episode as a Redshank tile: the library item, the
    /// selection, the pane, and the graph association, in that order.
    pub fn open_redshank_episode(&mut self, payload: RedshankEpisodeSourceV1) -> Vec<Effect> {
        let item = payload.item_id();
        let outcome = self.redshank.open_item(payload.library_item());
        let mut effects = self.fold_redshank_outcome(&item, outcome);
        if let Some(member) = payload.member.or_else(|| self.redshank_member(&item)) {
            self.redshank_members.insert(item.clone(), member);
            self.write_redshank_progress(&item, member);
        }
        effects.extend(self.summon_contributed_pane(
            crate::panes::PaneKindId::new(PANE_KIND),
            episode_source(payload),
        ));
        effects
    }

    /// Apply the commands the mounted docks issued this frame.
    pub fn apply_redshank_commands(
        &mut self,
        commands: Vec<(ItemId, CompactCommand)>,
    ) -> Vec<Effect> {
        if commands.is_empty() {
            return Vec::new();
        }
        let now_ms = self.now_ms.unwrap_or_else(crate::denizen::now_ms);
        let mut effects = Vec::new();
        for (item, command) in commands {
            let outcome = self.redshank.command(&item, command, now_ms);
            effects.extend(self.fold_redshank_outcome(&item, outcome));
        }
        effects.push(Effect::Redraw);
        effects
    }

    /// Persist and project whatever one host command changed.
    fn fold_redshank_outcome(&mut self, item: &ItemId, outcome: HostOutcome) -> Vec<Effect> {
        if let Some(notice) = outcome.notice {
            self.events
                .push(crate::observe::AppEvent::RedshankRefused(notice));
        }
        let mut effects = Vec::new();
        if let Some(note) = outcome.added_note {
            self.project_redshank_note(item, &note);
            effects.push(Effect::SaveSession);
        }
        if outcome.model_changed {
            self.redshank.save();
            effects.push(Effect::Redraw);
        }
        effects
    }

    /// Record where playback reached and keep the member's progress field in
    /// step. Runs on the host's clock tick, beside the feed schedules.
    pub fn redshank_tick(&mut self, now_ms: u64) -> Vec<Effect> {
        if !self.redshank.record_progress(now_ms) {
            return Vec::new();
        }
        let Some(item) = self.redshank.selected().cloned() else {
            return Vec::new();
        };
        if let Some(member) = self.redshank_member(&item) {
            self.write_redshank_progress(&item, member);
        }
        self.redshank.save();
        vec![Effect::SaveSession, Effect::Redraw]
    }

    /// What a mounted dock should show for `item` right now.
    pub fn redshank_projection(&self, item: &ItemId) -> CompactPlayerState {
        self.redshank.project(item)
    }

    /// The graph member an episode projects to: the association this session
    /// already made, or the one recorded on a member's progress field (which
    /// is how it survives a restart).
    pub fn redshank_member(&self, item: &ItemId) -> Option<Uuid> {
        if let Some(member) = self.redshank_members.get(item) {
            return Some(*member);
        }
        // Node fields are derived and are not written to the session graph, so
        // the durable association is the pair of stores that ARE: the library
        // item names its feed and GUID, and the feed store names the member
        // it bound. Re-minted on adoption, never repaired.
        let redshank_model::LibraryItem::FeedEpisode {
            feed_url, guid, ..
        } = self.redshank.model().library.get(item)?
        else {
            return None;
        };
        self.feeds.episode_member(feed_url, guid)
    }

    /// Re-mint the derived Redshank node fields for this session.
    ///
    /// Progress and note anchors are inspector fields over durable model
    /// truth; a reopened session rebuilds them rather than reading them back
    /// out of the graph.
    pub(super) fn reconcile_redshank_fields(&mut self) {
        let items: Vec<ItemId> = self.redshank.model().library.keys().cloned().collect();
        for item in items {
            let Some(member) = self.redshank_member(&item) else {
                continue;
            };
            self.redshank_members.insert(item.clone(), member);
            self.write_redshank_progress(&item, member);
        }
        let notes: Vec<(ItemId, redshank_model::AnnotationId)> = self
            .redshank
            .model()
            .annotations
            .values()
            .map(|note| (note.target.item_id.clone(), note.id.clone()))
            .collect();
        for (item, note) in notes {
            self.project_redshank_note(&item, &note);
        }
    }

    /// Write the episode's progress onto its member as an inspectable field.
    fn write_redshank_progress(&mut self, item: &ItemId, member: Uuid) {
        let progress = self.redshank.model().progress.get(item).cloned();
        let value = json!({
            "version": 1,
            "item_id": item.0,
            "position_ms": progress.as_ref().map_or(0, |progress| progress.position_ms),
            "completed": progress.as_ref().is_some_and(|progress| progress.completed),
            "updated_at_ms": progress.as_ref().map_or(0, |progress| progress.updated_at_ms),
        });
        let validator = crate::content_classes::BuiltinContentClasses::new().validator;
        let Some(graph) = self.graph_runtimes.graph_containing_member(member) else {
            return;
        };
        let Some(canvas) = self.graph_runtimes.canvas_mut(graph) else {
            return;
        };
        if let Err(error) =
            canvas
                .facets_mut()
                .set(member, FacetId::new(REDSHANK_PROGRESS_FACET), value, &validator)
        {
            tracing::warn!(?error, "the episode's progress field was refused");
        }
    }

    /// Project one timed note as a graph node hanging off its episode.
    fn project_redshank_note(&mut self, item: &ItemId, id: &redshank_model::AnnotationId) {
        let Some(note) = self.redshank.annotation(id).cloned() else {
            return;
        };
        let redshank_model::NoteBody::Text { plain_text } = &note.body else {
            return;
        };
        let member = self.redshank_member(item);
        let address = note_address(id);
        let validator = crate::content_classes::BuiltinContentClasses::new().validator;
        let graph = member
            .and_then(|member| self.graph_runtimes.graph_containing_member(member))
            .unwrap_or_else(|| self.graph_runtimes.active_graph());
        let title = note_title(plain_text, note.target.offset_ms);
        let Some(canvas) = self.graph_runtimes.canvas_mut(graph) else {
            return;
        };
        let existing = canvas
            .graph()
            .get_node_by_url(&address)
            .map(|(_, node)| node.id);
        let node = existing.unwrap_or_else(|| {
            let selected = canvas.selected_members();
            let node = canvas.open_member_as_new_node(member, &address);
            canvas.set_selected_members(&selected);
            node
        });
        canvas.set_node_title_for(node, title);
        canvas.set_node_body_for(node, Some(plain_text.clone()));
        canvas.tag_node(node, REDSHANK_NOTE_TAG);
        if let Some(member) = member {
            canvas.assert_relation_between_members(
                member,
                node,
                mere::kernel::graph::SemanticSubKind::Quotes,
            );
        }
        let value = json!({
            "version": 1,
            "item_id": item.0,
            "annotation_id": id.0,
            "offset_ms": note.target.offset_ms,
        });
        if let Err(error) =
            canvas
                .facets_mut()
                .set(node, FacetId::new(REDSHANK_NOTE_FACET), value, &validator)
        {
            tracing::warn!(?error, "the note's anchor field was refused");
        }
    }

    /// The one decision point: does this address open as a listening tile?
    ///
    /// An enclosure named by a subscribed feed entry wins first, because it
    /// carries the show, the GUID, and the artwork. A bare audio address
    /// still opens the dock, anchored on the address itself.
    pub(crate) fn redshank_episode_for_address(
        &self,
        url: &str,
    ) -> Option<RedshankEpisodeSourceV1> {
        if let Some(facts) = self.feeds.episode_for_address(url) {
            return Some(RedshankEpisodeSourceV1 {
                feed_url: facts.feed_url,
                guid: facts.guid,
                enclosure_url: facts.enclosure_url,
                title: facts.title,
                published: facts.published,
                artwork: facts.artwork,
                member: facts.member,
                media_type: facts.media_type,
            });
        }
        if !crate::feed::is_podcast_audio(url, None) {
            return None;
        }
        Some(RedshankEpisodeSourceV1 {
            feed_url: String::new(),
            guid: url.to_owned(),
            enclosure_url: url.to_owned(),
            title: url
                .rsplit('/')
                .next()
                .filter(|name| !name.is_empty())
                .unwrap_or(url)
                .to_owned(),
            published: None,
            artwork: None,
            member: None,
            media_type: None,
        })
    }
}

/// A note's node title: its first line, elided, with the anchor in front so
/// the graph reads in source time.
fn note_title(plain_text: &str, offset_ms: u64) -> String {
    let seconds = offset_ms / 1000;
    let stamp = format!("{}:{:02}", seconds / 60, seconds % 60);
    let first = plain_text.lines().next().unwrap_or("").trim();
    let mut body: String = first.chars().take(48).collect();
    if first.chars().count() > 48 {
        body.push('…');
    }
    if body.is_empty() {
        format!("Note at {stamp}")
    } else {
        format!("{stamp} {body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Action, FetchedPage, Update};
    use crate::panes::{PaneContent, PaneKindId, SessionId};
    use crate::redshank_host::Output;
    use redshank_model::{AnnotationId, CaptureAnchor, RepresentationReceipt};
    use std::path::{Path, PathBuf};

    const FEED: &str = "https://podcast.test/feed.xml";
    const PAGE: &str = "https://podcast.test/episodes/one";
    const ENCLOSURE: &str = "https://cdn.podcast.test/one.mp3";

    fn rss() -> FetchedPage {
        FetchedPage::text(
            Some("application/rss+xml".into()),
            format!(
                "<rss version=\"2.0\"><channel><title>Pod</title><item><guid>episode-1</guid>\
                 <title>Marsh at dawn</title><link>{PAGE}</link>\
                 <enclosure url=\"{ENCLOSURE}\" type=\"audio/mpeg\" /></item></channel></rss>"
            ),
        )
    }

    fn scratch_root() -> PathBuf {
        std::env::temp_dir().join(format!("turnstone-redshank-{}", uuid::Uuid::new_v4()))
    }

    /// An app on `root`/`session` with one subscribed feed and its episode.
    fn subscribed(root: &Path, session: SessionId) -> App {
        let mut app = App::test_stub();
        app.data_root = root.to_path_buf();
        std::fs::create_dir_all(crate::session::session_dir(root, session)).unwrap();
        app.adopt_session(session);
        app.now_ms = Some(1_000);
        // Visited directly on the active canvas rather than through the
        // omnibar: an adopted session installs its own pane selection, so
        // `OpenAddress` would leave the node in a pane the test cannot name.
        let key = app.graph_runtimes.visit(FEED);
        let source = app.graph_runtimes.graph().get_node(key).unwrap().id;
        app.feeds
            .subscribe(source, FEED.to_string(), servitor::Period::Hour);
        app.feeds.start(source);
        app.apply_update(Update::FeedFetched {
            node: source,
            url: FEED.into(),
            result: Ok(rss()),
        });
        app
    }

    fn episode_panes(app: &App) -> usize {
        app.frisket
            .iter_leaves()
            .filter(|(_, content, _)| {
                *content == &PaneContent::Registered(PaneKindId::new(PANE_KIND))
            })
            .count()
    }

    #[test]
    fn opening_an_enclosure_yields_a_redshank_episode_pane() {
        let root = scratch_root();
        let mut app = subscribed(&root, SessionId::new());
        assert_eq!(episode_panes(&app), 0);

        // The entry's own page routes to the tile, because the feed says it
        // carries playable audio.
        app.update(Action::OpenAddress(PAGE.to_string()));
        assert_eq!(episode_panes(&app), 1);
        let item = crate::redshank_episode_surface::item_id(FEED, "episode-1");
        assert!(app.redshank.model().library.contains_key(&item));
        assert_eq!(app.redshank.selected(), Some(&item));

        // A bare audio address opens the tile too, anchored on itself.
        let mut bare = App::test_stub();
        bare.data_root = scratch_root();
        bare.update(Action::OpenAddress(
            "https://elsewhere.test/talk.m4a".to_string(),
        ));
        assert_eq!(episode_panes(&bare), 1);

        // A page that is not audio still opens the reader.
        let mut reader = App::test_stub();
        reader.data_root = scratch_root();
        reader.update(Action::OpenAddress("https://elsewhere.test/post".to_string()));
        assert_eq!(episode_panes(&reader), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_note_and_progress_project_into_the_graph_and_reopen_after_a_restart() {
        let root = scratch_root();
        let session = SessionId::new();
        let sdir = crate::session::session_dir(&root, session);
        let item = crate::redshank_episode_surface::item_id(FEED, "episode-1");

        let mut app = subscribed(&root, session);
        app.update(Action::OpenAddress(PAGE.to_string()));
        let member = app.redshank_member(&item).expect("the episode's member");

        let anchor = CaptureAnchor {
            item_id: item.clone(),
            offset_ms: 62_000,
            end_offset_ms: None,
            pressed_offset_ms: None,
            representation: RepresentationReceipt::default(),
        };
        // Begin freezes a capture target; cancelling releases it, so the
        // editor's own anchor is the one that reaches the model.
        app.apply_redshank_commands(vec![(item.clone(), CompactCommand::BeginTextNote)]);
        assert_eq!(
            app.redshank.pending_anchor(&item).map(|a| a.item_id.clone()),
            Some(item.clone())
        );
        app.apply_redshank_commands(vec![(item.clone(), CompactCommand::CancelTextNote)]);
        assert!(app.redshank.pending_anchor(&item).is_none());
        let effects = app.apply_redshank_commands(vec![(
            item.clone(),
            CompactCommand::SaveTextNote {
                anchor,
                plain_text: "The bittern calls here".into(),
            },
        )]);
        assert!(effects.iter().any(|effect| matches!(effect, Effect::SaveSession)));

        let note_id = AnnotationId("note-0".into());
        let address = note_address(&note_id);
        let node = app
            .graph_runtimes
            .graph()
            .get_node_by_url(&address)
            .expect("the note projects as a graph node")
            .1
            .id;
        let key = app.graph_runtimes.graph().get_node_by_id(node).unwrap().0;
        let tags = app.graph_runtimes.graph().node_tags(key).unwrap();
        assert!(tags.contains(REDSHANK_NOTE_TAG));
        assert!(
            app.graph_runtimes
                .graph()
                .get_node_by_id(node)
                .unwrap()
                .1
                .body
                .as_deref()
                == Some("The bittern calls here")
        );
        let anchor_field = app
            .graph_runtimes
            .facets()
            .get(&node, &FacetId::new(REDSHANK_NOTE_FACET))
            .expect("the note carries its anchor as a field");
        assert_eq!(anchor_field.get("offset_ms").and_then(|v| v.as_u64()), Some(62_000));
        let progress_field = app
            .graph_runtimes
            .facets()
            .get(&member, &FacetId::new(REDSHANK_PROGRESS_FACET))
            .expect("the episode carries progress as a field");
        assert_eq!(
            progress_field.get("item_id").and_then(|v| v.as_str()),
            Some(item.0.as_str())
        );

        // Restart: the session graph and the listening model are both durable.
        crate::session::save_session_graph(&sdir, app.graph_runtimes.graph());
        app.feeds.save(&sdir).unwrap();
        drop(app);

        let mut reopened = App::test_stub();
        reopened.data_root = root.clone();
        reopened.adopt_session(session);
        assert_eq!(reopened.redshank.output(), Output::Silent);
        let reborn = reopened
            .graph_runtimes
            .graph()
            .get_node_by_url(&address)
            .expect("the note node reopens with the session graph")
            .1
            .id;
        assert_eq!(reborn, node);
        assert!(reopened.redshank.annotation(&note_id).is_some());
        assert_eq!(
            reopened.redshank.model().library.get(&item).map(|item| item.title()),
            Some("Marsh at dawn")
        );
        assert_eq!(reopened.redshank_member(&item), Some(member));
        // The derived fields are re-minted on adoption, so the inspector
        // shows the reopened note's anchor and the episode's progress again.
        assert!(
            reopened
                .graph_runtimes
                .facets()
                .get(&member, &FacetId::new(REDSHANK_PROGRESS_FACET))
                .is_some()
        );
        assert_eq!(
            reopened
                .graph_runtimes
                .facets()
                .get(&reborn, &FacetId::new(REDSHANK_NOTE_FACET))
                .and_then(|value| value.get("offset_ms"))
                .and_then(|value| value.as_u64()),
            Some(62_000)
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
