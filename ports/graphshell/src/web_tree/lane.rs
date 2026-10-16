// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Product observations and actions for Mesquite on the tree page.
use super::*;
pub(super) mod overlay_observation;

/// The tree page's half of the scenario lane.
pub(super) struct TreeLane {
    pub(super) shared: Rc<Shared>,
    pub(super) errors: Vec<String>,
    pub(super) pointer: Option<(f32, f32)>,
    /// Page px where the last `release-at` let go, for `drag-return`.
    pub(super) drop: Option<(f32, f32)>,
    /// The same drop in world units, for `drag-return-world`.
    pub(super) drop_world: Option<(f32, f32)>,
    pub(super) camera_mark: Option<(f32, f32)>,
}

/// `layout-*` fields are a pairwise pass plus all-pairs hop distances, so a
/// snapshot computes them only for graphs up to this size.
const LAYOUT_STATS_LIMIT: usize = 512;

/// The page's own accessibility instrument, `window.graphshellSemanticTree()`
/// (loader.js), read as JSON (dynamics grammar plan, F68).
fn semantic_tree() -> Option<serde_json::Value> {
    use wasm_bindgen::JsCast;
    let window = web_sys::window()?;
    let read = js_sys::Reflect::get(&window, &"graphshellSemanticTree".into())
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;
    let tree = read.call0(&window).ok()?;
    let text = js_sys::JSON::stringify(&tree).ok()?.as_string()?;
    serde_json::from_str(&text).ok()
}

/// The host dataset as the semantic tree tells it (S1): the labels of the
/// "Relations" region's list items, and the text of the first alert.
fn heard_dataset(tree: &serde_json::Value) -> (Vec<String>, Option<String>) {
    let field = |node: &serde_json::Value, name: &str| {
        node.get(name).and_then(|v| v.as_str()).map(str::to_owned)
    };
    let children = |node: &serde_json::Value| {
        node.get("children")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    };
    let (mut relations, mut alert) = (Vec::new(), None);
    let mut stack = vec![tree.clone()];
    while let Some(node) = stack.pop() {
        let role = field(&node, "role");
        if role.as_deref() == Some("alert") && alert.is_none() {
            alert = field(&node, "label").or_else(|| Some(String::new()));
        }
        if role.as_deref() == Some("region")
            && field(&node, "label").as_deref() == Some("Relations")
        {
            let mut items = vec![node.clone()];
            while let Some(item) = items.pop() {
                if field(&item, "role").as_deref() == Some("listitem") {
                    relations.extend(field(&item, "label"));
                }
                items.extend(children(&item));
            }
            continue;
        }
        stack.extend(children(&node));
    }
    relations.sort();
    (relations, alert)
}

/// The named region's list items and its status line, as the semantic tree
/// tells them (S3's "History" region).
fn heard_region(tree: &serde_json::Value, name: &str) -> (Vec<String>, Option<String>) {
    let field = |node: &serde_json::Value, key: &str| {
        node.get(key).and_then(|v| v.as_str()).map(str::to_owned)
    };
    let children = |node: &serde_json::Value| {
        node.get("children")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    };
    let mut stack = vec![tree.clone()];
    while let Some(node) = stack.pop() {
        if field(&node, "role").as_deref() == Some("region")
            && field(&node, "label").as_deref() == Some(name)
        {
            let (mut items, mut status) = (Vec::new(), None);
            let mut inner = vec![node];
            while let Some(item) = inner.pop() {
                match field(&item, "role").as_deref() {
                    Some("listitem") => items.extend(field(&item, "label")),
                    Some("status") if status.is_none() => status = field(&item, "label"),
                    _ => {},
                }
                inner.extend(children(&item));
            }
            items.sort();
            return (items, status);
        }
        stack.extend(children(&node));
    }
    (Vec::new(), None)
}

/// One item of the canvas slot as the semantic tree tells it: its name, its
/// buttons' names, and how many of them carry a description.
struct HeardItem {
    name: String,
    buttons: Vec<String>,
    described: usize,
}

/// The canvas slot in the page's semantic tree: the group named "... shown",
/// and its items (F65, F67).
fn heard_canvas(tree: &serde_json::Value) -> Option<(String, Vec<HeardItem>)> {
    let field = |node: &serde_json::Value, name: &str| {
        node.get(name).and_then(|v| v.as_str()).map(str::to_owned)
    };
    let children = |node: &serde_json::Value| {
        node.get("children")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    };
    let mut stack = vec![tree.clone()];
    while let Some(node) = stack.pop() {
        let is_slot = field(&node, "role").as_deref() == Some("group")
            && field(&node, "label").is_some_and(|label| label.ends_with(" shown"));
        if !is_slot {
            stack.extend(children(&node));
            continue;
        }
        let items = children(&node)
            .iter()
            .filter(|item| field(item, "role").as_deref() == Some("group"))
            .map(|item| {
                let buttons: Vec<_> = children(item)
                    .into_iter()
                    .filter(|button| field(button, "role").as_deref() == Some("button"))
                    .collect();
                HeardItem {
                    name: field(item, "label").unwrap_or_default(),
                    described: buttons
                        .iter()
                        .filter(|button| {
                            field(button, "description").is_some_and(|d| !d.is_empty())
                        })
                        .count(),
                    buttons: buttons
                        .iter()
                        .map(|button| field(button, "label").unwrap_or_default())
                        .collect(),
                }
            })
            .collect();
        return Some((field(&node, "label").unwrap_or_default(), items));
    }
    None
}

/// A CSS attribute value, quoted.
fn css_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The canvas leaf's painted rect in page px.
fn leaf_rect(ctx: &AppCtx<'_, TreePage, Logic, Child>) -> Option<(f32, f32, f32, f32)> {
    let leaf = {
        let dom = ctx.runner.dom();
        let dom = dom.borrow();
        taproot::matching(&dom, &Selector::class("tree-canvas"))
            .into_iter()
            .next()
    }?;
    ctx.painted_rect(leaf)
}

