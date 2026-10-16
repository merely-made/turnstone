// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The H1 reference fixture: literal demo truth for [`MereHost`].
//!
//! Data only. It sits beside `mere_host` rather than in it because none of it
//! is host behaviour — every line is a node, a relation, a facet, or an access
//! record that the demo and the tests read back, and keeping it here leaves
//! the host module about what the host does.

use mere::kernel::geometry::PortablePoint;
use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta, assert_relation};
use mere::kernel::graph::{
    ArrangementSubKind, ContainmentSubKind, Coupling, CouplingId, CouplingResponse, EdgeAssertion,
    Field, FieldDefinition, FieldExtent, FieldId, FieldLifecycle, Graph, NodeKey, NodeSelector,
    ProvenanceSubKind, RelationKind, ResourceNode, ScalarField, SemanticStatement, SemanticSubKind,
    VectorField,
};
use mere::kernel::persistence::PersistedResourceRecord;
use mere::kernel::types::{AssertionBasis, AssertionMethod, GraphScope};
use muniment::Backend;
use serde_json::json;

use crate::access::{AccessContext, record_access};
use crate::handlers::HandlerRegistry;
use crate::mere_host::{
    FIXTURE_DEVICE_ONE_ADDRESS, FIXTURE_DEVICE_TWO_ADDRESS, FIXTURE_FILE_ADDRESS,
    FIXTURE_GRANT_ADDRESS, FIXTURE_KEY_ADDRESS, FIXTURE_NON_WEB_ADDRESS, FIXTURE_PERSONA_ADDRESS,
    FIXTURE_RECEIPT_ADDRESS, FIXTURE_REMOTE_ADDRESS, FIXTURE_SCENE_ADDRESS, FIXTURE_WEB_ADDRESS,
    LINK_REST_A1_ADDRESS, LINK_REST_A2_ADDRESS, LINK_REST_B1_ADDRESS, LINK_REST_B2_ADDRESS,
    MereHost, MereHostError, SelectedPersonaRef, UNKNOWN_FIXTURE_FACET,
};

const LINK_REST_SOURCE: &str = "urn:graphshell:fixture:link-rest";
const LINK_FAMILY_SOURCE: &str = "urn:graphshell:fixture:link-family-assertions";

impl<B: Backend + Clone> MereHost<B> {
    /// P7 has its own browser database. Reuse the small topology, then supply
    /// two personal, independently reasoned assertions by the journal's persona.
    pub fn seed_link_promotion_fixture(&mut self) -> Result<(), MereHostError> {
        self.seed_link_family_assertion_fixture()?;
        self.mutate_product_graph(|graph| {
            let (from, _) = graph.get_node_by_url(LINK_REST_A1_ADDRESS).expect("fixture first");
            let (to, _) = graph.get_node_by_url(LINK_REST_B1_ADDRESS).expect("fixture second");
            let from_id = graph.get_node(from).unwrap().id;
            let to_id = graph.get_node(to).unwrap().id;
            let mut edges = graph.persisted_edges_between(from, to);
            let actor = graph.write_author().asserter_iri();
            for edge in &mut edges {
                for claim in edge.retained_assertions_mut() {
                    claim.statement_id = claim.statement_id.replace("fixture:p6:family:", "fixture:p7:");
                    claim.provenance_iri = Some(actor.clone());
                    claim.basis.method = None;
                }
            }
            apply_graph_delta(graph, GraphDelta::ReplaySetEdgesByIds { from_id, to_id, edges });
        })
    }

