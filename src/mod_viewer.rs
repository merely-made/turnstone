// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The Mod viewer: one contributed pane that feeds an installed viewer mod the live graph and
//! paints the scene and sheet it returns (mod projects MP7b; D45, D50, D51, D52).
//!
//! The pane is opened with the mod's id as its source (`source`). The shell publishes the live
//! graph to a [`Handles`] before the pane draws; the session asks the mod only when the graph
//! (its revision) or the pane's width changed, never every frame. MP7c gives each viewer its own
//! pane kind and retires the single kind; the feed, the re-run gate and the painter here carry
//! over unchanged.

use std::sync::{Arc, Mutex, MutexGuard};

use cambium::{
    AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, ResolvedSurfaceEvent,
    RetainedSurfaceSession, SurfaceEffect, SurfaceViewport, custom_leaf, el, status_note,
};
use genet_scripted_dom::NodeId;
use kernel::graph::Graph;
use mere_surface_api::{
    ProviderId, SourceKindId, SurfaceAvailability, SurfaceDescriptor, SurfaceId,
    SurfaceSourceShape, SurfaceUnavailableReason,
};
use mod_host::ViewedScene;
use sceno::Footprint;
use serde::Deserialize;
use sheet_leaf::{ColorF, SheetLeaf, SheetTheme, TextRun};
use sprigging::LeafRegistry;

use crate::contributed_surface::{SurfaceAdmissionError, SurfaceProvider};
use crate::mod_projects::ModProjectHost;
use crate::panes::{PaneKindId, PaneSource, SerializedSource, SourceRef, SourceSchemaId};

pub(crate) const PANE_KIND: &str = "turnstone.mod-viewer";
pub(crate) const LABEL: &str = "Mod viewer";
pub(crate) const SOURCE_SCHEMA: &str = "turnstone.mod-viewer.v1";
const SHEET_LEAF: u64 = 0x6d6f_6476;
const CSS: &str = "
.mod-viewer { color: var(--tabard-color-text, rgb(220, 226, 238)); }
.mod-viewer h2 { margin: 0; padding: 8px 8px 2px; font-size: 16px; }
.mod-viewer-note, .mod-viewer-regions { margin: 0; padding: 2px 8px; font-size: 13px; overflow-wrap: anywhere; }
.mod-viewer-regions { color: var(--tabard-color-text-dim, rgb(150, 158, 172)); }
.mod-viewer-sheet { position: relative; }
.mod-viewer-region { position: absolute; box-sizing: border-box; border: 1px dashed var(--tabard-color-text-dim, rgb(150, 158, 172)); pointer-events: none; }
";

/// The pane's source: the viewer mod to run, by id.
pub(crate) fn source(mod_id: &str) -> PaneSource {
    PaneSource::Fixed(SourceRef::External {
        schema: SourceSchemaId::new(SOURCE_SCHEMA),
        payload: SerializedSource {
            version: 1,
            payload: serde_json::json!({ "mod": mod_id }),
        },
    })
}

#[derive(Deserialize)]
struct ViewerSource {
    #[serde(rename = "mod")]
    mod_id: String,
}

// ---- the feed -------------------------------------------------------------------------

/// The graph as `mere.graph-snapshot-and-facets/json`, the object a viewer reads:
/// `{"snapshot": ..., "facets": ...}`, written by authority-host, the format's one home.
///
/// # Errors
///
/// The graph could not be serialised.
pub(crate) fn encode_feed(graph: &Graph) -> Result<Vec<u8>, String> {
    authority_host::encode_graph_feed(graph)
}

/// What a graph change is read from: the structural revision and the content revision. A
/// title edit moves only the second, and a viewer reads titles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FeedKey {
    revision: u64,
    content: u64,
}

impl FeedKey {
    pub(crate) fn of(graph: &Graph) -> Self {
        Self {
            revision: graph.revision(),
            content: graph.content_revision(),
        }
    }
}

#[derive(Default)]
struct Published {
    key: Option<FeedKey>,
    graph: Option<Result<Arc<[u8]>, String>>,
    theme: Option<(SheetTheme, ColorF)>,
}

/// What the shell publishes and the sessions read: the encoded live graph, keyed by its
/// revisions, and the sheet theme. Encoding happens only when the key moved.
#[derive(Clone, Default)]
pub(crate) struct Handles(Arc<Mutex<Published>>);

