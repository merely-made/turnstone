//! The Frozen Projection pane: the disclosed scene as navigable semantics.
//!
//! Turnstone is the endpoint the graphshell wire was proven against, and
//! `remote_projection::disclose_scene` is the exact scene a remote peer is
//! served. This pane freezes that scene through
//! `graphshell_client::frozen::FrozenScene` and shows the result in cambium
//! chrome: every instance by name, every relation named at both ends, the
//! WAI-style summary, and the placement-satisfaction line when there is one.
//!
//! Two jobs at once, deliberately. It is the first host surface for the frozen
//! realization, which makes it the thing a probe scenario can finally drive;
//! and it is satisfaction in host chrome, read off the scene rather than
//! recomputed. Names come from `node_display_label`, the same label the
//! endpoint's presentation plane sends, so the frozen form and the remote
//! peer agree about what things are called.

use std::cell::RefCell;
use std::rc::Rc;

use cambium::{AnyView, DomHandle, GenetAppRunner, GenetCtx, GenetElement, el, text};
use genet_scripted_dom::ScriptedDom;
use graphshell_client::frozen::{FrozenScene, Satisfaction};

use crate::app::App;

struct FrozenProjectionPaneState {
    frozen: Option<FrozenScene>,
    satisfaction: Option<String>,
    viewport_w: f32,
    viewport_h: f32,
}

type FrozenProjectionPaneView =
    Box<dyn AnyView<FrozenProjectionPaneState, (), GenetCtx, GenetElement>>;
type FrozenProjectionPaneRunner = GenetAppRunner<
    FrozenProjectionPaneState,
    fn(&FrozenProjectionPaneState) -> FrozenProjectionPaneView,
    FrozenProjectionPaneView,
    (),
>;

fn role_word(role: graphshell_client::frozen::FrozenRole) -> &'static str {
    use graphshell_client::frozen::FrozenRole;
    match role {
        FrozenRole::Symbol => "symbol",
        FrozenRole::Object => "object",
        FrozenRole::LiveContent => "live content",
    }
}

fn frozen_projection_pane_view(state: &FrozenProjectionPaneState) -> FrozenProjectionPaneView {
    let mut rows: Vec<FrozenProjectionPaneView> = Vec::new();

    let Some(frozen) = &state.frozen else {
        rows.push(Box::new(
            el::<_, FrozenProjectionPaneState, ()>("div", text("No projection to freeze yet."))
                .attr("class", "pane-empty"),
        ));
        return wrap(state, rows);
    };

    rows.push(Box::new(
        el::<_, FrozenProjectionPaneState, ()>("div", text(frozen.name.clone()))
            .attr("class", "list-section-title"),
    ));
    rows.push(Box::new(
        el::<_, FrozenProjectionPaneState, ()>("div", text(frozen.summary.clone()))
            .attr("class", "list-row muted")
            .attr("data-frozen-summary", ""),
    ));
    if let Some(line) = &state.satisfaction {
        // C1's chrome line: spoken only when a hold exists, because a scene
        // with nothing pinned has nothing to satisfy.
        rows.push(Box::new(
            el::<_, FrozenProjectionPaneState, ()>("div", text(line.clone()))
                .attr("class", "list-row muted")
                .attr("data-frozen-satisfaction", ""),
        ));
    }

    for instance in &frozen.instances {
        // The carried identity a probe drives by, and the name a reader hears.
        rows.push(Box::new(
            el::<_, FrozenProjectionPaneState, ()>(
                "div",
                text(format!("{} · {}", instance.name, role_word(instance.role))),
            )
            .attr("class", "list-row")
            .attr("role", "graphics-symbol")
            .attr("aria-label", instance.name.clone())
            .attr("data-source-id", instance.source.id.clone()),
        ));
    }
    for relation in &frozen.relations {
        let kind = relation
            .kind
            .clone()
            .unwrap_or_else(|| "related to".to_owned());
        rows.push(Box::new(
            el::<_, FrozenProjectionPaneState, ()>(
                "div",
                text(format!("{} {} {}", relation.from, kind, relation.to)),
            )
            .attr("class", "list-row muted")
            .attr("data-frozen-relation", ""),
        ));
    }
    for held in &frozen.unmet_holds {
        rows.push(Box::new(
            el::<_, FrozenProjectionPaneState, ()>(
                "div",
                text(format!(
                    "{} could not be placed at ({}, {})",
                    held.source.id, held.at.x, held.at.y
                )),
            )
            .attr("class", "list-row action")
            .attr("data-frozen-unmet", ""),
        ));
    }
    wrap(state, rows)
}

