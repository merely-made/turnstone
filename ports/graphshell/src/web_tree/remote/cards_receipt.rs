// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! In-process carrier for the headed Cards receipt. It sends real NDJSON
//! snapshots and resume diffs into the remote client without transport jitter.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use cambium_rootstock::{HostTree, HostWindow};
use cartography::canvas::BoardFit;
use chirograph::{
    BoundsRelationship, CachePolicy, CarrierNotice, CarrierOutput, CarrierRequest,
    CarrierRequestBody, CarrierResponse, CarrierResponseBody, ContentHash, EndpointDescriptor,
    PresentationBinding, PresentationCapability, PresentationChange, PresentationCodec,
    PresentationKey, PresentationManifest, PresentationOffer, PresentationSemantics,
    ProjectionDiff, ProjectionOffer, ProjectionRequest, ProjectionSession, ProjectionSnapshot,
    ProtocolVersion, ResumeReply, SemanticRole,
};
use graphshell_client::remote::RemoteSession;
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use sceno::{
    Arrangement, Footprint, InstanceId, ProjectedItem, Representation, Scene, Score, Size2,
    SourceRef, Transform2,
};
use scenotime::{Revision, SceneDiff, SceneEpoch, SceneSnapshot};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use super::super::{Child, Logic, TREE, TreePage, now_ms};
use super::{Session, cards, web_rtc_link};

const EPOCH: SceneEpoch = SceneEpoch(1);

#[derive(Default, serde::Serialize)]
pub(in super::super) struct Sample {
    pub labels_ms: f64,
    pub labels_projected: usize,
    pub labels_patched: usize,
    pub rows_built: usize,
    pub actions_ms: f64,
    pub action_projections: usize,
    pub board_sync_ms: f64,
    pub board_label_patches: usize,
    pub board_tick_ms: f64,
    pub board_paint_ms: f64,
    pub canvas_revision: u64,
    pub canvas_reused: bool,
    title_runs: usize,
    board_rects: usize,
    panned_title_glyphs: Vec<Vec<u32>>,
    pub semantic_ms: f64,
    pub semantic_cards: usize,
    revision: Option<u64>,
    presentation: Option<u64>,
    rows: Vec<(usize, String, String, bool)>,
}

