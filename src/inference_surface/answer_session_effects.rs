// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl AnswerSession {
    pub(super) fn effects(&mut self) -> Vec<SurfaceEffect> {
        let state = self.runner.state();
        *state.draft.borrow_mut() = Draft {
            question: state.question.clone(),
            active: state.active.clone(),
        };
        let mut effects = vec![SurfaceEffect::Redraw];
        if let Some(exit) = self.runner.take_focus_exit() {
            effects.push(SurfaceEffect::FocusExit(exit));
        }
        effects
    }
}
