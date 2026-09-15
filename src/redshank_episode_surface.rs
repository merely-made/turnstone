// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone admission for Redshank's compact listening dock.
//!
//! The fourth independent provider over the contributed-surface seam, and the
//! first from outside Mere's own workspace: the descriptor, stylesheet, and
//! erased session all come from `redshank-surfaces`, so there is no Turnstone
//! copy of the dock. The versioned source carries only projection facts the
//! feed already selected — mounting a tile grants a projection, never
//! Redshank's library, store, or device authority, which the app owns once in
//! [`crate::redshank_host`].

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use cambium::{DomHandle, RetainedSurfaceSession};
use mere_surface_api::SurfaceDescriptor;
use redshank_model::{FeedEpisodeFacts, ItemId, LibraryItem, MediaSource};
use redshank_surfaces::surface_api::{
    CompactDock, EPISODE_SOURCE_KIND, compact_descriptor, compact_session, compact_stylesheet,
};
use redshank_surfaces::{
    CompactCommand, CompactPlayerState, Face, Mode, NowPlaying, Seed, SourceKind, TransportState,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::contributed_surface::{SurfaceAdmissionError, SurfaceProvider};
use crate::panes::{PaneKindId, PaneSource, SerializedSource, SourceRef, SourceSchemaId};

pub const PANE_KIND: &str = "turnstone.redshank-episode";
/// The schema a pane source must name. It is Redshank's word, not Turnstone's:
/// the port publishes the source kind its descriptor accepts, and a registry
/// refuses a provider whose schema disagrees with the descriptor.
pub const SOURCE_SCHEMA: &str = EPISODE_SOURCE_KIND;
pub const SOURCE_VERSION: u32 = 1;

/// The durable source payload for one podcast episode tile.
///
/// Every field is a projection fact Turnstone's feed store already holds.
/// Nothing here opens a feed, a cache, or an output device.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RedshankEpisodeSourceV1 {
    pub feed_url: String,
    pub guid: String,
    pub enclosure_url: String,
    pub title: String,
    #[serde(default)]
    pub published: Option<String>,
    #[serde(default)]
    pub artwork: Option<String>,
    /// The graph member this episode already projects to, when the feed bound
    /// one. Progress and notes hang off it.
    #[serde(default)]
    pub member: Option<Uuid>,
    #[serde(default)]
    pub media_type: Option<String>,
}

impl RedshankEpisodeSourceV1 {
    /// The library identity: GUID-stable, as the feed projection is, so a
    /// moved enclosure keeps one item, its progress, and its notes.
    pub fn item_id(&self) -> ItemId {
        item_id(&self.feed_url, &self.guid)
    }

    /// Mint the library item these projection facts describe. Nothing here
    /// opens a feed, a store, or a device.
    pub fn library_item(&self) -> LibraryItem {
        LibraryItem::FeedEpisode {
            id: self.item_id(),
            feed_url: self.feed_url.clone(),
            guid: self.guid.clone(),
            title: self.title.clone(),
            source: MediaSource::Enclosure {
                url: self.enclosure_url.clone(),
            },
            facts: Box::new(FeedEpisodeFacts {
                published: self.published.clone(),
                artwork: self.artwork.clone(),
                enclosure_media_type: self.media_type.clone(),
                ..FeedEpisodeFacts::default()
            }),
        }
    }
}

/// One library identity from a feed URL and an episode GUID.
pub fn item_id(feed_url: &str, guid: &str) -> ItemId {
    ItemId(format!("{feed_url}#{guid}"))
}

/// Mint the versioned pane source for one episode tile.
pub fn episode_source(payload: RedshankEpisodeSourceV1) -> PaneSource {
    PaneSource::Fixed(SourceRef::External {
        schema: SourceSchemaId::new(SOURCE_SCHEMA),
        payload: SerializedSource {
            version: SOURCE_VERSION,
            payload: serde_json::to_value(payload)
                .expect("a data-only Redshank episode source is serializable"),
        },
    })
}

/// Every mounted dock, by library identity.
///
/// One dock per episode, not per pane: the app owns one Redshank authority,
/// so two tiles on the same episode are two views of one transport. The
/// handles are `Rc`-shared with the admitted sessions, so this lives in the
/// shell beside the other retained product state.
#[derive(Clone, Default)]
pub struct RedshankDocks {
    docks: Rc<RefCell<BTreeMap<ItemId, CompactDock>>>,
}