    /// Add a compact, source-owned link topology to a separately routed
    /// acceptance store. The four surfaces represent two Resource aliases;
    /// assertions remain ordinary exact graph statements with fixture IDs.
    pub fn seed_link_rest_fixture(&mut self) -> Result<(), MereHostError> {
        self.begin_profiled_session(
            Graph::new(),
            Some(pandect::graph_placement::PlacementProfile::RecordedStrataV1),
        )?;
        self.mutate_product_graph(|graph| {
            let a1 = add_link_rest_node(graph, LINK_REST_A1_ADDRESS, "A first", -180.0, -80.0);
            let a2 = add_link_rest_node(graph, LINK_REST_A2_ADDRESS, "A alias", -180.0, 80.0);
            let b1 = add_link_rest_node(graph, LINK_REST_B1_ADDRESS, "B first", 180.0, -80.0);
            let b2 = add_link_rest_node(graph, LINK_REST_B2_ADDRESS, "B alias", 180.0, 80.0);

            let resource_a = ResourceNode::new(LINK_REST_A1_ADDRESS);
            let resource_b = ResourceNode::new(LINK_REST_B1_ADDRESS);
            for resource in [&resource_a, &resource_b] {
                apply_graph_delta(
                    graph,
                    GraphDelta::ReplaySetResourceRecordById {
                        resource_id: resource.id(),
                        record: Some(PersistedResourceRecord {
                            canonical_iri: resource.canonical_iri().to_owned(),
                            facets: Vec::new(),
                        }),
                    },
                );
            }
            for surface in [a1, a2] {
                apply_graph_delta(
                    graph,
                    GraphDelta::ReplaySetShownResourceById {
                        surface_id: graph.get_node(surface).expect("fixture surface").id,
                        resource_id: Some(resource_a.id()),
                    },
                );
            }
            for surface in [b1, b2] {
                apply_graph_delta(
                    graph,
                    GraphDelta::ReplaySetShownResourceById {
                        surface_id: graph.get_node(surface).expect("fixture surface").id,
                        resource_id: Some(resource_b.id()),
                    },
                );
            }

            for (from, to, id, kind, note, at_ms) in [
                (
                    a1,
                    b1,
                    "fixture:p4:link-rest:resource-support-primary",
                    SemanticSubKind::Supports,
                    "Primary support reason",
                    1_700_000_000_100,
                ),
                (
                    a1,
                    b1,
                    "fixture:p4:link-rest:resource-support-independent",
                    SemanticSubKind::Supports,
                    "Independent support reason",
                    1_700_000_000_200,
                ),
                (
                    b1,
                    a1,
                    "fixture:p4:link-rest:resource-contradiction",
                    SemanticSubKind::Contradicts,
                    "Opposing reading",
                    1_700_000_000_300,
                ),
                (
                    a2,
                    b2,
                    "fixture:p4:link-rest:surface-alias-grouping",
                    SemanticSubKind::UserGrouped,
                    "Alias surface grouping",
                    1_700_000_000_400,
                ),
                (
                    a1,
                    b1,
                    "fixture:p4:link-rest:surface-primary-grouping",
                    SemanticSubKind::UserGrouped,
                    "Primary surface grouping",
                    1_700_000_000_450,
                ),
                (
                    a1,
                    b1,
                    "fixture:p4:link-rest:resource-forward",
                    SemanticSubKind::Cites,
                    "Resource-level forward citation",
                    1_700_000_000_500,
                ),
                (
                    b1,
                    a1,
                    "fixture:p4:link-rest:resource-reverse",
                    SemanticSubKind::Cites,
                    "Resource-level reverse citation",
                    1_700_000_000_600,
                ),
            ] {
                graph
                    .try_assert_persisted_semantic_statement(
                        from,
                        to,
                        SemanticStatement {
                            statement_id: id.into(),
                            predicate: mere::kernel::graph::predicate_iri(kind).to_owned(),
                            recognized_sub_kind: Some(kind),
                            label: Some(note.into()),
                            basis: AssertionBasis {
                                note: Some(note.into()),
                                method: None,
                                evidence: vec![format!(
                                    "urn:graphshell:fixture:link-rest:evidence:{id}"
                                )],
                            },
                            graph_scope: GraphScope::User,
                            provenance_iri: Some(LINK_REST_SOURCE.into()),
                            first_asserted_ms: Some(at_ms),
                            asserted_at_ms: Some(at_ms),
                            withdrawn: None,
                        },
                    )
                    .expect("fixture assertion IDs are unique and endpoints are present");
            }
        })
    }

