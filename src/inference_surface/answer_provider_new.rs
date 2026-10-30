// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl AnswerProvider {
    pub(crate) fn new(
        owner: Arc<dyn RetainedSourceOwner>,
        wake: armillary::Wake,
        bridge: crate::research::Bridge,
    ) -> Self {
        Self {
            kind: PaneKindId::new(PANE_KIND),
            schema: SourceSchemaId::new(SOURCE_SCHEMA),
            descriptor: SurfaceDescriptor {
                provider_id: ProviderId::from("turnstone"),
                surface_id: SurfaceId::from(SOURCE_SCHEMA),
                label: "Answers".into(),
                accepted_source: SurfaceSourceShape::One(SourceKindId::from(SOURCE_SCHEMA)),
            },
            owner,
            wake,
            connections: Default::default(),
            drafts: Default::default(),
            bridge,
        }
    }
}