fn wrap(
    state: &FrozenProjectionPaneState,
    rows: Vec<FrozenProjectionPaneView>,
) -> FrozenProjectionPaneView {
    Box::new(
        el::<_, FrozenProjectionPaneState, ()>("div", rows)
            .attr("class", "pane")
            .attr(
                "style",
                format!(
                    "width: {}px; height: {}px;",
                    state.viewport_w, state.viewport_h
                ),
            ),
    )
}

/// Retained panel over the frozen realization of the disclosed scene.
pub struct FrozenProjectionPane {
    dom: DomHandle,
    runner: FrozenProjectionPaneRunner,
    scroll: crate::ui::PaneScroll,
    layout: crate::ui::RetainedLayout,
}

impl FrozenProjectionPane {
    pub fn new() -> Self {
        let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
        let state = FrozenProjectionPaneState {
            frozen: None,
            satisfaction: None,
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        let runner = FrozenProjectionPaneRunner::new(
            dom.clone(),
            frozen_projection_pane_view
                as fn(&FrozenProjectionPaneState) -> FrozenProjectionPaneView,
            state,
        );
        Self {
            dom,
            runner,
            scroll: crate::ui::PaneScroll::new(),
            layout: crate::ui::RetainedLayout::new(),
        }
    }

    pub fn scroll_by(&mut self, dx: f32, dy: f32) {
        self.scroll.nudge(dx, dy);
    }

    pub fn bars_visible(&mut self) -> bool {
        self.scroll.bars_visible()
    }

    /// Rebuild the frozen realization from the live graph.
    ///
    /// Same recipe the endpoint serves a remote peer, frozen with the same
    /// display labels the presentation plane sends. Rebuilt on sync because the
    /// graph is the truth and this pane is a reading of it, not a copy.
    pub fn sync(&mut self, app: &App, pane_w: f32, pane_h: f32) {
        let graph = app.graph_runtimes.graph();
        let scene = crate::remote_projection::disclose_scene(
            graph,
            app.graph_runtimes.focused_key(),
            (248.0, 168.0),
            sceno::Spiral::default(),
        );
        let names = graph
            .nodes()
            .map(|(key, node)| {
                (
                    sceno::SourceRef::new(cartography::MERE_GRAPH_ADAPTER, node.id.to_string()),
                    graph.node_display_label(key),
                )
            })
            .collect();
        let satisfaction = Satisfaction {
            honored: scene.honored_holds.len(),
            unmet: scene.unmet_holds.len(),
        }
        .line();
        let frozen = FrozenScene::freeze(&scene, "Disclosed projection", &names);
        self.runner.update(|state| {
            state.frozen = Some(frozen);
            state.satisfaction = satisfaction;
            state.viewport_w = pane_w;
            state.viewport_h = pane_h;
        });
    }

    pub fn scene(&mut self, w: u32, h: u32) -> netrender::Scene {
        self.layout.scene_scrolled(
            &mut self.dom.borrow_mut(),
            crate::ui::CAMBIUM_SHEET,
            w,
            h,
            &mut self.scroll,
        )
    }

    pub fn click(&mut self, x: f32, y: f32, w: u32, h: u32) {
        let hit = self.layout.hit_test_scrolled(
            &mut self.dom.borrow_mut(),
            crate::ui::CAMBIUM_SHEET,
            w,
            h,
            x,
            y,
            &self.scroll,
        );
        if let Some(node) = hit {
            let _: Vec<()> = self
                .runner
                .dispatch_click(node, cambium::PointerClick::at((x, y)));
        }
    }

    pub fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }
}