    /// Seed a separate exact-assertion visibility fixture. It deliberately
    /// starts from the same small endpoint topology as P4, then replaces the
    /// link facts with two independent CollectionMember reasons and one
    /// Resource-stratum citation control.
    pub fn seed_link_family_assertion_fixture(&mut self) -> Result<(), MereHostError> {
        self.seed_link_rest_fixture()?;
        self.mutate_product_graph(|graph| {
            let (a1, _) = graph
                .get_node_by_url(LINK_REST_A1_ADDRESS)
                .expect("fixture A1 endpoint");
            let (a2, _) = graph
                .get_node_by_url(LINK_REST_A2_ADDRESS)
                .expect("fixture A2 endpoint");
            let (b1, _) = graph
                .get_node_by_url(LINK_REST_B1_ADDRESS)
                .expect("fixture B1 endpoint");
            let (b2, _) = graph
                .get_node_by_url(LINK_REST_B2_ADDRESS)
                .expect("fixture B2 endpoint");

            // Clear the source-owned P4 records from both strata before
            // installing this fixture's independent set.
            for (from, to) in [(a1, b1), (b1, a1), (a2, b2)] {
                let (from_id, to_id) = (
                    graph.get_node(from).expect("fixture source").id,
                    graph.get_node(to).expect("fixture target").id,
                );
                apply_graph_delta(
                    graph,
                    GraphDelta::ReplaySetEdgesByIds {
                        from_id,
                        to_id,
                        edges: Vec::new(),
                    },
                );
            }
            let resource_a = graph.shown_resource_id(a1).expect("fixture A resource");
            let resource_b = graph.shown_resource_id(b1).expect("fixture B resource");
            for (from_id, to_id) in [(resource_a, resource_b), (resource_b, resource_a)] {
                apply_graph_delta(
                    graph,
                    GraphDelta::ReplaySetResourceEdgesByIds {
                        from_resource_id: from_id,
                        to_resource_id: to_id,
                        edges: Vec::new(),
                    },
                );
            }

            let collection = RelationKind::Containment(ContainmentSubKind::CollectionMember);
            for (id, note, method, evidence, at_ms) in [
                (
                    "fixture:p6:family:collection-primary",
                    "Primary collection membership",
                    "urn:graphshell:fixture:p6:method:primary",
                    "urn:graphshell:fixture:p6:evidence:primary",
                    1_800_000_000_100,
                ),
                (
                    "fixture:p6:family:collection-independent",
                    "Independent collection membership",
                    "urn:graphshell:fixture:p6:method:independent",
                    "urn:graphshell:fixture:p6:evidence:independent",
                    1_800_000_000_200,
                ),
            ] {
                graph
                    .try_assert_persisted_relation_statement(
                        a1,
                        b1,
                        SemanticStatement {
                            statement_id: id.into(),
                            predicate: collection.predicate_iri().expect("fixed predicate").into(),
                            recognized_sub_kind: None,
                            label: Some(note.into()),
                            basis: AssertionBasis {
                                note: Some(note.into()),
                                method: Some(AssertionMethod {
                                    iri: method.into(),
                                    version: Some("1".into()),
                                }),
                                evidence: vec![evidence.into()],
                            },
                            graph_scope: GraphScope::User,
                            provenance_iri: Some(LINK_FAMILY_SOURCE.into()),
                            first_asserted_ms: Some(at_ms),
                            asserted_at_ms: Some(at_ms),
                            withdrawn: None,
                        },
                    )
                    .expect("fixture CollectionMember IDs are unique");
            }

            let kind = SemanticSubKind::Cites;
            let note = "Resource stratum citation control";
            let at_ms = 1_800_000_000_300;
            graph
                .try_assert_persisted_relation_statement(
                    a2,
                    b2,
                    SemanticStatement {
                        statement_id: "fixture:p6:family:resource-control".into(),
                        predicate: mere::kernel::graph::predicate_iri(kind).into(),
                        recognized_sub_kind: Some(kind),
                        label: Some(note.into()),
                        basis: AssertionBasis {
                            note: Some(note.into()),
                            method: None,
                            evidence: vec!["urn:graphshell:fixture:p6:evidence:resource".into()],
                        },
                        graph_scope: GraphScope::User,
                        provenance_iri: Some(LINK_FAMILY_SOURCE.into()),
                        first_asserted_ms: Some(at_ms),
                        asserted_at_ms: Some(at_ms),
                        withdrawn: None,
                    },
                )
                .expect("fixture resource control ID is unique");
        })
    }

