// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The retained Workbench uses Pelt for content; graph actions stay with its owner.
use super::*;
use graphshell::retained_content::ClipExportScope;
use graphshell::retained_content::{ContentHost, ContentProducer, ContentSlot, HeldContent};
use mere_document_lanes::reading_lifecycle::OperationKind;
use std::collections::BTreeMap;
use uuid::Uuid;
use workbench::{ContentSource, TileTree};

struct Binding {
    restored: bool,
    key: u64,
    slot: Rc<RefCell<ContentSlot>>,
    producer: Rc<RefCell<ContentProducer>>,
}
type ClipAnswer = Rc<RefCell<Option<Result<HeldContent, String>>>>;
struct PendingClip {
    address: String,
    ticket: mere_document_lanes::reading_lifecycle::OperationTicket,
    answer: ClipAnswer,
}
type ExportAnswer =
    Rc<RefCell<Option<Result<mere_document_lanes::retained_clip::ClipExport, String>>>>;
struct PendingExport {
    scope: ClipExportScope,
    ticket: mere_document_lanes::reading_lifecycle::OperationTicket,
    answer: ExportAnswer,
    cancellation: Rc<RefCell<ExportCancellation>>,
}
#[derive(Default)]
struct ExportCancellation {
    cancelled: bool,
    waker: Option<std::task::Waker>,
}
impl Drop for PendingExport {
    fn drop(&mut self) {
        let waker = {
            let mut cancellation = self.cancellation.borrow_mut();
            cancellation.cancelled = true;
            cancellation.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}
struct DownloadUrl {
    url: String,
    revoke_at: f64,
}
impl Drop for DownloadUrl {
    fn drop(&mut self) {
        let _ = web_sys::Url::revoke_object_url(&self.url);
    }
}
struct MediaPlayer {
    element: web_sys::HtmlAudioElement,
    clip: eidetic::ManifestId,
}
impl Drop for MediaPlayer {
    fn drop(&mut self) {
        let _ = self.element.pause();
    }
}
pub(super) struct Reading {
    host: ContentHost,
    lifecycle: mere_document_lanes::reading_lifecycle::ReadingLifecycle<
        mere::kernel::graph::SurfaceNodeLifetime,
    >,
    bindings: BTreeMap<Uuid, Binding>,
    registered: BTreeMap<u64, Rc<RefCell<ContentProducer>>>,
    statuses: BTreeMap<Uuid, String>,
    clips: BTreeMap<Uuid, PendingClip>,
    scope: String,
    persona: String,
    current_owner: Option<(Uuid, String)>,
    exports: BTreeMap<Uuid, PendingExport>,
    download_urls: BTreeMap<Uuid, DownloadUrl>,
    players: BTreeMap<Uuid, MediaPlayer>,
    media_answers: BTreeMap<
        Uuid,
        (
            mere_document_lanes::reading_lifecycle::OperationTicket,
            Rc<RefCell<Option<String>>>,
        ),
    >,
    requests: Vec<(Uuid, String)>,
    dictionaries: Dictionaries,
    pending_progress: Option<(BTreeMap<Uuid, tabard::ReadingAnchor>, f64)>,
}

/// Segmenter dictionaries fetched beside the page when reading text needs
/// them, rather than compiled into it (Thai, Lao, Khmer and Burmese breaking;
/// Chinese and Japanese word boundaries).
#[derive(Default)]
struct Dictionaries {
    requested: mere_document_lanes::SegmenterDictionaries,
    held: Vec<mere_document_lanes::DictionaryBlob>,
    arrived: Rc<RefCell<Vec<Result<mere_document_lanes::DictionaryBlob, String>>>>,
    in_flight: usize,
}

impl Dictionaries {
    const SOUTHEAST_ASIAN: &str = "dictionaries/segmenter-southeast-asian.dict";
    const CHINESE_JAPANESE: &str = "dictionaries/segmenter-chinese-japanese.dict";

    /// Fetch what `host`'s sessions want and nobody has asked for; hand
    /// arrivals to the host. True when a session must lay out again.
    fn poll(&mut self, host: &mut ContentHost) -> bool {
        let arrived: Vec<_> = self.arrived.borrow_mut().drain(..).collect();
        let mut loaded = false;
        for result in arrived {
            self.in_flight -= 1;
            match result {
                Ok(blob) => {
                    self.held.push(blob);
                    loaded = true;
                },
                Err(error) => web_sys::console::warn_1(&error.into()),
            }
        }
        if loaded {
            host.set_segmenter_dictionaries(self.held.clone());
        }
        let wanted = host
            .wanted_segmenter_dictionaries()
            .missing_from(self.requested);
        for (want, url) in [
            (wanted.southeast_asian, Self::SOUTHEAST_ASIAN),
            (wanted.chinese_japanese, Self::CHINESE_JAPANESE),
        ] {
            if want {
                self.fetch(url);
            }
        }
        self.requested.southeast_asian |= wanted.southeast_asian;
        self.requested.chinese_japanese |= wanted.chinese_japanese;
        loaded
    }

    fn fetch(&mut self, url: &'static str) {
        self.in_flight += 1;
        let arrived = self.arrived.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = fetch_bytes(url)
                .await
                .and_then(|bytes| {
                    mere_document_lanes::DictionaryBlob::new(bytes).map_err(|e| e.to_string())
                })
                .map_err(|error| format!("segmenter dictionary {url}: {error}"));
            arrived.borrow_mut().push(result);
        });
    }
}

