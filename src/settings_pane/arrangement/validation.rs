// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{BindingFingerprint, SETTING_KIND, Sources};
use cambium::composition::{
    BindingAdmission, ComponentDescriptor, Composition, CompositionValidator, ContainerKey,
    ContainerKind,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(super) fn binding(reference: &str, setting_id: &str) -> Value {
    json!({ "reference": reference, "setting_id": setting_id })
}

fn binding_ref(value: &Value) -> Option<&str> {
    value.get("reference").and_then(Value::as_str)
}
fn binding_id(value: &Value) -> Option<&str> {
    value.get("setting_id").and_then(Value::as_str)
}

pub(super) fn fingerprints(model: &Composition) -> BTreeSet<BindingFingerprint> {
    model
        .components
        .iter()
        .filter_map(|item| {
            Some((
                item.key.clone(),
                binding_ref(&item.binding)?.to_owned(),
                binding_id(&item.binding)?.to_owned(),
            ))
        })
        .collect()
}

pub(super) struct Validator<'a> {
    pub(super) sources: &'a Sources,
    pub(super) historical: &'a BTreeSet<BindingFingerprint>,
    pub(super) historical_descriptors: &'a [ComponentDescriptor],
}

impl CompositionValidator for Validator<'_> {
    fn validate_component(
        &self,
        candidate: &ComponentDescriptor,
        previous: Option<&ComponentDescriptor>,
    ) -> Result<BindingAdmission, String> {
        if candidate.kind.0 != SETTING_KIND {
            let same_previous = previous.is_some_and(|old| old == candidate);
            if same_previous
                || self
                    .historical_descriptors
                    .iter()
                    .any(|old| old == candidate)
            {
                return Ok(BindingAdmission::PreviouslyAuthoredUnavailable);
            }
            return Err("new Settings component kind is unsupported".into());
        }
        if !candidate.config.is_null() {
            return Err("Settings rows do not accept component config".into());
        }
        let object = candidate
            .binding
            .as_object()
            .ok_or("binding must be an object")?;
        if object.len() != 2 {
            return Err("binding has unsupported fields".into());
        }
        let reference = binding_ref(&candidate.binding)
            .filter(|value| !value.is_empty())
            .ok_or("binding reference is empty")?;
        let setting_id = binding_id(&candidate.binding)
            .filter(|value| !value.is_empty())
            .ok_or("binding setting identity is empty")?;
        if self
            .sources
            .get(reference)
            .is_some_and(|source| source.contains_key(setting_id))
        {
            return Ok(BindingAdmission::Available);
        }
        let fingerprint = (
            candidate.key.clone(),
            reference.to_owned(),
            setting_id.to_owned(),
        );
        let same_previous = previous.is_some_and(|old| old.binding == candidate.binding);
        if same_previous || self.historical.contains(&fingerprint) {
            return Ok(BindingAdmission::PreviouslyAuthoredUnavailable);
        }
        Err("new Settings binding target is not registered".into())
    }
}

pub(super) struct RecordValidator;

impl cambium::composition::CompositionValidator for RecordValidator {
    fn validate_component(
        &self,
        candidate: &ComponentDescriptor,
        previous: Option<&ComponentDescriptor>,
    ) -> Result<BindingAdmission, String> {
        if candidate.kind.0 != SETTING_KIND {
            return if previous.is_some_and(|old| old == candidate) {
                Ok(BindingAdmission::PreviouslyAuthoredUnavailable)
            } else {
                Err("unsupported new Settings component kind".into())
            };
        }
        if !candidate.config.is_null() {
            return Err("Settings row config must be null".into());
        }
        let object = candidate
            .binding
            .as_object()
            .ok_or("Settings binding must be an object")?;
        if object.len() != 2 {
            return Err("Settings binding has unsupported fields".into());
        }
        let reference = binding_ref(&candidate.binding)
            .filter(|value| !value.is_empty())
            .ok_or("Settings binding reference is empty")?;
        let setting_id = binding_id(&candidate.binding)
            .filter(|value| !value.is_empty())
            .ok_or("Settings binding identity is empty")?;
        // Persisted references are authored identifiers, not provider
        // capabilities. Keep unavailable sources inert on restore; AddSetting
        // and Rebind enforce current registry membership before introducing a
        // new target.
        let _ = (reference, setting_id);
        Ok(BindingAdmission::Available)
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewPayload {
    collapsed: Vec<ViewContainerKey>,
    scroll: ScrollPosition,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewContainerKey {
    kind: String,
    key: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ScrollPosition {
    x: f64,
    y: f64,
}

pub(super) fn validate_view_state(
    payload: &Value,
    composition: &Composition,
) -> Result<(), String> {
    if payload.is_null() {
        return Ok(());
    }
    let view: ViewPayload = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid Settings view state: {error}"))?;
    const MAX_SCROLL: f64 = 10_000_000.0;
    if !view.scroll.x.is_finite()
        || !view.scroll.y.is_finite()
        || view.scroll.x < 0.0
        || view.scroll.y < 0.0
        || view.scroll.x > MAX_SCROLL
        || view.scroll.y > MAX_SCROLL
    {
        return Err("Settings scroll position is outside its finite bounds".into());
    }
    let declared = declared_containers(composition);
    let mut seen = BTreeSet::new();
    for container in view.collapsed {
        if container.key.is_empty() || !seen.insert((container.kind.clone(), container.key.clone()))
        {
            return Err("Settings collapsed-container identity is empty or duplicated".into());
        }
        let valid = match container.kind.as_str() {
            "group" => declared.contains(&ContainerKey {
                kind: ContainerKind::Group,
                key: container.key,
            }),
            "drawer" => declared.contains(&ContainerKey {
                kind: ContainerKind::Drawer,
                key: container.key,
            }),
            _ => false,
        };
        if !valid {
            return Err("Settings view state names an undeclared container".into());
        }
    }
    Ok(())
}

fn declared_containers(composition: &Composition) -> BTreeSet<ContainerKey> {
    fn visit(nodes: &[cambium::composition::PlacementNode], out: &mut BTreeSet<ContainerKey>) {
        for node in nodes {
            match node {
                cambium::composition::PlacementNode::Component { .. } => {},
                cambium::composition::PlacementNode::Group { key, children } => {
                    out.insert(ContainerKey {
                        kind: ContainerKind::Group,
                        key: key.0.clone(),
                    });
                    visit(children, out);
                },
                cambium::composition::PlacementNode::Drawer { key, children } => {
                    out.insert(ContainerKey {
                        kind: ContainerKind::Drawer,
                        key: key.0.clone(),
                    });
                    visit(children, out);
                },
            }
        }
    }
    let mut result = BTreeSet::new();
    visit(&composition.placement, &mut result);
    result
}