    /// Declared presentation coordinates for this isolated fixture. Graph
    /// truth is position-free; these are reapplied only by its fixture route.
    pub fn link_rest_fixture_positions(&self) -> Vec<(NodeKey, PortablePoint)> {
        [
            (LINK_REST_A1_ADDRESS, -180.0, -80.0),
            (LINK_REST_A2_ADDRESS, -180.0, 80.0),
            (LINK_REST_B1_ADDRESS, 180.0, -80.0),
            (LINK_REST_B2_ADDRESS, 180.0, 80.0),
        ]
        .into_iter()
        .filter_map(|(address, x, y)| {
            self.graph()
                .get_node_by_url(address)
                .map(|(key, _)| (key, PortablePoint::new(x, y)))
        })
        .collect()
    }

    /// Replace the reference pair's demo assertion with a deterministic
    /// lifecycle fixture. The browser route gives this fixture its own store.
    pub fn seed_link_history_fixture(&mut self) -> Result<(), MereHostError> {
        self.mutate_product_graph(|graph| {
            let (web, _) = graph
                .get_node_by_url(FIXTURE_WEB_ADDRESS)
                .expect("reference web node is present");
            let (file, _) = graph
                .get_node_by_url(FIXTURE_FILE_ADDRESS)
                .expect("reference file node is present");

            // The normal reference fixture has one Cites assertion on this
            // pair. Replace only its isolated fixture buckets so the history
            // has exactly the four explicit records below.
            let web_id = graph.get_node(web).expect("web node").id;
            let file_id = graph.get_node(file).expect("file node").id;
            apply_graph_delta(
                graph,
                GraphDelta::ReplaySetEdgesByIds {
                    from_id: web_id,
                    to_id: file_id,
                    edges: Vec::new(),
                },
            );
            let web_resource = graph.shown_resource_id(web).expect("web resource");
            let file_resource = graph.shown_resource_id(file).expect("file resource");
            apply_graph_delta(
                graph,
                GraphDelta::ReplaySetResourceEdgesByIds {
                    from_resource_id: web_resource,
                    to_resource_id: file_resource,
                    edges: Vec::new(),
                },
            );

            let predicate = mere::kernel::graph::predicate_iri(SemanticSubKind::Cites).to_owned();
            let rows = [
                (
                    "fixture:p2:link-history:withdrawn-timed",
                    Some(1_000),
                    Some(1_100),
                    "Timed withdrawal basis",
                    "urn:graphshell:link-history:method:timed",
                    "urn:graphshell:link-history:evidence:timed",
                    Some(Some(1_200)),
                ),
                (
                    "fixture:p2:link-history:held-dated",
                    Some(2_000),
                    Some(2_500),
                    "Dated held basis",
                    "urn:graphshell:link-history:method:dated",
                    "urn:graphshell:link-history:evidence:dated",
                    None,
                ),
                (
                    "fixture:p2:link-history:withdrawn-undated",
                    Some(3_000),
                    Some(3_100),
                    "Undated withdrawal basis",
                    "urn:graphshell:link-history:method:undated",
                    "urn:graphshell:link-history:evidence:undated",
                    Some(None),
                ),
                (
                    "fixture:p2:link-history:held-undated",
                    None,
                    None,
                    "Undated held basis",
                    "urn:graphshell:link-history:method:legacy",
                    "urn:graphshell:link-history:evidence:legacy",
                    None,
                ),
            ];
            for (id, first, last, note, method, evidence, withdrawal) in rows {
                let statement = SemanticStatement {
                    statement_id: id.into(),
                    predicate: predicate.clone(),
                    recognized_sub_kind: Some(SemanticSubKind::Cites),
                    label: Some(note.into()),
                    basis: AssertionBasis {
                        note: Some(note.into()),
                        method: Some(AssertionMethod {
                            iri: method.into(),
                            version: Some("p2-fixture-v1".into()),
                        }),
                        evidence: vec![evidence.into()],
                    },
                    graph_scope: GraphScope::Default,
                    provenance_iri: Some("urn:graphshell:fixture:link-history".into()),
                    first_asserted_ms: first,
                    asserted_at_ms: last,
                    withdrawn: None,
                };
                graph
                    .try_assert_persisted_semantic_statement(web, file, statement)
                    .expect("fixture assertion handles are unique and pair-valid");
                if let Some(at_ms) = withdrawal {
                    graph.apply_imported_semantic_withdrawal_for_pair(
                        web,
                        file,
                        id,
                        at_ms,
                        Some("urn:graphshell:fixture:link-history:withdrawal".into()),
                    );
                }
            }
        })
    }

