// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl inker::DocumentSession<netrender::Scene> for OverlapDocument {
    fn frame(&mut self, _width: u32, _height: u32) -> netrender::Scene {
        Default::default()
    }
    fn scroll_by(&mut self, _dx: f32, _dy: f32) -> bool {
        true
    }
    fn scroll_for_key(&mut self, _key: inker::SessionScrollKey) -> bool {
        true
    }
    fn click_at(&mut self, _x: f32, _y: f32) -> inker::SessionClick {
        inker::SessionClick::Miss
    }
    fn links(&self) -> Vec<inker::SessionLink> {
        Vec::new()
    }
    fn input(&mut self, input: inker::SessionInput) -> inker::SessionInputResult {
        use inker::{SessionButtonState, SessionEffect, SessionInput};
        self.inputs.lock().unwrap().push(input.clone());
        let effect = match input {
            SessionInput::PointerButton {
                state: SessionButtonState::Released,
                ..
            } if self.submit => SessionEffect::Submit(inker::SessionFormSubmission {
                action: "post".into(),
                method: inker::SessionFormMethod::Post,
                fields: vec![("producer".into(), "upper".into())],
            }),
            SessionInput::PointerButton {
                state: SessionButtonState::Released,
                ..
            } => SessionEffect::Navigate("next".into()),
            _ => SessionEffect::Handled,
        };
        inker::SessionInputResult {
            effect,
            cursor: None,
            capture: Some(true),
            editable: false,
        }
    }
    fn as_any_ref(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