/// Source-backed observations for the isolated P4 fixture. This deliberately
/// uses the same reader as the opened-link inspector and Canvas adapter.
fn link_rest_fields(canvas: &Canvas, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    use graphshell::mere_host::{
        LINK_REST_A1_ADDRESS, LINK_REST_A2_ADDRESS, LINK_REST_B1_ADDRESS, LINK_REST_B2_ADDRESS,
    };
    use scenograph::link::{LinkPairReading, LinkSource};

    let graph = canvas.graph();
    let endpoint_pairs = [
        ("a1", LINK_REST_A1_ADDRESS, "b1", LINK_REST_B1_ADDRESS),
        ("a1", LINK_REST_A1_ADDRESS, "b2", LINK_REST_B2_ADDRESS),
        ("a2", LINK_REST_A2_ADDRESS, "b1", LINK_REST_B1_ADDRESS),
        ("a2", LINK_REST_A2_ADDRESS, "b2", LINK_REST_B2_ADDRESS),
    ];
    let mut readings = Vec::<LinkPairReading>::new();
    for (_, from_address, _, to_address) in endpoint_pairs {
        let (Some((from, _)), Some((to, _))) = (
            graph.get_node_by_url(from_address),
            graph.get_node_by_url(to_address),
        ) else {
            return snapshot
                .with_field("link-rest-ready", "false")
                .with_field("link-rest-source-facts", "")
                .with_field("link-rest-keys", "")
                .with_field("link-rest-graph-revision", "")
                .with_field("link-rest-appearance-count", "0")
                .with_field("link-rest-source-appearance-count", "0")
                .with_field("link-rest-visible-appearances", "0")
                .with_field("link-rest-summaries", "")
                .with_field("link-rest-source-assertion-ids", "");
        };
        let reading = match mere::canvas::link_reading::read_link_history(
            graph,
            graph.get_node(from).expect("resolved fixture node").id,
            graph.get_node(to).expect("resolved fixture node").id,
            LinkSource::Unavailable {
                reason: "isolated fixture graph has no public source revision".into(),
            },
        ) {
            Ok(reading) => reading,
            Err(error) => {
                return snapshot
                    .with_field("link-rest-ready", "false")
                    .with_field("link-rest-source-error", error)
                    .with_field("link-rest-source-facts", "")
                    .with_field("link-rest-keys", "")
                    .with_field("link-rest-graph-revision", "")
                    .with_field("link-rest-appearance-count", "0")
                    .with_field("link-rest-source-appearance-count", "0")
                    .with_field("link-rest-visible-appearances", "0")
                    .with_field("link-rest-summaries", "")
                    .with_field("link-rest-source-assertion-ids", "");
            },
        };
        readings.push(reading);
    }

    let facts = serde_json::to_string(&readings).unwrap_or_default();
    let mut keys = std::collections::BTreeSet::new();
    let mut appearances = std::collections::BTreeSet::new();
    let mut assertion_ids = std::collections::BTreeSet::new();
    let mut summaries = std::collections::BTreeSet::new();
    for reading in &readings {
        for link in &reading.links {
            let stable_key = link.key.stable_id();
            keys.insert(stable_key.clone());
            for bucket in &link.buckets {
                let (first, second) = if bucket.displayed_from <= bucket.displayed_to {
                    (&bucket.displayed_from, &bucket.displayed_to)
                } else {
                    (&bucket.displayed_to, &bucket.displayed_from)
                };
                appearances.insert((stable_key.clone(), first.clone(), second.clone()));
                for assertion in &bucket.assertions {
                    assertion_ids.insert(assertion.id.clone());
                }
                let summary = link.rest_summary(&bucket.displayed_from, &bucket.displayed_to);
                summaries.insert(
                    format!(
                        "{}:{}:{}:{}:{}:{:.0}",
                        stable_key,
                        first,
                        second,
                        summary.marks.forward,
                        summary.marks.reverse,
                        summary.strength,
                    ) + &format!(":{}", summary.marks.opposed),
                );
            }
        }
    }
    let hidden = canvas.hidden_link_keys();
    let visible = canvas
        .visible_link_appearances()
        .into_iter()
        .filter(|appearance| keys.contains(&appearance.key.stable_id()))
        .collect::<Vec<_>>();
    let visible_strokes = visible
        .iter()
        .map(|appearance| {
            let from = graph.get_node(appearance.display_from).map(|node| node.id);
            let to = graph.get_node(appearance.display_to).map(|node| node.id);
            serde_json::json!({
                "key": appearance.key.stable_id(),
                "from": from,
                "to": to,
                "forward": appearance.marks.forward,
                "reverse": appearance.marks.reverse,
                "opposed": appearance.marks.opposed,
                "strength": appearance.strength,
            })
        })
        .collect::<Vec<_>>();
    let selected = canvas
        .selected_link_keys()
        .into_iter()
        .filter(|key| keys.contains(&key.stable_id()))
        .map(|key| key.stable_id())
        .collect::<Vec<_>>();
    let selected_appearance = canvas
        .selected_link_keys()
        .into_iter()
        .filter(|key| keys.contains(&key.stable_id()))
        .filter_map(|key| {
            canvas
                .link_appearance_pair(&key)
                .map(|(from, to)| format!("{}:{}:{}", key.stable_id(), from, to))
        })
        .collect::<Vec<_>>();
    let resource_appearances = visible
        .iter()
        .filter(|appearance| appearance.key.stratum == scenograph::link::LinkStratum::Resource)
        .collect::<Vec<_>>();
    let surface_appearances = visible
        .iter()
        .filter(|appearance| appearance.key.stratum == scenograph::link::LinkStratum::Surface)
        .collect::<Vec<_>>();
    snapshot
        .with_field("link-rest-ready", "true")
        .with_field("link-rest-source-facts", facts)
        .with_field("link-rest-graph-revision", graph.revision().to_string())
        .with_field(
            "link-rest-keys",
            keys.into_iter().collect::<Vec<_>>().join("|"),
        )
        .with_field("link-rest-appearance-count", visible.len().to_string())
        .with_field(
            "link-rest-source-appearance-count",
            appearances.len().to_string(),
        )
        .with_field("link-rest-visible-appearances", visible.len().to_string())
        .with_field(
            "link-rest-visible-strokes",
            serde_json::to_string(&visible_strokes).unwrap_or_default(),
        )
        .with_field(
            "link-rest-resource-forward",
            resource_appearances
                .iter()
                .any(|appearance| appearance.marks.forward)
                .to_string(),
        )
        .with_field(
            "link-rest-resource-reverse",
            resource_appearances
                .iter()
                .any(|appearance| appearance.marks.reverse)
                .to_string(),
        )
        .with_field(
            "link-rest-resource-opposed",
            resource_appearances
                .iter()
                .any(|appearance| appearance.marks.opposed)
                .to_string(),
        )
        .with_field(
            "link-rest-resource-strength",
            resource_appearances
                .iter()
                .map(|appearance| appearance.strength)
                .fold(0.0_f32, f32::max)
                .to_string(),
        )
        .with_field(
            "link-rest-surface-strength",
            surface_appearances
                .iter()
                .map(|appearance| appearance.strength)
                .fold(0.0_f32, f32::max)
                .to_string(),
        )
        .with_field(
            "link-rest-summaries",
            summaries.into_iter().collect::<Vec<_>>().join("|"),
        )
        .with_field(
            "link-rest-source-assertion-ids",
            assertion_ids.into_iter().collect::<Vec<_>>().join("|"),
        )
        .with_field("link-rest-selected", selected.join("|"))
        .with_field(
            "link-rest-selected-appearance",
            selected_appearance.join("|"),
        )
        .with_field(
            "link-rest-hidden",
            hidden
                .iter()
                .map(scenograph::link::LinkKey::stable_id)
                .collect::<Vec<_>>()
                .join("|"),
        )
        .with_field("link-rest-hidden-count", hidden.len().to_string())
        .with_field(
            "link-rest-hidden-assertions",
            canvas.hidden_assertion_ids().join("|"),
        )
        .with_field(
            "link-rest-hidden-assertion-count",
            canvas.hidden_assertion_ids().len().to_string(),
        )
}

impl TreeLane {
    /// The focused node's page point, for `press-focused`.
    fn focused_point(
        &self,
        ctx: &AppCtx<'_, TreePage, Logic, Child>,
    ) -> Result<(f32, f32), String> {
        let (x, y) = self
            .shared
            .canvas
            .borrow()
            .focused_screen_position()
            .ok_or("wants exactly one focused node")?;
        let (left, top, _, _) = leaf_rect(ctx).ok_or("the canvas leaf is not painted")?;
        Ok((left + x, top + y))
    }

    /// The physics panel's and canvas's observations.
    fn physics_fields(
        &self,
        ctx: &AppCtx<'_, TreePage, Logic, Child>,
        snapshot: ProbeSnapshot,
    ) -> ProbeSnapshot {
        let page = ctx.runner.state();
        let canvas = self.shared.canvas.borrow();
        let choice = mere::canvas::PhysicsChoice::live(&canvas);
        let checked = {
            let dom = ctx.runner.dom();
            let dom = dom.borrow();
            mere::canvas::CANVAS_PHYSICS_OVERLAYS
                .iter()
                .filter(|(_, label)| {
                    !taproot::matching(
                        &dom,
                        &Selector::role("checkbox")
                            .containing(*label)
                            .with_attr("aria-checked", "true"),
                    )
                    .is_empty()
                })
                .map(|(id, _)| *id)
                .collect::<Vec<_>>()
                .join(",")
        };
        // The overlay boxes greyed: those the picked law refuses.
        let disabled = {
            let dom = ctx.runner.dom();
            let dom = dom.borrow();
            mere::canvas::CANVAS_PHYSICS_OVERLAYS
                .iter()
                .filter(|(_, label)| {
                    !taproot::matching(
                        &dom,
                        &Selector::role("checkbox")
                            .containing(*label)
                            .with_attr("aria-disabled", "true"),
                    )
                    .is_empty()
                })
                .count()
                .to_string()
        };
        let drag_return = self.drop.and_then(|(dx, dy)| {
            let (x, y) = canvas.focused_screen_position()?;
            let (left, top, _, _) = leaf_rect(ctx)?;
            Some((left + x - dx).hypot(top + y - dy))
        });
        let mut snapshot = snapshot
            .with_field("ready", "true")
            .with_field(
                "tools-mode",
                if tools_docked(page) {
                    "docked"
                } else if page.tools_open {
                    "open"
                } else {
                    "collapsed"
                },
            )
            .with_field("layout", page.physics.layout_id.clone())
            .with_field("physics-law", choice.law.id())
            .with_field(
                "physics-overlays",
                choice
                    .overlays
                    .iter()
                    .map(|overlay| overlay.id())
                    .collect::<Vec<_>>()
                    .join(","),
            )
            .with_field(
                "physics-profile",
                mere::canvas::physics_controls::profile_id(&canvas),
            )
            .with_field("physics-kind-source", choice.kind.id())
            .with_field("physics-group-source", choice.groups.id())
            // The Meaning channel (dynamics grammar plan, G2): what the next
            // run computes on, and how many runs the page has made.
            .with_field("meaning-backend", canvas.meaning_backend().id())
            .with_field("meaning-runs", canvas.meaning_runs().to_string())
            .with_field("meaning-pending", canvas.meaning_pending().to_string())
            .with_field(
                "meaning-slice",
                canvas
                    .meaning_slice()
                    .map(|scores| scores.to_string())
                    .unwrap_or_else(|| "whole".to_string()),
            )
            .with_field(
                "meaning-steps",
                canvas
                    .meaning()
                    .map(|snapshot| snapshot.steps.to_string())
                    .unwrap_or_default(),
            )
            .with_field(
                "meaning-snapshot-backend",
                canvas
                    .meaning()
                    .map(|snapshot| snapshot.backend.id())
                    .unwrap_or(""),
            )
            .with_field("community-runs", canvas.community_runs().to_string())
            .with_field("physics-mass-source", choice.mass.id())
            .with_field("physics-depth-source", choice.depth.id())
            .with_field("panel-law", page.physics.choice().law.id())
            .with_field("panel-group-source", page.physics.choice().groups.id())
            .with_field("panel-overlays", page.physics.ticked())
            .with_field("panel-profile", page.physics.profile_id())
            .with_field("panel-status", page.physics.status.clone())
            .with_field("checked-overlays", checked)
            .with_field("disabled-overlays", disabled)
            .with_field("canvas-nodes", canvas.graph().node_count().to_string())
            .with_field("physics-settling", canvas.is_settling().to_string())
            .with_field(
                "physics-continuous",
                canvas.physics_tick_demand().0.to_string(),
            )
            .with_field("physics-budget", canvas.physics_tick_demand().1.to_string())
            .with_field(
                "dragging-node",
                canvas
                    .dragging_node()
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            )
            .with_field(
                "drag-return",
                drag_return
                    .map(|distance| format!("{distance:.0}"))
                    .unwrap_or_default(),
            )
            // The drop distance on the first frame after a release that
            // executed physics steps, and how many it executed: keyed to
            // steps, not render frames.
            .with_field(
                "drag-return-step",
                self.shared
                    .release_step
                    .get()
                    .map(|(distance, _)| format!("{distance:.1}"))
                    .unwrap_or_default(),
            )
            .with_field(
                "drag-return-step-world",
                self.shared
                    .release_step_world
                    .get()
                    .map(|distance| format!("{distance:.1}"))
                    .unwrap_or_default(),
            )
            .with_field(
                "drag-return-world",
                self.drop_world
                    .zip(canvas.focused_world_position())
                    .map(|((dx, dy), (x, y))| format!("{:.0}", (x - dx).hypot(y - dy)))
                    .unwrap_or_default(),
            )
            .with_field(
                "drag-return-steps",
                self.shared
                    .release_step
                    .get()
                    .map(|(_, steps)| steps.to_string())
                    .unwrap_or_default(),
            );
        if canvas.graph().node_count() <= LAYOUT_STATS_LIMIT {
            let stats = canvas.layout_stats();
            snapshot = snapshot
                .with_field("layout-spread", format!("{:.0}", stats.spread))
                .with_field("layout-overlaps", stats.overlaps.to_string())
                .with_field("layout-stretch", format!("{:.2}", stats.stretch))
                .with_field(
                    "layout-mass-area-rank",
                    format!("{:.2}", stats.mass_area_rank),
                )
                .with_field("layout-density-cv", format!("{:.3}", stats.density_cv));
            if let Some(start) = page.physics.law_start {
                for (name, value) in start.fields(&stats) {
                    snapshot = snapshot.with_field(name, value);
                }
            }
        } else {
            // Past the all-pairs limit, spread and overlaps (same
            // definition, a grid) still report; stretch does not.
            let stats = canvas.layout_stats_without_stretch();
            snapshot = snapshot
                .with_field("layout-spread", format!("{:.0}", stats.spread))
                .with_field("layout-overlaps", stats.overlaps.to_string());
        }
        for (name, value) in mere::canvas::physics_controls::framing_fields(&canvas) {
            snapshot = snapshot.with_field(name, value);
        }
        // The GPU repulsion lane (P5c): whether the page has a device, and
        // the lane's counts, the receipt's proof the device ran.
        let device = self.shared.physics_device.borrow();
        let lane = canvas.repulsion_stats().unwrap_or_default();
        snapshot = snapshot
            .with_field(
                "physics-device",
                if device.is_some() { "on" } else { "off" },
            )
            .with_field(
                "gpu-threshold",
                device
                    .as_ref()
                    .map(|device| device.threshold().to_string())
                    .unwrap_or_default(),
            )
            .with_field("gpu-device-steps", lane.device_steps.to_string())
            .with_field("gpu-cpu-steps", lane.cpu_steps.to_string())
            .with_field("gpu-submissions", lane.submissions.to_string())
            .with_field("gpu-failures", lane.failures.to_string())
            .with_field(
                "gpu-answers",
                device
                    .as_ref()
                    .map(|device| device.answers().to_string())
                    .unwrap_or_default(),
            );
        snapshot
    }