async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    let window = web_sys::window().ok_or("no browser window")?;
    let response = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|_| "fetch failed")?
        .dyn_into::<web_sys::Response>()
        .map_err(|_| "not a response")?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    let buffer =
        wasm_bindgen_futures::JsFuture::from(response.array_buffer().map_err(|_| "no body")?)
            .await
            .map_err(|_| "body read failed")?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

impl Default for Reading {
    fn default() -> Self {
        Self {
            host: ContentHost::with_native_font_roles(
                vec![
                    include_bytes!("../../web/Newsreader-Regular.ttf").to_vec(),
                    include_bytes!("../../web/IBMPlexSans-Regular.ttf").to_vec(),
                    include_bytes!("../../web/IBMPlexMono-Regular.ttf").to_vec(),
                    include_bytes!("../../web/NotoSansMath-Regular.ttf").to_vec(),
                ],
                vec![
                    "Newsreader".into(),
                    "IBM Plex Sans".into(),
                    "Noto Sans Math".into(),
                ],
                vec![
                    (
                        "serif".into(),
                        include_bytes!("../../web/Newsreader-Regular.ttf").to_vec(),
                    ),
                    (
                        "sans-serif".into(),
                        include_bytes!("../../web/IBMPlexSans-Regular.ttf").to_vec(),
                    ),
                    (
                        "monospace".into(),
                        include_bytes!("../../web/IBMPlexMono-Regular.ttf").to_vec(),
                    ),
                ],
            ),
            lifecycle: Default::default(),
            bindings: Default::default(),
            registered: Default::default(),
            statuses: Default::default(),
            clips: Default::default(),
            scope: String::new(),
            persona: String::new(),
            current_owner: None,
            exports: Default::default(),
            download_urls: Default::default(),
            players: Default::default(),
            media_answers: Default::default(),
            requests: Default::default(),
            dictionaries: Default::default(),
            pending_progress: None,
        }
    }
}

pub(super) fn clip_limits() -> Result<mere_document_lanes::retained_clip::ClipLimits, String> {
    let params = web_sys::UrlSearchParams::new_with_str(
        &web_sys::window()
            .ok_or("No browser host")?
            .location()
            .search()
            .map_err(|_| "No page options")?,
    )
    .map_err(|_| "Invalid page options")?;
    let setting = |name: &str, default: u64| -> Result<u64, String> {
        match params.get(name) {
            Some(value) => value
                .parse::<u64>()
                .ok()
                .filter(|v| *v > 0)
                .ok_or_else(|| format!("Invalid {name}")),
            None => Ok(default),
        }
    };
    Ok(mere_document_lanes::retained_clip::ClipLimits {
        max_parts: usize::try_from(setting("clip-max-parts", 64)?).map_err(|e| e.to_string())?,
        max_bytes: setting("clip-max-bytes", 16 * 1024 * 1024)?,
        max_record_bytes: usize::try_from(setting("clip-max-record-bytes", 1024 * 1024)?)
            .map_err(|e| e.to_string())?,
    })
}
fn active(tree: &TileTree, out: &mut Vec<Uuid>) {
    match tree {
        TileTree::Stack(stack) => {
            if let Some(tile) = stack.tabs.get(stack.active)
                && let ContentSource::Open { id, .. } = &tile.content
                && let Ok(id) = id.parse()
            {
                out.push(id);
            }
        },
        TileTree::Split { children, .. } => {
            for child in children {
                active(&child.tree, out);
            }
        },
    }
}
pub(super) fn active_members(tree: &TileTree) -> Vec<Uuid> {
    let mut ids = Vec::new();
    active(tree, &mut ids);
    ids
}
pub(super) fn prepare(page: &TreePage) {
    let retained = page
        .forme
        .as_ref()
        .map(|p| p.model().members())
        .unwrap_or_default();
    // Project only identities needed by this owner while borrowing the live
    // graph. `tile_tree` masks candidate accesses which are absent from it.
    let (tree, projection) = {
        let canvas = page.shared.canvas.borrow();
        let graph = canvas.graph();
        let tree = page
            .forme
            .as_ref()
            .filter(|p| p.workbench)
            .and_then(|p| p.current().tile_tree(graph));
        let visible = tree.as_ref().map(active_members).unwrap_or_default();
        let projection = retained
            .iter()
            .filter_map(|access| {
                let (_, node) = graph.get_node_by_id(*access)?;
                // A removed inline body is no longer a readable source, even
                // if the access and address survive. Clips are supplied by
                // their immutable retained reference and need no inline body.
                if node.body.is_none()
                    && !matches!(
                        mere_document_lanes::retained_clip::request(node.url()),
                        Ok(Some(mere_document_lanes::retained_clip::ClipRequest::Read(
                            _
                        )))
                    )
                {
                    return None;
                }
                Some(mere_document_lanes::reading_lifecycle::AccessProjection {
                    access: *access,
                    membership: graph.surface_node_lifetime(*access)?,
                    source: node.url().to_owned(),
                    visible: visible.contains(access),
                })
            })
            .collect::<Vec<_>>();
        (tree, projection)
    };
    let owner = page.product.as_ref().and_then(|p| p.retained_scope());
    let scope = owner
        .as_ref()
        .map(|(session, _)| session.to_string())
        .or_else(|| page.product.as_ref().map(|p| p.session.clone()))
        .unwrap_or_default();
    let mut reading = page.shared.reading.borrow_mut();
    let persona = owner
        .as_ref()
        .map(|(_, persona)| persona.clone())
        .unwrap_or_else(|| reading.persona.clone());
    reading.current_owner = owner;
    let owner_key = mere_document_lanes::reading_lifecycle::ReadingOwner {
        session: scope.clone(),
        persona: persona.clone(),
    };
    let changes = reading.lifecycle.reconcile(owner_key, projection);
    if changes.iter().any(|change| {
        matches!(
            change,
            mere_document_lanes::reading_lifecycle::AccessChange::Added(_)
        )
    }) {
        page.shared.dirty.set(true);
    }
    if changes.iter().any(|change| {
        matches!(
            change,
            mere_document_lanes::reading_lifecycle::AccessChange::OwnerChanged
        )
    }) {
        reading.retire_all();
    } else {
        for change in &changes {
            match change {
                mere_document_lanes::reading_lifecycle::AccessChange::Retired(id)
                | mere_document_lanes::reading_lifecycle::AccessChange::SourceChanged(id) => {
                    reading.retire_access(*id);
                },
                _ => {},
            }
        }
    }
    reading.scope = scope;
    reading.persona = persona;
    reading.prepare(tree.as_ref());
    if let Some(pane) = &page.forme {
        for (member, binding) in &mut reading.bindings {
            if !binding.restored {
                if let Some(anchor) = pane.model().preferences.reading_anchors.get(member) {
                    binding
                        .slot
                        .borrow_mut()
                        .restore_reading_anchor(anchor.clone());
                }
                binding.restored = true;
            }
            let preferences = pane.model().preferences.reading_preferences.get(member);
            let _ = binding.slot.borrow_mut().reading_preferences(preferences);
        }
    }
}
impl Reading {
    fn retire_access(&mut self, id: Uuid) {
        self.host.close(id);
        self.bindings.remove(&id);
        self.clips.remove(&id);
        self.statuses.remove(&id);
        self.players.remove(&id);
        self.media_answers.remove(&id);
        self.exports.remove(&id);
        self.download_urls.remove(&id);
        self.requests.retain(|(access, _)| *access != id);
        let empty_progress = self.pending_progress.as_mut().is_some_and(|(anchors, _)| {
            anchors.remove(&id);
            anchors.is_empty()
        });
        if empty_progress {
            self.pending_progress = None;
        }
    }

