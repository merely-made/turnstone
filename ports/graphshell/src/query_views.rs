// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Query definitions are shared Resources, written through the current owner journal.
use crate::app::GraphshellApp;
use mere::query_view::{self, QueryViewSpec};
use muniment::Backend;
use uuid::Uuid;
pub fn save<B: Backend>(
    app: &mut GraphshellApp<B>,
    owner: Option<Uuid>,
    expected: Option<&QueryViewSpec>,
    nonce: Uuid,
    spec: &QueryViewSpec,
) -> Result<Uuid, String> {
    let mut candidate = app.host.graph().clone();
    let id = query_view::save_spec(&mut candidate, owner, expected, nonce, spec)?;
    app.host
        .mutate_product_graph(|graph| query_view::save_spec(graph, owner, expected, nonce, spec))
        .map_err(|e| e.to_string())??;
    Ok(id)
}
pub fn freeze<B: Backend>(
    app: &mut GraphshellApp<B>,
    owner: Uuid,
    nonce: Uuid,
) -> Result<Uuid, String> {
    let mut candidate = app.host.graph().clone();
    let id = query_view::freeze_view(&mut candidate, owner, nonce)?;
    app.host
        .mutate_product_graph(|graph| query_view::freeze_view(graph, owner, nonce))
        .map_err(|e| e.to_string())??;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, SelectedPersonaRef};
    use muniment::MemoryBackend;
    #[test]
    fn query_resource_save_freeze_undo_and_reopen_use_current_owner_journal() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let persona = SelectedPersonaRef {
                persona: FIXTURE_PERSONA_ADDRESS.into(),
                profile: "profile:graphshell-h3".into(),
            };
            let mut app = GraphshellApp::open_or_fixture(backend.clone(), persona.clone())
                .await
                .unwrap();
            let member = app
                .host
                .create_address("https://query.test/source", "source")
                .unwrap();
            let spec = QueryViewSpec::select(
                "Source query",
                "SELECT ?member WHERE { VALUES ?member { <https://query.test/source> } }",
            );
            let owner = save(&mut app, None, None, Uuid::from_u128(801), &spec).unwrap();
            let iri = app
                .host
                .graph()
                .resource(owner)
                .unwrap()
                .canonical_iri()
                .to_owned();
            let access = app.host.create_address(&iri, "Source query").unwrap();
            assert_eq!(
                query_view::read_spec(app.host.graph(), owner).unwrap(),
                spec
            );
            let projection = query_view::project_view(app.host.graph(), owner).unwrap();
            let resource = projection.members[0];
            projection
                .access_for(app.host.graph(), resource, member)
                .unwrap();
            let frozen = freeze(&mut app, owner, Uuid::from_u128(802)).unwrap();
            app.host.persist(2).await.unwrap();
            let mut reopened = GraphshellApp::open_or_fixture(backend.clone(), persona)
                .await
                .unwrap();
            assert_eq!(
                query_view::read_spec(reopened.host.graph(), owner).unwrap(),
                spec
            );
            assert!(reopened.host.graph().get_node_by_id(access).is_some());
            assert_eq!(
                reopened
                    .host
                    .graph()
                    .open_frozen_selection(frozen)
                    .unwrap()
                    .selection
                    .members(),
                &[resource]
            );
            let mut next = spec.clone();
            next.title = "Changed".into();
            save(&mut reopened, Some(owner), Some(&spec), Uuid::nil(), &next).unwrap();
            reopened
                .host
                .undo_now("graphshell")
                .unwrap()
                .expect("journaled query edit");
            assert_eq!(
                query_view::read_spec(reopened.host.graph(), owner).unwrap(),
                spec
            );
        });
    }
}

/// Explicit acceptance fixture; ordinary saved graphs never call this path.
pub async fn seed_fixture<B: Backend + Clone>(app: &mut GraphshellApp<B>) -> Result<(), String> {
    let a = app
        .host
        .create_address("https://query.example/source", "Query source")
        .map_err(|e| e.to_string())?;
    app.host
        .mutate_product_graph(|graph| {
            let key = mere::kernel::graph::apply::add_node(
                graph,
                Some(Uuid::new_v4()),
                "https://query.example/source".into(),
                Default::default(),
            );
            mere::kernel::graph::apply::apply_graph_delta(
                graph,
                mere::kernel::graph::apply::GraphDelta::SetNodeTitle {
                    key,
                    title: "Independent source access".into(),
                },
            );
        })
        .map_err(|e| e.to_string())?;
    let b = app
        .host
        .create_address("https://query.example/target", "Query target")
        .map_err(|e| e.to_string())?;
    app.host
        .mutate_product_graph(|graph| -> Result<(), String> {
            use mere::kernel::graph::apply::{GraphDelta, apply_graph_delta};
            use mere::kernel::graph::resource_tags::TagConcept;
            use mere::kernel::graph::{EdgeAssertion, SemanticSubKind};
            let from = graph.get_node_key_by_id(a).ok_or("Missing source")?;
            let to = graph.get_node_key_by_id(b).ok_or("Missing target")?;
            let resource = graph
                .shown_resource_id(from)
                .ok_or("Missing shown Resource")?;
            for (owner, iri) in [
                ("urn:author:alice", "urn:tag:alice"),
                ("urn:author:bob", "urn:tag:bob"),
            ] {
                graph
                    .tag_resource_with_concept(
                        resource,
                        iri,
                        TagConcept {
                            owner_iri: owner.into(),
                            label: "Music".into(),
                        },
                    )
                    .map_err(|e| e.to_string())?;
            }
            for kind in [SemanticSubKind::Cites, SemanticSubKind::UserGrouped] {
                apply_graph_delta(
                    graph,
                    GraphDelta::AssertRelation {
                        from,
                        to,
                        assertion: EdgeAssertion::Semantic {
                            sub_kind: kind,
                            label: Some("query fixture".into()),
                            decay_progress: None,
                        },
                        asserter_iri: "urn:author:fixture".into(),
                    },
                );
            }
            Ok(())
        })
        .map_err(|e| e.to_string())??;
    app.host.persist(7).await.map_err(|e| e.to_string())?;
    Ok(())
}
