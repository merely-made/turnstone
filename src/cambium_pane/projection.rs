// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::collections::BTreeMap;

use cambium::projection_table::{ProjectionTable, TableViewers};
use sceno::{Frame, Size2, SourceRef, Vec2};
use scenograph::dataset::{
    ProjectionDataset, ProjectionFieldType, ProjectionOccurrence, ProjectionValue,
};
use scenograph::{Channel, FieldSpec, ProjectionDraft, SourceBinding};
use scenomise::projection::{CompileIssue, ItemSizes, ProjectionCompiler, TABLE_REALIZATION_ID};

use super::{RosterGraphSource, RosterGridRow, RosterState, TABLE_TAB};

#[derive(Default)]
pub(super) struct RosterProjection {
    source: Option<RosterGraphSource>,
    column_width: u32,
    pub(super) table: Option<ProjectionTable>,
}

impl RosterState {
    pub(super) fn sync_projection(&mut self) {
        let projection = &mut self.projection;
        if projection.source.map(|source| source.id) != self.source.map(|source| source.id) {
            *projection = RosterProjection::default();
        }
        if self.tabs.selected != TABLE_TAB {
            return;
        }
        let Some(source) = self.source else {
            return;
        };
        let column_width = (self.viewport_w / 2.0).floor().max(1.0) as u32;
        if projection.source != self.source || projection.column_width != column_width {
            let sort = projection
                .table
                .as_ref()
                .map(|table| table.draft().reading.sort.clone())
                .unwrap_or_default();
            projection.source = self.source;
            projection.column_width = column_width;
            projection.table = match open(source, column_width, &self.rows) {
                Ok(mut table) => {
                    if !sort.is_empty() {
                        table.apply_sort(sort, 0);
                    }
                    Some(table)
                },
                Err(issues) => {
                    tracing::warn!(?issues, "could not open the Roster's graph table");
                    None
                },
            };
        }
        if let Some(table) = &mut projection.table {
            // Leave space for the source control, tabs, caption, and status.
            let height = (self.viewport_h - 110.0 - table.header_height()).max(0.0);
            if table.viewport_height != height {
                table.viewport_height = height;
                table.scroll_to(table.scroll);
            }
        }
    }
}

fn open(
    graph: RosterGraphSource,
    column_width: u32,
    rows: &[RosterGridRow],
) -> Result<ProjectionTable, Vec<CompileIssue>> {
    let graph_id = graph.id.as_uuid().to_string();
    let source = SourceBinding {
        authority: "turnstone".into(),
        domain: "graph".into(),
        resource: graph_id.clone(),
    };
    let dataset = ProjectionDataset {
        source: source.clone(),
        revision: format!("{}:{}", graph.revision, graph.content_revision).into(),
        fields: BTreeMap::from([
            ("member".into(), ProjectionFieldType::Text),
            ("title".into(), ProjectionFieldType::Text),
            ("address".into(), ProjectionFieldType::Text),
        ]),
        occurrences: rows
            .iter()
            .map(|row| {
                let member = row.member.to_string();
                ProjectionOccurrence::new(
                    member.clone(),
                    SourceRef::new(&graph_id, &member),
                    BTreeMap::from([
                        ("member".into(), ProjectionValue::Text(member)),
                        ("title".into(), ProjectionValue::Text(row.title.clone())),
                        ("address".into(), ProjectionValue::Text(row.url.clone())),
                    ]),
                )
            })
            .collect(),
    };
    let mut draft = ProjectionDraft::new();
    draft.id = "roster-table".into();
    draft.label = "Graph nodes".into();
    draft.source = source;
    draft.reading.key = "member".into();
    draft.encoding.x = Channel::Field("member".into());
    draft.encoding.y = Channel::Field("member".into());
    draft.encoding.label = Some(Channel::Field("title".into()));
    draft.arrangement.kind = "table".into();
    draft.arrangement.direction = "coordinates".into();
    draft.arrangement.options = BTreeMap::from([
        ("column_width".into(), column_width.to_string()),
        ("row_height".into(), "26".into()),
        ("header".into(), "30".into()),
    ]);
    draft.frame = Some(Frame::Flow {
        origin: Vec2::ZERO,
        width: 2.0 * column_width as f32,
    });
    draft.appearance.realization = TABLE_REALIZATION_ID.into();
    draft.appearance.title = "Graph nodes".into();
    draft.appearance.theme = "slate".into();
    draft.interaction.pan = false;
    draft.interaction.zoom = false;
    draft.provenance.author = "Turnstone".into();
    draft.provenance.source_revision = Some(dataset.revision.clone());
    draft.fields = vec![
        FieldSpec::new("title", "Title"),
        FieldSpec::new("address", "Address"),
    ];
    ProjectionTable::open(
        draft,
        dataset,
        ProjectionCompiler::new(ItemSizes {
            card: Size2::new(column_width as f32, 26.0),
        }),
        TableViewers::built_in(),
    )
}