impl Handles {
    fn lock(&self) -> MutexGuard<'_, Published> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Publish the live graph, and the theme panes paint a sheet in.
    pub(crate) fn publish(&self, graph: &Graph, theme: (SheetTheme, ColorF)) {
        let mut published = self.lock();
        published.theme = Some(theme);
        let key = FeedKey::of(graph);
        if published.key == Some(key) {
            return;
        }
        published.graph = Some(encode_feed(graph).map(Arc::from));
        published.key = Some(key);
    }

    fn read(
        &self,
    ) -> Option<(
        FeedKey,
        Result<Arc<[u8]>, String>,
        Option<(SheetTheme, ColorF)>,
    )> {
        let published = self.lock();
        Some((published.key?, published.graph.clone()?, published.theme))
    }
}

// ---- what a run leaves ----------------------------------------------------------------

/// One region the viewer named, as the box its first member item occupies: the item's
/// translate is its centre (block-adapter's `regions()` reads it the same way).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RegionBox {
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub(crate) fn region_boxes(scene: &sceno::Scene) -> Vec<RegionBox> {
    scene
        .regions
        .iter()
        .filter_map(|region| {
            let label = region.label.clone()?;
            let item = scene.items.get(region.members.first()?.0 as usize)?;
            let Footprint::Rect { size } = item.hit.as_ref().unwrap_or(&item.footprint) else {
                return None;
            };
            let c = item.transform.translate;
            Some(RegionBox {
                label,
                x: c.x - size.w / 2.0,
                y: c.y - size.h / 2.0,
                w: size.w,
                h: size.h,
            })
        })
        .collect()
}

struct Shown {
    title: String,
    viewed: ViewedScene,
    runs: Vec<TextRun>,
    regions: Vec<RegionBox>,
}

enum Outcome {
    Waiting,
    Shown(Box<Shown>),
    Failed(String),
}

struct State {
    mod_id: String,
    mods: Arc<ModProjectHost>,
    handles: Handles,
    /// What the viewer last ran on: the graph's key and the pane's logical width.
    ran: Option<(FeedKey, u32)>,
    /// How many times the viewer has been asked: what the gate saves.
    runs: u32,
    theme: Option<(SheetTheme, ColorF)>,
    outcome: Outcome,
}

impl State {
    /// Ask the viewer again when the graph or the width moved; whether anything changed.
    fn refresh(&mut self, width: f32) -> bool {
        let Some((key, graph, theme)) = self.handles.read() else {
            return false;
        };
        let wide = width.round().max(1.0) as u32;
        let theme_moved = theme != self.theme;
        if self.ran == Some((key, wide)) && !theme_moved {
            return false;
        }
        self.theme = theme;
        if self.ran != Some((key, wide)) {
            self.ran = Some((key, wide));
            self.runs += 1;
            self.outcome = match graph {
                Err(why) => Outcome::Failed(why),
                Ok(bytes) => match self.mods.view_graph(&self.mod_id, &bytes, wide as f32) {
                    Ok((title, viewed)) => Outcome::Shown(Box::new(Shown {
                        title,
                        regions: region_boxes(&viewed.scene),
                        viewed,
                        runs: Vec::new(),
                    })),
                    Err(why) => Outcome::Failed(why),
                },
            };
        }
        self.paint_runs();
        true
    }

    /// The sheet's text runs in the current theme, for the host to draw as text.
    fn paint_runs(&mut self) {
        let Some((theme, ground)) = self.theme else {
            return;
        };
        if let Outcome::Shown(shown) = &mut self.outcome {
            shown.runs = SheetLeaf::new(shown.viewed.sheet.clone(), theme)
                .map(|leaf| leaf.on_ground(ground).texts().to_vec())
                .unwrap_or_default();
        }
    }
}

// ---- the view -------------------------------------------------------------------------

type Child = Box<dyn AnyView<State, (), GenetCtx, GenetElement>>;
type Runner = GenetAppRunner<State, fn(&State) -> Child, Child, ()>;

