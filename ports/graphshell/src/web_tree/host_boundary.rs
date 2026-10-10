// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Scenario fixture boundary: external browser input is performed by the headed
//! driver; this module only requests it and observes externally owned elements.
use super::*;

fn external(selector: &str) -> Result<Element, String> {
    if !["#host-title", "#search-input", "#session-local"].contains(&selector) {
        return Err(format!(
            "Unregistered external fixture selector: {selector}"
        ));
    }
    let element = crate::document()?
        .query_selector(selector)
        .map_err(|_| "Invalid external fixture selector")?
        .ok_or("External fixture target absent")?;
    if element
        .closest("graphshell-view,graphshell-tree")
        .map_err(|_| "Cannot inspect fixture ownership")?
        .is_some()
    {
        return Err("External fixture target belongs to Graphshell".into());
    }
    Ok(element)
}
#[derive(serde::Deserialize)]
struct Observation {
    selector: String,
    attribute: Option<String>,
    value: Option<String>,
    text: Option<String>,
    focused: Option<bool>,
}
pub(super) fn observe(json: &str) -> Result<(), String> {
    let value: Observation = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let element = external(&value.selector)?;
    if let Some(attribute) = value.attribute {
        let got = element.get_attribute(&attribute);
        if got != value.value {
            return Err(format!(
                "External {} {attribute}: expected {:?}, got {got:?}",
                value.selector, value.value
            ));
        }
    }
    if let Some(expected) = value.text {
        let got = element.text_content().unwrap_or_default();
        if !got.contains(&expected) {
            return Err(format!(
                "External {}: expected {expected:?}, got {got:?}",
                value.selector
            ));
        }
    }
    if let Some(expected) = value.focused {
        let got = crate::document()?.active_element().as_ref() == Some(&element);
        if got != expected {
            return Err(format!(
                "External {}: expected focused={expected}, got {got}",
                value.selector
            ));
        }
    }
    Ok(())
}
pub(super) fn request(root: &Element, command: &str) -> Result<(), String> {
    if crate::document()?
        .body()
        .and_then(|body| body.get_attribute("data-scenario"))
        .as_deref()
        != Some("running")
    {
        return Err("External fixture input requires an explicitly running scenario".into());
    }
    let (kind, target) = command
        .split_once(' ')
        .ok_or("host-input wants focus/click/key and target")?;
    match kind {
        "focus" | "click" if ["#search-input", "#session-local"].contains(&target) => {
            external(target)?;
        },
        "key" if target == "ArrowLeft" => {},
        _ => return Err("Unsupported external fixture input".into()),
    }
    if root.has_attribute("data-host-input-request") {
        return Err("External input still pending".into());
    }
    let id = root
        .get_attribute("data-host-input-sequence")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
        + 1;
    root.remove_attribute("data-host-input-result")
        .map_err(|_| "Cannot clear fixture result")?;
    root.set_attribute("data-host-input-sequence", &id.to_string())
        .map_err(|_| "Cannot mark fixture sequence")?;
    root.set_attribute(
        "data-host-input-request",
        &json!({"id":id,"kind":kind,"target":target}).to_string(),
    )
    .map_err(|_| "Cannot publish fixture input request".into())
}
pub(super) fn pending(root: &Element, errors: &mut Vec<String>) -> bool {
    let Some(request) = root.get_attribute("data-host-input-request") else {
        return false;
    };
    let Some(result) = root.get_attribute("data-host-input-result") else {
        return true;
    };
    let request = serde_json::from_str::<serde_json::Value>(&request).ok();
    let result = serde_json::from_str::<serde_json::Value>(&result).ok();
    let Some((request, result)) = request.zip(result) else {
        return true;
    };
    if request.get("id") != result.get("id") {
        return true;
    }
    if let Some(error) = result.get("error").and_then(|v| v.as_str()) {
        errors.push(format!("External input fixture: {error}"));
    }
    let _ = root.remove_attribute("data-host-input-request");
    let _ = root.remove_attribute("data-host-input-result");
    false
}