thread_local! {
    static SAMPLE: RefCell<Option<Sample>> = const { RefCell::new(None) };
    static START: Cell<(f64, f64)> = const { Cell::new((0.0, 0.0)) };
    static RESULT: RefCell<Option<String>> = const { RefCell::new(None) };
    static REVEAL: Cell<bool> = const { Cell::new(false) };
    static LAST_PAGE: Cell<bool> = const { Cell::new(false) };
    static COMPACT_LABELS: Cell<bool> = const { Cell::new(false) };
    static CHANGED_LABELS: RefCell<HashSet<u32>> = RefCell::new(HashSet::new());
    static PAN_TO: Cell<Option<u32>> = const { Cell::new(None) };
    static MODE: Cell<Mode> = const { Cell::new(Mode::Optimized) };
    static PAINT: RefCell<(usize, usize, Vec<Vec<u32>>)> = const { RefCell::new((0, 0, Vec::new())) };
    static FIXTURE_MOUNTED: Cell<bool> = const { Cell::new(false) };
    static GEOMETRY_PROBE: Cell<bool> = const { Cell::new(false) };
    static GEOMETRY_RESULT: RefCell<Option<String>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Mode {
    Legacy,
    QuietRevision,
    Optimized,
}

pub(super) fn mode() -> Mode {
    MODE.get()
}

pub(in super::super) fn record(f: impl FnOnce(&mut Sample)) {
    SAMPLE.with_borrow_mut(|sample| {
        if let Some(sample) = sample {
            f(sample);
        }
    });
}

pub(in super::super) fn record_paint(scene: &netrender::Scene) {
    if FIXTURE_MOUNTED.get() {
        let runs = scene.ops.iter().filter_map(|op| match op {
            netrender::SceneOp::GlyphRun(run) => Some(run),
            _ => None,
        });
        let count = runs.clone().count();
        let glyphs = if PAN_TO.get().is_some() {
            runs.map(|run| run.glyphs.iter().map(|glyph| glyph.id).collect())
                .collect()
        } else {
            Vec::new()
        };
        let rects = scene
            .ops
            .iter()
            .filter(|op| matches!(op, netrender::SceneOp::Rect(_)))
            .count();
        PAINT.set((count, rects, glyphs));
        record_retained_paint();
    }
}

pub(in super::super) fn record_retained_paint() {
    record(|sample| {
        PAINT.with_borrow(|(count, rects, glyphs)| {
            sample.title_runs = *count;
            sample.board_rects = *rects;
            sample.panned_title_glyphs.clone_from(glyphs);
        })
    });
}

pub(super) fn board_fit(
    board: &graphshell_host::board::RemoteBoard,
    fit: BoardFit,
    (width, height): (u32, u32),
) -> BoardFit {
    let Some(id) = PAN_TO.get().map(InstanceId) else {
        return fit;
    };
    let scene = board.scene();
    let Some(card) = board.drawn_card(id) else {
        return fit;
    };
    let width = width as f32;
    let height = height as f32;
    let (x, y) = fit
        .transform(scene.bounds, width, height)
        .to_viewport(scene.position(card, board.board()));
    fit.camera(1.0, (width * 0.5 - x, height * 0.5 - y))
}

fn offer(label: String) -> PresentationOffer {
    PresentationOffer {
        codec: PresentationCodec::NativeGlyphV1,
        resource: ContentHash::of(label.as_bytes()),
        byte_size: label.len() as u64,
        requires: PresentationCapability::NativeGlyph,
        semantics: PresentationSemantics {
            label,
            role: SemanticRole::Graphic,
            bounds: BoundsRelationship::FillFootprint,
            actions: Vec::new(),
        },
    }
}

fn key(index: u32) -> PresentationKey {
    PresentationKey(format!("cards:{index}"))
}

fn label(index: u32, changed: bool) -> String {
    if COMPACT_LABELS.get() {
        let prefix = if changed { b'A' } else { b'0' };
        let prefix = char::from(prefix + (index / 1000) as u8);
        format!("{prefix}{:03}", index % 1000)
    } else {
        let version = if changed { 'B' } else { 'A' };
        format!("Card {index:05} {version}")
    }
}

fn send(remote: &mut RemoteSession, message: CarrierOutput) {
    remote.on_line(&serde_json::to_string(&message).expect("fixture line"));
}

fn answer(
    remote: &mut RemoteSession,
    body: CarrierResponseBody,
    accepts: impl Fn(&CarrierRequestBody) -> bool,
) {
    let request = remote
        .take_outgoing()
        .into_iter()
        .find_map(|line| {
            let request: CarrierRequest = serde_json::from_str(&line).expect("client request");
            accepts(&request.body).then_some(request)
        })
        .expect("client requested the fixture response");
    send(
        remote,
        CarrierOutput::Response(CarrierResponse {
            id: request.id,
            body: Ok(body),
        }),
    );
}

fn mount(count: u32) -> RemoteSession {
    let session = ProjectionSession("cards-scale".into());
    let request = ProjectionRequest {
        version: ProtocolVersion::V1,
        session: session.clone(),
        score: Score::new(Arrangement::Spiral(Default::default())),
    };
    let mut scene = Scene::new();
    scene.bounds = sceno::Rect::new(sceno::Vec2::new(0.0, 0.0), Size2::new(800.0, 500.0));
    let source = scene.intern_source(SourceRef::new("fixture", "cards"));
    let mut presentation = PresentationManifest::default();
    for index in 0..count {
        let (x, y) = if index < 12 {
            (
                70.0 + (index % 4) as f32 * 180.0,
                70.0 + (index / 4) as f32 * 145.0,
            )
        } else {
            (
                1200.0 + (index % 80) as f32 * 140.0,
                900.0 + (index / 80) as f32 * 100.0,
            )
        };
        scene.items.push(ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::translation(x, y),
            footprint: Footprint::Rect {
                size: Size2::new(120.0, 80.0),
            },
            representation: Representation::Glyph,
            layer: 0,
            visible: true,
            hit: None,
            channels: Vec::new(),
        });
        let key = key(index);
        presentation.bindings.push(PresentationBinding {
            instance: InstanceId(index),
            key: key.clone(),
        });
        presentation
            .offers
            .insert(key, vec![offer(label(index, false))]);
    }
    let mut remote = RemoteSession::new(web_rtc_link::remote_profile());
    remote.joined();
    answer(
        &mut remote,
        CarrierResponseBody::Descriptor(EndpointDescriptor {
            label: "Cards scale fixture".into(),
            projections: vec![ProjectionOffer {
                label: "Cards".into(),
                request,
            }],
        }),
        |body| matches!(body, CarrierRequestBody::Discover),
    );
    answer(
        &mut remote,
        CarrierResponseBody::Snapshot(Box::new(ProjectionSnapshot {
            version: ProtocolVersion::V1,
            session,
            scene: SceneSnapshot::from_dense(EPOCH, Revision(1), scene)
                .expect("valid fixture scene"),
            presentation,
            cache_policy: CachePolicy::default(),
        })),
        |body| matches!(body, CarrierRequestBody::Snapshot(_)),
    );
    assert_eq!(remote.revision(), Some(1));
    remote
}