    fn retire_all(&mut self) {
        let ids: std::collections::BTreeSet<_> = self
            .bindings
            .keys()
            .chain(self.clips.keys())
            .chain(self.exports.keys())
            .copied()
            .collect();
        for id in ids {
            self.retire_access(id);
        }
        self.statuses.clear();
        self.players.clear();
        self.media_answers.clear();
        self.requests.clear();
        self.exports.clear();
        self.download_urls.clear();
        self.pending_progress = None;
    }

    pub(super) fn prepare(&mut self, tree: Option<&TileTree>) {
        let mut shown = vec![];
        if let Some(tree) = tree {
            active(tree, &mut shown);
        }
        self.host.show_only(&shown);
        let hidden: Vec<_> = self
            .exports
            .keys()
            .copied()
            .filter(|id| !shown.contains(id))
            .collect();
        for id in hidden {
            self.exports.remove(&id);
            self.statuses.insert(
                id,
                "Selected clip export canceled because its access was hidden".into(),
            );
        }
    }
    fn open(
        &mut self,
        member: Uuid,
        source: HeldContent,
    ) -> Result<(u64, Rc<RefCell<ContentSlot>>), String> {
        let slot = self.host.open(member, source)?;
        if let Some(binding) = self.bindings.get(&member)
            && Rc::ptr_eq(&binding.slot, &slot)
        {
            return Ok((binding.key, slot));
        }
        if self.bindings.contains_key(&member) {
            // `ContentHost::open` has compared the complete HeldContent and
            // rebound its Pelt session when bytes/engine/media changed. This
            // also revokes any completion ticket minted for the old source.
            self.lifecycle.rebind(member);
            let canceled_export = self.exports.remove(&member).is_some();
            self.players.remove(&member);
            self.media_answers.remove(&member);
            self.download_urls.remove(&member);
            self.statuses.remove(&member);
            if canceled_export {
                self.statuses.insert(
                    member,
                    "Selected clip export canceled because its content changed".into(),
                );
            }
        }
        let key = ContentProducer::allocate_key();
        let producer = Rc::new(RefCell::new(ContentProducer::new(slot.clone(), key)));
        self.bindings.insert(
            member,
            Binding {
                restored: false,
                key,
                slot: slot.clone(),
                producer,
            },
        );
        // A clip read can finish after its tab becomes hidden. Opening its
        // retained session must not make that access visible again.
        let mut shown: Vec<_> = self
            .bindings
            .iter()
            .filter_map(|(id, _)| self.lifecycle.is_visible(*id).then_some(*id))
            .collect();
        if self.lifecycle.is_visible(member) {
            shown.push(member);
        }
        self.host.show_only(&shown);
        Ok((key, slot))
    }
    pub(super) fn register(&mut self, ctx: &mut AppCtx<'_, TreePage, Logic, Child>) {
        let retired: Vec<_> = self
            .registered
            .keys()
            .copied()
            .filter(|key| !self.bindings.values().any(|b| b.key == *key))
            .collect();
        for key in retired {
            ctx.producers.remove(key);
            self.registered.remove(&key);
        }
        for binding in self.bindings.values() {
            if !ctx.producers.contains(binding.key) {
                match ctx
                    .producers
                    .register(binding.key, binding.producer.clone(), &[])
                {
                    Ok(()) => {
                        self.registered
                            .insert(binding.key, binding.producer.clone());
                    },
                    Err(error) => web_sys::console::error_1(
                        &format!("Retained content producer registration refused: {error:?}")
                            .into(),
                    ),
                }
            }
        }
    }
    pub(super) fn poll(&mut self) -> bool {
        let mut changed = self.dictionaries.poll(&mut self.host);
        self.download_urls
            .retain(|_, url| js_sys::Date::now() < url.revoke_at);
        // While a product operation owns the app asynchronously, retain the
        // bounded answer until its actual owner is available for revalidation.
        let completed_exports: Vec<_> = self
            .exports
            .iter()
            .filter_map(|(id, pending)| {
                self.current_owner.as_ref()?;
                pending
                    .answer
                    .borrow_mut()
                    .take()
                    .map(|result| (*id, pending.scope.clone(), pending.ticket.clone(), result))
            })
            .collect();
        for (id, scope, ticket, result) in completed_exports {
            self.exports.remove(&id);
            if !self.lifecycle.accepts(&ticket)
                || !scope.is_current(self.export_scope(id).as_ref())
                || !scope.is_owned_by(self.current_owner.as_ref())
                || !self.lifecycle.complete(&ticket)
            {
                continue;
            }
            let status = match result.and_then(download_clip) {
                Ok(url) => {
                    self.download_urls.insert(id, url);
                    "Selected clip download handed to the browser".into()
                },
                Err(error) => format!("Selected clip export refused: {error}"),
            };
            self.statuses.insert(id, status);
            changed = true;
        }
        let completed: Vec<_> =
            self.clips
                .iter()
                .filter_map(|(id, pending)| {
                    pending.answer.borrow_mut().take().map(|result| {
                        (*id, pending.address.clone(), pending.ticket.clone(), result)
                    })
                })
                .collect();
        for (id, address, ticket, result) in completed {
            self.clips.remove(&id);
            if !self.lifecycle.contains(id) || !self.lifecycle.complete(&ticket) {
                continue;
            }
            match result {
                Ok(source) => {
                    if source.address != address {
                        continue;
                    }
                    if let Err(error) = self.open(id, source) {
                        self.statuses.insert(id, error);
                    }
                },
                Err(error) => {
                    self.statuses.insert(id, error);
                },
            }
            changed = true;
        }
        for (id, binding) in &self.bindings {
            let effects = binding.slot.borrow_mut().take_effects();
            for effect in effects {
                if let Some(navigation) = &effect.navigation {
                    let command =
                        mere_document_lanes::retained_clip::request(&navigation.request.address);
                    if let Ok(Some(
                        command @ (mere_document_lanes::retained_clip::ClipRequest::Play(_)
                        | mere_document_lanes::retained_clip::ClipRequest::Stop(_)),
                    )) = command
                    {
                        use mere_document_lanes::retained_clip::ClipRequest;
                        let slot = binding.slot.borrow();
                        let requested = match command {
                            ClipRequest::Play(id) | ClipRequest::Stop(id) => id,
                            _ => unreachable!(),
                        };
                        if let Some(media) = &slot.source().media
                            && media.clip == requested
                        {
                            match command {
                                ClipRequest::Play(_) => {
                                    if self.players.get(id).is_none_or(|p| p.clip != media.clip) {
                                        match web_sys::HtmlAudioElement::new_with_src(
                                            &media.data_url(),
                                        ) {
                                            Ok(element) => {
                                                self.players.insert(
                                                    *id,
                                                    MediaPlayer {
                                                        element,
                                                        clip: media.clip,
                                                    },
                                                );
                                            },
                                            Err(error) => {
                                                self.statuses.insert(
                                                    *id,
                                                    format!("Audio player refused: {error:?}"),
                                                );
                                                changed = true;
                                                continue;
                                            },
                                        }
                                    }
                                    let player = self.players.get(id).unwrap();
                                    let Some(ticket) = self
                                        .lifecycle
                                        .begin_operation(*id, OperationKind::MediaPlayback)
                                    else {
                                        continue;
                                    };
                                    let answer = Rc::new(RefCell::new(None));
                                    self.media_answers.insert(*id, (ticket, answer.clone()));
                                    match player.element.play() {
                                        Ok(promise) => {
                                            wasm_bindgen_futures::spawn_local(async move {
                                                *answer.borrow_mut() = Some(
                                                    match wasm_bindgen_futures::JsFuture::from(
                                                        promise,
                                                    )
                                                    .await
                                                    {
                                                        Ok(_) => {
                                                            "Retained audio playback started".into()
                                                        },
                                                        Err(error) => format!(
                                                            "Audio playback refused: {error:?}"
                                                        ),
                                                    },
                                                );
                                            });
                                        },
                                        Err(error) => {
                                            *answer.borrow_mut() =
                                                Some(format!("Audio playback refused: {error:?}"));
                                        },
                                    }
                                },
                                ClipRequest::Stop(_) => {
                                    let _ = self
                                        .lifecycle
                                        .begin_operation(*id, OperationKind::MediaPlayback);
                                    self.media_answers.remove(id);
                                    if let Some(player) = self.players.get(id) {
                                        let _ = player.element.pause();
                                        player.element.set_current_time(0.);
                                    }
                                    self.statuses.insert(*id, "Retained audio stopped".into());
                                },
                                _ => {},
                            }
                            changed = true;
                            continue;
                        }
                    }
                }
                let status = if let Some(navigation) = effect.navigation {
                    self.requests
                        .push((*id, navigation.request.address.clone()));
                    format!("Retained link requested: {}", navigation.request.address)
                } else if effect.submission.is_some() {
                    "This retained view handed its form request to the host".into()
                } else {
                    effect
                        .error
                        .unwrap_or_else(|| "Content requested a host operation".into())
                };
                self.statuses.insert(*id, status);
                changed = true;
            }
        }
        let answers: Vec<_> = self
            .media_answers
            .iter()
            .filter_map(|(id, (ticket, answer))| {
                answer
                    .borrow_mut()
                    .take()
                    .map(|message| (*id, ticket.clone(), message))
            })
            .collect();
        for (id, ticket, message) in answers {
            self.media_answers.remove(&id);
            if self.lifecycle.complete(&ticket) {
                self.statuses.insert(id, message);
                changed = true;
            }
        }
        changed
    }
    pub(super) fn busy(&self) -> bool {
        !self.clips.is_empty()
            || !self.media_answers.is_empty()
            || !self.exports.is_empty()
            || self.dictionaries.in_flight > 0
    }
    fn export_scope(&self, access: Uuid) -> Option<ClipExportScope> {
        if !self.lifecycle.is_visible(access) {
            return None;
        }
        let binding = self.bindings.get(&access)?;
        let slot = binding.slot.borrow();
        if !slot.is_visible() {
            return None;
        }
        let mere_document_lanes::retained_clip::ClipRequest::Read(clip) =
            mere_document_lanes::retained_clip::request(&slot.source().address).ok()??
        else {
            return None;
        };
        Some(ClipExportScope {
            persona: self.persona.clone(),
            graph_session: self.scope.parse().ok()?,
            access,
            content_session: slot.session_identity()?.instance_id,
            clip,
        })
    }
    pub(super) fn export_clip(
        &mut self,
        access: Uuid,
        clip: eidetic::ManifestId,
        owner: Option<(Uuid, String)>,
        store: Option<muniment::IndexedDbBackend>,
    ) {
        let result: Result<_, String> = (|| {
            let scope = self
                .export_scope(access)
                .ok_or("The selected clip access is no longer shown")?;
            if scope.clip != clip || !scope.is_owned_by(owner.as_ref()) {
                return Err("The selected clip owner or session changed".into());
            }
            let store = store.ok_or("This view has no available supplying Mere store")?;
            Ok((scope, store, clip_limits()?))
        })();
        match result {
            Ok((scope, mut store, limits)) => {
                let Some(ticket) = self
                    .lifecycle
                    .begin_operation(access, OperationKind::ClipExport)
                else {
                    return;
                };
                let answer = Rc::new(RefCell::new(None));
                let cancellation = Rc::new(RefCell::new(ExportCancellation::default()));
                self.exports.insert(
                    access,
                    PendingExport {
                        scope,
                        ticket,
                        answer: answer.clone(),
                        cancellation: cancellation.clone(),
                    },
                );
                self.statuses
                    .insert(access, "Preparing verified selected clip download".into());
                wasm_bindgen_futures::spawn_local(async move {
                    use std::{future::Future, task::Poll};
                    let mut operation =
                        std::pin::pin!(mere_document_lanes::retained_clip::export_body(
                            &mut store, None, clip, limits,
                        ));
                    let result = std::future::poll_fn(|context| {
                        {
                            let mut state = cancellation.borrow_mut();
                            if state.cancelled {
                                return Poll::Ready(None);
                            }
                            state.waker = Some(context.waker().clone());
                        }
                        operation.as_mut().poll(context).map(Some)
                    })
                    .await;
                    if let Some(result) = result {
                        *answer.borrow_mut() = Some(result);
                    }
                });
            },
            Err(error) => {
                self.statuses
                    .insert(access, format!("Selected clip export refused: {error}"));
            },
        }
    }
    pub(super) fn take_requests(&mut self) -> Vec<(Uuid, String)> {
        std::mem::take(&mut self.requests)
    }
    pub(super) fn set_status(&mut self, id: Uuid, status: String) {
        self.statuses.insert(id, status);
    }
    pub(super) fn status_summary(&self) -> String {
        self.statuses
            .values()
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ")
    }
    pub(super) fn glyph_runs(&self) -> usize {
        self.bindings
            .values()
            .map(|binding| binding.slot.borrow().glyph_runs())
            .sum()
    }
    pub(super) fn registered_len(&self) -> usize {
        self.registered.len()
    }
    pub(super) fn len(&self) -> usize {
        self.host.len()
    }
    pub(super) fn session(&self, member: Uuid) -> Option<u64> {
        self.host
            .get(member)?
            .borrow()
            .session_identity()
            .map(|i| i.instance_id)
    }
}