fn view(state: &State) -> Child {
    let mut body: Vec<Child> = Vec::new();
    match &state.outcome {
        Outcome::Waiting => {
            body.push(Box::new(el("h2", state.mod_id.clone())));
            body.push(Box::new(status_note(
                "mod-viewer-note",
                format!("Waiting for {}", state.mod_id),
            )));
        },
        Outcome::Failed(why) => {
            body.push(Box::new(el("h2", state.mod_id.clone())));
            body.push(Box::new(status_note(
                "mod-viewer-note",
                format!("Mod viewer: {why}"),
            )));
        },
        Outcome::Shown(shown) => {
            let viewed = &shown.viewed;
            body.push(Box::new(el("h2", shown.title.clone())));
            let mut sheet: Vec<Child> = vec![Box::new(
                custom_leaf::<State, ()>(
                    SHEET_LEAF,
                    viewed.width.ceil() as u32,
                    viewed.height.ceil() as u32,
                )
                .attr(
                    "aria-label",
                    format!("{} · {}", shown.title, viewed.description),
                )
                .attr("class", "mod-viewer-leaf"),
            )];
            for run in &shown.runs {
                sheet.push(Box::new(el("span", run.text.clone()).attr("aria-hidden", "true").attr(
                    "style",
                    format!(
                        "position:absolute;left:{}px;top:{}px;font-size:{}px;color:{};pointer-events:none;",
                        run.x,
                        run.y,
                        run.size,
                        run.css_colour()
                    ),
                )));
            }
            for region in &shown.regions {
                sheet.push(Box::new(
                    el("div", ())
                        .attr("class", "mod-viewer-region")
                        .attr("aria-label", region.label.clone())
                        .attr(
                            "style",
                            format!(
                                "left:{}px;top:{}px;width:{}px;height:{}px;",
                                region.x, region.y, region.w, region.h
                            ),
                        ),
                ));
            }
            body.push(Box::new(
                el("div", sheet).attr("class", "mod-viewer-sheet").attr(
                    "style",
                    format!("width:{}px;height:{}px;", viewed.width, viewed.height),
                ),
            ));
            let labels: Vec<&str> = shown.regions.iter().map(|r| r.label.as_str()).collect();
            body.push(Box::new(
                el("p", format!("Regions: {}", labels.join(" · ")))
                    .attr("class", "mod-viewer-regions"),
            ));
        },
    }
    Box::new(el("div", body).attr("class", "mod-viewer"))
}

// ---- the provider and its session -----------------------------------------------------

/// Register the provider when mod projects are open; the handles the shell publishes through.
pub(crate) fn register(
    registry: &mut crate::contributed_surface::SurfaceProviderRegistry,
    mods: Option<Arc<ModProjectHost>>,
) -> Handles {
    let handles = Handles::default();
    if let Some(mods) = mods {
        registry
            .register_provider(ModViewerProvider::new(mods, handles.clone()))
            .expect("the Mod viewer provider is unique");
    }
    handles
}

pub(crate) struct ModViewerProvider {
    kind: PaneKindId,
    schema: SourceSchemaId,
    descriptor: SurfaceDescriptor,
    mods: Arc<ModProjectHost>,
    handles: Handles,
}

impl ModViewerProvider {
    pub(crate) fn new(mods: Arc<ModProjectHost>, handles: Handles) -> Self {
        Self {
            kind: PaneKindId::new(PANE_KIND),
            schema: SourceSchemaId::new(SOURCE_SCHEMA),
            descriptor: SurfaceDescriptor {
                provider_id: ProviderId::from("turnstone"),
                surface_id: SurfaceId::from(SOURCE_SCHEMA),
                label: LABEL.into(),
                accepted_source: SurfaceSourceShape::One(SourceKindId::from(SOURCE_SCHEMA)),
            },
            mods,
            handles,
        }
    }
}

impl SurfaceProvider for ModViewerProvider {
    fn pane_kind(&self) -> &PaneKindId {
        &self.kind
    }
    fn source_schema(&self) -> &SourceSchemaId {
        &self.schema
    }
    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }
    fn stylesheet(&self) -> &str {
        CSS
    }

    fn admit(
        &self,
        source: &PaneSource,
        dom: DomHandle,
    ) -> Result<Box<dyn RetainedSurfaceSession>, SurfaceAdmissionError> {
        let invalid = |message: &str| SurfaceAdmissionError::InvalidPayload {
            schema: self.schema.clone(),
            message: message.into(),
        };
        let PaneSource::Fixed(SourceRef::External { schema, payload }) = source else {
            return Err(invalid("The Mod viewer needs a viewer mod id."));
        };
        if schema != &self.schema || payload.version != 1 {
            return Err(invalid("This Mod viewer source version is unsupported."));
        }
        let ViewerSource { mod_id } = serde_json::from_value(payload.payload.clone())
            .map_err(|error| invalid(&error.to_string()))?;
        if mod_id.is_empty() {
            return Err(invalid("The Mod viewer needs a viewer mod id."));
        }
        let state = State {
            mod_id,
            mods: self.mods.clone(),
            handles: self.handles.clone(),
            ran: None,
            runs: 0,
            theme: None,
            outcome: Outcome::Waiting,
        };
        Ok(Box::new(Session {
            descriptor: self.descriptor.clone(),
            runner: GenetAppRunner::new(dom, view as fn(&State) -> Child, state),
            leaves: LeafRegistry::new(),
            leaf_dirty: true,
        }))
    }
}

