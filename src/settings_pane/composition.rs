// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Application-owned authored composition for the Settings rows.
//!
//! Component identity comes from the provider setting identity. The descriptor
//! binding remains opaque to Cambium and is resolved here against the current
//! provider projection before a row factory can be invoked.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use cambium::composition::{
    BindingAdmission, ComponentDescriptor, ComponentKey, ComponentKind, Composition,
    CompositionLimits, CompositionTransaction, CompositionValidator, ContainerKey, PlacementNode,
};
use cambium::{LifecycleEventSink, RetainedComposition, el, setting_row};
use mere_surface_api::settings::SettingSpec;
use serde_json::{Value, json};

use super::{APPLICATION_REFERENCE, SettingsState, SettingsView, apply_value};

const SETTING_KIND: &str = "turnstone.setting";

/// Build the default flat Settings composition from the provider projection.
/// Each setting keeps the same component identity across insertion and reorder.
#[cfg(test)]
pub(super) fn setting_rows(specs: &[SettingSpec]) -> SettingsView {
    settings_rows_for_specs(specs, LifecycleEventSink::default())
}

/// Validate the provider-derived default model, then render only the admitted
/// transaction. This is the production composition path until Settings owns
/// an authored transaction for user edits.
pub(super) fn settings_rows_for_specs(
    specs: &[SettingSpec],
    sink: LifecycleEventSink,
) -> SettingsView {
    let specs_by_id = specs
        .iter()
        .cloned()
        .map(|spec| (spec.id.clone(), spec))
        .collect::<BTreeMap<_, _>>();
    if specs_by_id.len() != specs.len() {
        return composition_error("provider returned duplicate setting identities");
    }
    let model = default_model(specs.iter());
    let validator = SettingsCompositionValidator {
        specs: &specs_by_id,
    };
    let transaction =
        match CompositionTransaction::new(model, CompositionLimits::default(), &validator) {
            Ok(transaction) => transaction,
            Err(error) => return composition_error(&format!("{error:?}")),
        };
    retained_rows(&specs_by_id, &transaction, BTreeSet::new(), sink)
}

/// Raw model-rendering seams are test-only. A future authored editor must
/// call `CompositionTransaction::apply` on its accepted transaction and only
/// publish the accepted snapshot after validation succeeds; rendering a
/// rejected candidate as an error view would discard retained component state.
#[cfg(test)]
pub(super) fn setting_rows_with_model(
    specs: &[SettingSpec],
    model: Composition,
    collapsed: BTreeSet<ContainerKey>,
) -> SettingsView {
    setting_rows_with_sink(specs, model, collapsed, LifecycleEventSink::default())
}

/// Test and host adapter for rebuilding an admitted composition while keeping
/// its bounded lifecycle evidence sink alive across successive view props.
#[cfg(test)]
pub(super) fn setting_rows_with_sink(
    specs: &[SettingSpec],
    model: Composition,
    collapsed: BTreeSet<ContainerKey>,
    sink: LifecycleEventSink,
) -> SettingsView {
    let specs_by_id = specs
        .iter()
        .cloned()
        .map(|spec| (spec.id.clone(), spec))
        .collect::<BTreeMap<_, _>>();
    if specs_by_id.len() != specs.len() {
        return composition_error("provider returned duplicate setting identities");
    }
    let validator = SettingsCompositionValidator {
        specs: &specs_by_id,
    };
    let transaction = match CompositionTransaction::restore_authored(
        model,
        CompositionLimits::default(),
        &validator,
    ) {
        Ok(transaction) => transaction,
        Err(error) => {
            return composition_error(&format!("{error:?}"));
        },
    };
    retained_rows(&specs_by_id, &transaction, collapsed, sink)
}

fn retained_rows(
    specs_by_id: &BTreeMap<String, SettingSpec>,
    transaction: &CompositionTransaction,
    collapsed: BTreeSet<ContainerKey>,
    sink: LifecycleEventSink,
) -> SettingsView {
    let factory_specs = Rc::new(specs_by_id.clone());
    let factory = Rc::new(move |descriptor: &ComponentDescriptor| {
        let spec_id = descriptor
            .binding
            .get("setting_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match factory_specs.get(spec_id) {
            Some(spec) => pane_setting_row(spec),
            None => Box::new(
                el::<_, SettingsState, ()>("div", "This setting is unavailable.")
                    .attr("class", "setting-unavailable")
                    .attr("role", "status"),
            ) as SettingsView,
        }
    });
    Box::new(
        RetainedComposition::<SettingsState, ()>::new(&transaction, factory)
            .with_collapsed(collapsed)
            .with_lifecycle_sink(sink),
    )
}

#[cfg(test)]
pub(super) fn default_settings_model(specs: &[SettingSpec]) -> Composition {
    default_model(specs.iter())
}