pub(super) fn clip_appearance(
    shared: &Rc<Shared>,
    member: Uuid,
    address: &str,
    store: Option<muniment::IndexedDbBackend>,
) -> Appearance {
    let has_binding = {
        let reading = shared.reading.borrow();
        if let Some(binding) = reading.bindings.get(&member) {
            // A retained clip address names immutable verified content. The
            // reconciled owner/access lease already fences this slot; reuse
            // its handle without copying its document or media payloads.
            if binding.slot.borrow().source().address == address {
                return Appearance::Ready {
                    key: binding.key,
                    slot: binding.slot.clone(),
                    status: reading.statuses.get(&member).cloned().unwrap_or_default(),
                };
            }
            true
        } else {
            false
        }
    };
    if has_binding {
        let mut reading = shared.reading.borrow_mut();
        reading.lifecycle.rebind(member);
        reading.retire_access(member);
    }
    let id = match mere_document_lanes::retained_clip::request(address) {
        Ok(Some(mere_document_lanes::retained_clip::ClipRequest::Read(id))) => id,
        _ => return Appearance::Message("Invalid retained clip reference".into()),
    };
    let mut reading = shared.reading.borrow_mut();
    if reading
        .clips
        .get(&member)
        .is_some_and(|pending| pending.address != address)
    {
        reading.clips.remove(&member);
        reading.statuses.remove(&member);
    }
    let status = reading.statuses.get(&member).cloned();
    if status.is_none() && !reading.clips.contains_key(&member) {
        let Some(mut store) = store else {
            return Appearance::Message("This view has no supplying Mere store".into());
        };
        let limits = match clip_limits() {
            Ok(limits) => limits,
            Err(error) => return Appearance::Message(error),
        };
        let Some(ticket) = reading
            .lifecycle
            .begin_operation(member, OperationKind::ClipRead)
        else {
            return Appearance::Message("Retained access is no longer current".into());
        };
        let answer = Rc::new(RefCell::new(None));
        reading.clips.insert(
            member,
            PendingClip {
                address: address.into(),
                ticket,
                answer: answer.clone(),
            },
        );
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let clip =
                    mere_document_lanes::retained_clip::read_store(&mut store, None, id, limits)
                        .await?;
                HeldContent::clip(&clip)
            }
            .await;
            *answer.borrow_mut() = Some(result);
        });
    }
    Appearance::Message(
        status.unwrap_or_else(|| "Reading verified retained clip from this Mere".into()),
    )
}

