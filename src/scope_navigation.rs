// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Scope routes are view intent. Reading, graph truth and pane arrangement keep
//! their owners; history records accepted projections, never synthetic parents.
use crate::{
    action::{Action, Effect},
    app::App,
    panes::{GraphId, PaneContent, PaneId},
    reading_view::ReadTarget,
};
use cambium::scope_navigation::{
    NodeActivation, ScopeCamera, ScopeControls, ScopeDirection, ScopeVisit,
};
pub use mere::navigation::browser::{ScopeContext, ScopeDestination, ScopeLevel, ScopePlace};
pub type Destination = ScopeDestination<GraphId, Uuid>;
pub type ScopeCommand = cambium::scope_navigation::ScopeCommand<Destination>;
pub type ScopeCrumb = cambium::scope_navigation::ScopeCrumb<Destination>;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ScopeBookmark {
    pub reader: Option<PaneId>,
    pub map: Option<PaneId>,
    pub reading: Option<tabard::ReadingAnchor>,
}

pub type Navigation =
    cambium::scope_navigation::ScopeNavigation<ScopePlace<GraphId, Uuid>, ScopeBookmark>;
type Visit = cambium::scope_navigation::ScopeVisit<ScopePlace<GraphId, Uuid>, ScopeBookmark>;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScopeState {
    pub navigation: Navigation,
    pub reader: Option<PaneId>,
    #[serde(skip)]
    pub hint: Option<ScopeDirection>,
    #[serde(skip)]
    pub restoring: bool,
    #[serde(skip)]
    pub pending: Option<(Visit, Option<bool>)>,
    #[serde(skip)]
    pub activation: std::collections::BTreeMap<PaneId, NodeActivation<(Uuid, Uuid)>>,
    #[serde(skip)]
    pub read_request: Option<(GraphId, Uuid, Uuid)>,
    #[serde(skip)]
    pub editing_location: bool,
}

#[cfg(test)]
mod tests;

mod presentation;

#[path = "scope_navigation/app_apply_scope_visit.rs"]
mod app_apply_scope_visit;
#[path = "scope_navigation/app_scope_place.rs"]
mod app_scope_place;
#[path = "scope_navigation/impl_de_deserialize_de_for_scope.rs"]
mod impl_de_deserialize_de_for_scope;
#[path = "scope_navigation/scope_bookmark_validate.rs"]
mod scope_bookmark_validate;