fn edit(remote: &mut RemoteSession, index: u32) -> f64 {
    let changed = CHANGED_LABELS.with_borrow_mut(|labels| {
        if labels.remove(&index) {
            false
        } else {
            labels.insert(index);
            true
        }
    });
    let base = Revision(remote.revision().expect("mounted fixture"));
    let revision = Revision(base.0 + 1);
    let session = remote.session().expect("fixture session").clone();
    send(
        remote,
        CarrierOutput::Notice(CarrierNotice {
            session: session.clone(),
            epoch: EPOCH,
            revision,
        }),
    );
    let diff = ProjectionDiff {
        version: ProtocolVersion::V1,
        session,
        scene: SceneDiff::new(EPOCH, base, revision, Vec::new()),
        presentation: vec![PresentationChange::ReplaceOffers {
            key: key(index),
            offers: vec![offer(label(index, changed))],
        }],
        status: None,
    };
    let start = now_ms();
    answer(
        remote,
        CarrierResponseBody::Resume(ResumeReply::Diffs(vec![diff])),
        |body| matches!(body, CarrierRequestBody::Resume(_)),
    );
    assert_eq!(remote.revision(), Some(revision.0));
    start
}

pub(in super::super) fn show_fixture(page: &mut TreePage) {
    if page.shared.remote.borrow().fixture.is_none() {
        return;
    }
    if page.session != Session::Remote {
        page.show(Session::Remote);
        page.tools_open = true;
    }
    if LAST_PAGE.replace(false) {
        cards::handle_key(
            page,
            cambium::KeyEvent::new(cambium::Key::Named(cambium::NamedKey::End)),
        );
    }
}