fn download_clip(
    file: mere_document_lanes::retained_clip::ClipExport,
) -> Result<DownloadUrl, String> {
    use wasm_bindgen::JsCast;
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or("No browser document")?;
    let parts = js_sys::Array::new();
    parts.push(&js_sys::Uint8Array::from(file.bytes.as_slice()));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mere_document_lanes::retained_clip::PACKAGE_MEDIA_TYPE);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|e| format!("Could not create clip package: {e:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|e| format!("Could not create clip download URL: {e:?}"))?;
    let held_url = DownloadUrl {
        url,
        revoke_at: js_sys::Date::now() + 60_000.,
    };
    let anchor: web_sys::HtmlElement = document
        .create_element("a")
        .map_err(|e| format!("Could not create clip download: {e:?}"))?
        .dyn_into()
        .map_err(|_| "Clip download element unavailable")?;
    anchor
        .set_attribute("href", &held_url.url)
        .map_err(|e| format!("Could not set clip download URL: {e:?}"))?;
    anchor
        .set_attribute("download", &file.filename)
        .map_err(|e| format!("Could not name clip download: {e:?}"))?;
    anchor
        .set_attribute("hidden", "")
        .map_err(|e| format!("Could not prepare clip download: {e:?}"))?;
    document
        .body()
        .ok_or("No browser document body")?
        .append_child(&anchor)
        .map_err(|e| format!("Could not attach clip download: {e:?}"))?;
    anchor.click();
    anchor.remove();
    // Give the browser time to consume its download, then release on frame,
    // access close, source replacement or owner change. At most one per access.
    Ok(held_url)
}

