// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{arrangement::SettingsArrangement, editor::EditorState, *};
use crate::panes::{PaneConfig, PaneId, PaneViewState};
use cambium::composition_scheduler::{
    CompositionInputError, CompositionInputQueue, CompositionInputs,
};
use cambium::{SettingRowSession, SettingRowSnapshot, composition::ComponentKey};
use mere_surface_api::settings::{
    RevisionedSettingsProvider, RevisionedSettingsSnapshot, SettingsProvider,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

pub(super) type RevisionCauseKey = (u64, u128, String, u64);

pub(super) type SettingsSource = Rc<RefCell<dyn RevisionedSettingsProvider>>;

pub(super) struct RowState {
    pub reference: SettingsRef,
    pub setting_id: String,
    pub binding: Option<SettingsBinding>,
    pub session: SettingRowSession,
    pub reviewed: Option<RevisionedSettingsSnapshot>,
}

/// Ephemeral pane ownership. Only arrangement/config and selected view fields
/// are encoded; rows, drafts, provider guards and requests never enter a save.
pub(super) struct SettingsState {
    pub provider: ApplicationSettingsProvider,
    pub owner: Option<SharedApplicationSettingsOwner>,
    pub sources: BTreeMap<String, SettingsSource>,
    pub snapshots: BTreeMap<String, RevisionedSettingsSnapshot>,
    pub source_ids: BTreeMap<String, u64>,
    pub owner_tokens: RefCell<BTreeMap<u64, (String, u128)>>,
    pub next_owner_token: Cell<Option<u128>>,
    pub revision_causes: BTreeMap<RevisionCauseKey, cambium::reactive_diagnostics::ProducerRef>,
    pub input_queue: Option<CompositionInputQueue>,
    pub input_queue_error: RefCell<Option<CompositionInputError>>,
    pub published_inputs: RefCell<CompositionInputs>,
    pub presentation_cache: RefCell<BTreeMap<ComponentKey, (SettingRowSnapshot, u64)>>,
    pub rows: BTreeMap<ComponentKey, RowState>,
    pub arrangement: SettingsArrangement,
    pub live_settings: LiveSettingsHandle,
    pub composition_lifecycle: LifecycleEventSink,
    pub editor: EditorState,
    pub pane_id: Option<PaneId>,
    pub pane_admission: Option<uuid::Uuid>,
    pub saving_record: Option<(uuid::Uuid, PaneConfig, PaneViewState)>,
    pub loaded_record: Option<(PaneConfig, PaneViewState)>,
    pub saved_record: Option<(PaneConfig, PaneViewState)>,
    pub pending_record: Option<(PaneConfig, PaneViewState)>,
    pub collapsed: std::collections::BTreeSet<cambium::composition::ContainerKey>,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub status: String,
    pub reload_epoch: u64,
    pub viewport_w: f32,
    pub viewport_h: f32,
}

impl SettingsState {
    pub(super) fn new(
        provider: ApplicationSettingsProvider,
        live_settings: LiveSettingsHandle,
        mut status: String,
    ) -> Self {
        let owner = match live_settings.owner_for(&provider) {
            Ok(owner) => Some(owner),
            Err(error) => {
                status = format!("Could not connect application settings owner: {error:?}");
                None
            },
        };
        let provider = owner
            .as_ref()
            .map_or(provider.clone(), |owner| owner.provider_snapshot());
        let specs = provider
            .describe(&SettingsRef(APPLICATION_REFERENCE.into()))
            .unwrap_or_default();
        let arrangement =
            SettingsArrangement::from_application_specs(&specs).unwrap_or_else(|error| {
                status = format!("Settings arrangement is unavailable: {error:?}");
                SettingsArrangement::from_application_specs(&[])
                    .expect("empty composition is valid")
            });
        let mut sources: BTreeMap<String, SettingsSource> = BTreeMap::new();
        if let Some(owner) = &owner {
            let source: SettingsSource = Rc::new(RefCell::new(owner.clone()));
            sources.insert(APPLICATION_REFERENCE.into(), source);
            live_settings.publish_provider(&provider);
        }
        let source_ids = sources
            .keys()
            .enumerate()
            .map(|(index, reference)| (reference.clone(), index as u64 + 1))
            .collect();
        let mut state = Self {
            provider,
            owner,
            sources,
            snapshots: BTreeMap::new(),
            source_ids,
            owner_tokens: RefCell::new(BTreeMap::new()),
            next_owner_token: Cell::new(Some(1)),
            revision_causes: BTreeMap::new(),
            input_queue: CompositionInputQueue::new().ok(),
            input_queue_error: RefCell::new(None),
            published_inputs: RefCell::new(CompositionInputs::default()),
            presentation_cache: RefCell::new(BTreeMap::new()),
            rows: BTreeMap::new(),
            arrangement,
            live_settings,
            composition_lifecycle: LifecycleEventSink::default(),
            editor: EditorState::default(),
            pane_id: None,
            pane_admission: None,
            saving_record: None,
            loaded_record: None,
            saved_record: None,
            pending_record: None,
            collapsed: Default::default(),
            scroll_x: 0.0,
            scroll_y: 0.0,
            status,
            reload_epoch: 0,
            viewport_w: 0.0,
            viewport_h: 0.0,
        };
        super::source_sync::refresh_settings(&mut state);
        state
    }

    #[cfg(test)]
    pub(super) fn replace_owner_for_test(&mut self, provider: ApplicationSettingsProvider) {
        let live = self.live_settings.clone();
        let owner = live.replace_owner_for_test(provider);
        let mut next = Self::new(owner.provider_snapshot(), live, String::new());
        next.viewport_w = self.viewport_w;
        next.viewport_h = self.viewport_h;
        next.reload_epoch = self
            .reload_epoch
            .checked_add(1)
            .expect("test reload generation");
        *self = next;
    }
}

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod scheduler_tests;

#[cfg(test)]
#[path = "owner_input_tests.rs"]
mod owner_input_tests;
