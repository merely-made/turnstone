// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Optional, bounded, read-only observations of the real retained overlay layout.
//! This never changes exclusion policy, dispatches input, waits, or requests paint.
use super::*;
use cambium_rootstock::HostTree as _;
use genet_scripted_dom::NodeId;
use layout_dom_api::{LayoutDom as _, LocalName, Namespace};
use serde_json::Value;
use std::collections::HashSet;

const FRAME_ATTR: &str = "data-graphshell-painted-overlay";
const EVENT_ATTR: &str = "data-graphshell-overlay-before-event";
const CLASSES: [&str; 5] = [
    "tree-canvas",
    "tools-overlay",
    "tree-product",
    "tree-projection",
    "tree-command-menu",
];
const MAX_NODES: usize = 2048;
const MAX_PER_CLASS: usize = 8;
const MAX_ANCESTORS: usize = 64;
const MAX_FRAME_BYTES: usize = 65536;
const MAX_EVENT_BYTES: usize = 98304;

fn enabled() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let Ok(search) = window.location().search() else {
        return false;
    };
    if search.len() > 4096 {
        return false;
    }
    let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) else {
        return false;
    };
    // Duplicate flags and other values never enable this observer.
    let values = params.get_all("overlay_observe");
    values.length() == 1 && values.get(0).as_string().as_deref() == Some("1")
}

fn incomplete(reason: &str) -> Value {
    json!({"schema":"graphshell.painted-overlay-observation/v1", "status":"incomplete", "reason":reason})
}

fn attr<'a>(
    dom: &'a genet_scripted_dom::ScriptedDom,
    node: NodeId,
    name: &str,
) -> Result<Option<&'a str>, &'static str> {
    let value = dom.attribute(node, &Namespace::default(), &LocalName::from(name));
    if value.is_some_and(|s| s.len() > 512) {
        return Err("retained_attribute_limit");
    }
    Ok(value)
}

fn node_record(dom: &genet_scripted_dom::ScriptedDom, node: NodeId) -> Result<Value, &'static str> {
    let tag = dom.element_name(node).map(|n| n.local.as_ref());
    if tag.is_some_and(|s: &str| s.len() > 64) {
        return Err("retained_tag_limit");
    }
    let class = attr(dom, node, "class")?;
    let role = attr(dom, node, "role")?;
    let contenteditable = attr(dom, node, "contenteditable")?;
    let tabindex = attr(dom, node, "tabindex")?;
    let interactive = tag.is_some_and(|s| {
        matches!(
            s,
            "button" | "a" | "input" | "select" | "textarea" | "summary"
        )
    }) || role.is_some_and(|s| {
        matches!(
            s,
            "button"
                | "link"
                | "checkbox"
                | "radio"
                | "textbox"
                | "combobox"
                | "slider"
                | "spinbutton"
                | "menuitem"
                | "option"
        )
    }) || contenteditable.is_some_and(|s| s != "false")
        || tabindex.is_some();
    Ok(
        json!({"node":dom.opaque_id(node),"element":tag.is_some(),"tag":tag,"class":class,"role":role,
        "contenteditable":contenteditable,"tabindex":tabindex,"interactive_marker":interactive}),
    )
}

fn policy(shared: &Shared) -> Result<Value, &'static str> {
    let gestures = shared.gestures.borrow();
    if gestures.overlays.len() > 32 {
        return Err("policy_rect_limit");
    }
    Ok(json!({"graph":gestures.graph,"exclusions":gestures.overlays}))
}

/// The gate is checked before any retained DOM/style/hit getter is called.
pub(crate) fn observe(ctx: &AppCtx<'_, TreePage, Logic, Child>, shared: &Shared) -> Value {
    if !enabled() {
        return json!({"schema":"graphshell.painted-overlay-observation/v1","status":"disabled"});
    }
    match observe_enabled(ctx, shared) {
        Ok(value) => value,
        Err(reason) => incomplete(reason),
    }
}