#[derive(Clone)]
pub(super) enum Appearance {
    Message(String),
    Ready {
        key: u64,
        slot: Rc<RefCell<ContentSlot>>,
        status: String,
    },
}
pub(super) fn appearance(shared: &Rc<Shared>, member: Uuid, source: HeldContent) -> Appearance {
    let (key, slot) = match shared.reading.borrow_mut().open(member, source) {
        Ok(binding) => binding,
        Err(error) => return Appearance::Message(error),
    };
    let status = shared
        .reading
        .borrow()
        .statuses
        .get(&member)
        .cloned()
        .unwrap_or_default();
    Appearance::Ready { key, slot, status }
}

/// Resolve an existing held source against borrowed graph bytes before
/// allocating a new `HeldContent` byte vector. Changed content falls back to
/// the ordinary owner open/rebind path.
pub(super) fn appearance_parts(
    shared: &Rc<Shared>,
    member: Uuid,
    address: &str,
    title: &str,
    media_type: &str,
    body: &str,
) -> Appearance {
    let existing = {
        let reading = shared.reading.borrow();
        reading
            .host
            .source_matches_parts(member, address, title, media_type, body.as_bytes())
            .then(|| reading.bindings.get(&member))
            .flatten()
            .map(|binding| {
                (
                    binding.key,
                    binding.slot.clone(),
                    reading.statuses.get(&member).cloned().unwrap_or_default(),
                )
            })
    };
    if let Some((key, slot, status)) = existing {
        return Appearance::Ready { key, slot, status };
    }
    appearance(
        shared,
        member,
        HeldContent {
            address: address.to_owned(),
            title: title.to_owned(),
            media_type: media_type.to_owned(),
            bytes: body.as_bytes().to_vec(),
            engine: None,
            media: None,
        },
    )
}

pub(super) fn render(appearance: Appearance) -> Child {
    match appearance {
        Appearance::Message(message) => Box::new(el("p", message).attr("role", "status")),
        Appearance::Ready { key, slot, status } => {
            let leaf =
                graphshell::retained_content::filling_content_view::<TreePage, ()>(key, slot);
            Box::new(
                el(
                    "div",
                    (
                        leaf,
                        (!status.is_empty()).then(|| {
                            el("p", status)
                                .attr("role", "status")
                                .attr("class", "shell-reading-status")
                        }),
                    ),
                )
                .attr("class", "pelt-reading cambium-reading-surface"),
            )
        },
    }
}