struct Session {
    descriptor: SurfaceDescriptor,
    runner: Runner,
    leaves: LeafRegistry<u64>,
    leaf_dirty: bool,
}

impl Session {
    fn effects(&mut self) -> Vec<SurfaceEffect> {
        let mut effects = vec![SurfaceEffect::Redraw];
        if let Some(exit) = self.runner.take_focus_exit() {
            effects.push(SurfaceEffect::FocusExit(exit));
        }
        effects
    }
}

impl RetainedSurfaceSession for Session {
    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }
    fn availability(&self) -> SurfaceAvailability {
        match &self.runner.state().outcome {
            Outcome::Failed(why) => {
                SurfaceAvailability::Unavailable(SurfaceUnavailableReason::Other(why.clone()))
            },
            _ => SurfaceAvailability::Available,
        }
    }
    fn dom(&self) -> DomHandle {
        self.runner.dom()
    }
    fn root(&self) -> NodeId {
        self.runner.root()
    }
    fn focus(&self) -> Option<NodeId> {
        self.runner.focus()
    }
    fn set_focus(&mut self, node: Option<NodeId>) -> Vec<SurfaceEffect> {
        self.runner.set_focus(node);
        self.effects()
    }
    fn focus_traverse(&mut self, forward: bool) -> Vec<SurfaceEffect> {
        self.runner.focus_traverse(forward);
        self.effects()
    }
    fn focusables(&self) -> Vec<NodeId> {
        self.runner.focusables()
    }
    fn pointer_capture(&self) -> Option<NodeId> {
        self.runner.pointer_capture()
    }
    fn pointer_target(&self, hit: NodeId) -> Option<NodeId> {
        self.runner.pointer_target(hit)
    }
    fn hover_target(&self, hit: NodeId) -> Option<NodeId> {
        self.runner.hover_target(hit)
    }
    fn wheel_target(&self, hit: NodeId) -> Option<NodeId> {
        self.runner.wheel_target(hit)
    }
    fn set_focus_exits(&mut self, exits: bool) -> bool {
        self.runner.set_focus_exits(exits);
        true
    }
    fn sync_viewport(&mut self, viewport: SurfaceViewport) -> Vec<SurfaceEffect> {
        let mut changed = false;
        self.runner
            .update(|state| changed = state.refresh(viewport.width));
        if !changed {
            return Vec::new();
        }
        self.leaf_dirty = true;
        self.effects()
    }
    fn dispatch(&mut self, event: ResolvedSurfaceEvent) -> Vec<SurfaceEffect> {
        match event {
            ResolvedSurfaceEvent::Click { target, event } => {
                self.runner.dispatch_click(target, event)
            },
            ResolvedSurfaceEvent::Key(event) => self.runner.dispatch_key(event),
            ResolvedSurfaceEvent::PointerDown { target, event } => {
                self.runner.dispatch_pointer_down(target, event)
            },
            ResolvedSurfaceEvent::PointerMove(event) => self.runner.dispatch_pointer_move(event),
            ResolvedSurfaceEvent::PointerUp(event) => self.runner.dispatch_pointer_up(event),
            ResolvedSurfaceEvent::Hover { target, event } => {
                self.runner.dispatch_hover(target, event)
            },
            ResolvedSurfaceEvent::Wheel { target, event } => {
                self.runner.dispatch_wheel(target, event)
            },
        };
        self.effects()
    }
    fn leaves(&mut self) -> Option<&mut LeafRegistry<u64>> {
        if std::mem::take(&mut self.leaf_dirty) {
            let state = self.runner.state();
            let leaf = match (&state.outcome, state.theme) {
                (Outcome::Shown(shown), Some((theme, ground))) => {
                    SheetLeaf::new(shown.viewed.sheet.clone(), theme)
                        .ok()
                        .map(|l| l.on_ground(ground))
                },
                _ => None,
            };
            match leaf {
                Some(leaf) => self.leaves.insert(SHEET_LEAF, Box::new(leaf)),
                None => drop(self.leaves.remove(&SHEET_LEAF)),
            }
        }
        Some(&mut self.leaves)
    }
}

#[cfg(test)]
#[path = "mod_viewer/tests.rs"]
mod tests;
