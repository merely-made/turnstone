// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta};
use mere::kernel::graph::{FieldLifecycle, NodeSelector, ScalarField, VectorField};

fn fid(n: u128) -> FieldId {
    FieldId::from_uuid(Uuid::from_u128(n))
}
fn cid(n: u128) -> CouplingId {
    CouplingId::from_uuid(Uuid::from_u128(n))
}
fn add_field(graph: &mut Graph, id: u128, lifecycle: FieldLifecycle) {
    let mut field = Field::new(fid(id), FieldDefinition::Scalar(ScalarField::Const(0.)))
        .with_name("Repeated name")
        .with_extent(FieldExtent::Region {
            min_x: -100.,
            min_y: -100.,
            max_x: 100.,
            max_y: 100.,
        });
    field.lifecycle = lifecycle;
    apply_graph_delta(graph, GraphDelta::AddField { field });
}
fn fixture() -> (Graph, Vec<Uuid>) {
    let mut graph = Graph::new();
    let members: Vec<_> = (1..=3).map(Uuid::from_u128).collect();
    for member in &members {
        let key = add_node(
            &mut graph,
            Some(*member),
            "https://source.test/".into(),
            Default::default(),
        );
        apply_graph_delta(
            &mut graph,
            GraphDelta::InsertNodeTag {
                key,
                tag: "study".into(),
            },
        );
    }
    // Reverse insertion order and equal names do not become hit or edit order.
    add_field(&mut graph, 102, FieldLifecycle::Active);
    add_field(&mut graph, 101, FieldLifecycle::Active);
    add_field(&mut graph, 103, FieldLifecycle::Retired);
    for coupling in [
        Coupling::new(
            cid(202),
            fid(101),
            NodeSelector::Tagged("study".into()),
            CouplingResponse::Open {
                predicate: "urn:study:paint".into(),
            },
            0.7,
        ),
        Coupling::new(
            cid(201),
            fid(101),
            NodeSelector::All,
            CouplingResponse::AttractToMin,
            0.2,
        ),
    ] {
        apply_graph_delta(&mut graph, GraphDelta::AddCoupling { coupling });
    }
    (graph, members)
}

#[test]
fn individually_identified_couplings_and_retired_fields_remain_inspectable() {
    let (graph, members) = fixture();
    let before = serde_json::to_value(graph.to_snapshot()).unwrap();
    let inspected = inspect_field(&graph, fid(101)).unwrap();
    assert_eq!(
        inspected
            .couplings
            .iter()
            .map(|c| c.coupling.id)
            .collect::<Vec<_>>(),
        vec![cid(201), cid(202)]
    );
    assert_eq!(inspected.couplings[0].coupling.strength, 0.2);
    assert_eq!(inspected.couplings[1].coupling.strength, 0.7);
    assert_eq!(
        inspected.couplings[0]
            .selector_members
            .iter()
            .map(|n| n.member)
            .collect::<Vec<_>>(),
        members
    );
    assert_eq!(inspected.couplings[1].selector_members.len(), 3);
    assert_eq!(
        inspected.couplings[0].rebuild_eligibility,
        ForceEligibility::Recognized
    );
    assert_eq!(
        inspected.couplings[1].rebuild_eligibility,
        ForceEligibility::OpenResponse
    );
    assert_eq!(
        inspect_field(&graph, fid(103)).unwrap().field.lifecycle,
        FieldLifecycle::Retired
    );
    assert_eq!(serde_json::to_value(graph.to_snapshot()).unwrap(), before);
}

#[test]
fn overlap_is_deterministic_and_visibility_never_rewrites_source() {
    let (graph, _) = fixture();
    let before = serde_json::to_value(graph.to_snapshot()).unwrap();
    let candidates = region_candidates(&graph, [0., 0.], |_| true, None).unwrap();
    assert_eq!(
        candidates.iter().map(|c| c.target).collect::<Vec<_>>(),
        vec![FieldTarget::Numen(fid(101)), FieldTarget::Numen(fid(102))]
    );
    assert_eq!(
        region_candidates(&graph, [0., 0.], |_| true, None).unwrap(),
        candidates
    );
    let hidden = region_candidates(&graph, [0., 0.], |id| id != fid(101), None).unwrap();
    assert_eq!(hidden.len(), 1);
    assert_eq!(hidden[0].target, FieldTarget::Numen(fid(102)));
    assert_eq!(field_roster(&graph).len(), 3);
    assert_eq!(inspect_field(&graph, fid(101)).unwrap().couplings.len(), 2);
    assert_eq!(serde_json::to_value(graph.to_snapshot()).unwrap(), before);
}