pub(in super::super) fn observe_frame(
    ctx: &mut cambium_rootstock::AppCtx<'_, TreePage, Logic, Child>,
) {
    let accepted_at_ms = now_ms();
    if ctx.runner.state().shared.remote.borrow().fixture.is_none() {
        return;
    }
    let Some(stamp) = ctx.presentation else {
        return;
    };
    if GEOMETRY_PROBE.replace(false) {
        let geometry = ctx
            .producer_content_rect(super::super::CANVAS_KEY)
            .map(|content| {
                let shared = &ctx.runner.state().shared;
                let remote = shared.remote.borrow();
                let (width, height) = shared.size.get();
                // Deliberately use the authoritative full projection only when an
                // explicit correctness probe asks, outside measured frame loops.
                let cards = remote.board.scene().card_rects(
                    remote.board.board(),
                    width,
                    height,
                    remote.board_fit((width, height)),
                );
                let device = web_sys::window().map_or(1.0, |window| window.device_pixel_ratio());
                let scale = f64::from(stamp.layout_scale) / device;
                let mut indices = vec![0, 5, cards.len() / 2, cards.len().saturating_sub(1)];
                indices.sort_unstable();
                indices.dedup();
                let samples: Vec<_> = indices
                    .into_iter()
                    .filter_map(|index| {
                        let (key, label, rect) = cards.get(index)?;
                        Some(json!({
                            "key": key,
                            "label": label,
                            "rect": [
                                (f64::from(content.0) + f64::from(rect.x)) * scale,
                                (f64::from(content.1) + f64::from(rect.y)) * scale,
                                f64::from(rect.width) * scale,
                                f64::from(rect.height) * scale,
                            ],
                        }))
                    })
                    .collect();
                json!({
                    "presentation": stamp.sequence,
                    "scene_size": cards.len(),
                    "canvas_size": [width, height],
                    "scale": scale,
                    "samples": samples,
                })
            });
        GEOMETRY_RESULT.set(Some(
            geometry
                .unwrap_or_else(|| json!({"error":"producer content unavailable"}))
                .to_string(),
        ));
    }
    if !REVEAL.get() && !SAMPLE.with_borrow(|sample| sample.is_some()) {
        return;
    }
    let dom = ctx.runner.dom();
    let dom = dom.borrow();
    let ns = Namespace::from("");
    let class = LocalName::from("class");
    let mut pending = vec![ctx.runner.mount()];
    let Some(list) = std::iter::from_fn(|| {
        let node = pending.pop()?;
        pending.extend(dom.dom_children(node));
        Some(node)
    })
    .find(|node| dom.attribute(*node, &ns, &class) == Some("tools-cards")) else {
        return;
    };
    if REVEAL.replace(false) {
        drop(dom);
        ctx.scroll_into_view(list, cambium_rootstock::ScrollAlign::Start);
        if let Some(window) = ctx.window {
            window.request_redraw();
        }
        return;
    }
    let pos = LocalName::from("aria-posinset");
    let revision = ctx.runner.state().shared.remote.borrow().revision();
    record(|sample| {
        sample.presentation = Some(stamp.sequence);
        sample.revision = revision;
        sample.rows = dom
            .dom_children(list)
            .filter_map(|node| {
                let index = dom.attribute(node, &ns, &pos)?.parse().ok()?;
                let label = dom
                    .dom_children(node)
                    .find_map(|text| dom.text(text))?
                    .to_owned();
                Some((
                    index,
                    format!("{node:?}"),
                    label,
                    ctx.visible_rect(node).is_some(),
                ))
            })
            .collect();
    });
    let (start, ingress_ms) = START.get();
    RESULT.set(Some(
        json!({
            "elapsed_ms": accepted_at_ms - start,
            "ingress_ms": ingress_ms,
            "sample": SAMPLE.take(),
            "frame": ctx.frame_profile.map(frame_profile),
        })
        .to_string(),
    ));
}

