// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use crate::panes::{PaneSource, SerializedSource, SourceRef, SourceSchemaId};
use serde::{Deserialize, Serialize};
pub const PANE_KIND: &str = "turnstone.answers";
pub const SOURCE_SCHEMA: &str = "turnstone.answers.v1";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerSourceV2 {
    pub session: uuid::Uuid,
    pub view: uuid::Uuid,
    pub chosen: Vec<crate::research::Target>,
}

#[path = "inference_surface/answer_source.rs"]
mod answer_source;
pub use answer_source::answer_source;
#[path = "inference_surface/answer_source_value.rs"]
mod answer_source_value;
pub(crate) use answer_source_value::answer_source_value;