    /// The remote session's observations, named as the old page names them.
    fn remote_fields(
        &self,
        ctx: &AppCtx<'_, TreePage, Logic, Child>,
        snapshot: ProbeSnapshot,
    ) -> ProbeSnapshot {
        let page = ctx.runner.state();
        let remote = self.shared.remote.borrow();
        let board = &remote.board;
        let live = remote.live.as_ref();
        #[cfg(feature = "remote")]
        let snapshot = reservoir::fields(page, snapshot);
        snapshot
            .with_field(
                "session",
                match page.session {
                    remote::Session::Local => "local",
                    remote::Session::Remote => "remote",
                },
            )
            .with_field("active-session", remote::active_line(page))
            .with_field(
                "tools-sections",
                format!(
                    "physics:{},remote:{}",
                    if page.sections.physics.expanded {
                        "open"
                    } else {
                        "closed"
                    },
                    if page.sections.remote.expanded {
                        "open"
                    } else {
                        "closed"
                    },
                ),
            )
            .with_field(
                "remote-link",
                if live.is_some() { "webrtc" } else { "none" },
            )
            .with_field("remote-state", remote.status())
            .with_field(
                "remote-generation",
                remote
                    .mounted()
                    .map(|mounted| mounted.scene.tables.generation.to_string())
                    .unwrap_or_default(),
            )
            .with_field("remote-card-labels", remote.card_labels().join("\n"))
            .with_field(
                "remote-revision",
                remote.revision().map(|r| r.to_string()).unwrap_or_default(),
            )
            .with_field(
                "remote-cards",
                remote
                    .mounted()
                    .map(|mounted| mounted.scene.tables.items.len().to_string())
                    .unwrap_or_default(),
            )
            .with_field(
                "remote-resume",
                live.map(|live| live.session.last_resume().to_string())
                    .unwrap_or_default(),
            )
            .with_field(
                "remote-rejoins",
                live.map(|live| live.session.rejoins().to_string())
                    .unwrap_or_default(),
            )
            .with_field(
                "remote-actions",
                remote
                    .actions()
                    .into_iter()
                    .map(|(label, _)| label)
                    .collect::<Vec<_>>()
                    .join("|"),
            )
            .with_field(
                "remote-physics-law",
                mere::canvas::PhysicsChoice::view(remote.board.board().stage())
                    .map(|view| view.law.id())
                    .unwrap_or_default(),
            )
            .with_field(
                "remote-physics-speed",
                crate::web_speed::field(remote.board.speed()),
            )
            .with_field("remote-energy", format!("{:.1}", board.energy()))
            .with_field(
                "remote-gap",
                board
                    .gap("0", "1")
                    .map(|gap| format!("{gap:.0}"))
                    .unwrap_or_default(),
            )
            .with_field("remote-overlaps", board.overlaps().to_string())
            .with_field(
                "remote-draft-open",
                remote.form().draft.is_some().to_string(),
            )
            .with_field("action-status", remote.form().status.clone())
            // What the board's slot tells a reader: each card's name and the
            // rectangle it is painted in, leaf-local px.
            .with_field(
                "board-nodes",
                if self.shared.remote_shown.get() {
                    let (width, height) = self.shared.size.get();
                    remote::board_semantics(&remote, width, height)
                        .children
                        .iter()
                        .map(|node| {
                            let [x, y, w, h] = node.rect;
                            format!("{}@{x:.0},{y:.0},{w:.0},{h:.0}", node.name)
                        })
                        .collect::<Vec<_>>()
                        .join("|")
                } else {
                    String::new()
                },
            )
    }

    /// `click-node <url>`: press and release over a node, through the host's
    /// pointer path, so the pick runs as a real click's would.
    fn node_point(
        &self,
        ctx: &mut AppCtx<'_, TreePage, Logic, Child>,
        url: &str,
    ) -> Result<(f32, f32), String> {
        let (x, y) = {
            let canvas = self.shared.canvas.borrow();
            let (key, _) = canvas
                .graph()
                .get_node_by_url(url)
                .ok_or_else(|| format!("click-node {url}: no such node"))?;
            canvas
                .screen_position_of(key)
                .ok_or_else(|| format!("click-node {url}: not on screen"))?
        };
        let leaf = {
            let dom = ctx.runner.dom();
            let dom = dom.borrow();
            taproot::matching(&dom, &Selector::class("tree-canvas"))
                .into_iter()
                .next()
        }
        .ok_or("the canvas leaf is not in the tree")?;
        let (left, top, width, height) = ctx
            .painted_rect(leaf)
            .ok_or("the canvas leaf is not painted")?;
        if x < 0.0 || y < 0.0 || x >= width || y >= height {
            return Err(format!(
                "click-node {url}: ({x}, {y}) is outside canvas ({left}, {top}, {width}, {height})"
            ));
        }
        Ok((left + x, top + y))
    }
}

impl Product for TreeLane {
    type State = TreePage;
    type Logic = Logic;
    type View = Child;
    const KIND: &'static str = "graphshell-tree";
    const SURFACE: &'static str = "tree";
    const LOG_PREFIX: &'static str = "graphshell-tree";

    fn close_on_completion(&self) -> bool {
        false
    }
    fn inspect(&mut self, name: &str, frame: &Frame) {
        if let Err(error) =
            web_scenario::publish_capture(name, frame.width, frame.height, &frame.rgba)
        {
            self.errors.push(error);
        }
    }
    fn complete(
        &mut self,
        ctx: &mut mesquite::Ctx<'_, Self>,
        outcome: &taproot::Outcome,
    ) -> Result<(), String> {
        let mut lines = vec![format!("RESULT {}", if outcome.ok { "ok" } else { "fail" })];
        lines.extend(outcome.log.iter().cloned());
        lines.extend(self.receipt_lines());
        let saved = ctx.runner.state().product.as_ref().map(|product| {
            json!({
                "session": product.session, "selected": product.selected,
                "address": product.address, "file": product.saved_file(),
                "title": product.title.text(), "tags": product.tags.text(),
                "save_state": product.save_state, "reopened": product.reopened,
                "owner_observation": owner_receipt(ctx.runner.state()),
            })
        });
        publish(outcome.ok, &lines.join("\n"), &self.shared, saved);
        Ok(())
    }

    fn sheet(&self) -> &str {
        SHEET
    }

