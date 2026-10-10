// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Read-only assertions against the current shared controls. Input still uses Mesquite.
use super::*;
use layout_dom_api::{LayoutDom, LocalName, Namespace};

#[derive(serde::Deserialize)]
struct Observation {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    class: Option<String>,
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    key: Option<(String, String)>,
    #[serde(default)]
    attribute: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    visible: Option<bool>,
    #[serde(default)]
    focused: Option<bool>,
}
pub(super) fn assertion(
    ctx: &AppCtx<'_, TreePage, Logic, Child>,
    json: &str,
) -> Result<(), String> {
    let observation: Observation = serde_json::from_str(json)
        .map_err(|error| format!("Invalid retained observation: {error}"))?;
    let mut selector = match (observation.role, observation.class) {
        (Some(role), None) => Selector::role(role),
        (None, Some(class)) => Selector::class(class),
        _ => return Err("Retained observation requires exactly one of role or class".into()),
    };
    if let Some(label) = observation.label {
        selector = selector.containing(label);
    }
    if let Some((name, value)) = observation.key {
        selector = selector.with_attr(name, value);
    }
    let dom = ctx.runner.dom();
    let dom = dom.borrow();
    let projection = ctx
        .a11y_projection()
        .ok_or("retained accessibility projection is unavailable")?;
    let nodes = taproot::matching_with_projection(&dom, &selector, &projection);
    if let Some(count) = observation.count {
        if nodes.len() != count {
            return Err(format!(
                "Retained {selector:?}: expected {count} targets, got {}",
                nodes.len()
            ));
        }
        if count == 0 {
            return Ok(());
        }
    }
    let [node] = nodes.as_slice() else {
        return Err(format!(
            "Retained observation needs one target for {selector:?}, got {}",
            nodes.len()
        ));
    };
    if let Some(attribute) = observation.attribute {
        let got = dom
            .attribute(
                *node,
                &Namespace::default(),
                &LocalName::from(attribute.as_str()),
            )
            .map(str::to_owned);
        if got != observation.value {
            return Err(format!(
                "Retained {selector:?} {attribute}: expected {:?}, got {got:?}",
                observation.value
            ));
        }
    }
    if let Some(expected) = observation.text {
        fn text(
            dom: &genet_scripted_dom::ScriptedDom,
            node: genet_scripted_dom::NodeId,
            result: &mut String,
        ) {
            if let Some(value) = dom.text(node) {
                result.push_str(value);
                result.push(' ');
            }
            for child in dom.dom_children(node) {
                text(dom, child, result);
            }
        }
        let mut got = String::new();
        text(&dom, *node, &mut got);
        if !got.contains(&expected) {
            return Err(format!(
                "Retained {selector:?}: expected text {expected:?}, got {got:?}"
            ));
        }
    }
    if let Some(expected) = observation.visible {
        let got = ctx.visible_rect(*node).is_some();
        if got != expected {
            return Err(format!(
                "Retained {selector:?}: expected visible {expected}, got {got}"
            ));
        }
    }
    if let Some(expected) = observation.focused {
        let got = ctx.runner.focus() == Some(*node);
        if got != expected {
            return Err(format!(
                "Retained {selector:?}: expected focused {expected}, got {got}"
            ));
        }
    }
    Ok(())
}
