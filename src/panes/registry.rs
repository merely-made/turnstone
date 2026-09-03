// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Built-in pane registration data.
//!
//! This is the A1 replacement authority for the distributed `PaneKind` label,
//! source, multiplicity, capability, and renderer tables. Call sites still use
//! the legacy enum during the migration; new pane facts belong here first.

use super::{PaneContent, PaneKindId, PaneMultiplicity, PaneSource, SourceRef, SourceSelector};

pub mod kind {
    pub const GRAPH: &str = "turnstone.graph";
    pub const WORKBENCH: &str = "turnstone.workbench";
    pub const TILE: &str = "turnstone.tile";
    pub const GLOSS: &str = "turnstone.gloss";
    pub const ROSTER: &str = "turnstone.roster";
    pub const INSPECTOR: &str = "turnstone.inspector";
    pub const APPARATUS: &str = "turnstone.apparatus";
    pub const TRAIL: &str = "turnstone.trail";
    pub const ALEMBIC: &str = "turnstone.alembic";
    pub const STEWARD: &str = "turnstone.steward";
    pub const COMMS: &str = "turnstone.comms";
    pub const OVERMAP: &str = "turnstone.overmap";
    pub const PUBLISHING: &str = "turnstone.publishing";
    pub const SHARED_KNOT: &str = "turnstone.shared-knot";
    pub const DEVICE_RECEIPTS: &str = "turnstone.device-receipts";
    pub const FROZEN_PROJECTION: &str = "turnstone.frozen-projection";
    pub const SETTINGS: &str = "turnstone.settings";
    pub const ARRANGE: &str = "turnstone.arrange";
    pub const TRANSCRIPT: &str = "turnstone.transcript";
}

pub mod source_schema {
    pub const PERSONA_MEMORY: &str = "turnstone.persona-memory";
    pub const PLACE: &str = "turnstone.place";
    pub const PUBLISHING_TARGET: &str = "turnstone.publishing-target";
    pub const SHARE_TICKET: &str = "turnstone.share-ticket";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixedSourceKind {
    Graph,
    Forme,
    Member,
    Settings,
    Session,
    SessionSet,
    Application,
    External(&'static str),
}

/// Sources a pane kind accepts. Context selectors are separate from fixed
/// source kinds because following is a policy on an instance, not a source id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneSourceShape {
    pub fixed: &'static [FixedSourceKind],
    pub contextual: &'static [SourceSelector],
}

impl PaneSourceShape {
    pub const fn fixed(fixed: &'static [FixedSourceKind]) -> Self {
        Self {
            fixed,
            contextual: &[],
        }
    }

    pub const fn contextual(
        fixed: &'static [FixedSourceKind],
        contextual: &'static [SourceSelector],
    ) -> Self {
        Self { fixed, contextual }
    }

