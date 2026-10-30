// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl KnotDocumentSession {
    pub(super) fn pointer_event(
        &self,
        node: genet_scripted_dom::NodeId,
        phase: PointerPhase,
        x: f32,
        y: f32,
    ) -> PointerEvent {
        let (left, top, width, height) = self
            .layout
            .painted_rect(&self.dom.borrow(), node)
            .unwrap_or_default();
        PointerEvent::new(phase, (x - left, y - top), (width, height))
    }

    pub(super) fn drag_selection_to(&mut self, x: f32, y: f32, extend: bool) -> bool {
        let Some(node) = self.text_drag else {
            return false;
        };
        if self.runner.focus() != Some(node) {
            self.text_drag = None;
            return false;
        }
        let Some(focus) = self
            .layout
            .text_caret_at(&mut self.dom.borrow_mut(), node, x, y)
        else {
            return false;
        };
        let previous = self.runner.state().editor.selection();
        let selection = cambium::CaretSelection {
            anchor: if extend { previous.anchor } else { focus },
            focus,
        };
        if previous == selection {
            return false;
        }
        self.runner.update(|state| {
            state
                .editor
                .apply(cambium::TextCommand::SetSelection(selection));
        });
        true
    }

    pub(super) fn cancel_pointer(&mut self) -> bool {
        let had_drag = self.text_drag.take().is_some();
        let capture = self.runner.pointer_capture();
        if let Some(node) = capture {
            let event = self.pointer_event(node, PointerPhase::Cancel, 0.0, 0.0);
            self.runner.dispatch_pointer_cancel(event);
        }
        tracing::debug!(target: "turnstone::knot_pointer", had_drag, ?capture, "cancel pointer");
        had_drag || capture.is_some()
    }

    pub(super) fn run_authoring_requests(&mut self) -> Option<String> {
        // One drain, in the order the toolbar was pressed. Taking the whole
        // queue in a single `update` also costs one rebuild rather than one
        // per verb.
        let requests = if self.runner.state().requests.is_empty() {
            Vec::new()
        } else {
            let mut taken = Vec::new();
            self.runner
                .update(|state| taken = std::mem::take(&mut state.requests));
            taken
        };
        let mut navigation = None;
        for request in requests {
            match request {
                AuthoringRequest::OpenSavedCitation {
                    address,
                    expected_source,
                } => {
                    let source = self.runner.state().editor.source();
                    if blake3::hash(source.as_bytes()) == expected_source
                        && crate::knot_saved_citations::controls(source).is_ok_and(|controls| {
                            controls.iter().any(|(_, target)| target == &address)
                        })
                    {
                        navigation = Some(address);
                        continue;
                    }
                    self.runner.update(|state| {
                        state.detail =
                            "Saved citation changed; use the current passage control.".into();
                        state.status = AuthoringStatus::Rejected;
                    });
                },
                AuthoringRequest::Save => self.save(),
                AuthoringRequest::Resolve => self.invoke_effect(KnotEffectKind::Resolve),
                AuthoringRequest::Run => self.invoke_effect(KnotEffectKind::Run),
                AuthoringRequest::Reload => self.reload(),
            }
        }
        navigation
    }

    pub(super) fn new(
        hub: Arc<KnotHub>,
        opened: OpenedDocument,
        viewport: (u32, u32),
        auto_resolve: bool,
        auto_run: bool,
        visiting: bool,
    ) -> Self {
        let address = opened.binding.editable.address.clone();
        let base_token = opened.binding.editable.base_token.clone();
        let projection_input = knot_projection_input_binding(&opened.binding.editable);
        let resolve_available = opened.binding.resolve_action.is_some();
        let run_available = opened.binding.run_action.is_some();
        let dom: DomHandle = Rc::new(std::cell::RefCell::new(ScriptedDom::new()));
        let state = AuthoringState {
            editor: KnotEditor::scratch(&address, opened.binding.editable.source),
            projection_target: opened.binding.target,
            last_intent: None,
            status: AuthoringStatus::Current,
            detail: String::new(),
            requests: Vec::new(),
            resolve_available,
            run_available,
            derived: opened.binding.editable.derived,
            width: viewport.0.max(1),
            height: viewport.1.max(1),
        };
        let runner = AuthoringRunner::new(
            dom.clone(),
            authoring_view as fn(&AuthoringState) -> AuthoringView,
            state,
        );
        let session = Self {
            clip_available: opened.binding.clip_action.payload_schema == KNOT_CLIP_INSERT_SCHEMA_V2,
            hub,
            registration: opened.registration,
            address,
            visiting,
            base_token,
            projection_input: Some(projection_input),
            events: opened.events,
            revision_refreshes: 0,
            dom,
            runner,
            layout: crate::ui::RetainedLayout::new(),
            text_drag: None,
        };
        if auto_resolve && resolve_available {
            let _ = session.hub.commands.send(HubCommand::Effect {
                registration: session.registration,
                kind: KnotEffectKind::Resolve,
                confirmed: false,
            });
        }
        if auto_run && run_available {
            let _ = session.hub.commands.send(HubCommand::Effect {
                registration: session.registration,
                kind: KnotEffectKind::Run,
                confirmed: false,
            });
        }
        session
    }

    pub fn dispatch_key(&mut self, event: KeyEvent) -> bool {
        self.drain_events();
        if matches!(&event.key, Key::Character(value) if event.mods.ctrl && value.eq_ignore_ascii_case("s"))
        {
            self.save();
            return true;
        }
        if self.runner.focus().is_none() {
            return false;
        }
        let before = self.runner.state().editor.source().to_string();
        self.runner.dispatch_key(event);
        if self.runner.state().editor.source() != before {
            self.runner.update(|state| state.derived = None);
        }
        true
    }

    /// The retained document surface exposed to Genet Probe. The borrow stays
    /// inside `Automatable::with_surfaces`, matching Turnstone's pane DOMs.
    pub(crate) fn research_clip_handle(&self) -> Result<KnotClipHandle, String> {
        if self.visiting || !self.clip_available {
            return Err("This note does not offer local citation retention.".into());
        }
        if self.runner.state().editor.is_dirty() {
            return Err("Save note edits before clipping a passage.".into());
        }
        if !matches!(self.runner.state().status, AuthoringStatus::Current) {
            return Err("The note is not currently writable; reopen or reload it.".into());
        }
        Ok(KnotClipHandle {
            expected_base: Some(self.base_token.clone()),
            hub: self.hub.clone(),
            target: self.address.clone(),
            status: Arc::new(Mutex::new(KnotClipStatus::Ready)),
        })
    }

    pub(crate) fn retained_selection(&self) -> Option<crate::producer_retention::KnotSelection> {
        let editor = &self.runner.state().editor;
        if self.visiting
            || editor.is_dirty()
            || editor.input().composition().is_some()
            || self.runner.state().status != AuthoringStatus::Current
        {
            return None;
        }
        let selection = editor.selection();
        let start = selection.anchor.byte.min(selection.focus.byte);
        let end = selection.anchor.byte.max(selection.focus.byte);
        if start == end || editor.source().get(start..end).is_none() {
            return None;
        }
        Some(crate::producer_retention::KnotSelection {
            source_hash: blake3::hash(editor.source().as_bytes())
                .to_hex()
                .to_string(),
            start,
            end,
        })
    }

    pub(crate) fn selected_retained_export(
        &mut self,
        expected: &crate::producer_retention::KnotSelection,
    ) -> Result<(serde_json::Value, Vec<u8>), String> {
        self.drain_events();
        if self.retained_selection().as_ref() != Some(expected) {
            return Err("The saved Knot source or selection changed; select it again".into());
        }
        crate::producer_retention::export_knot_selection(
            &self.address,
            self.runner.state().editor.source(),
            expected,
        )
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    /// The holder's Personae root, when this document is place-held.
    pub fn holder(&self) -> Option<String> {
        parse_place_held_address(&self.address).map(|(root, _)| crate::place::hex32(&root))
    }

    /// Where this document is held and whether it can be reached: `local`,
    /// `visiting`, or `unavailable`. A refusal never reaches a session, so it
    /// is not one of these.
    pub fn hold_status(&self) -> &'static str {
        if matches!(self.runner.state().status, AuthoringStatus::Unavailable) {
            "unavailable"
        } else if self.visiting {
            "visiting"
        } else {
            "local"
        }
    }

    /// Blake3 hex over the derived source text this surface currently holds.
    ///
    /// The derived text when the endpoint disclosed one, the editor's own
    /// source otherwise; a local and a visited window that agree on the
    /// document agree on this string.
    pub fn derived_digest(&self) -> String {
        let state = self.runner.state();
        let source = state
            .derived
            .as_ref()
            .map(|derived| derived.source.as_str())
            .unwrap_or_else(|| state.editor.source());
        blake3::hash(source.as_bytes()).to_hex().to_string()
    }

    pub fn dom_ref(&self) -> std::cell::Ref<'_, ScriptedDom> {
        self.dom.borrow()
    }

    pub(crate) fn selector_point(
        &self,
        selector: &taproot::Selector,
    ) -> Result<Option<(f32, f32)>, &'static str> {
        if !selector.matches_surface("knot") {
            return Ok(None);
        }
        self.layout.selector_point(&self.dom.borrow(), selector)
    }

    pub fn status(&mut self) -> &'static str {
        self.drain_events();
        match self.runner.state().status {
            AuthoringStatus::Current if self.runner.state().editor.is_dirty() => "unsaved",
            AuthoringStatus::Current => "saved",
            AuthoringStatus::Saving => "saving",
            AuthoringStatus::Resolving => "resolving",
            AuthoringStatus::Running => "running",
            AuthoringStatus::Reloading => "reloading",
            AuthoringStatus::Stale => "stale",
            AuthoringStatus::Rejected => "rejected",
            AuthoringStatus::Revoked => "revoked",
            AuthoringStatus::Unavailable => "unavailable",
        }
    }

    pub(super) fn save(&mut self) {
        if matches!(
            self.runner.state().status,
            AuthoringStatus::Saving
                | AuthoringStatus::Resolving
                | AuthoringStatus::Running
                | AuthoringStatus::Reloading
                | AuthoringStatus::Revoked
                | AuthoringStatus::Unavailable
        ) || !self.runner.state().editor.is_dirty()
        {
            return;
        }
        let source = self.runner.state().editor.source().to_string();
        let command = HubCommand::Save {
            registration: self.registration,
            base_token: self.base_token.clone(),
            source,
        };
        if self.hub.commands.send(command).is_ok() {
            self.runner.update(|state| {
                state.status = AuthoringStatus::Saving;
                state.detail.clear();
            });
        } else {
            self.runner.update(|state| {
                state.status = AuthoringStatus::Rejected;
                state.detail = "Knot worker stopped".into();
            });
        }
    }

    pub(super) fn reload(&mut self) {
        if matches!(
            self.runner.state().status,
            AuthoringStatus::Saving
                | AuthoringStatus::Resolving
                | AuthoringStatus::Running
                | AuthoringStatus::Reloading
                | AuthoringStatus::Revoked
                | AuthoringStatus::Unavailable
        ) {
            return;
        }
        if self
            .hub
            .commands
            .send(HubCommand::Reload {
                registration: self.registration,
            })
            .is_ok()
        {
            self.runner.update(|state| {
                state.status = AuthoringStatus::Reloading;
                state.detail.clear();
            });
        } else {
            self.runner.update(|state| {
                state.status = AuthoringStatus::Rejected;
                state.detail = "Knot worker stopped".into();
            });
        }
    }

    /// The current Knot input in Scenograph's durable binding vocabulary.
    ///
    /// A synchronized vault resource carries its authority-issued operation
    /// head as a [`PublicSourceRevision`]. A file or other source without one
    /// instead reports `RuntimeVerified`, and is bindable only through
    /// [`Self::runtime_projection_source`].
    pub fn projection_input_binding(&self) -> Option<&ProjectionInputBinding> {
        self.projection_input.as_ref()
    }

    /// Return the live-only Scenograph source binding for an unversioned Knot
    /// resource.
    ///
    /// The returned witness retains the save token privately. It cannot be
    /// produced for a synchronized vault, whose public operation head belongs
    /// in durable provenance instead.
    pub fn runtime_projection_source(&self) -> Option<RuntimeSourceBinding<KnotRuntimeWitness>> {
        self.projection_input
            .as_ref()
            .filter(|binding| binding.revision_evidence == RevisionEvidence::RuntimeVerified)
            .map(|binding| {
                knot_runtime_source_binding(binding.source.clone(), self.base_token.clone())
            })
    }
}