#[derive(Clone)]
pub(super) struct ToolbarSnapshot {
    title: String,
    preferences: tabard::ReadingPreferences,
    ui: cambium::ReadFind,
    text_controls: bool,
    find_enabled: bool,
}
pub(super) fn toolbar_snapshot(page: &TreePage, member: Uuid) -> ToolbarSnapshot {
    let pane = page.forme.as_ref().expect("reading workspace");
    ToolbarSnapshot {
        title: page
            .shared
            .canvas
            .borrow()
            .graph()
            .get_node_by_id(member)
            .map(|(_, node)| node.title.clone())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "document".into()),
        find_enabled: page
            .shared
            .reading
            .borrow()
            .bindings
            .get(&member)
            .is_some_and(|binding| binding.slot.borrow().has_document_find()),
        text_controls: page
            .shared
            .reading
            .borrow()
            .bindings
            .get(&member)
            .is_some_and(|binding| binding.slot.borrow().has_reading_controls()),
        preferences: pane.model().preferences.reading_preferences.get(&member),
        ui: pane.reading_ui.get(&member).cloned().unwrap_or_default(),
    }
}
fn find(
    page: &mut TreePage,
    member: Uuid,
    step: Option<graphshell::retained_content::ReadFindDirection>,
) {
    let Some(pane) = &mut page.forme else {
        return;
    };
    let ui = pane.reading_ui.entry(member).or_default();
    let reading = page.shared.reading.borrow();
    let Some(binding) = reading.bindings.get(&member) else {
        return;
    };
    let result = binding.slot.borrow_mut().find(ui.input.text(), step);
    ui.status = match result {
        Ok(state) => cambium::find_status(
            state.query.text.is_empty(),
            false,
            state.count,
            state.current,
        ),
        Err(error) => cambium::find_failed(&error),
    };
    page.shared.dirty.set(true);
}
pub(super) fn read_command(page: &mut TreePage, member: Uuid, command: cambium::ReadCommand) {
    use cambium::ReadCommand as C;
    let Some(pane) = &mut page.forme else {
        return;
    };
    if pane.saving
        && matches!(
            command,
            C::Back | C::Forward | C::Smaller | C::Larger | C::Narrower | C::Wider | C::Reset
        )
    {
        if pane.pending_read_commands.len() < 32 {
            pane.pending_read_commands.push_back((member, command));
        }
        page.shared.dirty.set(true);
        return;
    }
    let mut value = pane.model().preferences.reading_preferences.get(&member);
    match command {
        C::Appearance => {
            let ui = pane.reading_ui.entry(member).or_default();
            ui.appearance_open = !ui.appearance_open;
            page.shared.dirty.set(true);
            return;
        },
        C::ShowGraph => {
            show_graph(page, member);
            return;
        },
        C::Back | C::Forward => {
            if pane.saving {
                return;
            }
            let (mut anchors, mut trail) = capture_progress(page);
            let Some(destination) = trail.step(command == C::Forward) else {
                return;
            };
            if page
                .shared
                .canvas
                .borrow()
                .graph()
                .get_node_by_id(destination)
                .is_none()
            {
                return;
            }
            if let Some(anchor) = trail.current_anchor().cloned() {
                anchors.insert(destination, anchor.clone());
                if let Some(binding) = page.shared.reading.borrow().bindings.get(&destination) {
                    binding.slot.borrow_mut().restore_reading_anchor(anchor);
                }
            }
            super::forme::edit(page, None, true, move |workspace, graph| {
                workspace.preferences.reading_anchors = anchors;
                workspace.preferences.reading_trail = trail;
                workspace.read(destination, graph)
            });
            return;
        },
        C::Smaller => value.step_text(false),
        C::Larger => value.step_text(true),
        C::Narrower => value.step_column(false),
        C::Wider => value.step_column(true),
        C::Reset => value = Default::default(),
        C::Find => {
            pane.reading_ui.entry(member).or_default().open = true;
            find(page, member, None);
            return;
        },
        C::CloseFind => {
            pane.reading_ui.entry(member).or_default().open = false;
            if let Some(binding) = page.shared.reading.borrow().bindings.get(&member) {
                binding.slot.borrow_mut().clear_find();
            }
            page.shared.dirty.set(true);
            return;
        },
        C::Previous | C::Next => {
            find(
                page,
                member,
                Some(if command == C::Next {
                    graphshell::retained_content::ReadFindDirection::Next
                } else {
                    graphshell::retained_content::ReadFindDirection::Previous
                }),
            );
            return;
        },
    }
    // Uses the same acknowledged workspace store without unlocking or replacing
    // a layout draft or creating a graph edit/history entry.
    let update = pane.session.preferences_update(true, |preferences| {
        preferences.reading_preferences.set(member, value);
    });
    pane.save(update);
    page.shared.dirty.set(true);
}
pub(super) fn toolbar(member: Uuid, snapshot: ToolbarSnapshot, tabs_visible: bool) -> Child {
    let mut ui = snapshot.ui;
    ui.available = snapshot.find_enabled;
    let field = Some(&ui);
    let close = cambium::shell_control(
        "×",
        format!("Close reading: {}", snapshot.title),
        Default::default(),
        move |page: &mut TreePage, _| super::forme::close_reading(page, member),
    );
    cambium::read_toolbar_with_trailing(
        snapshot.preferences.text_size,
        snapshot.preferences.column_em,
        snapshot.text_controls,
        field,
        if tabs_visible {
            Vec::new()
        } else {
            vec![close]
        },
        move |page: &mut TreePage, event| match event {
            cambium::ReadEvent::Command(command) => read_command(page, member, command),
            cambium::ReadEvent::Query(input) => {
                let Some(pane) = &mut page.forme else {
                    return;
                };
                let ui = pane.reading_ui.entry(member).or_default();
                let changed = ui.input.text() != input.text();
                ui.input = input;
                if changed {
                    find(page, member, None);
                }
                page.shared.dirty.set(true);
            },
        },
    )
}

