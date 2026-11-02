// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Admission and persistence handoff for the Settings pane's authored record.

use crate::{
    action::Effect,
    app::App,
    panes::{
        ContextBinding, PaneConfig, PaneContent, PaneId, PaneKindId, PaneSource, PaneSpec,
        PaneViewState, SourceRef,
    },
};

const SETTINGS_KIND: &str = crate::panes::kind::SETTINGS;
const CONFIG_SCHEMA: &str = "turnstone.settings.config";
const VIEW_SCHEMA: &str = "turnstone.settings.view";

impl App {
    /// Capture the current process-local admission for this exact Settings
    /// pane. It expires whenever pane context or its owning scope changes.
    pub(crate) fn settings_pane_record_admission(
        &self,
        pane: PaneId,
    ) -> Result<uuid::Uuid, String> {
        if !matches!(self.pane_content(pane), Some(PaneContent::Registered(kind)) if kind.as_str() == SETTINGS_KIND)
        {
            return Err("Pane is not a Settings pane".into());
        }
        let space = self.space_of(pane).ok_or("Settings pane is not placed")?;
        self.space(space).ok_or("Settings pane window is closed")?;
        Ok(self.pane_context.admission())
    }

    /// Read the durable Settings pane record. Legacy layouts without an
    /// explicit descriptor/view entry receive schema-versioned empty defaults.
    pub(crate) fn read_settings_pane_record(
        &self,
        pane: PaneId,
    ) -> Result<(PaneConfig, PaneViewState), String> {
        if !matches!(self.pane_content(pane), Some(PaneContent::Registered(kind)) if kind.as_str() == SETTINGS_KIND)
        {
            return Err("Pane is not a Settings pane".into());
        }
        let space = self.space_of(pane).ok_or("Settings pane is not placed")?;
        let layout = self.space(space).ok_or("Settings pane window is closed")?;
        let frame = layout.outer_workspace()?;
        let config = if let Some(spec) = self
            .blueprint_space(space)
            .and_then(|blueprint| blueprint.pane(pane))
        {
            if spec.id != pane || spec.kind.as_str() != SETTINGS_KIND {
                return Err("Settings pane descriptor identity mismatch".into());
            }
            spec.config.clone()
        } else {
            frame
                .descriptors
                .get(&pane.0)
                .map(|value| -> Result<PaneConfig, String> {
                    let spec: PaneSpec = serde_json::from_value(value.clone())
                        .map_err(|error| format!("Invalid Settings pane descriptor: {error}"))?;
                    if spec.id != pane || spec.kind.as_str() != SETTINGS_KIND {
                        return Err("Settings pane descriptor identity mismatch".into());
                    }
                    Ok(spec.config)
                })
                .transpose()?
                .unwrap_or_else(|| PaneConfig::empty(CONFIG_SCHEMA))
        };
        let view = frame
            .view_states
            .get(&pane.0)
            .map(|value| {
                serde_json::from_value(value.clone())
                    .map_err(|error| format!("Invalid Settings pane view state: {error}"))
            })
            .transpose()?
            .unwrap_or_else(|| PaneViewState::empty(VIEW_SCHEMA));
        Ok((config, view))
    }

