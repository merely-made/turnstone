// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's routing vocabulary over inker's host-neutral policy.
//!
//! Inker's default policy carries only engine-shaped rules (genet rungs,
//! nematic formats, surface engines) plus the neutral hand-to-OS fallback.
//! The app-flavored, host-handled ids live here (moved out of inker with the
//! 2026-07-10 genet adoption, then out of mere's facade): internal pages and the
//! JSON-LD graph-contribution marker. Turnstone routes with [`route_policy`] instead of
//! `EngineRoutePolicy::default()`.

use inker::routing::{
    EngineRouteDecision, EngineRoutePolicy, EngineRouteRequest, EngineRouteRule, SurfaceContractMode,
};

/// Internal pages (`about:` / `graphshell:` / `mere:` addresses —
/// settings, node views). Host-handled, never dispatched to a render engine.
/// Keeps the legacy `graphshell.internal` value so existing per-node pins and
/// persisted decisions resolve.
pub const ENGINE_GRAPHSHELL_INTERNAL: &str = "graphshell.internal";

/// Marker route target for JSON-LD graph ingest (linked-data plan Phase 2).
/// **Not a render engine.** A host that receives this decision feeds the body
/// to `linked_data::from_jsonld` to produce a `GraphContribution` it merges
/// into the graph, instead of dispatching to the engine registry. Routed
/// Headless (no surface); recognize it with [`is_graph_contribution_route`].
pub const ENGINE_LINKED_DATA_INGEST: &str = "linked-data.ingest";

/// Whether a route decision's `engine_id` is the JSON-LD ingest marker
/// ([`ENGINE_LINKED_DATA_INGEST`]) rather than a render engine. A host checks
/// this before dispatching a decision to the engine registry.
pub fn is_graph_contribution_route(engine_id: &str) -> bool {
    engine_id == ENGINE_LINKED_DATA_INGEST
}

pub(crate) fn route_for_enabled_engines(
    policy: &EngineRoutePolicy,
    request: &EngineRouteRequest,
    enablement: &crate::engine_enablement::EngineEnableSet,
) -> EngineRouteDecision {
    policy.route_filtered(request, |id| enablement.is_enabled(id))
}

/// Inker's default engine policy plus the app rules: internal-page schemes
/// route to [`ENGINE_GRAPHSHELL_INTERNAL`] and `application/ld+json` routes to
/// [`ENGINE_LINKED_DATA_INGEST`], both Headless. Rule order within the policy
/// is immaterial here: scheme rules and content-type rules match disjoint
/// requests, and neither app rule overlaps an engine rule's matcher.
pub fn route_policy() -> EngineRoutePolicy {
    let mut policy = EngineRoutePolicy::default();
    policy.rules.push(EngineRouteRule::new(
        ["about", "graphshell", "mere"],
        ENGINE_GRAPHSHELL_INTERNAL,
        SurfaceContractMode::Headless,
    ));
    // JSON-LD is a graph contribution, not a render: the host feeds it to
    // linked-data ingest rather than an engine. Headless.
    policy.rules.push(EngineRouteRule::content_type(
        ["application/ld+json"],
        ENGINE_LINKED_DATA_INGEST,
        SurfaceContractMode::Headless,
    ));
    policy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(address: &str) -> inker::routing::EngineRouteRequest {
        inker::routing::EngineRouteRequest {
            workspace_id: inker::routing::WorkspaceRouteId::new("main"),
            view: None,
            node: None,
            address: address.to_string(),
            content_type: None,
            pinned_engine: None,
        }
    }

    #[test]
    fn policy_routes_internal_addresses_headless() {
        let decision = route_policy().route(&request("graphshell://settings"));

        assert_eq!(decision.engine_id, ENGINE_GRAPHSHELL_INTERNAL);
        assert_eq!(
            decision.surface_contract.mode,
            SurfaceContractMode::Headless
        );
    }

    #[test]
    fn ld_json_routes_to_graph_contribution_ingest() {
        let mut request = request("https://example.test/data");
        request.content_type = Some("application/ld+json".to_string());
        let decision = route_policy().route(&request);

        assert_eq!(decision.engine_id, ENGINE_LINKED_DATA_INGEST);
        // A graph contribution has no visible surface.
        assert_eq!(
            decision.surface_contract.mode,
            SurfaceContractMode::Headless
        );
        // And it is recognized as a non-render route the host handles itself.
        assert!(is_graph_contribution_route(&decision.engine_id));
        assert!(!is_graph_contribution_route(
            inker::routing::ENGINE_GENET_WEB
        ));
    }

    #[test]
    fn engine_rules_still_route_beneath_the_app_layer() {
        let decision = route_policy().route(&request("gemini://example.test"));
        assert_eq!(decision.engine_id, inker::routing::ENGINE_NEMATIC_GEMTEXT);
    }

    #[test]
    fn engine_setting_skips_a_disabled_pin_and_preserves_the_viewer_row() {
        use inker::routing::{ENGINE_GENET_LIVERY, ENGINE_GENET_READER};

        let mut app = crate::app::App::test_stub();
        let mut request = request("https://example.test");
        request.pinned_engine = Some(ENGINE_GENET_READER.into());
        let policy = super::super::standard_route_policy();
        assert_eq!(policy.route(&request).engine_id, ENGINE_GENET_READER);

        let settings = pandect::ApplicationSettings {
            disabled_engines: vec![
                ENGINE_GENET_READER.into(),
                ENGINE_GRAPHSHELL_INTERNAL.into(),
            ],
            ..Default::default()
        };
        app.apply_chrome_settings_snapshot(&crate::settings_pane::ChromeSettings::from(&settings));
        let route = route_for_enabled_engines(&policy, &request, &app.engine_enablement);
        assert_eq!(route.engine_id, ENGINE_GENET_LIVERY);
        assert!(app.engine_enablement.is_enabled(ENGINE_GRAPHSHELL_INTERNAL));

        let option = crate::inspector_controls::viewer_options(&app, None)
            .into_iter()
            .find(|option| option.viewer.as_deref() == Some(ENGINE_GENET_READER))
            .expect("the globally disabled reader remains in the viewer inventory");
        assert!(option.disabled_in_settings);
        assert!(!option.selectable);
    }
}