    fn snapshot(
        &self,
        ctx: &AppCtx<'_, TreePage, Logic, Child>,
        _: usize,
        _: f32,
    ) -> ProbeSnapshot {
        let page = ctx.runner.state();
        let focus = ctx.runner.focus();
        let focus_kind = focus.map_or("none", |node| {
            let dom = ctx.runner.dom();
            let dom = dom.borrow();
            if taproot::matching(&dom, &Selector::class("tree-canvas")).contains(&node) {
                "graph"
            } else if taproot::matching(&dom, &Selector::role("button")).contains(&node) {
                "button"
            } else {
                "other"
            }
        });
        let canvas = self.shared.canvas.borrow();
        let geometry = canvas.cartography_geometry();
        let mut positions: Vec<_> = geometry.iter().collect();
        positions.sort_by_key(|(id, _)| *id);
        let finite = positions
            .iter()
            .all(|(_, (x, y))| x.is_finite() && y.is_finite());
        let hash = positions
            .iter()
            .flat_map(|(_, (x, y))| {
                x.to_bits()
                    .to_le_bytes()
                    .into_iter()
                    .chain(y.to_bits().to_le_bytes())
            })
            .fold(0xcbf29ce484222325_u64, |hash, b| {
                (hash ^ u64::from(b)).wrapping_mul(0x100000001b3)
            });
        let drag_distance = canvas
            .dragging_node()
            .and_then(|id| canvas.graph().get_node_by_id(id))
            .and_then(|(key, _)| canvas.screen_position_of(key))
            .and_then(|(x, y)| {
                let (px, py) = self.pointer?;
                let dom = ctx.runner.dom();
                let leaf = taproot::matching(&dom.borrow(), &Selector::class("tree-canvas"))
                    .into_iter()
                    .next()?;
                let (left, top, _, _) = ctx.painted_rect(leaf)?;
                Some((left + x - px).hypot(top + y - py))
            });
        let step = canvas.elapsed_step_report().unwrap_or_default();
        let snapshot = self.remote_fields(ctx, self.physics_fields(ctx, ProbeSnapshot::default()));
        let snapshot = super::speed::fields(snapshot, &canvas, &self.shared);
        let snapshot = snapshot.with_field(
            "painted-overlay-geometry",
            overlay_observation::observe(ctx, &self.shared).to_string(),
        );
        #[cfg(feature = "product")]
        let snapshot = forme_fields(page, &canvas, snapshot);
        #[cfg(feature = "product")]
        let snapshot = inspection::fields(page, snapshot);
        #[cfg(feature = "product")]
        let snapshot = link_inspection::fields(page, snapshot);
        #[cfg(feature = "product")]
        let snapshot = super::promoted_links::fields(page, snapshot);
        #[cfg(feature = "product")]
        let snapshot = field_inspection::fields(page, &canvas, snapshot);
        #[cfg(feature = "product")]
        let snapshot = coupling::fields(page, snapshot);
        #[cfg(feature = "product")]
        let snapshot = snapshot.with_field(
            "field-inspector-focus",
            field_inspection::focused_detail(ctx),
        );
        #[cfg(feature = "product")]
        let snapshot = presentation::fields(page, snapshot);
        #[cfg(feature = "product")]
        let snapshot = super::query_view::fields(page, snapshot);
        #[cfg(feature = "product")]
        let snapshot = dynamics_inspection::fields(page, snapshot);
        #[cfg(feature = "product")]
        let snapshot = node_presentation::fields(page, snapshot);
        #[cfg(feature = "moot-resources")]
        let snapshot = moot_resources::fields(page, snapshot);
        let snapshot = snapshot
            .with_field("physics-steps", step.steps.to_string())
            .with_field(
                "physics-dropped-us",
                step.discarded_elapsed.as_micros().to_string(),
            )
            .with_field(
                "physics-step-cap",
                self.shared.physics_config.max_steps.to_string(),
            )
            .with_field("focus", format!("{focus:?}"))
            .with_field("focus-kind", focus_kind)
            .with_field("physics-paused", canvas.physics_paused().to_string())
            .with_field(
                "physics-composition",
                match canvas.physics_composition() {
                    None => "none",
                    Some(mere::canvas::PhysicsComposition::Mix(_)) => "mix",
                    Some(mere::canvas::PhysicsComposition::Grouped(_)) => "grouped",
                },
            )
            .with_field("camera-centered", {
                let size = self.shared.size.get();
                let offset = canvas.camera().offset;
                ((offset.0 - size.0 as f32 / 2.0).abs() < 0.005
                    && (offset.1 - size.1 as f32 / 2.0).abs() < 0.005)
                    .to_string()
            })
            // Arrangement roles (dynamics grammar plan, G7): the recipe's role
            // and how many settles Settled has recorded.
            .with_field("arrangement-role", canvas.arrangement_roles().default.id())
            .with_field("panel-role", page.physics.role().id())
            .with_field("settles", canvas.settle_count().to_string())
            .with_field("anchored-home", canvas.anchored_home_count().to_string())
            .with_field("physics-energy", canvas.physics_energy().to_string())
            .with_field(
                "finite",
                (finite && canvas.physics_energy().is_finite()).to_string(),
            )
            .with_field("geometry", format!("{hash:016x}"))
            .with_field("dragging", canvas.dragging_node().is_some().to_string())
            .with_field(
                "drag-distance",
                drag_distance
                    .map(|distance| format!("{distance:.3}"))
                    .unwrap_or_else(|| "unavailable".into()),
            )
            .with_field("zoom", canvas.camera().zoom.to_string())
            .with_field(
                "camera",
                format!(
                    "{:.2},{:.2},{:.3}",
                    canvas.camera().offset.0,
                    canvas.camera().offset.1,
                    canvas.camera().zoom
                ),
            )
            .with_field("camera-x", format!("{:.2}", canvas.camera().offset.0))
            .with_field("camera-y", format!("{:.2}", canvas.camera().offset.1))
            .with_field(
                "camera-dx",
                self.camera_mark.map_or_else(String::new, |(x, _)| {
                    format!("{:.2}", canvas.camera().offset.0 - x)
                }),
            )
            .with_field(
                "camera-dy",
                self.camera_mark.map_or_else(String::new, |(_, y)| {
                    format!("{:.2}", canvas.camera().offset.1 - y)
                }),
            )
            .with_field(
                "selection-fit-available",
                canvas.can_fit_selection().to_string(),
            )
            .with_field(
                "selection-center-distance",
                canvas
                    .focused_screen_position()
                    .map(|(x, y)| {
                        let (w, h) = self.shared.size.get();
                        format!("{:.3}", (x - w as f32 * 0.5).hypot(y - h as f32 * 0.5))
                    })
                    .unwrap_or_else(|| "unavailable".into()),
            )
            .with_field("picked", page.picked.clone().unwrap_or_default())
            .with_field("presenter", page.presenter.is_some().to_string())
            .with_field(
                "canvas-selection-count",
                canvas.selected_members().len().to_string(),
            )
            .with_field(
                "presenter-selection-count",
                page.presenter
                    .as_ref()
                    .map_or(0, |p| p.model.selected.len())
                    .to_string(),
            )
            .with_field(
                "presenter-inspector-open",
                page.presenter
                    .as_ref()
                    .is_some_and(|p| p.model.inspected.is_some())
                    .to_string(),
            )
            .with_field(
                "presenter-hidden-count",
                page.presenter
                    .as_ref()
                    .map_or(0, |p| {
                        p.model
                            .selected
                            .iter()
                            .filter(|id| !p.model.visible.contains_key(*id))
                            .count()
                    })
                    .to_string(),
            )
            .with_field(
                "presenter-selected",
                page.presenter
                    .as_ref()
                    .map(|p| {
                        p.model
                            .selected
                            .iter()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .unwrap_or_default(),
            )
            .with_field(
                "presenter-inspected",
                page.presenter
                    .as_ref()
                    .and_then(|p| p.model.inspected.clone())
                    .unwrap_or_default(),
            )
            .with_field(
                "presenter-hidden",
                page.presenter
                    .as_ref()
                    .map(|p| {
                        p.model
                            .selected
                            .iter()
                            .filter(|id| !p.model.visible.contains_key(*id))
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .unwrap_or_default(),
            )
            .with_field(
                "presenter-notice",
                page.presenter
                    .as_ref()
                    .map(|p| p.model.notice.clone())
                    .unwrap_or_default(),
            )
            .with_field("nodes", page.nodes.to_string())
            .with_field("source", page.source.clone())
            // The host dataset (S1): its state, the edges the canvas draws,
            // and what a reader reaches of its relations or its refusal.
            .with_field("dataset", page.dataset.state())
            .with_field(
                "dataset-error",
                match &page.dataset {
                    super::HostedDataset::Refused(error) => error.clone(),
                    _ => String::new(),
                },
            )
            .with_field(
                "edges",
                canvas.graph().projected_relations().count().to_string(),
            )
            // The host history (S3): which checkpoint shows, and its changes.
            .with_field(
                "history-checkpoints",
                page.history
                    .as_ref()
                    .map_or("0".into(), |hosted| hosted.len().to_string()),
            )
            .with_field(
                "history-index",
                page.history
                    .as_ref()
                    .map(|hosted| (hosted.index + 1).to_string())
                    .unwrap_or_default(),
            )
            .with_field(
                "history-summary",
                page.history
                    .as_ref()
                    .and_then(|hosted| hosted.summary())
                    .unwrap_or_default(),
            )
            .with_field(
                "history-error",
                page.history
                    .as_ref()
                    .and_then(|hosted| hosted.error.clone())
                    .unwrap_or_default(),
            )
            // The page's gestures and motion preference.
            .with_field("reduced-motion", self.shared.reduced_motion().to_string())
            .with_field("gesture", self.shared.gestures.borrow().probe.clone())
            .with_field("wheel-mode", self.shared.wheel_mode.get().name())
            .with_field("commands-open", page.commands.open.is_some().to_string())
            .with_field("command-last", page.commands.last.clone())
            .with_field(
                "wheel-use",
                format!("{:?}", self.shared.gestures.borrow().last_wheel),
            )
            .with_field(
                "touch-use",
                format!("{:?}", self.shared.gestures.borrow().last_touch),
            )
            .with_field("moving", self.shared.moving.get().to_string())
            .with_field(
                "gpu-timed",
                self.shared.timing.borrow().gpu_timed().to_string(),
            );
        // What a screen reader reaches of the canvas, read from the page's
        // own semantic tree, and the keyboard move (dynamics grammar plan,
        // F65 to F68).
        let tree = semantic_tree();
        let (heard_relations, heard_alert) = tree.as_ref().map(heard_dataset).unwrap_or_default();
        let heard = tree.as_ref().and_then(heard_canvas);
        let (heard_changes, heard_checkpoint) = tree
            .as_ref()
            .map(|tree| heard_region(tree, "History"))
            .unwrap_or_default();
        let snapshot = snapshot
            .with_field(
                "reader-canvas",
                heard
                    .as_ref()
                    .map_or("none".to_string(), |(name, _)| name.clone()),
            )
            .with_field(
                "reader-items",
                heard
                    .as_ref()
                    .map_or(0, |(_, items)| items.len())
                    .to_string(),
            )
            .with_field(
                "reader-buttons",
                heard
                    .as_ref()
                    .map(|(_, items)| {
                        items
                            .iter()
                            .map(|item| format!("{}:{}", item.name, item.buttons.join("+")))
                            .collect::<Vec<_>>()
                            .join("|")
                    })
                    .unwrap_or_default(),
            )
            .with_field(
                "reader-described",
                heard
                    .as_ref()
                    .map_or(0, |(_, items)| {
                        items.iter().map(|item| item.described).sum()
                    })
                    .to_string(),
            )
            .with_field(
                "reader-item-names",
                heard
                    .as_ref()
                    .map(|(_, items)| {
                        items
                            .iter()
                            .map(|item| item.name.clone())
                            .collect::<Vec<_>>()
                            .join("|")
                    })
                    .unwrap_or_default(),
            )
            .with_field("reader-changes", heard_changes.len().to_string())
            .with_field("reader-change-labels", heard_changes.join("|"))
            .with_field("reader-checkpoint", heard_checkpoint.unwrap_or_default())
            .with_field("reader-relations", heard_relations.len().to_string())
            .with_field("reader-relation-labels", heard_relations.join("|"))
            .with_field("reader-alert", heard_alert.unwrap_or_default())
            .with_field("key-moving", canvas.key_moving().is_some().to_string())
            .with_field(
                "key-move-offset",
                canvas
                    .key_move_offset()
                    .map_or("none".to_string(), |(dx, dy)| format!("{dx:.0},{dy:.0}")),
            )
            .with_field(
                "pinned",
                canvas
                    .graph()
                    .nodes()
                    .filter(|(key, _)| {
                        canvas.arrangement_role_of(*key) == mere::canvas::Role::Pinned
                    })
                    .map(|(key, _)| canvas.graph().node_display_label(key))
                    .collect::<Vec<_>>()
                    .join("|"),
            );
        let snapshot = link_rest_fields(&canvas, snapshot);
        let faces = self.shared.faces.get().unwrap_or_default().fields();
        let snapshot = practice::fields(page, snapshot);
        let snapshot = responsive_links::fields(page, snapshot);
        // Host-boundary receipts come from the existing validated loader and
        // Tabard adapter. Missing evidence stays absent, never a passing default.
        let snapshot = [
            "appearance-owner",
            "appearance-theme",
            "appearance-mode",
            "appearance-background",
            "appearance-source",
            "appearance-status",
            "chronicle-receipt",
            "chronicle-definition",
            "chronicle-datasets",
            "chronicle-shelfmark",
            "chronicle-variant",
            "chronicle-era-bands",
            "chronicle-generation",
        ]
        .into_iter()
        .fold(snapshot, |snapshot, name| {
            if let Some(value) = page.root.get_attribute(&format!("data-{name}")) {
                snapshot.with_field(name, value)
            } else {
                snapshot
            }
        });
        let snapshot = faces.into_iter().fold(snapshot, |snapshot, (name, value)| {
            snapshot.with_field(name, value)
        });
        if let Some(product) = &page.product {
            // Detail observations describe the selected node. Before selection,
            // observe the faces actually on the canvas rather than conflating
            // an absent detail selection with an absent representation.
            let face = if let Some(id) = product.selected {
                canvas
                    .graph()
                    .get_node_by_id(id)
                    .map(|(key, _)| canvas.node_face(key).as_code())
                    .unwrap_or("none")
            } else {
                let mut faces = canvas.graph().nodes().map(|(key, _)| canvas.node_face(key));
                match faces.next() {
                    Some(face) if faces.all(|other| other == face) => face.as_code(),
                    Some(_) => "mixed",
                    None => "none",
                }
            };
            let (link_visibility_state, link_visibility_status) = product.link_visibility_state();
            let (assertion_visibility_state, assertion_visibility_status) =
                product.assertion_visibility_state();
            product
                .history_fields(product.projection_fields(snapshot))
                .with_field("link-visibility-save-state", link_visibility_state)
                .with_field("link-visibility-status", link_visibility_status)
                .with_field(
                    "assertion-visibility-save-state",
                    assertion_visibility_state,
                )
                .with_field("assertion-visibility-status", assertion_visibility_status)
                .with_field(
                    "command-menu",
                    if product.menu_open() {
                        "open"
                    } else {
                        "closed"
                    },
                )
                .with_field("graph-session", product.session.clone())
                .with_field("storage", product.storage.clone())
                .with_field(
                    "node-count",
                    product
                        .authority_nodes()
                        .map(|count| count.to_string())
                        .unwrap_or_default(),
                )
                .with_field("storage-reopened", product.reopened.to_string())
                .with_field("save-state", product.save_state)
                .with_field("detail-open", product.detail_open.to_string())
                .with_field(
                    "detail-member",
                    product
                        .selected
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                )
                .with_field("detail-title", product.title.text())
                .with_field("detail-address", &product.address)
                .with_field("detail-tags", product.tags.text())
                .with_field("detail-facet", product.detail_facet())
                .with_field("action-count", product.action_count().to_string())
                .with_field("filter-count", product.filter_count().to_string())
                .with_field("saved-scene-facet", product.saved_scene_facet())
                .with_field("export-bytes", product.export_bytes().to_string())
                .with_field("imported-nodes", product.imported_nodes().to_string())
                .with_field(
                    "selected-count",
                    canvas.selected_members().len().to_string(),
                )
                .with_field("face", face)
                .with_field(
                    "item-role",
                    product
                        .selected
                        .and_then(|member| canvas.member_role(member))
                        .map_or("recipe", |role| role.id()),
                )
        } else {
            snapshot
        }
    }

    /// `timing start <label>` and `timing stop`, and `click-node <url>`.
    fn app_step(
        &mut self,
        ctx: &mut AppCtx<'_, TreePage, Logic, Child>,
        _: mesquite::Checkpoints<'_>,
        line: &str,
    ) -> Result<(), String> {
        if let Some(json) = line.strip_prefix("assert host ") {
            return host_boundary::observe(json);
        }
        if let Some(command) = line.strip_prefix("host-input ") {
            return host_boundary::request(&ctx.runner.state().root, command);
        }
        if let Some(json) = line.strip_prefix("assert retained ") {
            return observe::assertion(ctx, json);
        }
        if let Some(expected) = line.strip_prefix("assert title ") {
            let got = super::super::document()?.title();
            return if got.contains(expected) {
                Ok(())
            } else {
                Err(format!("title {expected:?}: got {got:?}"))
            };
        }
        let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
        if let Some(result) = super::gesture_steps::step(&self.shared, verb, rest) {
            return result;
        }
        match verb {
            // Translate the original explicit focus fixture without activating
            // the target. Subsequent keys still traverse shared host input.
            "focus-retained" => {
                let (head, label) = rest.split_once(' ').unwrap_or((rest, ""));
                let mut selector = if let Some(role) = head.strip_prefix("role:") {
                    Selector::role(role)
                } else if let Some(class) = head.strip_prefix('.') {
                    Selector::class(class)
                } else {
                    return Err("focus-retained wants role:name or .class".into());
                };
                if !label.is_empty() {
                    selector = selector.containing(label);
                }
                let projection = ctx
                    .a11y_projection()
                    .ok_or("retained focus projection is unavailable")?;
                let nodes = {
                    let dom = ctx.runner.dom();
                    let dom = dom.borrow();
                    taproot::matching_with_projection(&dom, &selector, &projection)
                };
                let [node] = nodes.as_slice() else {
                    return Err(format!(
                        "focus-retained requires one target, got {}",
                        nodes.len()
                    ));
                };
                ctx.scroll_into_view(*node, cambium_rootstock::ScrollAlign::Nearest);
                ctx.runner.set_focus(Some(*node));
                Ok(())
            },
            "mark-camera" => {
                self.camera_mark = Some(self.shared.canvas.borrow().camera().offset);
                Ok(())
            },
            "pointer" => {
                let words: Vec<_> = rest.split_whitespace().collect();
                if words.as_slice() == ["cancel"] {
                    ctx.pointer.push(HostPointer::Cancel);
                    return Ok(());
                }
                let [phase, button, x, y] = words.as_slice() else {
                    return Err("pointer wants <down|move|up> <left|right> <x> <y>".into());
                };
                let (left, top, _, _) =
                    leaf_rect(ctx).ok_or("graph leaf has no current rectangle")?;
                let x = x.parse::<f32>().map_err(|_| "pointer x is not numeric")? + left;
                let y = y.parse::<f32>().map_err(|_| "pointer y is not numeric")? + top;
                let pointer = match (*phase, *button) {
                    ("down", "left") => HostPointer::Press(x, y),
                    ("up", "left") => HostPointer::Release(x, y),
                    ("down", "right") => HostPointer::SecondaryPress(x, y),
                    ("up", "right") => HostPointer::SecondaryRelease(x, y),
                    ("move", "left" | "right") => HostPointer::Moved(x, y),
                    _ => return Err("unsupported pointer phase or button".into()),
                };
                ctx.pointer.push(pointer);
                Ok(())
            },
            // The browser instrument cannot operate an empty chooser. Exercise
            // the platform cancel event through the real chooser listener,
            // including its pending FileAnswer and the retained callback target.
            // This is a DOM-event fixture, not proof of the OS dialog's Cancel.
            "cancel-file-choice" => {
                let input = super::super::document()?
                    .get_element_by_id("cambium-file-input")
                    .ok_or("the browser file chooser is not mounted")?;
                let event = Event::new("cancel")
                    .map_err(|_| "could not create the file cancellation event")?;
                input
                    .dispatch_event(&event)
                    .map(|_| ())
                    .map_err(|_| "could not dispatch file cancellation".into())
            },
            "timing" => {
                let (what, label) = rest.split_once(' ').unwrap_or((rest, ""));
                self.shared.with_gpu(|gpu| {
                    let mut timing = self.shared.timing.borrow_mut();
                    match what {
                        "start" => timing.start(label.trim(), gpu),
                        "stop" => timing.stop(gpu),
                        _ => Err(format!("timing wants start <label> or stop, got '{rest}'")),
                    }
                })
            },
            // `log-layout <label>`: room by mass now and where the law
            // started, into the receipt.
            "log-layout" => {
                let canvas = self.shared.canvas.borrow();
                let start = ctx.runner.state().physics.law_start;
                self.shared.physics_log.borrow_mut().push(
                    mere::canvas::physics_controls::layout_line(
                        rest.trim(),
                        &canvas,
                        start.as_ref(),
                    ),
                );
                Ok(())
            },
            // `log-physics <label>`: the layout's signature and the GPU
            // lane's counts into the receipt, so a run can be read without
            // a failing assert to show the values.
            "log-physics" => {
                let canvas = self.shared.canvas.borrow();
                let stats = canvas.layout_stats_without_stretch();
                let lane = canvas.repulsion_stats().unwrap_or_default();
                let framing = mere::canvas::physics_controls::framing_fields(&canvas)
                    .map(|(name, value)| format!("{name} {value}"))
                    .join(" ");
                let answers = self
                    .shared
                    .physics_device
                    .borrow()
                    .as_ref()
                    .map_or(0, |device| device.answers());
                self.shared.physics_log.borrow_mut().push(format!(
                    "physics {}: law {} nodes {} energy {:.1} spread {:.0} overlaps {} \
                     device {} device-steps {} cpu-steps {} submissions {} answers {} failures {} \
                     waiting {} stale {} mismatched {} last-age {} {framing}",
                    rest.trim(),
                    mere::canvas::PhysicsChoice::live(&canvas).law.id(),
                    canvas.graph().node_count(),
                    stats.energy,
                    stats.spread,
                    stats.overlaps,
                    if self.shared.physics_device.borrow().is_some() {
                        "on"
                    } else {
                        "off"
                    },
                    lane.device_steps,
                    lane.cpu_steps,
                    lane.submissions,
                    answers,
                    lane.failures,
                    lane.waiting,
                    lane.stale,
                    lane.mismatched,
                    lane.last_age,
                ));
                Ok(())
            },
            // `log-pace <label>`: the speed dial's budget and every
            // window's worst frame into the receipt.
            "log-pace" => {
                let line = super::speed::pace_line(
                    rest.trim(),
                    &self.shared.canvas.borrow(),
                    &self.shared,
                );
                self.shared.physics_log.borrow_mut().push(line);
                Ok(())
            },
            // `mark-pace <label>`: the effective speed now, for a later
            // comparison on the same page.
            "mark-pace" => {
                let line = super::speed::mark_line(
                    rest.trim(),
                    &self.shared.canvas.borrow(),
                    &self.shared,
                );
                self.shared.physics_log.borrow_mut().push(line);
                Ok(())
            },
            // `log-intervals <label>`: the frame intervals the display
            // period is read from, into the receipt.
            "log-intervals" => {
                let line = super::speed::intervals_line(rest.trim(), &self.shared);
                self.shared.physics_log.borrow_mut().push(line);
                Ok(())
            },
            // `set-zoom <z>`: the zoom exactly, about the canvas centre.
            "set-zoom" => {
                let zoom = rest.trim().parse().map_err(|_| "set-zoom wants a number")?;
                let size = self.shared.size.get();
                CanvasCommand::SetZoom { zoom }.apply(&mut *self.shared.canvas.borrow_mut(), size);
                self.shared.dirty.set(true);
                Ok(())
            },
            // `measure-faces <label>`: each face in view against its drawn
            // body, into the snapshot and the receipt.
            "measure-faces" => {
                let faces = mere::canvas::face_alignment::FaceAlignment::measure(
                    &*self.shared.canvas.borrow(),
                    self.shared.size.get(),
                );
                self.shared.faces.set(Some(faces));
                self.shared
                    .physics_log
                    .borrow_mut()
                    .push(faces.line(rest.trim()));
                Ok(())
            },
            // `reader-click <item> | <button>`: press a canvas item's button
            // in the accessibility mirror, as a screen reader's activation
            // does: a click on the mirror element (F65, F66).
            "reader-click" => {
                use wasm_bindgen::JsCast;
                let (item, button) = rest
                    .split_once('|')
                    .map(|(item, button)| (item.trim(), button.trim()))
                    .ok_or("reader-click wants '<item> | <button>'")?;
                let document = web_sys::window()
                    .and_then(|window| window.document())
                    .ok_or("no document")?;
                let selector = format!(
                    "[data-cambium-mirror] [role=\"group\"][aria-label$=\" shown\"] > [role=\"group\"][aria-label={}]",
                    css_string(item)
                );
                let group = document
                    .query_selector(&selector)
                    .map_err(|_| format!("reader-click: bad selector for {item:?}"))?
                    .or(document
                        .query_selector(&format!(
                            "[data-cambium-mirror] [role=\"group\"][aria-label={}]",
                            css_string(item)
                        ))
                        .map_err(|_| format!("reader-click: bad selector for {item:?}"))?)
                    .ok_or_else(|| format!("reader-click: no item {item:?} in the mirror"))?;
                let mut child = group.first_element_child();
                while let Some(element) = child {
                    if element.get_attribute("role").as_deref() == Some("button")
                        && element.text_content().as_deref().map(str::trim) == Some(button)
                    {
                        let element = element
                            .dyn_into::<web_sys::HtmlElement>()
                            .map_err(|_| "reader-click: not an element")?;
                        // From the event loop, as a reader's click arrives: the
                        // mirror's listener takes the host, which this step holds.
                        let click = wasm_bindgen::closure::Closure::once_into_js(move || {
                            element.click();
                        });
                        web_sys::window()
                            .ok_or("no window")?
                            .set_timeout_with_callback_and_timeout_and_arguments_0(
                                click.unchecked_ref(),
                                0,
                            )
                            .map_err(|_| "reader-click: could not queue the click")?;
                        return Ok(());
                    }
                    child = element.next_element_sibling();
                }
                Err(format!("reader-click: {item:?} has no {button:?} button"))
            },
            "hover-forme" => {
                let (left, top, _, _) = leaf_rect(ctx).ok_or("the canvas leaf is not painted")?;
                let canvas = self.shared.canvas.borrow();
                let region = canvas.forme_region().ok_or("no forme in this view")?;
                let [x, y, r, b] = region.bounds;
                let world = match rest.trim() {
                    "inside" => (x + (r - x) * 0.1, y + (b - y) * 0.1),
                    "outside" => (x - 50., y - 50.),
                    _ => return Err("hover-forme wants inside or outside".into()),
                };
                let (x, y) = canvas.screen_point_of(world);
                ctx.pointer.push(HostPointer::Moved(left + x, top + y));
                Ok(())
            },
            #[cfg(feature = "product")]
            "pick-field-regions" => {
                if !ctx.runner.state().field_inspector.picking {
                    return Err("Arm Pick field regions before the pointer step".into());
                }
                let args: Vec<f32> = rest
                    .split_whitespace()
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| "pick-field-regions wants world x y")?;
                if args.len() != 2 || !args.iter().all(|n| n.is_finite()) {
                    return Err("pick-field-regions wants finite world x y".into());
                }
                let (left, top, width, height) =
                    leaf_rect(ctx).ok_or("the canvas leaf is not painted")?;
                let (x, y) = self
                    .shared
                    .canvas
                    .borrow()
                    .screen_point_of((args[0], args[1]));
                if x < 0. || y < 0. || x >= width || y >= height {
                    return Err(
                        "region point is outside the painted canvas; fit the view first".into(),
                    );
                }
                ctx.pointer.push(HostPointer::Press(left + x, top + y));
                ctx.pointer.push(HostPointer::Release(left + x, top + y));
                Ok(())
            },
            #[cfg(feature = "product")]
            "move-to-forme" => {
                let (address, side) = rest.trim().rsplit_once(' ').ok_or(
                    "move-to-forme wants target address and left/right/top/bottom/center/outside",
                )?;
                let (left, top, _, _) = leaf_rect(ctx).ok_or("the canvas leaf is not painted")?;
                let pane = ctx
                    .runner
                    .state()
                    .forme
                    .as_ref()
                    .ok_or("no Forme workspace")?;
                let region = pane.session.view().region();
                let canvas = self.shared.canvas.borrow();
                let member = canvas
                    .graph()
                    .nodes()
                    .find(|(_, node)| node.url() == address)
                    .map(|(_, node)| node.id)
                    .ok_or("unknown drop target")?;
                let cell = region
                    .cells
                    .iter()
                    .find(|cell| cell.member == member)
                    .ok_or("target has no active tile region")?;
                let [x, y, r, b] = region.cell_world_bounds(cell);
                let world = match side {
                    "left" => (x + (r - x) * 0.1, (y + b) * 0.5),
                    "right" => (r - (r - x) * 0.1, (y + b) * 0.5),
                    "top" => ((x + r) * 0.5, y + (b - y) * 0.1),
                    "bottom" => ((x + r) * 0.5, b - (b - y) * 0.1),
                    "center" => ((x + r) * 0.5, (y + b) * 0.5),
                    "outside" => (region.bounds[2] + 50., region.bounds[3] + 50.),
                    _ => return Err("unknown Forme drop region".into()),
                };
                let (x, y) = canvas.screen_point_of(world);
                let point = self
                    .pointer
                    .as_mut()
                    .ok_or("move-to-forme needs press-node first")?;
                *point = (left + x, top + y);
                ctx.pointer.push(HostPointer::Moved(point.0, point.1));
                Ok(())
            },
            "click-node" => {
                let (x, y) = self.node_point(ctx, rest.trim())?;
                ctx.pointer.push(HostPointer::Press(x, y));
                ctx.pointer.push(HostPointer::Release(x, y));
                Ok(())
            },
            // Pick a source-owned P4 appearance through the ordinary canvas
            // pointer route. The stratum offset selects one of the two
            // parallel strokes when both are present on the same surfaces.
            "click-link-rest" => {
                use graphshell::mere_host::{
                    LINK_REST_A1_ADDRESS, LINK_REST_A2_ADDRESS, LINK_REST_B1_ADDRESS,
                    LINK_REST_B2_ADDRESS,
                };
                let words = rest.split_whitespace().collect::<Vec<_>>();
                let [pair, stratum] = words.as_slice() else {
                    return Err(
                        "click-link-rest wants '<a1-b1|a1-b2|a2-b1|a2-b2> <surface|resource>'"
                            .into(),
                    );
                };
                let (from_address, to_address) = match *pair {
                    "a1-b1" => (LINK_REST_A1_ADDRESS, LINK_REST_B1_ADDRESS),
                    "a1-b2" => (LINK_REST_A1_ADDRESS, LINK_REST_B2_ADDRESS),
                    "a2-b1" => (LINK_REST_A2_ADDRESS, LINK_REST_B1_ADDRESS),
                    "a2-b2" => (LINK_REST_A2_ADDRESS, LINK_REST_B2_ADDRESS),
                    _ => return Err("click-link-rest has an unknown fixture pair".into()),
                };
                let stratum = match *stratum {
                    "surface" => scenograph::link::LinkStratum::Surface,
                    "resource" => scenograph::link::LinkStratum::Resource,
                    _ => return Err("click-link-rest stratum must be surface or resource".into()),
                };
                let point = {
                    let canvas = self.shared.canvas.borrow();
                    let graph = canvas.graph();
                    let (from, _) = graph
                        .get_node_by_url(from_address)
                        .ok_or("click-link-rest source fixture endpoint is missing")?;
                    let (to, _) = graph
                        .get_node_by_url(to_address)
                        .ok_or("click-link-rest target fixture endpoint is missing")?;
                    let (first, second) = if from <= to { (from, to) } else { (to, from) };
                    let (x1, y1) = canvas
                        .screen_position_of(first)
                        .ok_or("click-link-rest first endpoint is not visible")?;
                    let (x2, y2) = canvas
                        .screen_position_of(second)
                        .ok_or("click-link-rest second endpoint is not visible")?;
                    let dx = x2 - x1;
                    let dy = y2 - y1;
                    let length = dx.hypot(dy).max(f32::EPSILON);
                    let direction = if stratum == scenograph::link::LinkStratum::Surface {
                        -1.0
                    } else {
                        1.0
                    };
                    // The renderer offsets strata by 3 world units. Two more
                    // screen units put this pointer closer to the chosen line
                    // so nearest-segment picking selects this stratum.
                    let offset = 3.0 * canvas.camera().zoom + 2.0;
                    let point = (
                        (x1 + x2) * 0.5 - dy / length * direction * offset,
                        (y1 + y2) * 0.5 + dx / length * direction * offset,
                    );
                    point
                };
                let (left, top, width, height) =
                    leaf_rect(ctx).ok_or("the canvas leaf is not painted")?;
                if point.0 < 0.0 || point.1 < 0.0 || point.0 >= width || point.1 >= height {
                    return Err("click-link-rest target is outside the canvas".into());
                }
                ctx.pointer
                    .push(HostPointer::Press(left + point.0, top + point.1));
                ctx.pointer
                    .push(HostPointer::Release(left + point.0, top + point.1));
                Ok(())
            },
            "press-node" => {
                let point = self.node_point(ctx, rest.trim())?;
                self.pointer = Some(point);
                ctx.pointer.push(HostPointer::Press(point.0, point.1));
                Ok(())
            },
            "move-by" => {
                let args: Vec<f32> = rest
                    .split_whitespace()
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| "move-by wants dx dy")?;
                if args.len() != 2 {
                    return Err("move-by wants dx dy".into());
                }
                let point = self.pointer.as_mut().ok_or("move-by without a press")?;
                point.0 += args[0];
                point.1 += args[1];
                ctx.pointer.push(HostPointer::Moved(point.0, point.1));
                Ok(())
            },
            // The same move in world units, so a receipt's gesture means the
            // same distance to the law at any zoom; at zoom 1 it is `move-by`
            // (ruled 2026-10-04, "Measure in world units").
            "move-by-world" => {
                let args: Vec<f32> = rest
                    .split_whitespace()
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| "move-by-world wants dx dy")?;
                if args.len() != 2 {
                    return Err("move-by-world wants dx dy".into());
                }
                let (left, top, _, _) = leaf_rect(ctx).ok_or("the canvas leaf is not painted")?;
                let point = self
                    .pointer
                    .as_mut()
                    .ok_or("move-by-world without a press")?;
                let canvas = self.shared.canvas.borrow();
                let (wx, wy) = canvas.world_point_at((point.0 - left, point.1 - top));
                let (sx, sy) = canvas.screen_point_of((wx + args[0], wy + args[1]));
                *point = (sx + left, sy + top);
                ctx.pointer.push(HostPointer::Moved(point.0, point.1));
                Ok(())
            },
            "release-at" => {
                let point = self.pointer.take().ok_or("release-at without a press")?;
                self.drop = Some(point);
                // Watch for the first stepped frame after the release.
                let (left, top, _, _) = leaf_rect(ctx).ok_or("the canvas leaf is not painted")?;
                self.shared
                    .release_watch
                    .set(Some((point.0 - left, point.1 - top)));
                self.shared.release_step.set(None);
                let world = self
                    .shared
                    .canvas
                    .borrow()
                    .world_point_at((point.0 - left, point.1 - top));
                self.drop_world = Some(world);
                self.shared.release_watch_world.set(Some(world));
                self.shared.release_step_world.set(None);
                ctx.pointer.push(HostPointer::Release(point.0, point.1));
                Ok(())
            },
            // `reveal <role:name|.class> [text]`: scroll the first match into
            // view in its scrolling ancestors, as a click would, without
            // pressing it, so a capture can show a scrolled state.
            "reveal" => {
                let (head, text) = rest.trim().split_once(' ').unwrap_or((rest.trim(), ""));
                let mut selector = if let Some(role) = head.strip_prefix("role:") {
                    Selector::role(role)
                } else if let Some(class) = head.strip_prefix('.') {
                    Selector::class(class)
                } else {
                    return Err(format!("reveal wants role:name or .class, got '{head}'"));
                };
                if !text.trim().is_empty() {
                    selector = selector.containing(text.trim());
                }
                let node = {
                    let dom = ctx.runner.dom();
                    let dom = dom.borrow();
                    taproot::matching(&dom, &selector).into_iter().next()
                }
                .ok_or_else(|| format!("reveal {rest}: nothing matches"))?;
                ctx.scroll_into_view(node, cambium_rootstock::ScrollAlign::Nearest);
                Ok(())
            },
            // `center-node <url>`: bring a node into view by panning the camera
            // so it sits at the canvas centre. Zoom is kept, so screen-px
            // distances keep their meaning; the layout is untouched.
            "center-node" => {
                let url = rest.trim();
                let mut canvas = self.shared.canvas.borrow_mut();
                let (key, _) = canvas
                    .graph()
                    .get_node_by_url(url)
                    .ok_or_else(|| format!("center-node {url}: no such node"))?;
                let (x, y) = canvas
                    .screen_position_of(key)
                    .ok_or_else(|| format!("center-node {url}: no position"))?;
                let (width, height) = self.shared.size.get();
                let mut camera = canvas.camera();
                camera.offset.0 += width as f32 / 2.0 - x;
                camera.offset.1 += height as f32 / 2.0 - y;
                canvas.set_camera(camera);
                self.shared.dirty.set(true);
                Ok(())
            },
            // The drag gesture from the selected node: press where it is drawn.
            "press-focused" => {
                let point = self.focused_point(ctx)?;
                self.pointer = Some(point);
                if let Some((left, top, _, _)) = leaf_rect(ctx) {
                    self.shared
                        .press_point
                        .set(Some((point.0 - left, point.1 - top)));
                }
                ctx.pointer.push(HostPointer::Press(point.0, point.1));
                Ok(())
            },
            // `add-node <x> <y> <url>`: the empty-space add gesture at a
            // canvas-local point the receipt picks.
            "add-node" => {
                let mut words = rest.split_whitespace();
                let (Some(x), Some(y), Some(url), None) =
                    (words.next(), words.next(), words.next(), words.next())
                else {
                    return Err("add-node wants '<x> <y> <url>'".into());
                };
                let point = (
                    x.parse().map_err(|_| "add-node wants numeric x")?,
                    y.parse().map_err(|_| "add-node wants numeric y")?,
                );
                self.shared.canvas.borrow_mut().add_node_at(point, url);
                self.shared.dirty.set(true);
                Ok(())
            },
            _ => Err(format!("unknown verb: {line}")),
        }
    }

    fn app_step_with_clicks(
        &mut self,
        ctx: &mut mesquite::Ctx<'_, Self>,
        checkpoints: mesquite::Checkpoints<'_>,
        clicks: &mut mesquite::Clicks,
        line: &str,
    ) -> Result<(), String> {
        if let Some(action_spec) = line.strip_prefix("click-link-rest-assertion ") {
            let words = action_spec.split_whitespace().collect::<Vec<_>>();
            let (assertion_id, action) = match words.as_slice() {
                [id] => (*id, "inspect"),
                [id, action @ ("inspect" | "hide" | "withdraw")] => (*id, *action),
                _ => {
                    return Err(
                        "click-link-rest-assertion wants '<id> [inspect|hide|withdraw]'".into(),
                    );
                },
            };
            // Selector holds one attribute predicate. Use the independent
            // accessible-name filter for the action so the exact ID survives.
            let action_label = match action {
                "inspect" => "Inspect assertion",
                "hide" => "Hide this assertion from the view",
                "withdraw" => "Withdraw this personal assertion",
                _ => unreachable!("action was checked above"),
            };
            let selector = Selector::role("button")
                .containing(action_label)
                .with_attr("data-assertion-id", assertion_id);
            let projection = ctx
                .a11y_projection()
                .ok_or("shared assertion action projection is unavailable")?;
            let dom = ctx.runner.dom();
            let dom = dom.borrow();
            let matches = taproot::matching_with_projection(&dom, &selector, &projection);
            if matches.len() != 1 {
                return Err(format!(
                    "click-link-rest-assertion requires one {action} action for {assertion_id:?}, got {}",
                    matches.len()
                ));
            }
            drop(dom);
            return if clicks.click(ctx, &selector, |_, _, rect| {
                (rect[0] + rect[2] * 0.5, rect[1] + rect[3] * 0.5)
            }) {
                Ok(())
            } else {
                Err(format!(
                    "source {action} action {assertion_id:?} could not be clicked"
                ))
            };
        }
        if let Some(rest) = line.strip_prefix("check-role ") {
            use layout_dom_api::{LayoutDom, LocalName, Namespace};
            let (label, wanted) = rest
                .rsplit_once(' ')
                .ok_or("check-role wants <label> <on|off>")?;
            let wanted = match wanted {
                "on" => "true",
                "off" => "false",
                _ => return Err("check-role wants on or off".into()),
            };
            let selector = Selector::role("checkbox").containing(label);
            let projection = ctx
                .a11y_projection()
                .ok_or("shared checkbox projection is unavailable")?;
            let dom = ctx.runner.dom();
            let dom = dom.borrow();
            let matches = taproot::matching_with_projection(&dom, &selector, &projection);
            let [node] = matches.as_slice() else {
                return Err(format!(
                    "check-role requires one checkbox {label:?}, got {}",
                    matches.len()
                ));
            };
            let current = dom.attribute(
                *node,
                &Namespace::default(),
                &LocalName::from("aria-checked"),
            );
            if current == Some(wanted) {
                return Ok(());
            }
            drop(dom);
            return if clicks.click(ctx, &selector, |_, _, r| {
                (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5)
            }) {
                Ok(())
            } else {
                Err(format!("shared checkbox {label:?} cannot be clicked"))
            };
        }
        self.app_step(ctx, checkpoints, line)
    }

    fn act(&mut self, ctx: &mut mesquite::Ctx<'_, Self>, command: &str) -> bool {
        let mut handled = false;
        ctx.runner.update(|page| {
            handled = page.command(command);
        });
        handled
    }

    /// Events emitted by the same command owners as the visible controls.
    fn drain_events(&mut self, _ctx: &mut mesquite::Ctx<'_, Self>) -> Vec<String> {
        let mut events = std::mem::take(&mut *self.shared.events.borrow_mut());
        events.extend(std::mem::take(&mut self.shared.remote.borrow_mut().events));
        events
    }

    /// Busy while a capture is pending, a remote answer is still to come, the
    /// local canvas moves while it is the one shown, or a timing window's GPU
    /// times are out ("Moving counts only when local is shown").
    fn busy_mut(
        &mut self,
        ctx: &mut AppCtx<'_, TreePage, Logic, Child>,
        capture_pending: bool,
    ) -> Option<bool> {
        let host_pending = host_boundary::pending(&ctx.runner.state().root, &mut self.errors);
        let pending = self.shared.timing.borrow_mut().poll();
        let local_shown = ctx.runner.state().session == remote::Session::Local;
        // A hidden canvas cannot advance its last motion flag. Workbench
        // waits still need workspace store writes to finish before reopening.
        let canvas_shown = local_shown
            && leaf_rect(ctx).is_some_and(|(_, _, width, height)| width > 0. && height > 0.);
        #[cfg(feature = "product")]
        let workspace_saving = super::forme::owner_busy(ctx.runner.state())
            || ctx
                .runner
                .state()
                .forme
                .as_ref()
                .is_some_and(|pane| pane.saving)
            || ctx.runner.state().presentation.busy();
        #[cfg(not(feature = "product"))]
        let workspace_saving = false;
        #[cfg(feature = "moot-resources")]
        let resource_pending = self.shared.moot_resources.borrow().in_flight();
        #[cfg(not(feature = "moot-resources"))]
        let resource_pending = false;
        Some(
            capture_pending
                || host_pending
                || pending
                || self.shared.remote.borrow().in_flight()
                || (canvas_shown && self.shared.moving.get())
                || workspace_saving
                || {
                    #[cfg(feature = "product")]
                    {
                        self.shared.reading.borrow().busy()
                    }
                    #[cfg(not(feature = "product"))]
                    {
                        false
                    }
                }
                || resource_pending
                || ctx
                    .runner
                    .state()
                    .product
                    .as_ref()
                    .is_some_and(|product| product.saving),
        )
    }

    fn receipt_lines(&self) -> Vec<String> {
        let timing = self.shared.timing.borrow();
        let mut lines = timing.receipt_lines();
        lines.extend(self.shared.release_log.borrow().iter().cloned());
        lines.extend(self.shared.physics_log.borrow().iter().cloned());
        lines.push(format!(
            "gpu timestamps: {}",
            if timing.gpu_timed() { "yes" } else { "no" }
        ));
        lines
    }

    fn receipt_checks(&self, _captures: &[CaptureRecord]) -> Vec<String> {
        let mut errors = self.errors.clone();
        if self.shared.timing.borrow().any_hidden() {
            errors.push("the page was hidden while a timing window was open".to_string());
        }
        errors
    }
}

/// Read actual authority/model/canvas owners once at canonical completion.
#[cfg(feature = "product")]
fn owner_receipt(page: &TreePage) -> serde_json::Value {
    let actual = page.product.as_ref().and_then(|p| p.actual_session());
    let model = page.forme.as_ref().map(|p| p.model().document.graph_id);
    let canvas = page.shared.canvas.borrow();
    json!({"schema":"graphshell.actual-owner-observation/v1",
        "actual_app_present":actual.is_some(),"actual_graph_session":actual,
        "forme_model_session":model,"forme_region_id":canvas.forme_region().map(|r|r.id),
        "forme_session_matches":actual.zip(model).map(|(a,m)|a==m),
        "actual_session_changes":page.product.as_ref().map_or(0,|p|p.observed_session_changes)})
}
#[cfg(not(feature = "product"))]
fn owner_receipt(_page: &TreePage) -> serde_json::Value {
    serde_json::Value::Null
}

#[cfg(feature = "product")]
fn forme_fields(page: &TreePage, canvas: &Canvas, snapshot: ProbeSnapshot) -> ProbeSnapshot {
    let actual_owner = page.product.as_ref().and_then(|p| p.actual_session());
    let model_owner = page.forme.as_ref().map(|p| p.model().document.graph_id);
    let snapshot = snapshot
        .with_field("actual-app-present", actual_owner.is_some().to_string())
        .with_field(
            "actual-graph-session",
            actual_owner
                .map(|id| id.to_string())
                .unwrap_or_else(|| "busy".into()),
        )
        .with_field(
            "forme-model-session",
            model_owner
                .map(|id| id.to_string())
                .unwrap_or_else(|| "absent".into()),
        )
        .with_field(
            "forme-session-matches",
            match (actual_owner, model_owner) {
                (Some(actual), Some(model)) => (actual == model).to_string(),
                (None, _) => "unknown".into(),
                (Some(_), None) => "false".into(),
            },
        )
        .with_field(
            "actual-session-changes",
            page.product
                .as_ref()
                .map_or(0, |p| p.observed_session_changes)
                .to_string(),
        )
        .with_field(
            "forme-region-id",
            canvas
                .forme_region()
                .map(|r| r.id.to_string())
                .unwrap_or_else(|| "absent".into()),
        );
    let Some(pane) = &page.forme else {
        return snapshot.with_field(
            "forme",
            if page.forme_error.is_some() {
                "refused"
            } else {
                "absent"
            },
        );
    };
    let region = canvas.forme_region();
    let placed = region.map_or(0, |r| {
        r.cells
            .iter()
            .filter(|cell| {
                let Some((key, _)) = canvas.graph().get_node_by_id(cell.member) else {
                    return false;
                };
                let Some(at) = canvas.node_position(key) else {
                    return false;
                };
                let [x, y, right, bottom] = r.cell_world_bounds(cell);
                (at.x - (x + (right - x) / 2.)).abs() < 0.01
                    && (at.y - (y + (bottom - y) / 2.)).abs() < 0.01
            })
            .count()
    });
    snapshot
        .with_field("forme", pane.current().document.id.as_uuid().to_string())
        .with_field("forme-accesses", pane.current().members().len().to_string())
        .with_field(
            "forme-cells",
            region.map_or(0, |r| r.cells.len()).to_string(),
        )
        .with_field(
            "pelt-sessions",
            page.shared.reading.borrow().len().to_string(),
        )
        .with_field(
            "pelt-pending",
            page.shared.reading.borrow().busy().to_string(),
        )
        .with_field(
            "pelt-producers",
            page.shared.reading.borrow().registered_len().to_string(),
        )
        .with_field(
            "pelt-glyph-runs",
            page.shared.reading.borrow().glyph_runs().to_string(),
        )
        .with_field("pelt-status", page.shared.reading.borrow().status_summary())
        .with_field("forme-placed", placed.to_string())
        .with_field("forme-locked", pane.current().locked.to_string())
        .with_field("forme-visible", pane.current().visible.to_string())
        .with_field("forme-hovered", canvas.forme_region_hovered().to_string())
        .with_field("forme-saving", pane.saving.to_string())
        .with_field("forme-preview", pane.previewing().to_string())
        .with_field("forme-drop-effect", pane.drop_effect())
        .with_field("forme-draft", pane.session.editing().to_string())
        .with_field("forme-dirty", pane.session.dirty().to_string())
        .with_field("forme-can-undo", pane.session.can_undo().to_string())
        .with_field("forme-can-redo", pane.session.can_redo().to_string())
        .with_field(
            "forme-committed-geometry",
            serde_json::to_string(&pane.session.committed().geometry).unwrap_or_default(),
        )
        .with_field(
            "forme-committed-bounds",
            serde_json::to_string(&pane.session.committed().bounds).unwrap_or_default(),
        )
        .with_field(
            "forme-geometry",
            serde_json::to_string(&pane.current().geometry).unwrap_or_default(),
        )
        .with_field(
            "forme-bounds",
            serde_json::to_string(&pane.current().bounds).unwrap_or_default(),
        )
        .with_field("workbench", pane.workbench.to_string())
}
