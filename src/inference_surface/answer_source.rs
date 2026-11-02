// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn answer_source(session: uuid::Uuid) -> PaneSource {
    answer_source_value(AnswerSourceV2 {
        session,
        view: uuid::Uuid::new_v4(),
        chosen: Vec::new(),
    })
}
