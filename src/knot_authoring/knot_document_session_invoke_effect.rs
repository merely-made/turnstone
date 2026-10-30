// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl KnotDocumentSession {
    pub(super) fn invoke_effect(&mut self, kind: KnotEffectKind) {
        let available = match kind {
            KnotEffectKind::Resolve => self.runner.state().resolve_available,
            KnotEffectKind::Run => self.runner.state().run_available,
        };
        if !available
            || matches!(
                self.runner.state().status,
                AuthoringStatus::Saving
                    | AuthoringStatus::Resolving
                    | AuthoringStatus::Running
                    | AuthoringStatus::Reloading
                    | AuthoringStatus::Revoked
                    | AuthoringStatus::Unavailable
            )
            || self.runner.state().editor.is_dirty()
        {
            return;
        }
        if self
            .hub
            .commands
            .send(HubCommand::Effect {
                registration: self.registration,
                kind,
                confirmed: true,
            })
            .is_ok()
        {
            self.runner.update(|state| {
                state.last_intent = None;
                state.status = match kind {
                    KnotEffectKind::Resolve => AuthoringStatus::Resolving,
                    KnotEffectKind::Run => AuthoringStatus::Running,
                };
                state.detail.clear();
            });
        } else {
            self.runner.update(|state| {
                state.status = AuthoringStatus::Rejected;
                state.detail = "Knot worker stopped".into();
            });
        }
    }

    pub(super) fn drain_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            if let HubEvent::Remote(binding)
            | HubEvent::Reloaded(binding)
            | HubEvent::Saved { binding, .. } = &event
            {
                self.clip_available =
                    binding.clip_action.payload_schema == KNOT_CLIP_INSERT_SCHEMA_V2;
            }
            match event {
                HubEvent::Remote(binding) => {
                    self.revision_refreshes += 1;
                    let target = binding.target;
                    self.projection_input = Some(knot_projection_input_binding(&binding.editable));
                    if binding.editable.base_token == self.base_token {
                        let resolve_available = binding.resolve_action.is_some();
                        let run_available = binding.run_action.is_some();
                        let derived = binding.editable.derived;
                        if !self.runner.state().editor.is_dirty() {
                            self.runner.update(|state| {
                                state.projection_target = target;
                                state.resolve_available = resolve_available;
                                state.run_available = run_available;
                                state.derived = derived;
                                state.status = AuthoringStatus::Current;
                                state.detail.clear();
                            });
                        } else {
                            self.runner.update(|state| {
                                state.projection_target = target;
                                state.resolve_available = resolve_available;
                                state.run_available = run_available;
                            });
                        }
                        continue;
                    }
                    if self.runner.state().editor.is_dirty() {
                        self.runner.update(|state| {
                            state.projection_target = target;
                            state.status = AuthoringStatus::Stale;
                            state.detail = "the endpoint has a newer document".into();
                        });
                    } else {
                        self.base_token = binding.editable.base_token;
                        let source = binding.editable.source;
                        let derived = binding.editable.derived;
                        let resolve_available = binding.resolve_action.is_some();
                        let run_available = binding.run_action.is_some();
                        let address = self.address.clone();
                        self.runner.update(|state| {
                            state.projection_target = target;
                            state.editor = KnotEditor::scratch(address, source);
                            state.derived = derived;
                            state.resolve_available = resolve_available;
                            state.run_available = run_available;
                            state.status = AuthoringStatus::Current;
                            state.detail.clear();
                        });
                    }
                },
                HubEvent::Intent(receipt) => self.runner.update(|state| {
                    state.last_intent = Some(receipt);
                }),
                HubEvent::Reloaded(binding) => {
                    let target = binding.target;
                    self.projection_input = Some(knot_projection_input_binding(&binding.editable));
                    self.base_token = binding.editable.base_token;
                    let source = binding.editable.source;
                    let derived = binding.editable.derived;
                    let resolve_available = binding.resolve_action.is_some();
                    let run_available = binding.run_action.is_some();
                    let address = self.address.clone();
                    self.runner.update(|state| {
                        state.projection_target = target;
                        state.editor = KnotEditor::scratch(address, source);
                        state.derived = derived;
                        state.resolve_available = resolve_available;
                        state.run_available = run_available;
                        state.status = AuthoringStatus::Current;
                        state.detail.clear();
                    });
                },
                HubEvent::Saved { source, binding } => {
                    let target = binding.target;
                    self.projection_input = Some(knot_projection_input_binding(&binding.editable));
                    self.base_token = binding.editable.base_token;
                    let derived = binding.editable.derived;
                    let resolve_available = binding.resolve_action.is_some();
                    let run_available = binding.run_action.is_some();
                    self.runner.update(|state| {
                        state.projection_target = target;
                        state.editor.accept_saved_source(&source);
                        state.derived = derived;
                        state.resolve_available = resolve_available;
                        state.run_available = run_available;
                        state.status = AuthoringStatus::Current;
                        state.detail.clear();
                    });
                },
                HubEvent::Stale => self.runner.update(|state| {
                    state.status = AuthoringStatus::Stale;
                    state.detail = "the endpoint refused an old base token".into();
                }),
                HubEvent::Rejected(reason) => self.runner.update(|state| {
                    state.status = AuthoringStatus::Rejected;
                    state.detail = reason;
                }),
                HubEvent::Unavailable(reason) => {
                    // The editor is left exactly as it stands. Clearing it
                    // would erase the last thing the holder actually said,
                    // which is the only copy a visitor has any claim to.
                    self.base_token.clear();
                    self.projection_input = None;
                    self.runner.update(|state| {
                        state.resolve_available = false;
                        state.run_available = false;
                        state.status = AuthoringStatus::Unavailable;
                        state.detail = reason;
                    });
                },
                HubEvent::Revoked(reason) => {
                    self.base_token.clear();
                    self.projection_input = None;
                    let address = self.address.clone();
                    self.runner.update(|state| {
                        state.editor = KnotEditor::scratch(address, String::new());
                        state.derived = None;
                        state.resolve_available = false;
                        state.run_available = false;
                        state.status = AuthoringStatus::Revoked;
                        state.detail = reason;
                    });
                },
            }
        }
    }
}
