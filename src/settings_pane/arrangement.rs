// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Pure authored Settings arrangement, separate from retained row sessions.

use std::collections::{BTreeMap, BTreeSet};

use cambium::composition::{
    ComponentDescriptor, ComponentKey, ComponentKind, Composition, CompositionEdit,
    CompositionError, CompositionLimits, CompositionTransaction, ContainerKey, ContainerKind,
    PlacementLocation, PlacementNode,
};
use mere_surface_api::settings::SettingSpec;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod operations;
mod validation;
use operations::{apply_operation, effect};
use validation::{RecordValidator, Validator, binding, fingerprints, validate_view_state};

use crate::panes::{PaneConfig, PaneConfigSchemaId, PaneViewSchemaId, PaneViewState};

pub(super) const SETTINGS_CONFIG_SCHEMA: &str = "turnstone.settings.config";
pub(super) const SETTINGS_CONFIG_VERSION: u32 = 1;
pub(super) const SETTING_KIND: &str = "turnstone.setting";
const HISTORY_LIMIT: usize = 64;

pub(super) type SettingSources = BTreeMap<String, BTreeMap<String, SettingSpec>>;
type Sources = SettingSources;
type BindingFingerprint = (ComponentKey, String, String);

pub(super) fn application_sources(
    specs: &[SettingSpec],
) -> Result<SettingSources, ArrangementError> {
    let mut source = BTreeMap::new();
    for spec in specs {
        if source.insert(spec.id.clone(), spec.clone()).is_some() {
            return Err(ArrangementError::InvalidOperation(
                "duplicate provider setting identity",
            ));
        }
    }
    Ok(BTreeMap::from([(
        super::APPLICATION_REFERENCE.to_owned(),
        source,
    )]))
}

/// The accepted authored tree plus bounded model-only undo history. Expanded
/// groups and scroll position belong to the pane view-state, not this value.
pub(super) struct SettingsArrangement {
    transaction: CompositionTransaction,
    sources: Sources,
    revision: u64,
    undo: Vec<Composition>,
    redo: Vec<Composition>,
}

#[derive(Debug)]
pub(super) enum ArrangementError {
    ConfigSchema,
    ConfigVersion(u32),
    InvalidPayload(String),
    Composition(CompositionError),
    UnknownComponent(ComponentKey),
    UnknownContainer(ContainerKey),
    InvalidOperation(&'static str),
    StalePreview,
    RevisionExhausted,
    HistoryEmpty,
}

/// A transaction proposal that has passed full composition and product
/// validation but has not changed the accepted arrangement yet. Hosts inspect
/// its effect, prepare affected runner routes, then call `commit`.
pub(super) struct PreparedArrangement {
    base: Composition,
    base_revision: u64,
    candidate: Composition,
    effect: ArrangementEffect,
}

impl PreparedArrangement {
    pub(super) fn candidate(&self) -> &Composition {
        &self.candidate
    }