impl RedshankDocks {
    pub fn new() -> Self {
        Self::default()
    }

    /// The dock for `item`, minting one seeded with `seed` if it is new.
    fn dock(&self, item: &ItemId, seed: CompactPlayerState) -> CompactDock {
        self.docks
            .borrow_mut()
            .entry(item.clone())
            .or_insert_with(|| CompactDock::new(seed))
            .clone()
    }

    pub fn items(&self) -> Vec<ItemId> {
        self.docks.borrow().keys().cloned().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.docks.borrow().is_empty()
    }

    /// Show `state` on `item`'s dock, if one is mounted.
    pub fn project(&self, item: &ItemId, state: CompactPlayerState) {
        if let Some(dock) = self.docks.borrow().get(item) {
            dock.project(state);
        }
    }

    /// Take every command the listener issued on every mounted dock.
    pub fn drain(&self) -> Vec<(ItemId, CompactCommand)> {
        let docks = self.docks.borrow();
        let mut drained = Vec::new();
        for (item, dock) in docks.iter() {
            drained.extend(
                dock.drain()
                    .into_iter()
                    .map(|command| (item.clone(), command)),
            );
        }
        drained
    }

    /// Queue a command as the dock would. Turnstone's own chrome (a note
    /// editor beside the tile) speaks the same vocabulary.
    pub fn request(&self, item: &ItemId, command: CompactCommand) -> bool {
        let docks = self.docks.borrow();
        let Some(dock) = docks.get(item) else {
            return false;
        };
        dock.request(command);
        true
    }
}

/// What a tile shows before the host's first projection reaches it: the facts
/// the source payload carries, and an honest "loading" transport.
fn seed_state(source: &RedshankEpisodeSourceV1) -> CompactPlayerState {
    let mut state = CompactPlayerState::default();
    state.transport = TransportState::Buffering;
    state.voice_capture_available = false;
    state.now_playing = Some(NowPlaying {
        item_id: source.item_id(),
        title: source.title.clone(),
        feed_title: url::Url::parse(&source.feed_url)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_owned)),
        face: match source.artwork.clone() {
            Some(artwork) => Face::Artwork(artwork),
            None => Face::Tag("audio".into()),
        },
        source: SourceKind::Cloud,
        position_ms: 0,
        duration_ms: None,
        resumed_from_ms: None,
        buffered_percent: 0,
        markers: Vec::new(),
    });
    state
}

/// The registry adapter for one Redshank episode tile.
pub struct RedshankEpisodeProvider {
    pane_kind: PaneKindId,
    source_schema: SourceSchemaId,
    descriptor: SurfaceDescriptor,
    stylesheet: String,
    docks: RedshankDocks,
}

impl RedshankEpisodeProvider {
    pub fn new(docks: RedshankDocks) -> Self {
        Self {
            pane_kind: PaneKindId::new(PANE_KIND),
            source_schema: SourceSchemaId::new(SOURCE_SCHEMA),
            descriptor: compact_descriptor(),
            stylesheet: compact_stylesheet(),
            docks,
        }
    }

    pub fn docks(&self) -> &RedshankDocks {
        &self.docks
    }
}

impl Default for RedshankEpisodeProvider {
    fn default() -> Self {
        Self::new(RedshankDocks::new())
    }
}

impl SurfaceProvider for RedshankEpisodeProvider {
    fn pane_kind(&self) -> &PaneKindId {
        &self.pane_kind
    }