fn observe_enabled(
    ctx: &AppCtx<'_, TreePage, Logic, Child>,
    shared: &Shared,
) -> Result<Value, &'static str> {
    let frame = ctx.presentation.ok_or("missing_presented_frame")?;
    if !ctx.ui_zoom.is_finite() || ctx.ui_zoom <= 0.0 {
        return Err("invalid_ui_zoom");
    }
    let dom_handle = ctx.runner.dom();
    let dom = dom_handle.borrow();
    let mount = ctx.runner.mount();
    let mut stack = vec![mount];
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    let mut counts = [0_usize; 5];
    let mut graph_node = None;
    while let Some(node) = stack.pop() {
        if seen.len() >= MAX_NODES || !seen.insert(node) {
            return Err("retained_walk_limit_or_cycle");
        }
        let class = attr(&dom, node, "class")?;
        for (index, wanted) in CLASSES.iter().enumerate() {
            if class.is_some_and(|s| s.split_ascii_whitespace().any(|word| word == *wanted)) {
                counts[index] += 1;
                if index == 0 {
                    graph_node = Some(node);
                }
                if counts[index] > MAX_PER_CLASS {
                    return Err("overlay_candidate_limit");
                }
                let mut styles = serde_json::Map::new();
                for property in [
                    "padding-left",
                    "padding-right",
                    "padding-top",
                    "padding-bottom",
                    "border-left-width",
                    "border-right-width",
                    "border-top-width",
                    "border-bottom-width",
                    "z-index",
                    "transform",
                    "overflow-x",
                    "overflow-y",
                    "pointer-events",
                ] {
                    let value = ctx.painted_style_value(node, property);
                    // The existing computed getter returns an owned string; cap it before
                    // cloning/serializing. No source CSS/property text is inferred here.
                    if value.as_ref().is_some_and(|s| s.len() > 128) {
                        return Err("computed_style_limit");
                    }
                    styles.insert(property.into(), json!(value));
                }
                candidates.push(json!({"class":wanted,"node":dom.opaque_id(node),
                    "painted_rect":ctx.painted_rect(node),"visible_rect":ctx.visible_rect(node),"computed":styles}));
            }
        }
        for child in dom.flat_children(node) {
            if stack.len() + seen.len() >= MAX_NODES {
                return Err("retained_walk_limit");
            }
            stack.push(child);
        }
    }
    if counts[0] != 1 {
        return Err("graph_candidate_not_unique");
    }
    let graph = ctx
        .painted_rect(graph_node.ok_or("graph_node_missing")?)
        .ok_or("graph_not_painted")?;
    let size = shared.size.get();
    let points = [
        ("center", size.0 as f32 / 2.0, size.1 as f32 / 2.0),
        ("menu-padding", 404.0, 204.0),
    ];
    let mut hits = Vec::new();
    for (name, x, y) in points {
        let at = (graph.0 + x, graph.1 + y);
        let hit = ctx.painted_hit(at.0, at.1);
        let mut chain = Vec::new();
        let mut chain_seen = HashSet::new();
        let mut next = hit;
        let mut reached_mount = false;
        while let Some(node) = next {
            if chain.len() >= MAX_ANCESTORS || !chain_seen.insert(node) {
                return Err("hit_ancestor_limit_or_cycle");
            }
            let mut record = node_record(&dom, node)?;
            let transform = ctx.painted_style_value(node, "transform");
            if transform.as_ref().is_some_and(|s| s.len() > 128) {
                return Err("ancestor_style_limit");
            }
            record["transform"] = json!(transform);
            record["painted_rect"] = json!(ctx.painted_rect(node));
            record["visible_rect"] = json!(ctx.visible_rect(node));
            chain.push(record);
            if node == mount {
                reached_mount = true;
                break;
            }
            next = dom.parent(node).or_else(|| dom.shadow_host(node));
        }
        hits.push(json!({"name":name,"graph_local":[x,y],"layout_point":[at.0,at.1],
            "selected_hit":hit.map(|n|dom.opaque_id(n)),"ancestors":chain,"reached_mount":reached_mount}));
    }
    let window = web_sys::window().ok_or("window_missing")?;
    let document = window.document().ok_or("document_missing")?;
    let canvas_element = document
        .get_element_by_id("gs-graphshell-canvas")
        .ok_or("browser_canvas_missing")?;
    let canvas_rect = canvas_element.get_bounding_client_rect();
    let root_rect = shared.root.get_bounding_client_rect();
    let canvas = shared.canvas.borrow();
    let gestures = shared.gestures.borrow();
    Ok(
        json!({"schema":"graphshell.painted-overlay-observation/v1","status":"observed",
        "source":"AppCtx retained layout/styles/hit_test; not browser DOM mirror",
        "observed_ms":now_ms(),"browser_time_origin":window.performance().map(|p|p.time_origin()),
        "presentation":{"host":frame.host,"sequence":frame.sequence,"width":frame.width,"height":frame.height,"layout_scale":frame.layout_scale},
        "coordinates":{"logical_viewport":[ctx.logical_size.0,ctx.logical_size.1],"ui_zoom":ctx.ui_zoom,
            "root_client_rect":[root_rect.x(),root_rect.y(),root_rect.width(),root_rect.height()],
            "canvas_client_rect":[canvas_rect.x(),canvas_rect.y(),canvas_rect.width(),canvas_rect.height()],
            "graph_rect":[graph.0,graph.1,graph.2,graph.3],"graph_size":[size.0,size.1],
            "client_from_layout":"canvas_client_origin + layout_point * ui_zoom; no device-scale multiplication"},
        "candidates":candidates,"hits":hits,"policy":policy(shared)?,
        "state":{"camera":[canvas.camera().offset.0,canvas.camera().offset.1,canvas.camera().zoom],
            "physics_paused":canvas.physics_paused(),"dragging":format!("{:?}",canvas.dragging_node()),
            "last_wheel":format!("{:?}",gestures.last_wheel),"last_touch":format!("{:?}",gestures.last_touch)}}),
    )
}

