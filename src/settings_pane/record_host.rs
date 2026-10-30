// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{arrangement::SettingsArrangement, *};
use crate::panes::{PaneConfig, PaneId, PaneViewState};
use cambium::composition::{ContainerKey, ContainerKind};

impl SettingsPane {
    pub(crate) fn sync_for_pane(
        &mut self,
        app: &crate::app::App,
        pane: PaneId,
        width: f32,
        height: f32,
    ) {
        let admission = app.settings_pane_record_admission(pane);
        self.runner.update_deferred(|state| {
            state.pane_id = Some(pane);
            state.pane_admission = admission.ok();
        });
        match app.read_settings_pane_record(pane) {
            Ok((config, view)) => {
                if let Err(error) = self.adopt_record(config, view, false) {
                    self.runner.update(|state| state.editor.notice = error);
                }
            },
            Err(error) => self.runner.update(|state| state.editor.notice = error),
        }
        self.sync(width, height);
    }

    pub(super) fn adopt_record(
        &mut self,
        config: PaneConfig,
        view: PaneViewState,
        force: bool,
    ) -> Result<(), String> {
        if self.runner.state().loaded_record.as_ref() == Some(&(config.clone(), view.clone()))
            && !force
        {
            return Ok(());
        }
        validate_record(&config, &view)?;
        let same_requested_save = self.runner.state().saving_record.as_ref().is_some_and(
            |(_, saved_config, saved_view)| saved_config == &config && saved_view == &view,
        );
        if same_requested_save {
            self.runner.update_deferred(|state| {
                state.loaded_record = Some((config, view));
                if state.editor.save_request.is_none() {
                    state.saving_record = None;
                }
            });
            return Ok(());
        }
        let dirty = self.runner.state().rows.values().any(|row| {
            let s = row.session.snapshot();
            s.dirty || s.pending_generation.is_some()
        });
        if !force
            && self.runner.state().loaded_record.is_some()
            && (dirty || self.runner.state().editor.unsaved)
        {
            self.runner
                .update_deferred(|state| state.pending_record = Some((config, view)));
            return Err("The saved arrangement changed. Your edits were retained; resolve them before reloading.".into());
        }
        if let Ok((current_config, current_view)) = self.current_record()
            && current_config == config
            && current_view == view
        {
            self.runner.update_deferred(|state| {
                state.loaded_record = Some((config.clone(), view.clone()));
                if state.saved_record.is_none() {
                    state.saved_record = Some((config, view));
                }
            });
            return Ok(());
        }
        let specs = self
            .runner
            .state()
            .snapshots
            .get(APPLICATION_REFERENCE)
            .map(|snapshot| {
                snapshot
                    .specs
                    .iter()
                    .map(|entry| entry.spec.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let next = SettingsArrangement::restore(
            &config,
            &specs,
            source_sync::registered_specs(self.runner.state()),
        )
        .map_err(|error| format!("Could not restore arrangement: {error:?}"))?;
        let (collapsed, x, y) = decode_view(&view)?;
        let keys = self.runner.state().rows.keys().cloned().collect();
        self.prepare_keys(&keys, true)?;
        if self.runner.state().rows.values().any(|row| {
            let s = row.session.snapshot();
            s.dirty || s.pending_generation.is_some()
        }) {
            self.runner
                .update_deferred(|state| state.pending_record = Some((config, view)));
            return Err(
                "An edit was committed during move preparation. Resolve it before reloading."
                    .into(),
            );
        }
        let canonical_config = next.encode_config().map_err(|error| format!("{error:?}"))?;
        let canonical_view = arrangement::encode_view_state(next.accepted(), &collapsed, x, y)?;
        self.runner.update(|state| {
            state.arrangement = next;
            state.collapsed = collapsed;
            state.scroll_x = x;
            state.scroll_y = y;
            state.loaded_record = Some((config, view));
            state.saved_record = Some((canonical_config, canonical_view));
            state.pending_record = None;
            state.editor.unsaved = false;
            state.editor.confirmation = None;
            source_sync::reconcile_rows(state, &Default::default());
        });
        let current = self.scroll.offset();
        self.scroll
            .nudge(x as f32 - current.0, y as f32 - current.1);
        Ok(())
    }

    pub(super) fn current_record(&self) -> Result<(PaneConfig, PaneViewState), String> {
        let state = self.runner.state();
        let config = state
            .arrangement
            .encode_config()
            .map_err(|error| format!("{error:?}"))?;
        let offset = self.scroll.offset();
        let view = arrangement::encode_view_state(
            state.arrangement.accepted(),
            &state.collapsed,
            f64::from(offset.0.max(0.0)).min(10_000_000.0),
            f64::from(offset.1.max(0.0)).min(10_000_000.0),
        )?;
        Ok((config, view))
    }

    pub(super) fn recompute_unsaved(&mut self) {
        let current = self.current_record();
        self.runner.update_deferred(|state| {
            state.editor.unsaved = current
                .as_ref()
                .ok()
                .is_none_or(|record| state.saved_record.as_ref() != Some(record));
        });
    }

    pub(super) fn save_arrangement(&mut self) -> Result<(), String> {
        if self.runner.state().editor.save_request.is_some() {
            return Err("An arrangement save is already pending.".into());
        }
        if self.runner.state().pending_record.is_some() {
            return Err("Review the changed saved arrangement before saving over it.".into());
        }
        let (config, view) = self.current_record()?;
        validate_record(&config, &view)?;
        let pane =
            self.runner.state().pane_id.ok_or(
                "This pane is not connected to a session; its arrangement cannot be saved.",
            )?;
        let admission = self
            .runner
            .state()
            .pane_admission
            .ok_or("The pane's session admission expired. Refresh before saving.")?;
        let request = uuid::Uuid::new_v4();
        self.runner.update(|state| {
            state.editor.save_request = Some(request);
            state.saving_record = Some((request, config.clone(), view.clone()));
            state.editor.notice = "Saving arrangement…".into();
            state
                .live_settings
                .request_action(crate::action::Action::SetSettingsPaneRecord((
                    pane, admission, request, config, view,
                )));
        });
        Ok(())
    }

    pub(crate) fn arrangement_saved(&mut self, request: uuid::Uuid, result: Result<(), String>) {
        if self.runner.state().editor.save_request != Some(request) {
            return;
        }
        self.runner.update(|state| {
            state.editor.save_request = None;
            match result {
                Ok(()) => {
                    if let Some((expected, config, view)) = state.saving_record.take()
                        && expected == request
                    {
                        state.loaded_record = Some((config.clone(), view.clone()));
                        state.saved_record = Some((config, view));
                    }
                    state.editor.notice = "Arrangement saved.".into();
                },
                Err(error) => {
                    // The reducer may have accepted this exact record in
                    // memory before persistence failed. Keep its bounded
                    // submission identity until sync sees that echo, so it
                    // cannot be mistaken for an outside edit and block Retry.
                    state.editor.notice = format!("Could not save arrangement: {error}");
                },
            }
        });
        self.recompute_unsaved();
    }

    pub(super) fn reload_saved_arrangement(&mut self) -> Result<(), String> {
        let record = self
            .runner
            .state()
            .pending_record
            .clone()
            .or_else(|| self.runner.state().saved_record.clone())
            .ok_or("No saved arrangement is available.")?;
        self.adopt_record(record.0, record.1, true)
    }
}

fn decode_view(
    view: &PaneViewState,
) -> Result<(std::collections::BTreeSet<ContainerKey>, f64, f64), String> {
    if view.payload.is_null() {
        return Ok((Default::default(), 0.0, 0.0));
    }
    let collapsed = view
        .payload
        .get("collapsed")
        .and_then(serde_json::Value::as_array)
        .ok_or("Invalid collapsed state.")?
        .iter()
        .map(|entry| {
            let kind = match entry.get("kind").and_then(serde_json::Value::as_str) {
                Some("group") => ContainerKind::Group,
                Some("drawer") => ContainerKind::Drawer,
                _ => return Err("Invalid container kind."),
            };
            let key = entry
                .get("key")
                .and_then(serde_json::Value::as_str)
                .ok_or("Missing container key.")?;
            Ok(ContainerKey {
                kind,
                key: key.into(),
            })
        })
        .collect::<Result<_, _>>()
        .map_err(str::to_owned)?;
    let scroll = view.payload.get("scroll").ok_or("Missing scroll state.")?;
    Ok((
        collapsed,
        scroll
            .get("x")
            .and_then(serde_json::Value::as_f64)
            .ok_or("Invalid scroll x.")?,
        scroll
            .get("y")
            .and_then(serde_json::Value::as_f64)
            .ok_or("Invalid scroll y.")?,
    ))
}