    fn source_schema(&self) -> &SourceSchemaId {
        &self.source_schema
    }

    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }

    fn stylesheet(&self) -> &str {
        &self.stylesheet
    }

    fn admit(
        &self,
        source: &PaneSource,
        dom: DomHandle,
    ) -> Result<Box<dyn RetainedSurfaceSession>, SurfaceAdmissionError> {
        let PaneSource::Fixed(SourceRef::External { schema, payload }) = source else {
            return Err(SurfaceAdmissionError::InvalidSource {
                expected: self.source_schema.clone(),
                actual: None,
            });
        };
        if schema != &self.source_schema {
            return Err(SurfaceAdmissionError::InvalidSource {
                expected: self.source_schema.clone(),
                actual: Some(schema.clone()),
            });
        }
        if payload.version != SOURCE_VERSION {
            return Err(invalid_payload(format!(
                "version {} is not supported; expected {SOURCE_VERSION}",
                payload.version
            )));
        }
        let source: RedshankEpisodeSourceV1 = serde_json::from_value(payload.payload.clone())
            .map_err(|error| invalid_payload(error.to_string()))?;
        if source.enclosure_url.trim().is_empty() {
            return Err(invalid_payload("the episode has no enclosure".to_owned()));
        }
        if source.guid.trim().is_empty() {
            return Err(invalid_payload("the episode has no stable guid".to_owned()));
        }
        let dock = self.docks.dock(&source.item_id(), seed_state(&source));
        Ok(compact_session(dom, dock, Seed::Wetland, Mode::Dark))
    }
}

