// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{arrangement::SettingsArrangement, editor::EditorState, *};
use crate::panes::{PaneConfig, PaneId, PaneViewState};
use cambium::{SettingRowSession, composition::ComponentKey};
use mere_surface_api::settings::{
    RevisionedSettingsProvider, RevisionedSettingsSnapshot, SettingsProvider,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

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
        let mut sources = BTreeMap::new();
        if let Some(owner) = &owner {
            let source: SettingsSource = Rc::new(RefCell::new(owner.clone()));
            sources.insert(APPLICATION_REFERENCE.into(), source);
            live_settings.publish_provider(&provider);
        }
        let mut state = Self {
            provider,
            owner,
            sources,
            snapshots: BTreeMap::new(),
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