#[test]
fn authored_bounds_hit_is_separate_from_selector_membership() {
    let (graph, _) = fixture();
    assert!(
        region_candidates(&graph, [1000., 1000.], |_| true, None)
            .unwrap()
            .is_empty()
    );
    // All remains all outside the region; the existing bridge does not clip it.
    assert_eq!(
        inspect_coupling(&graph, cid(201))
            .unwrap()
            .selector_members
            .len(),
        3
    );
    assert_eq!(
        region_candidates(&graph, [100., 100.], |_| true, None)
            .unwrap()
            .len(),
        2
    );
    assert!(region_candidates(&graph, [f32::NAN, 0.], |_| true, None).is_err());
    assert!(region_candidates(&graph, [f32::INFINITY, 0.], |_| true, None).is_err());
}

#[test]
fn other_extent_shapes_stay_in_roster_without_fabricated_visible_regions() {
    let (mut graph, _) = fixture();
    for (id, extent) in [
        (104, FieldExtent::Global),
        (105, FieldExtent::AttachedToNode(Uuid::from_u128(1))),
        (
            106,
            FieldExtent::Polygon {
                points: vec![(-100., -100.), (100., -100.), (0., 100.)],
            },
        ),
    ] {
        apply_graph_delta(
            &mut graph,
            GraphDelta::AddField {
                field: Field::new(fid(id), FieldDefinition::Scalar(ScalarField::Const(0.)))
                    .with_extent(extent),
            },
        );
    }
    assert_eq!(field_roster(&graph).len(), 6);
    assert_eq!(
        region_candidates(&graph, [0., 0.], |_| true, None)
            .unwrap()
            .len(),
        2
    );
    assert!(matches!(
        inspect_field(&graph, fid(105)).unwrap().field.extent,
        FieldExtent::AttachedToNode(_)
    ));
}

#[test]
fn exact_coupling_refreshes_its_record_and_never_substitutes_a_peer() {
    let (mut graph, _) = fixture();
    let mut record = inspect_coupling(&graph, cid(201)).unwrap().coupling;
    record.strength = 0.9;
    record.field = fid(102);
    apply_graph_delta(&mut graph, GraphDelta::AddCoupling { coupling: record });
    let refreshed = inspect_coupling(&graph, cid(201)).unwrap();
    assert_eq!(refreshed.coupling.field, fid(102));
    assert_eq!(refreshed.coupling.strength, 0.9);
    assert_eq!(
        inspect_coupling(&graph, cid(202))
            .unwrap()
            .coupling
            .strength,
        0.7
    );
    apply_graph_delta(&mut graph, GraphDelta::RetractCoupling { id: cid(201) });
    assert!(inspect_coupling(&graph, cid(201)).is_err());
    assert_eq!(inspect_field(&graph, fid(101)).unwrap().couplings.len(), 1);
}

#[test]
fn missing_field_is_disclosed_without_name_based_retargeting() {
    let (mut graph, _) = fixture();
    let mut record = inspect_coupling(&graph, cid(201)).unwrap().coupling;
    record.field = fid(999);
    apply_graph_delta(&mut graph, GraphDelta::AddCoupling { coupling: record });
    assert!(inspect_field(&graph, fid(999)).is_err());
    let orphan = inspect_coupling(&graph, cid(201)).unwrap();
    assert!(orphan.owner.is_none());
    assert_eq!(orphan.rebuild_eligibility, ForceEligibility::MissingField);
    assert!(inspect_field(&graph, fid(102)).is_ok());
}

#[test]
fn force_eligibility_discloses_lifecycle_and_definition_mismatch() {
    let (mut graph, _) = fixture();
    for coupling in [
        Coupling::new(
            cid(203),
            fid(103),
            NodeSelector::All,
            CouplingResponse::AttractToMin,
            1.,
        ),
        Coupling::new(
            cid(204),
            fid(102),
            NodeSelector::All,
            CouplingResponse::AlignVelocity,
            1.,
        ),
    ] {
        apply_graph_delta(&mut graph, GraphDelta::AddCoupling { coupling });
    }
    assert_eq!(
        inspect_coupling(&graph, cid(203))
            .unwrap()
            .rebuild_eligibility,
        ForceEligibility::RetiredField
    );
    assert_eq!(
        inspect_coupling(&graph, cid(204))
            .unwrap()
            .rebuild_eligibility,
        ForceEligibility::DefinitionMismatch
    );
    let field = Field::new(
        fid(102),
        FieldDefinition::Vector(VectorField::ConstVec { x: 1., y: 0. }),
    );
    apply_graph_delta(&mut graph, GraphDelta::AddField { field });
    assert_eq!(
        inspect_coupling(&graph, cid(204))
            .unwrap()
            .rebuild_eligibility,
        ForceEligibility::Recognized
    );
}

