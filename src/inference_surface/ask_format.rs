// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn ask_format(state: &mut AnswerState, evidence: bool) {
    if !state.snapshot.ready {
        return;
    }
    if state.source.chosen.is_empty() {
        state.local_status = "Choose at least one saved source before asking.".into();
        return;
    }
    let question = state.question.text().to_owned();
    let result = if evidence {
        state
            .service
            .ask_with_evidence(question, state.source.chosen.clone())
    } else {
        state.service.ask(question, state.source.chosen.clone())
    };
    match result {
        Ok(run) => {
            state.active = Some(run);
            state.question = TextInput::default();
            state.local_status = "Sending question…".into();
        },
        Err(error) => state.local_status = error,
    }
}
