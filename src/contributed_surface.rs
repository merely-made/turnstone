// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Provider-neutral retained surfaces contributed to Turnstone panes.
//!
//! Products own source decoding and the concrete Cambium runner. Turnstone
//! owns admission, retained layout, viewport scrolling, hit testing, and the
//! small amount of event routing needed to get from a pane coordinate to a
//! retained session.

use std::collections::HashMap;
use std::fmt;

use cambium::{
    DomHandle, GenetAppRunner, GenetCtx, GenetElement, HoverEvent, HoverPhase, KeyEvent,
    PointerClick, PointerEvent, PointerPhase, ResolvedSurfaceEvent, RetainedSurfaceSession,
    RunnerSurfaceSession, SurfaceEffect, SurfaceViewport, View, WheelEvent, el,
};
use genet_scripted_dom::{NodeId, ScriptedDom};
use mere_surface_api::{
    SurfaceAvailability, SurfaceDescriptor, SurfaceId, SurfaceSourceShape, SurfaceUnavailableReason,
};

use crate::panes::{PaneId, PaneKindId, PaneSource, PaneSpec, SourceRef, SourceSchemaId};
use crate::ui::{PaneScroll, RetainedLayout};

const UNAVAILABLE_SURFACE_CSS: &str = ".contributed-surface-unavailable { padding: 12px; }";

/// The result of applying effects requested by a retained session.
///
/// `Redraw` is the current retained-surface effect and changes the host's
/// immediate frame request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SurfaceRequest {
    #[default]
    None,
    Redraw,
}

/// Why a provider could not be admitted for a pane/source pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceAdmissionError {
    /// The process-local admission identity space is exhausted. Never wrap and
    /// accidentally authorize a target from an older retained session.
    GenerationExhausted,
    /// No registered provider accepts the requested pane kind and source.
    ProviderNotFound {
        pane_kind: PaneKindId,
        source_schema: Option<SourceSchemaId>,
    },
    /// The source is not of the schema published by the provider.
    InvalidSource {
        expected: SourceSchemaId,
        actual: Option<SourceSchemaId>,
    },
    /// The source names the right schema but its versioned payload is invalid.
    InvalidPayload {
        schema: SourceSchemaId,
        message: String,
    },
    /// The product had the right identity but cannot currently supply a
    /// session. Availability stays typed and inspectable on admitted sessions.
    Unavailable { reason: SurfaceUnavailableReason },
}

/// Why a provider registration was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceRegistrationError {
    /// A duplicate provider identity.
    Duplicate {
        pane_kind: PaneKindId,
        surface_id: SurfaceId,
    },
    /// The descriptor's declared source kind and the provider's admission
    /// schema disagree. The descriptor is the stated admission truth, so a
    /// divergence is a provider bug surfaced at registration rather than a
    /// silent second truth consulted at admission.
    SourceShapeMismatch {
        surface_id: SurfaceId,
        declared: Option<String>,
        schema: SourceSchemaId,
    },
}

/// A product-owned source-to-session factory.
///
/// The trait is object-safe on purpose: Turnstone keeps providers erased and
/// does not need a concrete product map or runner state type.
pub trait SurfaceProvider {
    fn pane_kind(&self) -> &PaneKindId;

    fn source_schema(&self) -> &SourceSchemaId;

    fn descriptor(&self) -> &SurfaceDescriptor;

    fn stylesheet(&self) -> &str;

    fn admit(
        &self,
        source: &PaneSource,
        dom: DomHandle,
    ) -> Result<Box<dyn RetainedSurfaceSession>, SurfaceAdmissionError>;
}

/// Registry of product contributions. The registry has no concrete session
/// knowledge. A pane kind names exactly one route; descriptor identity is the
/// provider/surface pair published outside Turnstone.
#[derive(Default)]
pub struct SurfaceProviderRegistry {
    providers: Vec<Box<dyn SurfaceProvider>>,
}

struct UnavailableSurfaceState {
    label: String,
    reason: SurfaceUnavailableReason,
}

/// A single admitted retained surface projected into a Turnstone pane.
pub struct ContributedSurfacePane {
    generation: u64,
    pane_kind: PaneKindId,
    source: PaneSource,
    /// The admitted session's retained DOM, kept beside the erased session so
    /// host diagnostics and semantic automation can borrow it without
    /// downcasting the provider-owned state.
    dom: DomHandle,
    session: Box<dyn RetainedSurfaceSession>,
    layout: RetainedLayout,
    scroll: PaneScroll,
    stylesheet: String,
    viewport: (u32, u32, f32),
    hover: Option<NodeId>,
    mouse_gesture: Option<(NodeId, (f32, f32), bool)>,
    click_slop: f32,
}

/// All contributed product sessions retained by pane identity.
///
/// Re-resolving the same pane and source preserves product state. Re-pinning a
/// pane replaces the old session before the next frame, so a retained product
/// cannot silently keep authority over its previous source.
#[derive(Default)]
pub struct ContributedSurfaceSessions {
    next_generation: u64,
    panes: HashMap<PaneId, ContributedSurfacePane>,
}

#[cfg(test)]
#[path = "contributed_surface/tests.rs"]
pub(crate) mod tests;

#[path = "contributed_surface/fresh_dom.rs"]
mod fresh_dom;
#[path = "contributed_surface/impl_fmt_display_for_surface_admission.rs"]
mod impl_fmt_display_for_surface_admission;
#[path = "contributed_surface/impl_fmt_display_for_surface_registration.rs"]
mod impl_fmt_display_for_surface_registration;
#[path = "contributed_surface/impl_std_error_error_for_surface.rs"]
mod impl_std_error_error_for_surface;
#[path = "contributed_surface/impl_std_error_error_for_surface_2.rs"]
mod impl_std_error_error_for_surface_2;
#[path = "contributed_surface/surface_provider_registry_new.rs"]
mod surface_provider_registry_new;
use fresh_dom::fresh_dom;
#[path = "contributed_surface/source_schema.rs"]
mod source_schema;
use source_schema::source_schema;
#[path = "contributed_surface/source_matches.rs"]
mod source_matches;
use source_matches::source_matches;
#[path = "contributed_surface/unavailable_surface_view.rs"]
mod unavailable_surface_view;
use unavailable_surface_view::unavailable_surface_view;
#[path = "contributed_surface/unavailable_surface_session.rs"]
mod unavailable_surface_session;
use unavailable_surface_session::unavailable_surface_session;
#[path = "contributed_surface/contributed_surface_pane_new.rs"]
mod contributed_surface_pane_new;
#[path = "contributed_surface/contributed_surface_sessions_resolve.rs"]
mod contributed_surface_sessions_resolve;