fn composition_error(error: &str) -> SettingsView {
    Box::new(
        el::<_, SettingsState, ()>(
            "div",
            format!("Settings composition is unavailable: {error}"),
        )
        .attr("class", "setting-error")
        .attr("role", "alert"),
    )
}

fn default_model<'a>(specs: impl Iterator<Item = &'a SettingSpec>) -> Composition {
    let mut components = Vec::new();
    let mut placement = Vec::new();
    for spec in specs {
        let key = component_key(&spec.id);
        components.push(ComponentDescriptor {
            key: key.clone(),
            kind: ComponentKind(SETTING_KIND.into()),
            config: Value::Null,
            binding: json!({
                "reference": APPLICATION_REFERENCE,
                "setting_id": spec.id,
            }),
        });
        placement.push(PlacementNode::Component { key });
    }
    Composition {
        components,
        placement,
        unplaced: Vec::new(),
    }
}

pub(super) fn component_key(setting_id: &str) -> ComponentKey {
    ComponentKey(format!("turnstone.setting:{setting_id}"))
}

/// One provider spec as a Cambium row. The draft is the component's; this
/// pane only sees the applied value, which it forwards to the provider under
/// the id it passed in.
fn pane_setting_row(spec: &SettingSpec) -> SettingsView {
    let setting_id = spec.id.clone();
    let label = format!(
        "{} · {:?} · {:?} · {:?}",
        spec.label, spec.scope, spec.movement, spec.mutability
    );
    Box::new(setting_row(
        spec,
        label,
        move |state: &mut SettingsState, value: super::SettingValue| {
            apply_value(state, &setting_id, value);
        },
    ))
}

struct SettingsCompositionValidator<'a> {
    specs: &'a BTreeMap<String, SettingSpec>,
}

impl CompositionValidator for SettingsCompositionValidator<'_> {
    fn validate_component(
        &self,
        candidate: &ComponentDescriptor,
        _previous: Option<&ComponentDescriptor>,
    ) -> Result<BindingAdmission, String> {
        if candidate.kind.0 != SETTING_KIND {
            return Err(format!("unsupported component kind {:?}", candidate.kind.0));
        }
        if candidate.config != Value::Null {
            return Err("settings rows do not accept component configuration".into());
        }
        let binding = candidate
            .binding
            .as_object()
            .ok_or_else(|| "setting binding must be an object".to_owned())?;
        if binding.len() != 2 {
            return Err("setting binding contains unsupported fields".into());
        }
        if binding.get("reference").and_then(Value::as_str) != Some(APPLICATION_REFERENCE) {
            return Err("setting binding has an unsupported reference".into());
        }
        let setting_id = binding
            .get("setting_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| "setting binding has no setting identity".to_owned())?;
        if candidate.key != component_key(setting_id) {
            return Err("component key does not match its setting identity".into());
        }
        if !self.specs.contains_key(setting_id) {
            return Err(format!("unknown application setting {setting_id:?}"));
        }
        Ok(BindingAdmission::Available)
    }
}

/// Production composition: each durable instance has its own draft session and
/// guarded binding even when two instances edit the same source setting.
pub(super) fn managed_rows(state: &SettingsState) -> SettingsView {
    let rows = state
        .rows
        .iter()
        .map(|(key, row)| {
            let spec = row.session.snapshot().observed_spec;
            let label = format!(
                "{} · {:?} · {:?} · {:?}",
                spec.label, spec.scope, spec.movement, spec.mutability
            );
            (key.clone(), (row.session.clone(), label))
        })
        .collect::<BTreeMap<_, _>>();
    let rows = Rc::new(rows);
    let factory = Rc::new(move |descriptor: &ComponentDescriptor| -> SettingsView {
        let key = descriptor.key.clone();
        if let Some((session, label)) = rows.get(&key) {
            let callback_key = key.clone();
            Box::new(
                el::<_, SettingsState, ()>(
                    "div",
                    cambium::setting_row_managed(
                        session,
                        label.clone(),
                        move |state: &mut SettingsState, event| {
                            super::row_commands::row_event(state, &callback_key, event);
                        },
                    ),
                )
                .attr("class", "settings-owned-row")
                .attr("data-setting-instance", key.0),
            )
        } else {
            Box::new(
                el::<_, SettingsState, ()>(
                    "div",
                    format!("Unavailable saved component: {}", descriptor.kind.0),
                )
                .attr("class", "setting-unavailable")
                .attr("role", "status")
                .attr("data-setting-instance", key.0),
            )
        }
    });
    Box::new(
        RetainedComposition::<SettingsState, ()>::new(state.arrangement.transaction(), factory)
            .with_collapsed(state.collapsed.clone())
            .with_lifecycle_sink(state.composition_lifecycle.clone()),
    )
}