    /// Opt-in inspection data. Callers must use a separate fixture store.
    pub fn field_inspection_fixture(
        backend: B,
        selected_persona: SelectedPersonaRef,
        handlers: HandlerRegistry,
    ) -> Result<Self, MereHostError> {
        let mut host = Self::fixture(backend, selected_persona, handlers)?;
        host.mutate_product_graph(|graph| {
            let fid = |id| FieldId::from_uuid(uuid::Uuid::from_u128(id));
            let cid = |id| CouplingId::from_uuid(uuid::Uuid::from_u128(id));
            for (id, name, definition, bounds, lifecycle) in [
                (
                    101,
                    "Region study",
                    FieldDefinition::Scalar(ScalarField::Const(0.0)),
                    [-320., -220., 120., 180.],
                    FieldLifecycle::Active,
                ),
                (
                    102,
                    "Region overlap",
                    FieldDefinition::Vector(VectorField::ConstVec { x: 0., y: 0. }),
                    [-120., -180., 320., 220.],
                    FieldLifecycle::Active,
                ),
                (
                    103,
                    "Retired study",
                    FieldDefinition::Scalar(ScalarField::Const(0.0)),
                    [-80., -80., 80., 80.],
                    FieldLifecycle::Retired,
                ),
            ] {
                let [min_x, min_y, max_x, max_y] = bounds;
                let mut field = Field::new(fid(id), definition).with_name(name).with_extent(
                    FieldExtent::Region {
                        min_x,
                        min_y,
                        max_x,
                        max_y,
                    },
                );
                field.lifecycle = lifecycle;
                apply_graph_delta(graph, GraphDelta::AddField { field });
            }
            for coupling in [
                Coupling::new(
                    cid(201),
                    fid(101),
                    NodeSelector::All,
                    CouplingResponse::AttractToMin,
                    0.02,
                ),
                Coupling::new(
                    cid(202),
                    fid(101),
                    NodeSelector::Tagged("research".into()),
                    CouplingResponse::Open {
                        predicate: "urn:graphshell:inspection:highlight".into(),
                    },
                    0.08,
                ),
                Coupling::new(
                    cid(203),
                    fid(102),
                    NodeSelector::All,
                    CouplingResponse::FlowAdvect,
                    1.0,
                ),
            ] {
                apply_graph_delta(graph, GraphDelta::AddCoupling { coupling });
            }
        })?;
        Ok(host)
    }

