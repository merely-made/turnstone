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
use cambium::composition_scheduler::{
    CompositionCauseRef, CompositionInputError, CompositionInputSnapshot, CompositionInputs,
    DependencyStamp, InputVector, LocalInputStamp,
};
use cambium::{LifecycleEventSink, RetainedComposition, SettingRowSnapshot, el, setting_row};
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
    let mut inputs_by_key = BTreeMap::new();
    let mut row_props = BTreeMap::new();
    let mut input_error = None;
    let mut cache = state.presentation_cache.borrow_mut();
    for (key, row) in &state.rows {
        // This is the single session read for this retained-view flush. Its
        // frozen snapshot supplies both the dependency generation and label.
        let snapshot = row.session.snapshot();
        let generation = match cache.get(key) {
            Some((previous, generation)) if previous == &snapshot => *generation,
            Some((_, generation)) => match generation.checked_add(1) {
                Some(next) => next,
                None => {
                    // A local stamp only needs to differ from the immediately
                    // preceding snapshot. Resetting after exhaustion changes
                    // MAX to zero and forces the retained factory to run.
                    input_error.get_or_insert(CompositionInputError::GenerationExhausted);
                    0
                },
            },
            None => 1,
        };
        cache.insert(key.clone(), (snapshot.clone(), generation));
        let label = row_label(&snapshot);
        let vector = match input_vector(state, row, generation) {
            Ok(vector) => vector,
            Err(error) => {
                input_error.get_or_insert(error);
                local_only_input(generation)
            },
        };
        inputs_by_key.insert(key.clone(), vector);
        row_props.insert(key.clone(), (row.session.clone(), label));
    }
    cache.retain(|key, _| state.rows.contains_key(key));
    drop(cache);

    let inputs = match CompositionInputs::new(inputs_by_key) {
        Ok(inputs) => inputs,
        Err(error) => {
            input_error = Some(error);
            CompositionInputs::default()
        },
    };
    let previous = state.published_inputs.borrow().clone();
    let input_snapshot = if let Some(queue) = &state.input_queue {
        match publish_inputs(state, queue, &previous, &inputs) {
            Ok(()) => queue.capture(),
            Err(error) => {
                input_error.get_or_insert(error);
                CompositionInputSnapshot::detached(inputs.clone())
            },
        }
    } else {
        CompositionInputSnapshot::detached(inputs.clone())
    };
    *state.input_queue_error.borrow_mut() = input_error;
    *state.published_inputs.borrow_mut() = inputs;
    let attach_inputs = state.input_queue.is_some() && state.input_queue_error.borrow().is_none();

    let rows = Rc::new(row_props);
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
    let retained =
        RetainedComposition::<SettingsState, ()>::new(state.arrangement.transaction(), factory)
            .with_collapsed(state.collapsed.clone())
            .with_lifecycle_sink(state.composition_lifecycle.clone());
    if attach_inputs {
        Box::new(retained.with_inputs(input_snapshot))
    } else {
        // The previous accepted composition still renders through the legacy
        // path. A local-only fallback must never be treated as a complete
        // dependency set when one row's source stamp could not be built.
        Box::new(retained)
    }
}

fn row_label(snapshot: &SettingRowSnapshot) -> String {
    let spec = &snapshot.observed_spec;
    format!(
        "{} · {:?} · {:?} · {:?}",
        spec.label, spec.scope, spec.movement, spec.mutability
    )
}

fn input_vector(
    state: &SettingsState,
    row: &super::RowState,
    generation: u64,
) -> Result<InputVector, CompositionInputError> {
    let mut dependencies = Vec::new();
    if let Some(source) = state.snapshots.get(&row.reference.0) {
        let source_id = state
            .source_ids
            .get(&row.reference.0)
            .ok_or(CompositionInputError::InvalidName)?;
        let owner = owner_token(state, *source_id, &source.owner.0)?;
        for revision in source.revisions.iter().filter(|revision| {
            setting_domains(&row.setting_id).contains(&revision.domain.0.as_str())
        }) {
            let stamp = DependencyStamp::new(
                *source_id,
                owner,
                revision.domain.0.clone(),
                revision.revision,
            )?;
            dependencies.push(stamp);
        }
    }
    let local = LocalInputStamp::new("row-session", generation)?;
    InputVector::new(dependencies, vec![local])
}

fn local_only_input(generation: u64) -> InputVector {
    InputVector::new(
        Vec::new(),
        vec![
            LocalInputStamp::new("row-session", generation)
                .expect("static local input name is valid"),
        ],
    )
    .expect("one local input is valid")
}

/// Give each source incarnation a collision-free, per-pane numeric identity.
/// The source contract intentionally leaves incarnation strings opaque.
pub(super) fn owner_token(
    state: &SettingsState,
    source_id: u64,
    incarnation: &str,
) -> Result<u128, CompositionInputError> {
    let mut owners = state.owner_tokens.borrow_mut();
    if let Some((known_incarnation, token)) = owners.get(&source_id)
        && known_incarnation == incarnation
    {
        return Ok(*token);
    }
    let token = state
        .next_owner_token
        .get()
        .ok_or(CompositionInputError::GenerationExhausted)?;
    state.next_owner_token.set(token.checked_add(1));
    owners.insert(source_id, (incarnation.to_owned(), token));
    Ok(token)
}

fn setting_domains(setting_id: &str) -> &'static [&'static str] {
    if setting_id.starts_with("theme.") {
        &["appearance-choice", "theme-catalog"]
    } else if setting_id == "ui.zoom" || setting_id.starts_with("chrome.") {
        &["shell-chrome"]
    } else if setting_id.starts_with("behaviors.") {
        &["behaviors"]
    } else {
        &[]
    }
}

pub(super) fn changed_input_cause(
    state: &SettingsState,
    previous: &InputVector,
    next: &InputVector,
) -> Vec<(CompositionCauseRef, DependencyStamp)> {
    let mut changed = Vec::new();
    for dependency in next.dependencies() {
        let same_axis = previous.dependencies().iter().any(|old| {
            old.source_id() == dependency.source_id()
                && old.owner_incarnation() == dependency.owner_incarnation()
                && old.domain() == dependency.domain()
        });
        if !same_axis
            || previous.dependencies().iter().any(|old| {
                old.source_id() == dependency.source_id()
                    && old.owner_incarnation() == dependency.owner_incarnation()
                    && old.domain() == dependency.domain()
                    && old.revision() != dependency.revision()
            })
        {
            if let Some(cause) = state.revision_causes.get(&(
                dependency.source_id(),
                dependency.owner_incarnation(),
                dependency.domain().to_owned(),
                dependency.revision(),
            )) {
                changed.push((*cause, dependency.clone()));
            }
        }
    }
    changed
}

fn publish_inputs(
    state: &SettingsState,
    queue: &cambium::composition_scheduler::CompositionInputQueue,
    previous: &CompositionInputs,
    next: &CompositionInputs,
) -> Result<(), CompositionInputError> {
    let mut by_component = BTreeMap::new();
    for (key, vector) in next.components() {
        let Some(old) = previous.vector(key) else {
            continue;
        };
        let causes = changed_input_cause(state, old, vector)
            .into_iter()
            .map(|(cause, _)| cause)
            .collect::<BTreeSet<_>>();
        if !causes.is_empty() {
            by_component.insert(key.clone(), causes.into_iter().collect());
        }
    }
    queue.publish_with_causes(next.clone(), by_component)
}