    pub(super) fn set_settings_pane_record(
        &mut self,
        pane: PaneId,
        admission: uuid::Uuid,
        request_id: uuid::Uuid,
        config: PaneConfig,
        view: PaneViewState,
    ) -> Vec<Effect> {
        let refuse = |reason: String| {
            vec![Effect::SettingsPaneRecordResult {
                pane,
                request_id,
                result: Err(reason),
            }]
        };
        let current_admission = match self.settings_pane_record_admission(pane) {
            Ok(current) => current,
            Err(error) => return refuse(error),
        };
        if admission != current_admission {
            return refuse("Settings pane record admission is stale".into());
        }
        if let Err(error) = crate::settings_pane::validate_record(&config, &view) {
            return refuse(error);
        }
        let Some(space) = self.space_of(pane) else {
            return refuse("Settings pane is not placed".into());
        };
        if let Err(error) = self.ensure_shared_space(space) {
            return refuse(error);
        }
        {
            let Some(frame) = self
                .space_mut(space)
                .and_then(|layout| layout.workspace_mut())
            else {
                return refuse("Settings pane workspace is unavailable".into());
            };
            let mut spec = match frame.descriptors.get(&pane.0) {
                Some(value) => match serde_json::from_value::<PaneSpec>(value.clone()) {
                    Ok(spec) if spec.id == pane && spec.kind.as_str() == SETTINGS_KIND => spec,
                    Ok(_) => return refuse("Settings pane descriptor identity mismatch".into()),
                    Err(error) => {
                        return refuse(format!("Invalid Settings pane descriptor: {error}"));
                    },
                },
                None => PaneSpec {
                    id: pane,
                    kind: PaneKindId::new(SETTINGS_KIND),
                    source: PaneSource::Fixed(SourceRef::Application),
                    context: ContextBinding::Application,
                    config: PaneConfig::empty(CONFIG_SCHEMA),
                },
            };
            spec.config = config;
            let descriptor = match serde_json::to_value(spec) {
                Ok(value) => value,
                Err(error) => {
                    return refuse(format!(
                        "Could not encode Settings pane descriptor: {error}"
                    ));
                },
            };
            let view = match serde_json::to_value(view) {
                Ok(value) => value,
                Err(error) => {
                    return refuse(format!("Could not encode Settings pane view: {error}"));
                },
            };
            let mut candidate = frame.clone();
            candidate.descriptors.insert(pane.0, descriptor);
            candidate.view_states.insert(pane.0, view);
            if let Err(error) = candidate.validate() {
                return refuse(error);
            }
            *frame = candidate;
        }
        self.pane_context.invalidate();
        vec![
            Effect::PersistSettingsPaneRecord { pane, request_id },
            Effect::Redraw,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::panes;

    fn settings_pane(app: &App) -> PaneId {
        app.frisket
            .iter_leaves()
            .find_map(|(id, content, _)| {
                matches!(content, PaneContent::Registered(kind) if kind.as_str() == SETTINGS_KIND)
                    .then_some(id)
            })
            .expect("summoned Settings pane")
    }

    #[test]
    fn admitted_settings_record_round_trips_through_atomic_frame_store() {
        let root = tempfile::tempdir().unwrap();
        let mut app = App::test_stub_at(root.path().to_path_buf());
        app.update(Action::SummonPane(PaneKindId::new(SETTINGS_KIND)));
        let pane = settings_pane(&app);
        let admission = app.settings_pane_record_admission(pane).unwrap();
        let request_id = uuid::Uuid::new_v4();
        let config = PaneConfig::empty(CONFIG_SCHEMA);
        let view = PaneViewState {
            schema: crate::panes::PaneViewSchemaId::new(VIEW_SCHEMA),
            version: 1,
            payload: serde_json::json!({
                "collapsed": [],
                "scroll": {"x": 0.0, "y": 18.0},
            }),
        };
        let effects = app.update(Action::SetSettingsPaneRecord((
            pane,
            admission,
            request_id,
            config.clone(),
            view.clone(),
        )));
        assert!(effects.contains(&Effect::PersistSettingsPaneRecord { pane, request_id }));
        assert_ne!(app.settings_pane_record_admission(pane).unwrap(), admission);
        assert_eq!(
            app.read_settings_pane_record(pane).unwrap(),
            (config.clone(), view.clone())
        );

        let space = app.space_of(pane).unwrap();
        let layout = app.space(space).unwrap();
        let session_dir = app.session_dir();
        panes::store::save_frisket_layout(&session_dir, layout).unwrap();
        let restored = panes::store::load_frisket_layout(&session_dir)
            .unwrap()
            .unwrap();
        let frame = restored.outer_workspace().unwrap();
        let spec: PaneSpec = serde_json::from_value(frame.descriptors[&pane.0].clone()).unwrap();
        assert_eq!(spec.config, config);
        let restored_view: PaneViewState =
            serde_json::from_value(frame.view_states[&pane.0].clone()).unwrap();
        assert_eq!(restored_view, view);
    }

    #[test]
    fn refused_settings_view_record_does_not_mutate_the_accepted_frame() {
        let root = tempfile::tempdir().unwrap();
        let mut app = App::test_stub_at(root.path().to_path_buf());
        app.update(Action::SummonPane(PaneKindId::new(SETTINGS_KIND)));
        let pane = settings_pane(&app);
        let admission = app.settings_pane_record_admission(pane).unwrap();
        let before = app.space(app.space_of(pane).unwrap()).unwrap().clone();
        let request_id = uuid::Uuid::new_v4();
        let effects = app.update(Action::SetSettingsPaneRecord((
            pane,
            admission,
            request_id,
            PaneConfig::empty(CONFIG_SCHEMA),
            PaneViewState::empty("invalid.schema"),
        )));
        assert!(effects.iter().any(|effect| matches!(effect,
            Effect::SettingsPaneRecordResult { pane: result_pane, request_id: result_id, result: Err(_)}
                if *result_pane == pane && *result_id == request_id
        )));
        assert_eq!(app.space(app.space_of(pane).unwrap()).unwrap(), &before);
    }

    #[test]
    fn stale_settings_record_admission_refuses_without_mutating_the_frame() {
        let root = tempfile::tempdir().unwrap();
        let mut app = App::test_stub_at(root.path().to_path_buf());
        app.update(Action::SummonPane(PaneKindId::new(SETTINGS_KIND)));
        let pane = settings_pane(&app);
        let admission = app.settings_pane_record_admission(pane).unwrap();
        app.pane_context.invalidate();
        let before = app.space(app.space_of(pane).unwrap()).unwrap().clone();
        let request_id = uuid::Uuid::new_v4();
        let effects = app.update(Action::SetSettingsPaneRecord((
            pane,
            admission,
            request_id,
            PaneConfig::empty(CONFIG_SCHEMA),
            PaneViewState::empty(VIEW_SCHEMA),
        )));
        assert!(effects.iter().any(|effect| matches!(effect,
            Effect::SettingsPaneRecordResult { pane: result_pane, request_id: result_id, result: Err(reason)}
                if *result_pane == pane && *result_id == request_id && reason.contains("stale")
        )));
        assert_eq!(app.space(app.space_of(pane).unwrap()).unwrap(), &before);
    }
}