    pub fn accepts(self, source: &PaneSource) -> bool {
        match source {
            PaneSource::FromContext(selector) => self.contextual.contains(selector),
            PaneSource::Fixed(source) => self.fixed.iter().any(|kind| match (kind, source) {
                (FixedSourceKind::Graph, SourceRef::Graph(_))
                | (FixedSourceKind::Forme, SourceRef::Forme { .. })
                | (FixedSourceKind::Member, SourceRef::Member { .. })
                | (FixedSourceKind::Settings, SourceRef::Settings(_))
                | (FixedSourceKind::Session, SourceRef::Session(_))
                | (FixedSourceKind::SessionSet, SourceRef::SessionSet)
                | (FixedSourceKind::Application, SourceRef::Application) => true,
                (FixedSourceKind::External(expected), SourceRef::External { schema, .. }) => {
                    schema.as_str() == *expected
                }
                _ => false,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PublishedContext {
    pub graph: bool,
    pub forme: bool,
    pub member: bool,
    pub session: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaneCapabilities {
    pub pointer: bool,
    pub keyboard: bool,
    pub composed_sections: bool,
    pub mixed_surfaces: bool,
    pub publishes: PublishedContext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementPolicy {
    BesideFocused,
    FloatNearFocused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewPersistencePolicy {
    RuntimeOnly,
    Blueprint,
}

/// The shell-owned factory key. A definition selects one renderer; retained
/// instances are keyed by `PaneId` rather than by this value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneRenderer {
    Graph,
    Workbench,
    Document,
    Gloss,
    Roster,
    Inspector,
    Apparatus,
    Trail,
    Alembic,
    Steward,
    Comms,
    Overmap,
    Publishing,
    SharedKnot,
    DeviceReceipts,
    FrozenProjection,
    Settings,
    Transcript,
    Arrange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanePaletteEntry {
    pub order: u16,
    pub label: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct PaneDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub source_shape: PaneSourceShape,
    pub multiplicity: PaneMultiplicity,
    pub capabilities: PaneCapabilities,
    pub default_placement: PlacementPolicy,
    pub config_schema: &'static str,
    pub view_schema: &'static str,
    pub view_persistence: ViewPersistencePolicy,
    pub renderer: PaneRenderer,
    pub palette: Option<PanePaletteEntry>,
    content_factory: Option<fn() -> PaneContent>,
}

impl PaneDefinition {
    pub fn kind_id(self) -> PaneKindId {
        PaneKindId::new(self.id)
    }

    pub fn legacy_content(self) -> Option<PaneContent> {
        self.content_factory.map(|factory| factory())
    }
}

const GRAPH_CONTEXT: &[SourceSelector] = &[SourceSelector::Graph];
const MEMBER_CONTEXT: &[SourceSelector] = &[SourceSelector::Member];
const GRAPH_SESSION_CONTEXT: &[SourceSelector] = &[SourceSelector::Graph, SourceSelector::Session];

const fn capabilities(
    composed_sections: bool,
    mixed_surfaces: bool,
    publishes: PublishedContext,
) -> PaneCapabilities {
    PaneCapabilities {
        pointer: true,
        keyboard: true,
        composed_sections,
        mixed_surfaces,
        publishes,
    }
}

const NONE: PublishedContext = PublishedContext {
    graph: false,
    forme: false,
    member: false,
    session: false,
};
const GRAPH: PublishedContext = PublishedContext {
    graph: true,
    forme: true,
    member: true,
    session: false,
};
const MEMBER: PublishedContext = PublishedContext {
    graph: true,
    forme: false,
    member: true,
    session: false,
};
const SESSION: PublishedContext = PublishedContext {
    graph: false,
    forme: false,
    member: false,
    session: true,
};

macro_rules! pane {
    ($id:expr, $name:expr, $source:expr, $many:expr, $caps:expr, $config:expr, $view:expr, $renderer:expr, $palette:expr, $content:expr) => {
        PaneDefinition {
            id: $id,
            display_name: $name,
            source_shape: $source,
            multiplicity: $many,
            capabilities: $caps,
            default_placement: PlacementPolicy::BesideFocused,
            config_schema: $config,
            view_schema: $view,
            view_persistence: ViewPersistencePolicy::Blueprint,
            renderer: $renderer,
            palette: $palette,
            content_factory: $content,
        }
    };
}

/// The complete built-in pane vocabulary. `System` and layout placeholders are
/// deliberately absent. Shared Knot is registered explicitly instead of
/// extending the legacy `Custom(String)` convention.
pub static BUILTIN_PANES: &[PaneDefinition] = &[
    pane!(
        kind::GRAPH,
        "Orrery",
        PaneSourceShape::fixed(&[FixedSourceKind::Forme]),
        PaneMultiplicity::Many,
        capabilities(false, true, GRAPH),
        "turnstone.graph.config",
        "turnstone.graph.view",
        PaneRenderer::Graph,
        None,
        Some(|| PaneContent::Orrery)
    ),
    pane!(
        kind::WORKBENCH,
        "Workbench",
        PaneSourceShape::fixed(&[FixedSourceKind::Forme]),
        PaneMultiplicity::Many,
        capabilities(false, true, GRAPH),
        "turnstone.workbench.config",
        "turnstone.workbench.view",
        PaneRenderer::Workbench,
        Some(PanePaletteEntry {
            order: 50,
            label: "Open Workbench pane"
        }),
        Some(|| PaneContent::Workbench)
    ),
    pane!(
        kind::TILE,
        "Tile",
        PaneSourceShape::fixed(&[FixedSourceKind::Member]),
        PaneMultiplicity::Many,
        capabilities(false, true, MEMBER),
        "turnstone.tile.config",
        "turnstone.tile.view",
        PaneRenderer::Document,
        None,
        None
    ),
    pane!(
        kind::GLOSS,
        "Gloss",
        PaneSourceShape::contextual(&[FixedSourceKind::Graph], GRAPH_CONTEXT),
        PaneMultiplicity::Many,
        capabilities(true, false, GRAPH),
        "turnstone.gloss.config",
        "turnstone.gloss.view",
        PaneRenderer::Gloss,
        Some(PanePaletteEntry {
            order: 30,
            label: "Open Gloss pane"
        }),
        Some(|| PaneContent::Gloss(Default::default()))
    ),
    pane!(
        kind::ROSTER,
        "Roster",
        PaneSourceShape::contextual(&[FixedSourceKind::Graph], GRAPH_CONTEXT),
        PaneMultiplicity::PerSpaceAndContext,
        capabilities(false, false, GRAPH),
        "turnstone.roster.config",
        "turnstone.roster.view",
        PaneRenderer::Roster,
        Some(PanePaletteEntry {
            order: 10,
            label: "Open Roster pane"
        }),
        Some(|| PaneContent::Roster)
    ),
    pane!(
        kind::INSPECTOR,
        "Inspector",
        PaneSourceShape::contextual(&[FixedSourceKind::Member], MEMBER_CONTEXT),
        PaneMultiplicity::PerSpaceAndContext,
        capabilities(false, false, MEMBER),
        "turnstone.inspector.config",
        "turnstone.inspector.view",
        PaneRenderer::Inspector,
        Some(PanePaletteEntry {
            order: 40,
            label: "Open Inspector pane"
        }),
        Some(|| PaneContent::Inspector)
    ),
    pane!(
        kind::APPARATUS,
        "Apparatus",
        PaneSourceShape::contextual(&[FixedSourceKind::Member], MEMBER_CONTEXT),
        PaneMultiplicity::PerSpaceAndContext,
        capabilities(false, false, MEMBER),
        "turnstone.apparatus.config",
        "turnstone.apparatus.view",
        PaneRenderer::Apparatus,
        Some(PanePaletteEntry {
            order: 60,
            label: "Open Apparatus pane"
        }),
        Some(|| PaneContent::Apparatus)
    ),
    pane!(
        kind::TRAIL,
        "Trail",
        PaneSourceShape::contextual(
            &[FixedSourceKind::Graph, FixedSourceKind::Session],
            GRAPH_SESSION_CONTEXT
        ),
        PaneMultiplicity::PerSpaceAndContext,
        capabilities(false, false, GRAPH),
        "turnstone.trail.config",
        "turnstone.trail.view",
        PaneRenderer::Trail,
        Some(PanePaletteEntry {
            order: 20,
            label: "Open Trail pane"
        }),
        Some(|| PaneContent::Trail)
    ),
    pane!(
        kind::ALEMBIC,
        "Alembic",
        PaneSourceShape::fixed(&[
            FixedSourceKind::Application,
            FixedSourceKind::External(source_schema::PERSONA_MEMORY)
        ]),
        PaneMultiplicity::PerSpaceAndSource,
        capabilities(false, false, NONE),
        "turnstone.alembic.config",
        "turnstone.alembic.view",
        PaneRenderer::Alembic,
        None,
        Some(|| PaneContent::Alembic)
    ),
    pane!(
        kind::STEWARD,
        "Steward",
        PaneSourceShape::fixed(&[FixedSourceKind::Application]),
        PaneMultiplicity::PerSpace,
        capabilities(false, false, NONE),
        "turnstone.steward.config",
        "turnstone.steward.view",
        PaneRenderer::Steward,
        Some(PanePaletteEntry {
            order: 35,
            label: "Open Steward pane"
        }),
        Some(|| PaneContent::Steward)
    ),
    pane!(
        kind::COMMS,
        "Comms",
        PaneSourceShape::fixed(&[
            FixedSourceKind::Session,
            FixedSourceKind::External(source_schema::PLACE)
        ]),
        PaneMultiplicity::PerSpaceAndSource,
        capabilities(false, false, SESSION),
        "turnstone.comms.config",
        "turnstone.comms.view",
        PaneRenderer::Comms,
        None,
        Some(|| PaneContent::Comms)
    ),
    pane!(
        kind::OVERMAP,
        "Overmap",
        PaneSourceShape::fixed(&[FixedSourceKind::SessionSet]),
        PaneMultiplicity::PerSpace,
        capabilities(true, false, SESSION),
        "turnstone.overmap.config",
        "turnstone.overmap.view",
        PaneRenderer::Overmap,
        Some(PanePaletteEntry {
            order: 70,
            label: "Open Overmap pane"
        }),
        Some(|| PaneContent::Overmap(Default::default()))
    ),
    pane!(
        kind::PUBLISHING,
        "Publishing",
        PaneSourceShape::fixed(&[FixedSourceKind::External(source_schema::PUBLISHING_TARGET)]),
        PaneMultiplicity::Many,
        capabilities(false, false, NONE),
        "turnstone.publishing.config",
        "turnstone.publishing.view",
        PaneRenderer::Publishing,
        Some(PanePaletteEntry {
            order: 90,
            label: "Open Publishing pane"
        }),
        Some(|| PaneContent::Registered(PaneKindId::new(kind::PUBLISHING)))
    ),
    pane!(
        kind::SHARED_KNOT,
        "Shared Knot",
        PaneSourceShape::fixed(&[FixedSourceKind::External(source_schema::SHARE_TICKET)]),
        PaneMultiplicity::Many,
        capabilities(false, false, NONE),
        "turnstone.shared-knot.config",
        "turnstone.shared-knot.view",
        PaneRenderer::SharedKnot,
        Some(PanePaletteEntry {
            order: 100,
            label: "Open Shared Knot pane"
        }),
        Some(|| PaneContent::Registered(PaneKindId::new(kind::SHARED_KNOT)))
    ),
    pane!(
        kind::DEVICE_RECEIPTS,
        "Device Receipts",
        PaneSourceShape::fixed(&[FixedSourceKind::Application]),
        // One device, one receipts view: `Many` let every summon split the
        // active pane and add another copy of identical content, because the
        // dedupe in `summon_pane` deliberately skips `Many` kinds. Seven of
        // them were reachable in one window. `PerSpace` matches Steward, the
        // only other `Application`-sourced pane, for the same reason.
        PaneMultiplicity::PerSpace,
        capabilities(false, false, NONE),
        "turnstone.device-receipts.config",
        "turnstone.device-receipts.view",
        PaneRenderer::DeviceReceipts,
        Some(PanePaletteEntry {
            order: 105,
            label: "Open Device Receipts pane"
        }),
        Some(|| PaneContent::Registered(PaneKindId::new(kind::DEVICE_RECEIPTS)))
    ),
    pane!(
        kind::FROZEN_PROJECTION,
        "Frozen Projection",
        PaneSourceShape::fixed(&[FixedSourceKind::Application]),
        // One reading of one disclosed scene per space, same reasoning as
        // Device Receipts: `Many` would let every summon add an identical copy.
        PaneMultiplicity::PerSpace,
        capabilities(false, false, NONE),
        "turnstone.frozen-projection.config",
        "turnstone.frozen-projection.view",
        PaneRenderer::FrozenProjection,
        Some(PanePaletteEntry {
            order: 106,
            label: "Open Frozen Projection pane"
        }),
        Some(|| PaneContent::Registered(PaneKindId::new(kind::FROZEN_PROJECTION)))
    ),
    pane!(
        kind::SETTINGS,
        "Settings",
        PaneSourceShape::fixed(&[FixedSourceKind::Settings]),
        PaneMultiplicity::PerSpaceAndSource,
        capabilities(false, false, NONE),
        "turnstone.settings.config",
        "turnstone.settings.view",
        PaneRenderer::Settings,
        Some(PanePaletteEntry {
            order: 80,
            label: "Open Settings pane"
        }),
        Some(|| PaneContent::Registered(PaneKindId::new(kind::SETTINGS)))
    ),
    pane!(
        kind::ARRANGE,
        "Arrange",
        PaneSourceShape::fixed(&[FixedSourceKind::Graph]),
        PaneMultiplicity::PerSpaceAndSource,
        capabilities(false, false, GRAPH),
        "turnstone.arrange.config",
        "turnstone.arrange.view",
        PaneRenderer::Arrange,
        Some(PanePaletteEntry {
            order: 82,
            label: "Open Arrange pane"
        }),
        Some(|| PaneContent::Registered(PaneKindId::new(kind::ARRANGE)))
    ),
    pane!(
        kind::TRANSCRIPT,
        "Transcript",
        PaneSourceShape::fixed(&[FixedSourceKind::Settings]),
        PaneMultiplicity::PerSpaceAndSource,
        capabilities(false, false, NONE),
        "turnstone.transcript.config",
        "turnstone.transcript.view",
        PaneRenderer::Transcript,
        Some(PanePaletteEntry {
            order: 85,
            label: "Open Transcript pane"
        }),
        Some(|| PaneContent::Registered(PaneKindId::new(kind::TRANSCRIPT)))
    ),
];

pub fn pane_definition(id: &str) -> Option<&'static PaneDefinition> {
    BUILTIN_PANES.iter().find(|definition| definition.id == id)
}

pub fn pane_palette_entries() -> Vec<(&'static str, PaneKindId)> {
    let mut panes: Vec<_> = BUILTIN_PANES
        .iter()
        .filter_map(|definition| {
            definition
                .palette
                .map(|entry| (entry.order, entry.label, definition.kind_id()))
        })
        .collect();
    panes.sort_by_key(|(order, _, _)| *order);
    panes
        .into_iter()
        .map(|(_, label, kind)| (label, kind))
        .collect()
}

/// Transitional projection into the legacy leaf payload. Registration is the
/// authority; A2 removes this adapter when leaves carry `PaneSpec` directly.
pub fn legacy_pane_content(id: &PaneKindId) -> Option<PaneContent> {
    pane_definition(id.as_str())?.legacy_content()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn built_in_ids_and_schemas_are_unique_and_namespaced() {
        let mut ids = HashSet::new();
        let mut configs = HashSet::new();
        let mut views = HashSet::new();
        for pane in BUILTIN_PANES {
            assert!(pane.id.starts_with("turnstone."));
            assert!(pane.config_schema.starts_with("turnstone."));
            assert!(pane.view_schema.starts_with("turnstone."));
            assert!(ids.insert(pane.id));
            assert!(configs.insert(pane.config_schema));
            assert!(views.insert(pane.view_schema));
        }
    }

    #[test]
    fn publishing_workflows_are_typed_registrations() {
        let publishing = pane_definition(kind::PUBLISHING).unwrap();
        let shared = pane_definition(kind::SHARED_KNOT).unwrap();
        assert_eq!(publishing.renderer, PaneRenderer::Publishing);
        assert_eq!(shared.renderer, PaneRenderer::SharedKnot);
        assert_eq!(publishing.multiplicity, PaneMultiplicity::Many);
        assert_eq!(shared.multiplicity, PaneMultiplicity::Many);
        assert_eq!(
            shared.source_shape.fixed,
            &[FixedSourceKind::External(source_schema::SHARE_TICKET)]
        );
    }

    #[test]
    fn registration_is_the_legacy_content_factory() {
        assert_eq!(
            legacy_pane_content(&PaneKindId::new(kind::PUBLISHING)),
            Some(PaneContent::Registered(PaneKindId::new(kind::PUBLISHING)))
        );
        assert_eq!(
            legacy_pane_content(&PaneKindId::new(kind::SHARED_KNOT)),
            Some(PaneContent::Registered(PaneKindId::new(kind::SHARED_KNOT)))
        );
        assert_eq!(legacy_pane_content(&PaneKindId::new(kind::TILE)), None);
    }

    #[test]
    fn retired_system_and_placeholder_are_not_registered() {
        assert!(pane_definition("turnstone.system").is_none());
        assert!(pane_definition("__placeholder__").is_none());
    }

    #[test]
    fn source_shapes_validate_fixed_and_following_sources() {
        let roster = pane_definition(kind::ROSTER).unwrap();
        assert!(
            roster
                .source_shape
                .accepts(&PaneSource::FromContext(SourceSelector::Graph))
        );
        assert!(
            !roster
                .source_shape
                .accepts(&PaneSource::FromContext(SourceSelector::Member))
        );

        let shared = pane_definition(kind::SHARED_KNOT).unwrap();
        let source = |schema| {
            PaneSource::Fixed(SourceRef::External {
                schema: super::super::SourceSchemaId::new(schema),
                payload: super::super::SerializedSource {
                    version: 1,
                    payload: serde_json::Value::Null,
                },
            })
        };
        assert!(
            shared
                .source_shape
                .accepts(&source(source_schema::SHARE_TICKET))
        );
        assert!(
            !shared
                .source_shape
                .accepts(&source("turnstone.some-other-ticket"))
        );
    }
}