fn frame_profile(profile: cambium_rootstock::FrameProfile) -> Value {
    json!({
        "mutations": profile.layout_mutations,
        "layout_rebuilt": profile.layout_rebuilt,
        "layout_retained": profile.layout_retained,
        "restyled_elements": profile.restyled_elements,
        "layout_ms": profile.layout_with_text_us as f64 / 1000.0,
        "style_ms": profile.style_resolve_us as f64 / 1000.0,
        "emit_ms": profile.emit_scene_us as f64 / 1000.0,
        "portable_producer_plan_ms": profile.portable_producer_plan_us as f64 / 1000.0,
        "portable_assembly_ms": profile.portable_assembly_us as f64 / 1000.0,
        "portable_materialize_ms": profile.portable_materialize_us as f64 / 1000.0,
        "portable_materialization_reused": profile.portable_materialization_reused,
        "raster_ms": profile.raster_us as f64 / 1000.0,
        "raster_reused": profile.raster_reused,
        "a11y_publish_ms": profile.a11y_publish_us as f64 / 1000.0,
        "a11y_actions_ms": profile.a11y_actions_us as f64 / 1000.0,
        "host_ms": profile.total_us as f64 / 1000.0,
    })
}

#[wasm_bindgen]
pub fn cards_receipt_mount(count: u32, compact_labels: bool, mode: &str) -> Result<(), JsValue> {
    if !matches!(count, 1_000 | 5_000) {
        return Err(JsValue::from_str("fixture supports 1000 and 5000 cards"));
    }
    let mode = match mode {
        "legacy" => Mode::Legacy,
        "quiet-revision" => Mode::QuietRevision,
        "optimized" => Mode::Optimized,
        _ => return Err(JsValue::from_str("unknown Cards receipt mode")),
    };
    TREE.with_borrow(|tree| {
        let tree = tree
            .as_ref()
            .ok_or_else(|| JsValue::from_str("tree not mounted"))?;
        {
            let mut remote = tree.shared.remote.borrow_mut();
            MODE.set(mode);
            FIXTURE_MOUNTED.set(true);
            PAINT.set((0, 0, Vec::new()));
            CHANGED_LABELS.with_borrow_mut(HashSet::clear);
            COMPACT_LABELS.set(compact_labels);
            PAN_TO.set(None);
            remote.attachment = remote.attachment.wrapping_add(1);
            remote.fixture = Some(mount(count));
            remote.cards = cards::Cache::default();
            remote.board = graphshell_host::board::RemoteBoard::new();
            remote.generation += 1;
        }
        REVEAL.set(true);
        tree.appearance.window().request_redraw();
        Ok(())
    })
}

#[wasm_bindgen]
pub fn cards_receipt_request(
    edit_index: Option<u32>,
    last_page: bool,
    pan_to: Option<u32>,
) -> Result<(), JsValue> {
    TREE.with_borrow(|tree| {
        let tree = tree
            .as_ref()
            .ok_or_else(|| JsValue::from_str("tree not mounted"))?;
        let mut start = now_ms();
        let mut ingress_ms = 0.0;
        {
            let mut remote = tree.shared.remote.borrow_mut();
            let session = remote
                .fixture
                .as_mut()
                .ok_or_else(|| JsValue::from_str("fixture not mounted"))?;
            if let Some(index) = edit_index {
                start = edit(session, index);
                ingress_ms = now_ms() - start;
                remote.generation += 1;
            }
            if last_page {
                LAST_PAGE.set(true);
                remote.generation += 1;
            }
        }
        START.set((start, ingress_ms));
        RESULT.set(None);
        SAMPLE.set(Some(Sample::default()));
        if PAN_TO.replace(pan_to) != pan_to {
            tree.shared.dirty.set(true);
        }
        tree.appearance.window().request_redraw();
        Ok(())
    })
}

#[wasm_bindgen]
pub fn cards_receipt_take() -> Option<String> {
    RESULT.take()
}

/// Ask the next presented frame for a full geometry oracle. This is separate
/// from timing samples so verification never adds scene work to timed frames.
#[wasm_bindgen]
pub fn cards_receipt_geometry_request() {
    GEOMETRY_RESULT.set(None);
    GEOMETRY_PROBE.set(true);
    TREE.with_borrow(|tree| {
        if let Some(tree) = tree {
            tree.appearance.window().request_redraw();
        }
    });
}

#[wasm_bindgen]
pub fn cards_receipt_geometry_take() -> Option<String> {
    GEOMETRY_RESULT.take()
}