    /// Deterministic H1 source truth with synthetic public identity material,
    /// journaled as Graphshell's own edits.
    pub fn fixture(
        backend: B,
        selected_persona: SelectedPersonaRef,
        handlers: HandlerRegistry,
    ) -> Result<Self, MereHostError> {
        let mut host = Self::empty(
            backend,
            selected_persona.clone(),
            handlers,
            AccessContext {
                persona: selected_persona.persona.clone(),
                device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                at_ms: 3_000,
            },
        );

        let [
            web,
            file,
            scene,
            remote,
            persona,
            device_one,
            device_two,
            key,
            grant,
            receipt,
        ] = host.mutate_product_graph(|graph| {
            let web = add_fixture_node(
                graph,
                FIXTURE_WEB_ADDRESS,
                "I2P port notes",
                &["research", "transport"],
                10,
            );
            let non_web = add_fixture_node(
                graph,
                FIXTURE_NON_WEB_ADDRESS,
                "I2P service",
                &["address", "transport"],
                20,
            );
            let file = add_fixture_node(
                graph,
                FIXTURE_FILE_ADDRESS,
                "Reference notes",
                &["local", "reference"],
                30,
            );
            let scene = add_fixture_node(
                graph,
                FIXTURE_SCENE_ADDRESS,
                "Reference-host scene",
                &["scene"],
                40,
            );
            let remote = add_fixture_node(
                graph,
                FIXTURE_REMOTE_ADDRESS,
                "Mounted remote projection",
                &["remote", "projection"],
                50,
            );
            let persona = add_fixture_node(
                graph,
                FIXTURE_PERSONA_ADDRESS,
                "Alice",
                &["identity", "public"],
                60,
            );
            let device_one = add_fixture_node(
                graph,
                FIXTURE_DEVICE_ONE_ADDRESS,
                "Alice's laptop",
                &["device", "public"],
                70,
            );
            let device_two = add_fixture_node(
                graph,
                FIXTURE_DEVICE_TWO_ADDRESS,
                "Alice's phone",
                &["device", "public"],
                80,
            );
            let key = add_fixture_node(
                graph,
                FIXTURE_KEY_ADDRESS,
                "SSH signing key reference",
                &["key-reference", "public"],
                90,
            );
            let grant = add_fixture_node(
                graph,
                FIXTURE_GRANT_ADDRESS,
                "Open-address grant",
                &["grant", "public"],
                100,
            );
            let receipt = add_fixture_node(
                graph,
                FIXTURE_RECEIPT_ADDRESS,
                "Synthetic signing receipt",
                &["receipt", "public", "synthetic"],
                110,
            );

            assert_relation(
                graph,
                web,
                non_web,
                EdgeAssertion::Semantic {
                    sub_kind: SemanticSubKind::Hyperlink,
                    label: Some("I2P endpoint".to_string()),
                    decay_progress: None,
                },
            );
            assert_relation(
                graph,
                web,
                file,
                EdgeAssertion::Semantic {
                    sub_kind: SemanticSubKind::Cites,
                    label: Some("working notes".to_string()),
                    decay_progress: None,
                },
            );
            assert_relation(
                graph,
                scene,
                web,
                EdgeAssertion::Arrangement {
                    sub_kind: ArrangementSubKind::FrameMember,
                },
            );
            assert_relation(
                graph,
                scene,
                file,
                EdgeAssertion::Containment {
                    sub_kind: ContainmentSubKind::CollectionMember,
                },
            );
            assert_relation(
                graph,
                remote,
                scene,
                EdgeAssertion::Semantic {
                    sub_kind: SemanticSubKind::Elaborates,
                    label: Some("mounted beside local graph".to_string()),
                    decay_progress: None,
                },
            );
            assert_relation(
                graph,
                persona,
                device_one,
                EdgeAssertion::Containment {
                    sub_kind: ContainmentSubKind::CollectionMember,
                },
            );
            assert_relation(
                graph,
                persona,
                device_two,
                EdgeAssertion::Containment {
                    sub_kind: ContainmentSubKind::CollectionMember,
                },
            );
            assert_relation(
                graph,
                device_one,
                key,
                EdgeAssertion::Semantic {
                    sub_kind: SemanticSubKind::DependsOn,
                    label: Some("public key reference".to_string()),
                    decay_progress: None,
                },
            );
            assert_relation(
                graph,
                grant,
                persona,
                EdgeAssertion::Semantic {
                    sub_kind: SemanticSubKind::Supports,
                    label: Some("subject".to_string()),
                    decay_progress: None,
                },
            );
            assert_relation(
                graph,
                receipt,
                grant,
                EdgeAssertion::Provenance {
                    sub_kind: ProvenanceSubKind::GeneratedFrom,
                },
            );

            [
                web, file, scene, remote, persona, device_one, device_two, key, grant, receipt,
            ]
        })?;

        host.set_facet(
            remote,
            "graphshell.remote-mount/v1",
            json!({
                "endpoint": "loopback:g1",
                "session": "loopback:g1-presentation",
                "status": "mounted"
            }),
        )?;
        host.set_facet(
            persona,
            "personae.public-persona/v1",
            json!({
                "display_name": "Alice",
                "profile_ref": selected_persona.profile,
                "synthetic": true
            }),
        )?;
        host.set_facet(
            device_one,
            "personae.public-device/v1",
            json!({
                "label": "laptop",
                "subject": FIXTURE_PERSONA_ADDRESS,
                "synthetic": true
            }),
        )?;
        host.set_facet(
            device_two,
            "personae.public-device/v1",
            json!({
                "label": "phone",
                "subject": FIXTURE_PERSONA_ADDRESS,
                "synthetic": true
            }),
        )?;
        host.set_facet(
            key,
            "personae.public-key-reference/v1",
            json!({
                "algorithm": "ssh-ed25519",
                "fingerprint": "SHA256:synthetic-graphshell-h1",
                "synthetic": true
            }),
        )?;
        host.set_facet(
            grant,
            "personae.public-grant/v1",
            json!({
                "subject": FIXTURE_PERSONA_ADDRESS,
                "action": "graphshell.address.open",
                "expires_at_ms": 9_999_999,
                "synthetic": true
            }),
        )?;
        host.set_facet(
            receipt,
            "personae.signing-receipt/v1",
            json!({
                "key_ref": FIXTURE_KEY_ADDRESS,
                "payload_digest": "blake3:synthetic",
                "signature_ref": "personae-signature:synthetic",
                "synthetic": true
            }),
        )?;
        host.set_facet(
            file,
            UNKNOWN_FIXTURE_FACET,
            json!({
                "carrier": "future",
                "facets": ["opaque", "preserve-me"],
                "version": 7
            }),
        )?;

        host.mutate_product_graph(|graph| {
            record_access(
                graph,
                web,
                &AccessContext {
                    persona: selected_persona.persona.clone(),
                    device: FIXTURE_DEVICE_ONE_ADDRESS.to_string(),
                    at_ms: 1_000,
                },
                "graphshell.inspect",
            )?;
            record_access(
                graph,
                web,
                &AccessContext {
                    persona: selected_persona.persona,
                    device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                    at_ms: 2_000,
                },
                "system.default",
            )
        })??;

        let score = host.score();
        host.set_facet(
            scene,
            "graphshell.saved-scene/v1",
            serde_json::to_value(score).expect("Scenograph scores serialize"),
        )?;
        host.projection_revision = 1;
        Ok(host)
    }
}

