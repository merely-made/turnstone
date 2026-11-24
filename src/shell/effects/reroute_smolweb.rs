// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(in crate::shell) fn reroute_disabled_pack_parsers(&mut self, disabled: &[&str]) {
        let affected: Vec<_> = self
            .app
            .content
            .live_nodes()
            .filter_map(|node| {
                let engine = &self.app.content.facts(node)?.engine;
                if !disabled.contains(&engine.as_str()) {
                    return None;
                }
                Some((node, crate::engine_pack::builtin_parser(engine)?))
            })
            .collect();

        for (node, fallback) in affected {
            if let Err(error) = self.rebuild_pack_page(node, fallback) {
                tracing::warn!(%node, %error, "could not rebuild the open Smolweb page");
                self.live_settings.report_status(Some(format!(
                    "Could not rebuild open Smolweb page: {error}"
                )));
            }
        }
    }

    fn rebuild_pack_page(&mut self, node: uuid::Uuid, fallback: &str) -> Result<(), String> {
        let (_, member) = self
            .app
            .graph_runtimes
            .graph_containing_member(node)
            .and_then(|graph| self.app.graph_runtimes.canvas(graph))
            .and_then(|canvas| canvas.graph().get_node_by_id(node))
            .ok_or("the page is no longer open")?;
        let url = member.url().to_owned();
        let fetched = self
            .app
            .content
            .fetched(node, &url)
            .cloned()
            .ok_or("the response is no longer held")?;
        let request = inker::EngineRouteRequest {
            workspace_id: inker::WorkspaceRouteId::new("turnstone"),
            view: None,
            node: None,
            address: url.clone(),
            content_type: fetched.content_type.clone(),
            pinned_engine: None,
        };
        let decision = self.route_policy.route_filtered(&request, |id| {
            id == fallback
                && self.app.engine_enablement.is_enabled(id)
                && self.content_engines.contains(id)
        });
        if decision.engine_id != fallback {
            return Err("there is no enabled built-in parser for the held response".into());
        }

        let prepared = prepare_content_controller(
            &self.app,
            node,
            &url,
            fallback,
            Some(fetched),
            None,
            (self.width.max(1), self.height.max(1)),
        )?
        .ok_or("the response could not be prepared")?;
        let opened = open_prepared_content_controller(
            &self.app,
            prepared,
            self.content_engines.clone(),
            self.pelt_surface_engines.clone(),
            super::super::ShellClock(self.epoch),
        )?
        .ok_or("the replacement could not be opened")?;

        // Acquire the replacement before retiring the old producer. The
        // saved viewer pin and host-held response belong to the node.
        self.app.content.note_requested(node);
        self.run_effects(vec![Effect::CloseContent { node }]);
        let route = pelt_core::PeltRoute {
            decision,
            source: pelt_core::PeltRouteSource::Automatic,
            state: pelt_core::PeltRouteState::Document,
        };
        let update = self.install_document_controller(node, &url, opened, route, false);
        let effects = self.app.apply_update(update);
        self.run_effects(effects);
        Ok(())
    }
}
