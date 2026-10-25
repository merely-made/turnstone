// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Graphshell's opt-in graph browser policy and generic Cambium mechanics.
pub use cambium::scope_navigation::{ScopeCamera, ScopeControls, ScopeDirection};
pub use mere::navigation::browser::{ScopeContext, ScopeDestination, ScopeLevel, ScopePlace};
pub type Place = ScopePlace<uuid::Uuid, uuid::Uuid>;
pub type Destination = ScopeDestination<uuid::Uuid, uuid::Uuid>;
pub type ScopeCommand = cambium::scope_navigation::ScopeCommand<Destination>;
pub type ScopeCrumb = cambium::scope_navigation::ScopeCrumb<Destination>;
pub type ScopeVisit<A> = cambium::scope_navigation::ScopeVisit<Place, A>;
pub type ScopeNavigation<A> = cambium::scope_navigation::ScopeNavigation<Place, A>;