fn progress(page: &TreePage) -> BTreeMap<Uuid, tabard::ReadingAnchor> {
    let mut anchors = page
        .forme
        .as_ref()
        .map(|p| p.model().preferences.reading_anchors.clone())
        .unwrap_or_default();
    for (member, binding) in &page.shared.reading.borrow().bindings {
        if let Some(anchor) = binding.slot.borrow().reading_anchor() {
            anchors.insert(*member, anchor);
        }
    }
    anchors
}
pub(super) fn needs_progress_save(page: &TreePage) -> bool {
    let Some(pane) = page
        .forme
        .as_ref()
        .filter(|p| !p.saving && p.retry.is_none() && !p.session.editing())
    else {
        return false;
    };
    let anchors = progress(page);
    let mut reading = page.shared.reading.borrow_mut();
    if anchors == pane.model().preferences.reading_anchors {
        reading.pending_progress = None;
        return false;
    }
    let now = js_sys::Date::now();
    if let Some((held, since)) = &reading.pending_progress {
        if held == &anchors {
            return now - since >= 600.0;
        }
    }
    reading.pending_progress = Some((anchors, now));
    false
}
pub(super) fn persist_progress(page: &mut TreePage) {
    let anchors = progress(page);
    super::scope_navigation::retain_anchor(page, &anchors);
    if let Some(pane) = &mut page.forme {
        if !pane.saving {
            let mut trail = pane.model().preferences.reading_trail.clone();
            if let Some(anchor) = trail
                .current()
                .and_then(|member| anchors.get(member))
                .cloned()
            {
                trail.anchor_current(anchor);
            }
            let update = pane.session.preferences_update(true, |preferences| {
                preferences.reading_anchors = anchors.clone();
                preferences.reading_trail = trail.clone();
            });
            pane.save(update);
        }
    }
}
pub(super) fn show_graph(page: &mut TreePage, member: Uuid) {
    page.shared.canvas.borrow_mut().select_member(member);
    page.shared.canvas.borrow_mut().center_on_selected();
    if let Some(pane) = &mut page.forme {
        pane.workbench = false;
    }
    super::forme::selection_changed(page);
    page.shared.dirty.set(true);
}

pub(super) fn preview_excerpt(page: &TreePage, member: Uuid) -> Option<String> {
    let address = page
        .shared
        .canvas
        .borrow()
        .graph()
        .get_node_by_id(member)?
        .1
        .url()
        .to_owned();
    let excerpt = page
        .shared
        .reading
        .borrow()
        .bindings
        .get(&member)
        .and_then(|binding| {
            let slot = binding.slot.borrow();
            (slot.source().address == address)
                .then(|| slot.reading_excerpt())
                .flatten()
        });
    let excerpt = excerpt.or_else(|| {
        let canvas = page.shared.canvas.borrow();
        let (_, node) = canvas.graph().get_node_by_id(member)?;
        let body = node.body.as_ref().filter(|body| body.len() <= 256 * 1024)?;
        if node.media_type.as_deref() == Some("application/vnd.mere.document+json") {
            graphshell::retained_content::retained_packet_excerpt(body)
        } else {
            None
        }
    });
    excerpt
        .map(|text| format!("Retained document excerpt: {text}"))
        .or_else(|| {
            page.forme
                .as_ref()
                .and_then(|pane| pane.model().preferences.reading_anchors.get(&member))
                .filter(|anchor| anchor.source == address)
                .map(|anchor| format!("Last reading position: {}", anchor.exact))
        })
}
pub(super) fn capture_progress(
    page: &TreePage,
) -> (
    BTreeMap<Uuid, tabard::ReadingAnchor>,
    tabard::ReadingTrail<Uuid>,
) {
    let anchors = progress(page);
    let mut trail = page
        .forme
        .as_ref()
        .map(|pane| pane.model().preferences.reading_trail.clone())
        .unwrap_or_default();
    if let Some(anchor) = trail
        .current()
        .and_then(|member| anchors.get(member))
        .cloned()
    {
        trail.anchor_current(anchor);
    }
    (anchors, trail)
}

pub(super) fn restore_anchor(page: &TreePage, member: Uuid, anchor: tabard::ReadingAnchor) {
    if let Some(binding) = page.shared.reading.borrow().bindings.get(&member) {
        binding.slot.borrow_mut().restore_reading_anchor(anchor);
    }
}
pub(super) fn anchor_field(page: &TreePage) -> String {
    page.forme
        .as_ref()
        .and_then(|pane| pane.model().preferences.focused_reading)
        .and_then(|member| {
            page.shared
                .reading
                .borrow()
                .bindings
                .get(&member)
                .and_then(|binding| binding.slot.borrow().reading_anchor())
        })
        .map(|anchor| format!("{}:{}:{}", anchor.text, anchor.offset, anchor.exact))
        .unwrap_or_default()
}