    pub(super) fn effect(&self) -> &ArrangementEffect {
        &self.effect
    }
}

/// Product effects the host needs before publishing a candidate.
#[derive(Clone, Debug, Default)]
pub(super) struct ArrangementEffect {
    pub edits: Vec<CompositionEdit>,
    pub prepare_routes: BTreeSet<ComponentKey>,
    pub removed: BTreeSet<ComponentKey>,
    pub rebound: BTreeSet<ComponentKey>,
}

#[derive(Clone, Debug)]
pub(super) enum ArrangementOperation {
    AddSetting {
        reference: String,
        setting_id: String,
        container: ContainerKey,
        index: usize,
    },
    MoveUp(ComponentKey),
    MoveDown(ComponentKey),
    MoveTo {
        key: ComponentKey,
        container: ContainerKey,
        index: usize,
    },
    AddGroup {
        key: String,
        parent: ContainerKey,
        index: usize,
    },
    AddDrawer {
        key: String,
        parent: ContainerKey,
        index: usize,
    },
    RemoveEmptyContainer(ContainerKey),
    Hide(ComponentKey),
    Show {
        key: ComponentKey,
        container: ContainerKey,
        index: usize,
    },
    Duplicate(ComponentKey),
    Delete {
        key: ComponentKey,
        approval: DeletionApproval,
    },
    Rebind {
        key: ComponentKey,
        reference: String,
        setting_id: String,
        draft_clean: bool,
        no_pending_write: bool,
    },
}

/// Opaque confirmation produced only after the row is clean and has no pending
/// write. The product constructs it after its delete-confirmation flow.
#[derive(Clone, Debug)]
pub(super) struct DeletionApproval {
    key: ComponentKey,
}

impl DeletionApproval {
    pub(super) fn after_confirmation(
        key: ComponentKey,
        confirmed: bool,
        draft_clean: bool,
        no_pending_write: bool,
    ) -> Result<Self, ArrangementError> {
        if !confirmed || !draft_clean || !no_pending_write {
            return Err(ArrangementError::InvalidOperation(
                "deletion needs confirmation, a clean draft, and no pending write",
            ));
        }
        Ok(Self { key })
    }
}

pub(super) const SETTINGS_VIEW_SCHEMA: &str = "turnstone.settings.view";

/// Persistence-boundary validation for a pane record. Missing setting targets
/// and unknown authored kinds remain inert descriptors; runtime admission still
/// checks the current registered source before a new binding can be committed.
pub(super) fn validate_record(config: &PaneConfig, view: &PaneViewState) -> Result<(), String> {
    if config.schema.as_str() != SETTINGS_CONFIG_SCHEMA || config.version != SETTINGS_CONFIG_VERSION
    {
        return Err("unsupported Settings config schema or version".into());
    }
    if view.schema.as_str() != SETTINGS_VIEW_SCHEMA || view.version != SETTINGS_CONFIG_VERSION {
        return Err("unsupported Settings view-state schema or version".into());
    }
    let model = if config.payload.is_null() {
        Composition::default()
    } else {
        let encoded: EncodedArrangement = serde_json::from_value(config.payload.clone())
            .map_err(|error| format!("invalid Settings config payload: {error}"))?;
        if encoded.version != SETTINGS_CONFIG_VERSION {
            return Err(format!(
                "unsupported Settings arrangement version {}",
                encoded.version
            ));
        }
        encoded.composition
    };
    let validator = RecordValidator;
    let accepted =
        CompositionTransaction::restore_authored(model, CompositionLimits::default(), &validator)
            .map_err(|error| format!("invalid Settings composition: {error:?}"))?;
    validate_view_state(&view.payload, accepted.accepted())
}

pub(super) fn encode_view_state(
    composition: &Composition,
    collapsed: &BTreeSet<ContainerKey>,
    scroll_x: f64,
    scroll_y: f64,
) -> Result<PaneViewState, String> {
    let collapsed = collapsed
        .iter()
        .map(|container| {
            let kind = match container.kind {
                ContainerKind::Group => "group",
                ContainerKind::Drawer => "drawer",
                ContainerKind::Root => "root",
            };
            json!({ "kind": kind, "key": container.key })
        })
        .collect::<Vec<_>>();
    let payload = json!({ "collapsed": collapsed, "scroll": { "x": scroll_x, "y": scroll_y } });
    validate_view_state(&payload, composition)?;
    Ok(PaneViewState {
        schema: PaneViewSchemaId::new(SETTINGS_VIEW_SCHEMA),
        version: SETTINGS_CONFIG_VERSION,
        payload,
    })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EncodedArrangement {
    version: u32,
    composition: Composition,
}

impl SettingsArrangement {
    /// Start with provider rows in provider order. Production passes only the
    /// `APPLICATION_REFERENCE` source until additional providers are admitted.
    pub(super) fn from_application_specs(specs: &[SettingSpec]) -> Result<Self, ArrangementError> {
        let mut sources = Sources::new();
        let mut application_specs = BTreeMap::new();
        let mut composition = Composition::default();
        for spec in specs {
            if application_specs
                .insert(spec.id.clone(), spec.clone())
                .is_some()
            {
                return Err(ArrangementError::InvalidOperation(
                    "duplicate provider setting identity",
                ));
            }
            let key = ComponentKey(format!("turnstone.setting:{}", spec.id));
            composition.components.push(ComponentDescriptor {
                key: key.clone(),
                kind: ComponentKind(SETTING_KIND.into()),
                config: Value::Null,
                binding: binding(super::APPLICATION_REFERENCE, &spec.id),
            });
            composition.placement.push(PlacementNode::Component { key });
        }
        sources.insert(super::APPLICATION_REFERENCE.to_owned(), application_specs);
        Self::from_model(composition, sources, false)
    }

    /// Restore the pane-owned config. Null is the supported empty/default input.
    pub(super) fn restore(
        config: &PaneConfig,
        default_specs: &[SettingSpec],
        sources: SettingSources,
    ) -> Result<Self, ArrangementError> {
        if config.schema.as_str() != SETTINGS_CONFIG_SCHEMA {
            return Err(ArrangementError::ConfigSchema);
        }
        if config.version != SETTINGS_CONFIG_VERSION {
            return Err(ArrangementError::ConfigVersion(config.version));
        }
        if config.payload.is_null() {
            let mut arrangement = Self::from_application_specs(default_specs)?;
            arrangement.sources = sources;
            return Ok(arrangement);
        }
        let decoded: EncodedArrangement = serde_json::from_value(config.payload.clone())
            .map_err(|error| ArrangementError::InvalidPayload(error.to_string()))?;
        if decoded.version != SETTINGS_CONFIG_VERSION {
            return Err(ArrangementError::ConfigVersion(decoded.version));
        }
        Self::from_model(decoded.composition, sources, true)
    }

    fn from_model(
        composition: Composition,
        sources: Sources,
        restored: bool,
    ) -> Result<Self, ArrangementError> {
        let historical_bindings = fingerprints(&composition);
        let historical_descriptors = composition.components.clone();
        let validator = Validator {
            sources: &sources,
            historical: &historical_bindings,
            historical_descriptors: &historical_descriptors,
        };
        let transaction = if restored {
            CompositionTransaction::restore_authored(
                composition,
                CompositionLimits::default(),
                &validator,
            )
        } else {
            CompositionTransaction::new(composition, CompositionLimits::default(), &validator)
        }
        .map_err(ArrangementError::Composition)?;
        Ok(Self {
            transaction,
            sources,
            revision: 0,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    pub(super) fn transaction(&self) -> &CompositionTransaction {
        &self.transaction
    }

    pub(super) fn accepted(&self) -> &Composition {
        self.transaction.accepted()
    }

    pub(super) fn revision(&self) -> u64 {
        self.revision
    }
    pub(super) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub(super) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(super) fn encode_config(&self) -> Result<PaneConfig, ArrangementError> {
        let payload = serde_json::to_value(EncodedArrangement {
            version: SETTINGS_CONFIG_VERSION,
            composition: self.accepted().clone(),
        })
        .map_err(|error| ArrangementError::InvalidPayload(error.to_string()))?;
        Ok(PaneConfig {
            schema: PaneConfigSchemaId::new(SETTINGS_CONFIG_SCHEMA),
            version: SETTINGS_CONFIG_VERSION,
            payload,
        })
    }

    /// Refresh registered targets. Descriptors whose exact previous bindings
    /// disappeared remain accepted and render as unavailable placeholders.
    pub(super) fn update_sources(&mut self, sources: Sources) {
        self.sources = sources;
    }

    pub(super) fn preview(
        &self,
        operation: ArrangementOperation,
    ) -> Result<PreparedArrangement, ArrangementError> {
        let mut candidate = self.accepted().clone();
        apply_operation(&mut candidate, operation, &self.sources)?;
        self.validate_candidate(candidate)
    }

    pub(super) fn preview_undo(&self) -> Result<PreparedArrangement, ArrangementError> {
        let candidate = self
            .undo
            .last()
            .cloned()
            .ok_or(ArrangementError::HistoryEmpty)?;
        self.validate_candidate(candidate)
    }

    pub(super) fn preview_redo(&self) -> Result<PreparedArrangement, ArrangementError> {
        let candidate = self
            .redo
            .last()
            .cloned()
            .ok_or(ArrangementError::HistoryEmpty)?;
        self.validate_candidate(candidate)
    }

    fn validate_candidate(
        &self,
        candidate: Composition,
    ) -> Result<PreparedArrangement, ArrangementError> {
        let (historical, historical_descriptors) = self.validation_provenance();
        let validator = Validator {
            sources: &self.sources,
            historical: &historical,
            historical_descriptors: &historical_descriptors,
        };
        // A temporary transaction preserves the accepted-model provenance
        // needed to distinguish old unavailable bindings from new ones.
        let mut probe = CompositionTransaction::restore_authored(
            self.accepted().clone(),
            CompositionLimits::default(),
            &validator,
        )
        .map_err(ArrangementError::Composition)?;
        let edits = probe
            .apply(candidate.clone(), &validator)
            .map_err(ArrangementError::Composition)?;
        let effect = effect(edits, self.accepted(), &candidate);
        Ok(PreparedArrangement {
            base: self.accepted().clone(),
            base_revision: self.revision,
            candidate,
            effect,
        })
    }

    /// Commit only after the host has prepared the affected retained routes.
    pub(super) fn commit(
        &mut self,
        prepared: PreparedArrangement,
    ) -> Result<ArrangementEffect, ArrangementError> {
        if &prepared.base != self.accepted() || prepared.base_revision != self.revision {
            return Err(ArrangementError::StalePreview);
        }
        if prepared.candidate == prepared.base {
            return Ok(prepared.effect);
        }
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or(ArrangementError::RevisionExhausted)?;
        let (historical, historical_descriptors) = self.validation_provenance();
        let validator = Validator {
            sources: &self.sources,
            historical: &historical,
            historical_descriptors: &historical_descriptors,
        };
        let before = self.accepted().clone();
        let edits = self
            .transaction
            .apply(prepared.candidate, &validator)
            .map_err(ArrangementError::Composition)?;
        self.undo.push(before);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.revision = next_revision;
        let mut committed = prepared.effect;
        committed.edits = edits;
        Ok(committed)
    }

    pub(super) fn commit_undo(
        &mut self,
        prepared: PreparedArrangement,
    ) -> Result<ArrangementEffect, ArrangementError> {
        if &prepared.base != self.accepted() || prepared.base_revision != self.revision {
            return Err(ArrangementError::StalePreview);
        }
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or(ArrangementError::RevisionExhausted)?;
        let current = self.accepted().clone();
        if self.undo.last() != Some(&prepared.candidate) {
            return Err(ArrangementError::HistoryEmpty);
        }
        let (historical, historical_descriptors) = self.validation_provenance();
        let validator = Validator {
            sources: &self.sources,
            historical: &historical,
            historical_descriptors: &historical_descriptors,
        };
        let edits = self
            .transaction
            .apply(prepared.candidate, &validator)
            .map_err(ArrangementError::Composition)?;
        self.undo.pop();
        self.redo.push(current);
        self.revision = next_revision;
        let mut committed = prepared.effect;
        committed.edits = edits;
        Ok(committed)
    }

    pub(super) fn commit_redo(
        &mut self,
        prepared: PreparedArrangement,
    ) -> Result<ArrangementEffect, ArrangementError> {
        if &prepared.base != self.accepted() || prepared.base_revision != self.revision {
            return Err(ArrangementError::StalePreview);
        }
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or(ArrangementError::RevisionExhausted)?;
        if self.redo.last() != Some(&prepared.candidate) {
            return Err(ArrangementError::HistoryEmpty);
        }
        let (historical, historical_descriptors) = self.validation_provenance();
        let validator = Validator {
            sources: &self.sources,
            historical: &historical,
            historical_descriptors: &historical_descriptors,
        };
        let before = self.accepted().clone();
        let edits = self
            .transaction
            .apply(prepared.candidate, &validator)
            .map_err(ArrangementError::Composition)?;
        self.redo.pop();
        self.undo.push(before);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.revision = next_revision;
        let mut committed = prepared.effect;
        committed.edits = edits;
        Ok(committed)
    }

    /// Keep authored-unavailable provenance only for the accepted model and
    /// the bounded undo/redo snapshots. A component forgotten by bounded
    /// history cannot authorize a future unknown binding.
    fn validation_provenance(&self) -> (BTreeSet<BindingFingerprint>, Vec<ComponentDescriptor>) {
        let mut bindings = BTreeSet::new();
        let mut descriptors = Vec::new();
        let mut include = |model: &Composition| {
            bindings.extend(fingerprints(model));
            descriptors.extend(model.components.iter().cloned());
        };
        include(self.accepted());
        for model in &self.undo {
            include(model);
        }
        for model in &self.redo {
            include(model);
        }
        (bindings, descriptors)
    }
}

#[cfg(test)]
mod tests;