pub(crate) fn publish(ctx: &AppCtx<'_, TreePage, Logic, Child>, shared: &Shared) {
    if !enabled() {
        return;
    }
    let value = observe(ctx, shared);
    let mut text = value.to_string();
    if text.len() > MAX_FRAME_BYTES {
        text = incomplete("serialized_frame_limit").to_string();
    }
    // A failed attribute write is absence of evidence, never successful admission.
    let _ = shared.root.set_attribute(FRAME_ATTR, &text);
}

/// A passive entry receipt before the original gesture listener. Geometry is
/// explicitly the last observed retained frame, never a new input-time layout.
pub(crate) fn before_event(shared: &Shared, event: &web_sys::Event) {
    if !enabled() {
        return;
    }
    let value = (|| -> Result<Value,&'static str> {
        let text = shared.root.get_attribute(FRAME_ATTR).ok_or("frame_attribute_missing")?;
        if text.len() > MAX_FRAME_BYTES { return Err("frame_attribute_limit"); }
        let frame: Value = serde_json::from_str(&text).map_err(|_|"frame_attribute_invalid_json")?;
        let mouse = event.dyn_ref::<web_sys::MouseEvent>();
        let touch = event.dyn_ref::<web_sys::TouchEvent>().and_then(|e|e.changed_touches().get(0));
        let client = mouse.map(|e|(e.client_x(),e.client_y())).or_else(||touch.map(|t|(t.client_x(),t.client_y())));
        let zoom = shared.gestures.borrow().ui_zoom.as_ref().ok_or("event_ui_zoom_missing")?();
        if !zoom.is_finite() || zoom <= 0.0 { return Err("event_ui_zoom_invalid"); }
        let element = web_sys::window().and_then(|w|w.document())
            .and_then(|d|d.get_element_by_id("gs-graphshell-canvas")).ok_or("event_canvas_missing")?;
        let rect = element.get_bounding_client_rect();
        let logical_point = client.map(|(x,y)|[(x as f64 - rect.x()) / f64::from(zoom),
            (y as f64 - rect.y()) / f64::from(zoom)]);
        let event_type = event.type_();
        if event_type.len() > 32 { return Err("event_type_limit"); }
        Ok(json!({"schema":"graphshell.overlay-before-event/v1","status":"observed",
            "phase":"entry before unchanged Rust gesture handler","observed_ms":now_ms(),
            "event":{"type":event_type,"timestamp":event.time_stamp(),"is_trusted":event.is_trusted(),
                "client_point":client,"default_prevented_before":event.default_prevented(),"cancelable":event.cancelable()},
            "retained_frame":frame,"policy_at_event":policy(shared)?,
            "coordinates_at_event":{"ui_zoom":zoom,"canvas_client_rect":[rect.x(),rect.y(),rect.width(),rect.height()],
                "layout_point":logical_point}}))
    })().unwrap_or_else(|reason|json!({"schema":"graphshell.overlay-before-event/v1","status":"incomplete","reason":reason}));
    let mut text = value.to_string();
    if text.len() > MAX_EVENT_BYTES {
        text = json!({"schema":"graphshell.overlay-before-event/v1","status":"incomplete","reason":"serialized_event_limit"}).to_string();
    }
    let _ = shared.root.set_attribute(EVENT_ATTR, &text);
}
