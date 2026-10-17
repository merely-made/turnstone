// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The retained Workbench uses Pelt for content; graph actions stay with its owner.
use super::*;
use graphshell::retained_content::ClipExportScope;
use graphshell::retained_content::{ContentHost, ContentProducer, ContentSlot, HeldContent};
use std::collections::BTreeMap;
use uuid::Uuid;
use workbench::{ContentSource, TileTree};

struct Binding {
    key: u64,
    slot: Rc<RefCell<ContentSlot>>,
    producer: Rc<RefCell<ContentProducer>>,
}
type ClipAnswer = Rc<RefCell<Option<Result<HeldContent, String>>>>;
type PendingClip = (String, ClipAnswer);
type ExportAnswer =
    Rc<RefCell<Option<Result<mere_document_lanes::retained_clip::ClipExport, String>>>>;
struct PendingExport {
    scope: ClipExportScope,
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
    media_answers: BTreeMap<Uuid, (u64, Rc<RefCell<Option<String>>>)>,
    requests: Vec<(Uuid, String)>,
}
impl Default for Reading {
    fn default() -> Self {
        Self {
            host: ContentHost::with_native_presentation(
                vec![
                    include_bytes!("../../web/Newsreader-Regular.ttf").to_vec(),
                    include_bytes!("../../web/IBMPlexSans-Regular.ttf").to_vec(),
                    include_bytes!("../../web/IBMPlexMono-Regular.ttf").to_vec(),
                ],
                Some("Newsreader".into()),
            ),
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
    let graph = page.shared.canvas.borrow().graph().clone();
    let tree = page
        .forme
        .as_ref()
        .filter(|p| p.workbench)
        .and_then(|p| p.projected_model().tile_tree(&graph));
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
    if reading.scope != scope || reading.persona != persona {
        for id in reading.bindings.keys().copied().collect::<Vec<_>>() {
            reading.host.close(id);
        }
        reading.bindings.clear();
        reading.clips.clear();
        reading.statuses.clear();
        reading.players.clear();
        reading.media_answers.clear();
        reading.requests.clear();
        reading.exports.clear();
        reading.download_urls.clear();
        reading.scope = scope;
        reading.persona = persona;
    }
    reading.prepare(tree.as_ref(), &retained);
    reading.clips.retain(|id, (address, _)| {
        graph
            .get_node_by_id(*id)
            .is_some_and(|(_, node)| node.url() == *address)
    });
    // A retained access may survive a graph edit while its address changes.
    // Retire the old content before handling its queued host effects.
    let stale: Vec<_> = reading
        .bindings
        .iter()
        .filter_map(|(id, binding)| {
            graph
                .get_node_by_id(*id)
                .is_none_or(|(_, node)| node.url() != binding.slot.borrow().source().address)
                .then_some(*id)
        })
        .collect();
    for id in stale {
        reading.host.close(id);
        reading.bindings.remove(&id);
        reading.clips.remove(&id);
        reading.exports.remove(&id);
        reading.players.remove(&id);
        reading.media_answers.remove(&id);
        reading.download_urls.remove(&id);
        reading.statuses.remove(&id);
    }
}
impl Reading {
    pub(super) fn prepare(&mut self, tree: Option<&TileTree>, retained: &[Uuid]) {
        let closed: Vec<_> = self
            .bindings
            .keys()
            .copied()
            .filter(|id| !retained.contains(id))
            .collect();
        for id in closed {
            self.host.close(id);
            self.bindings.remove(&id);
            self.statuses.remove(&id);
            self.players.remove(&id);
            self.media_answers.remove(&id);
            self.exports.remove(&id);
            self.download_urls.remove(&id);
        }
        self.clips.retain(|id, _| retained.contains(id));
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
                key,
                slot: slot.clone(),
                producer,
            },
        );
        // This is called only by the active tile's fill callback.
        let mut shown: Vec<_> = self
            .bindings
            .iter()
            .filter_map(|(id, b)| b.slot.borrow().is_visible().then_some(*id))
            .collect();
        shown.push(member);
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
        let mut changed = false;
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
                    .map(|result| (*id, pending.scope.clone(), result))
            })
            .collect();
        for (id, scope, result) in completed_exports {
            self.exports.remove(&id);
            if !scope.is_current(self.export_scope(id).as_ref())
                || !scope.is_owned_by(self.current_owner.as_ref())
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
        let completed: Vec<_> = self
            .clips
            .iter()
            .filter_map(|(id, (_, answer))| answer.borrow_mut().take().map(|result| (*id, result)))
            .collect();
        for (id, result) in completed {
            self.clips.remove(&id);
            match result {
                Ok(source) => {
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
                                    let answer = Rc::new(RefCell::new(None));
                                    self.media_answers.insert(
                                        *id,
                                        (
                                            slot.session_identity().unwrap().instance_id,
                                            answer.clone(),
                                        ),
                                    );
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
            .filter_map(|(id, (session, answer))| {
                answer
                    .borrow_mut()
                    .take()
                    .map(|message| (*id, *session, message))
            })
            .collect();
        for (id, session, message) in answers {
            self.media_answers.remove(&id);
            if self.session(id) == Some(session) {
                self.statuses.insert(id, message);
                changed = true;
            }
        }
        changed
    }
    pub(super) fn busy(&self) -> bool {
        !self.clips.is_empty() || !self.media_answers.is_empty() || !self.exports.is_empty()
    }
    fn export_scope(&self, access: Uuid) -> Option<ClipExportScope> {
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
                let answer = Rc::new(RefCell::new(None));
                let cancellation = Rc::new(RefCell::new(ExportCancellation::default()));
                self.exports.insert(
                    access,
                    PendingExport {
                        scope,
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
    let held = {
        shared
            .reading
            .borrow()
            .bindings
            .get(&member)
            .map(|b| b.slot.borrow().source().clone())
    };
    if let Some(source) = held {
        if source.address == address {
            return appearance(shared, member, source);
        }
        let mut reading = shared.reading.borrow_mut();
        reading.host.close(member);
        reading.bindings.remove(&member);
        reading.statuses.remove(&member);
        reading.players.remove(&member);
        reading.media_answers.remove(&member);
        reading.exports.remove(&member);
        reading.download_urls.remove(&member);
    }
    let id = match mere_document_lanes::retained_clip::request(address) {
        Ok(Some(mere_document_lanes::retained_clip::ClipRequest::Read(id))) => id,
        _ => return Appearance::Message("Invalid retained clip reference".into()),
    };
    let mut reading = shared.reading.borrow_mut();
    if reading
        .clips
        .get(&member)
        .is_some_and(|(requested, _)| requested != address)
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
        let answer = Rc::new(RefCell::new(None));
        reading
            .clips
            .insert(member, (address.into(), answer.clone()));
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
        .unwrap_or_else(|| "Viewing held content through Pelt".into());
    Appearance::Ready { key, slot, status }
}
pub(super) fn render(appearance: Appearance) -> Child {
    match appearance {
        Appearance::Message(message) => Box::new(el("p", message).attr("role", "status")),
        Appearance::Ready { key, slot, status } => {
            let leaf =
                graphshell::retained_content::filling_content_view::<TreePage, ()>(key, slot);
            Box::new(
                el("div", (leaf, el("p", status).attr("role", "status")))
                    .attr("class", "pelt-reading"),
            )
        },
    }
}