#[test]
fn forme_and_numen_overlap_keep_independent_owners() {
    let (graph, members) = fixture();
    let mut workspace = FormeWorkspace::new(Uuid::from_u128(999));
    workspace.open(members[0], &graph).unwrap();
    let id = workspace.document.id.as_uuid();
    let forme = inspect_forme(&workspace, id).unwrap();
    let candidates = region_candidates(&graph, [0., 0.], |_| true, Some(&forme)).unwrap();
    assert_eq!(candidates.len(), 3);
    assert_eq!(candidates[2].target, FieldTarget::Forme(id));
    assert_eq!(graph.fields().count(), 3);
    assert!(inspect_forme(&workspace, Uuid::from_u128(1000)).is_err());
    workspace.visible = false;
    let hidden = inspect_forme(&workspace, id).unwrap();
    assert_eq!(
        region_candidates(&graph, [0., 0.], |_| true, Some(&hidden))
            .unwrap()
            .len(),
        2
    );
    assert_eq!(hidden.members, members[..1]);
}

#[test]
fn field_and_exact_coupling_roundtrip_with_source_snapshot() {
    let (graph, _) = fixture();
    let expected = inspect_field(&graph, fid(101)).unwrap();
    let reopened = Graph::from_snapshot(&graph.to_snapshot());
    assert_eq!(inspect_field(&reopened, fid(101)).unwrap(), expected);
    assert_eq!(
        inspect_coupling(&reopened, cid(201)).unwrap(),
        inspect_coupling(&graph, cid(201)).unwrap()
    );
    assert_eq!(
        serde_json::from_value::<FieldInspection>(serde_json::to_value(&expected).unwrap())
            .unwrap(),
        expected
    );
}

#[test]
fn dedicated_fixture_reopens_and_normal_reference_fixture_stays_separate() {
    use crate::app::GraphshellApp;
    use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, SelectedPersonaRef};
    use muniment::MemoryBackend;
    let persona = || SelectedPersonaRef {
        persona: FIXTURE_PERSONA_ADDRESS.into(),
        profile: "field-inspection-test".into(),
    };
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        let mut app = GraphshellApp::open_or_field_inspection_fixture(backend.clone(), persona())
            .await
            .unwrap();
        let before = inspect_field(app.host.graph(), fid(101)).unwrap();
        let before_roster = field_roster(app.host.graph());
        let before_vector = inspect_coupling(app.host.graph(), cid(203)).unwrap();
        assert_eq!(before.couplings.len(), 2);
        assert_eq!(
            before.couplings[1].rebuild_eligibility,
            ForceEligibility::OpenResponse
        );
        app.host.persist(17).await.unwrap();
        let reopened = GraphshellApp::open_or_field_inspection_fixture(backend, persona())
            .await
            .unwrap();
        assert!(reopened.host.was_reopened());
        assert_eq!(
            inspect_field(reopened.host.graph(), fid(101)).unwrap(),
            before
        );
        assert_eq!(field_roster(reopened.host.graph()), before_roster);
        assert_eq!(
            inspect_coupling(reopened.host.graph(), cid(203)).unwrap(),
            before_vector
        );
        let normal_backend = MemoryBackend::new();
        let mut normal = GraphshellApp::open_or_fixture(normal_backend.clone(), persona())
            .await
            .unwrap();
        assert_eq!(normal.host.graph().fields().count(), 0);
        normal.host.persist(18).await.unwrap();
        // The opt-in seeder cannot inject its fixture into an existing graph.
        let still_normal =
            GraphshellApp::open_or_field_inspection_fixture(normal_backend, persona())
                .await
                .unwrap();
        assert!(still_normal.host.was_reopened());
        assert_eq!(still_normal.host.graph().fields().count(), 0);
    });
}