fn invalid_payload(message: String) -> SurfaceAdmissionError {
    SurfaceAdmissionError::InvalidPayload {
        schema: SourceSchemaId::new(SOURCE_SCHEMA),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contributed_surface::SurfaceProviderRegistry;
    use cambium::{PointerClick, SurfaceViewport};
    use genet_scripted_dom::NodeId;
    use layout_dom_api::{LayoutDom, LocalName, Namespace};

    fn payload() -> RedshankEpisodeSourceV1 {
        RedshankEpisodeSourceV1 {
            feed_url: "https://podcast.test/feed.xml".into(),
            guid: "episode-1".into(),
            enclosure_url: "https://cdn.podcast.test/one.mp3".into(),
            title: "Marsh at dawn".into(),
            published: Some("2026-09-01".into()),
            artwork: None,
            member: None,
            media_type: Some("audio/mpeg".into()),
        }
    }

    fn registry(docks: &RedshankDocks) -> SurfaceProviderRegistry {
        let mut registry = SurfaceProviderRegistry::new();
        registry
            .register_provider(RedshankEpisodeProvider::new(docks.clone()))
            .expect("the Redshank episode provider registers");
        registry
    }

    fn node_with_label(dom: &genet_scripted_dom::ScriptedDom, root: NodeId, label: &str) -> NodeId {
        let aria = LocalName::from("aria-label");
        let empty = Namespace::from("");
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            if dom
                .attribute(node, &empty, &aria)
                .is_some_and(|value| value == label)
            {
                return node;
            }
            pending.extend(dom.dom_children(node));
        }
        panic!("missing control {label}");
    }

    fn text_present(dom: &genet_scripted_dom::ScriptedDom, needle: &str) -> bool {
        fn contains(
            dom: &genet_scripted_dom::ScriptedDom,
            node: NodeId,
            needle: &str,
        ) -> bool {
            dom.text(node).is_some_and(|text| text.contains(needle))
                || dom
                    .dom_children(node)
                    .any(|child| contains(dom, child, needle))
        }
        contains(dom, dom.document(), needle)
    }

    #[test]
    fn the_registry_admits_the_published_redshank_dock_from_a_versioned_source() {
        let docks = RedshankDocks::new();
        let pane = registry(&docks)
            .admit(&PaneKindId::new(PANE_KIND), &episode_source(payload()))
            .expect("Redshank episode admission");
        assert_eq!(pane.descriptor(), &compact_descriptor());
        assert_eq!(pane.descriptor().provider_id.as_str(), "redshank");
        assert_eq!(pane.descriptor().surface_id.as_str(), "redshank.compact");
        assert_eq!(docks.items(), [payload().item_id()]);
    }

    #[test]
    fn a_wrong_version_or_a_missing_enclosure_is_refused_before_admission() {
        let docks = RedshankDocks::new();
        let mut source = episode_source(payload());
        let PaneSource::Fixed(SourceRef::External { payload: body, .. }) = &mut source else {
            unreachable!()
        };
        body.version = SOURCE_VERSION + 1;
        assert!(matches!(
            registry(&docks).admit(&PaneKindId::new(PANE_KIND), &source),
            Err(SurfaceAdmissionError::InvalidPayload { .. })
        ));

        let mut headless = payload();
        headless.enclosure_url = String::new();
        assert!(matches!(
            registry(&docks).admit(&PaneKindId::new(PANE_KIND), &episode_source(headless)),
            Err(SurfaceAdmissionError::InvalidPayload { .. })
        ));

        let wrong_schema = PaneSource::Fixed(SourceRef::External {
            schema: SourceSchemaId::new("knot.document.v1"),
            payload: SerializedSource {
                version: 1,
                payload: serde_json::json!({}),
            },
        });
        assert!(matches!(
            registry(&docks).admit(&PaneKindId::new(PANE_KIND), &wrong_schema),
            Err(SurfaceAdmissionError::InvalidSource { .. })
        ));
        assert!(docks.is_empty());
    }

    #[test]
    fn the_admitted_surface_renders_the_episode_title_and_a_play_control() {
        let docks = RedshankDocks::new();
        let mut pane = registry(&docks)
            .admit(&PaneKindId::new(PANE_KIND), &episode_source(payload()))
            .expect("Redshank episode admission");
        let mut projected = CompactPlayerState::default();
        projected.transport = TransportState::Paused;
        projected.now_playing = seed_state(&payload()).now_playing;
        docks.project(&payload().item_id(), projected);
        let _ = pane.scene(720, 160, 1.0);
        let dom = pane.dom_ref();
        assert!(text_present(&dom, "Marsh at dawn"));
        assert!(text_present(&dom, "PODCAST.TEST"));
        let _ = node_with_label(&dom, pane.session().root(), "Play");
    }

    /// The done-condition the port plan calls "the same action and snapshot
    /// contract": the assertions `redshank-surfaces`' own dock tests make,
    /// re-run against a session admitted through Turnstone's registry.
    #[test]
    fn the_dock_emits_the_same_commands_through_turnstones_registry() {
        let docks = RedshankDocks::new();
        let item = payload().item_id();
        let mut pane = registry(&docks)
            .admit(&PaneKindId::new(PANE_KIND), &episode_source(payload()))
            .expect("Redshank episode admission");
        let mut playing = CompactPlayerState::default();
        playing.transport = TransportState::Playing;
        playing.now_playing = seed_state(&payload()).now_playing;
        docks.project(&item, playing);
        let _ = pane.scene(720, 160, 1.0);

        let pause = node_with_label(&pane.dom_ref(), pane.session().root(), "Pause");
        pane.session_mut()
            .dispatch(cambium::ResolvedSurfaceEvent::Click {
                target: pause,
                event: PointerClick::at((1.0, 1.0)),
            });
        assert_eq!(docks.drain(), [(item.clone(), CompactCommand::Pause)]);

        let note = node_with_label(&pane.dom_ref(), pane.session().root(), "Add text note");
        pane.session_mut()
            .dispatch(cambium::ResolvedSurfaceEvent::Click {
                target: note,
                event: PointerClick::at((1.0, 1.0)),
            });
        assert_eq!(docks.drain(), [(item.clone(), CompactCommand::AddTextNote)]);

        // Voice is disabled honestly: Turnstone has no microphone adapter.
        let voice = node_with_label(
            &pane.dom_ref(),
            pane.session().root(),
            "Hold to record voice note",
        );
        pane.session_mut()
            .dispatch(cambium::ResolvedSurfaceEvent::Click {
                target: voice,
                event: PointerClick::at((1.0, 1.0)),
            });
        assert!(docks.drain().is_empty());
    }

    #[test]
    fn a_host_projection_reaches_the_tile_through_the_viewport_pump() {
        let docks = RedshankDocks::new();
        let item = payload().item_id();
        let mut pane = registry(&docks)
            .admit(&PaneKindId::new(PANE_KIND), &episode_source(payload()))
            .expect("Redshank episode admission");
        let mut unavailable = CompactPlayerState::default();
        unavailable.transport = TransportState::Unavailable("No audio output".into());
        unavailable.now_playing = seed_state(&payload()).now_playing;
        docks.project(&item, unavailable);
        pane.session_mut().sync_viewport(SurfaceViewport {
            width: 720.0,
            height: 160.0,
            scale_factor: 1.0,
        });
        assert!(!pane.availability().is_available());
        assert!(text_present(&pane.dom_ref(), "No audio output"));
    }
}