fn add_fixture_node(
    graph: &mut Graph,
    address: &str,
    title: &str,
    tags: &[&str],
    at_ms: u64,
) -> NodeKey {
    let id = Graph::node_namespace_id(address);
    let key = add_node(
        graph,
        Some(id),
        address.to_string(),
        PortablePoint::new(0.0, 0.0),
    );
    apply_graph_delta(
        graph,
        GraphDelta::SetNodeTitle {
            key,
            title: title.to_string(),
        },
    );
    for tag in tags {
        apply_graph_delta(
            graph,
            GraphDelta::InsertNodeTag {
                key,
                tag: (*tag).to_string(),
            },
        );
    }
    apply_graph_delta(
        graph,
        GraphDelta::ReplayTouchNodeLastVisitedById {
            node_id: id,
            timestamp_ms: at_ms,
        },
    );
    key
}

fn add_link_rest_node(graph: &mut Graph, address: &str, title: &str, x: f32, y: f32) -> NodeKey {
    let key = add_node(
        graph,
        Some(Graph::node_namespace_id(address)),
        address.to_owned(),
        PortablePoint::new(x, y),
    );
    apply_graph_delta(
        graph,
        GraphDelta::SetNodeTitle {
            key,
            title: title.to_owned(),
        },
    );
    key
}
