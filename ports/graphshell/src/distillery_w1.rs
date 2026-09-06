// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Graphshell half of Distillery W1.
//!
//! Distillery owns the job board and the Chronicle endpoint that reads it.
//! Graphshell owns the client-side mount: discover the offered projection,
//! apply its snapshot and diffs, retain the acknowledgement across
//! same-session endpoint rediscovery,
//! and realize the acknowledged scene as a frozen semantic table.
//!
//! This module deliberately depends only on Graphshell's endpoint traits. It
//! does not mention Distillery's job types and therefore cannot become a
//! second owner of that product's state. When the Distillery endpoint lands,
//! its factory can be passed to [`ChronicleMount::discover`] unchanged.

use std::collections::HashMap;
use std::fmt::Display;

use chirograph::{
    EndpointDescriptor, PortableCardV1, PresentationCodec, ProjectionRequest, ProjectionSession,
    ResourceRequest, ResumeReply,
};
use graphshell_client::frozen::FrozenScene;
use graphshell_client::{ClientState, ResumeApplication};
use graphshell_endpoint::{
    PresentationSource, ProjectionCatalog, ProjectionSource, ResumableProjectionSource,
};
use sceno::InstanceId;
use serde::{Deserialize, Serialize};

/// A machine-readable summary of the Graphshell side of one W1 walk.
///
/// The endpoint remains the authority for revisions and diff contents. This
/// receipt records only what the client observed after applying them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChronicleMountReceipt {
    pub endpoint_label: String,
    pub projection_label: String,
    pub session: ProjectionSession,
    pub snapshot_revision: u64,
    pub rediscovered_same_session: bool,
    pub resumed_by_diff: bool,
    pub resumed_revision: u64,
    pub frozen_instances: usize,
    pub frozen_table_rows: usize,
}

/// A product-neutral Graphshell mount for a resumable Chronicle endpoint.
///
/// `E` is supplied by the product host. The mount owns the disclosed client
/// state and the remembered projection request, but never receives or stores
/// the product's source authority beyond the endpoint value it is driving.
pub struct ChronicleMount<E> {
    endpoint: E,
    descriptor: EndpointDescriptor,
    request: ProjectionRequest,
    session: ProjectionSession,
    client: ClientState,
    snapshot_revision: Option<u64>,
    rediscovered_same_session: bool,
    resumed_by_diff: bool,
    details: HashMap<InstanceId, String>,
}

impl<E> ChronicleMount<E>
where
    E: ProjectionCatalog + ProjectionSource + PresentationSource + ResumableProjectionSource,
    <E as ProjectionSource>::Error: Display,
    <E as PresentationSource>::Error: Display,
    <E as ResumableProjectionSource>::Error: Display,
{
    /// Discover an endpoint and select one offered projection.
    ///
    /// W1 has one Chronicle offer, so the default selects index zero. A host
    /// that exposes several projections can use [`Self::from_offer`] with the
    /// chosen index while keeping the same mount and frozen realization.
    pub fn discover(endpoint: E) -> Result<Self, String> {
        Self::from_offer(endpoint, 0)
    }

    /// Discover an endpoint and select an explicit catalog offer.
    pub fn from_offer(endpoint: E, offer_index: usize) -> Result<Self, String> {
        let descriptor = endpoint.describe();
        let request = descriptor
            .projections
            .get(offer_index)
            .map(|offer| offer.request.clone())
            .ok_or_else(|| format!("endpoint has no projection {offer_index}"))?;
        Ok(Self {
            endpoint,
            descriptor,
            session: request.session.clone(),
            request,
            client: ClientState::default(),
            snapshot_revision: None,
            rediscovered_same_session: false,
            resumed_by_diff: false,
            details: HashMap::new(),
        })
    }

    /// Apply the endpoint's first full snapshot.
    pub fn mount(&mut self) -> Result<(), String> {
        let snapshot = self
            .endpoint
            .snapshot(self.request.clone())
            .map_err(|error| format!("Chronicle snapshot failed: {error}"))?;
        if snapshot.session != self.session {
            return Err("Chronicle snapshot returned a different session".into());
        }
        self.snapshot_revision = Some(snapshot.scene.revision.0);
        self.client
            .apply_snapshot(snapshot)
            .map_err(|error| format!("Graphshell could not apply Chronicle snapshot: {error:?}"))?;
        self.refresh_details();
        Ok(())
    }

    /// Repeat discovery while retaining the same admitted projection session.
    ///
    /// The client keeps its mounted scene and acknowledgement. The next call
    /// to [`Self::resume`] asks the endpoint for a contiguous diff from that
    /// acknowledgement, instead of silently taking a new snapshot.
    pub fn rediscover_same_session(&mut self) -> Result<&EndpointDescriptor, String> {
        let descriptor = self.endpoint.describe();
        let request = descriptor
            .projections
            .iter()
            .find(|offer| offer.request.session == self.session)
            .map(|offer| offer.request.clone())
            .ok_or_else(|| {
                "rediscovered endpoint no longer offers the mounted session".to_string()
            })?;
        self.descriptor = descriptor;
        self.request = request;
        self.rediscovered_same_session = true;
        Ok(&self.descriptor)
    }

    /// Replace the endpoint instance while retaining the mounted session.
    ///
    /// A product host calls this when a fresh process or resident observer has
    /// the newer source materialization. The client acknowledgement survives;
    /// the replacement endpoint remains the product's authority.
    pub fn replace_endpoint(&mut self, endpoint: E) {
        self.endpoint = endpoint;
    }

    /// Resume from the client's last acknowledgement and apply the answer.
    pub fn resume(&mut self) -> Result<ResumeApplication, String> {
        let request = self
            .client
            .resume_request(&self.session)
            .ok_or_else(|| "Chronicle is not mounted".to_string())?;
        let reply = self
            .endpoint
            .resume(request)
            .map_err(|error| format!("Chronicle resume failed: {error}"))?;
        self.resumed_by_diff = matches!(reply, ResumeReply::Diffs(_));
        let result = self
            .client
            .apply_resume(&self.session, reply)
            .map_err(|error| format!("Graphshell could not apply Chronicle resume: {error:?}"))?;
        self.refresh_details();
        Ok(result)
    }

    /// Build the frozen semantic realization from the acknowledged snapshot.
    ///
    /// Item names come from endpoint presentation semantics. Graphshell does
    /// not invent labels from product fields, and falls back to the scene's
    /// source id when an endpoint omits one.
    pub fn frozen_scene(&self, name: &str) -> Result<FrozenScene, String> {
        let mounted = self
            .client
            .mounted(&self.session)
            .ok_or_else(|| "Chronicle is not mounted".to_string())?;
        let mut names = HashMap::<InstanceId, String>::new();
        for binding in &mounted.presentation.bindings {
            if let Some(offer) = mounted
                .presentation
                .offers
                .get(&binding.key)
                .and_then(|offers| offers.first())
            {
                names.insert(binding.instance, offer.semantics.label.clone());
            }
        }
        Ok(FrozenScene::freeze_snapshot_with_details(
            &mounted.scene,
            name,
            &names,
            &self.details,
        ))
    }

    /// Render the current frozen realization as the table-bearing HTML
    /// alternate. This is the host-facing readable form used by W1.
    pub fn frozen_html(&self, name: &str) -> Result<String, String> {
        Ok(self.frozen_scene(name)?.to_html("chronicle"))
    }

    /// Produce a receipt after the frozen realization has been built.
    pub fn receipt(&self, name: &str) -> Result<ChronicleMountReceipt, String> {
        let frozen = self.frozen_scene(name)?;
        let mounted = self
            .client
            .mounted(&self.session)
            .ok_or_else(|| "Chronicle is not mounted".to_string())?;
        Ok(ChronicleMountReceipt {
            endpoint_label: self.descriptor.label.clone(),
            projection_label: self
                .descriptor
                .projections
                .iter()
                .find(|offer| offer.request.session == self.session)
                .map(|offer| offer.label.clone())
                .unwrap_or_default(),
            session: self.session.clone(),
            snapshot_revision: self.snapshot_revision.unwrap_or(mounted.scene.revision.0),
            rediscovered_same_session: self.rediscovered_same_session,
            resumed_by_diff: self.resumed_by_diff,
            resumed_revision: mounted.scene.revision.0,
            frozen_instances: frozen.instances.len(),
            frozen_table_rows: frozen.instances.len()
                + frozen.relations.len()
                + frozen.unmet_holds.len(),
        })
    }

    /// Serialize the receipt for a test artifact or host-owned evidence file.
    pub fn receipt_json(&self, name: &str) -> Result<String, String> {
        serde_json::to_string_pretty(&self.receipt(name)?).map_err(|error| error.to_string())
    }

    /// Borrow the endpoint so a product-owned event can advance its source
    /// between the initial mount and the rediscovery/resume step.
    pub fn endpoint_mut(&mut self) -> &mut E {
        &mut self.endpoint
    }

    pub fn client(&self) -> &ClientState {
        &self.client
    }

    pub fn descriptor(&self) -> &EndpointDescriptor {
        &self.descriptor
    }

    fn refresh_details(&mut self) {
        let requests =
            self.client
                .mounted(&self.session)
                .map(|mounted| {
                    mounted
                        .presentation
                        .bindings
                        .iter()
                        .filter_map(|binding| {
                            let offer =
                                mounted.presentation.offers.get(&binding.key)?.iter().find(
                                    |offer| offer.codec == PresentationCodec::PortableCardV1,
                                )?;
                            Some((binding.instance, offer.resource))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

        self.details.clear();
        for (instance, resource) in requests {
            let Ok(response) = self.endpoint.resource(ResourceRequest {
                session: self.session.clone(),
                resource,
            }) else {
                continue;
            };
            if !response.has_valid_address() {
                continue;
            }
            let Ok(card) = serde_json::from_slice::<PortableCardV1>(&response.bytes) else {
                continue;
            };
            let detail = card
                .values
                .iter()
                .map(|value| format!("{}: {}", value.label, value.value))
                .collect::<Vec<_>>()
                .join("; ");
            self.details.insert(instance, detail);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_endpoint::LiveEndpoint;

    fn distillery_board() -> mesh::JobBoard {
        #[derive(Deserialize)]
        struct Fixture {
            jobs: Vec<mesh::Job>,
        }

        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../distillery/tests/fixtures/chronicle/distillery_board.json"
        ))
        .expect("W0 Chronicle fixture parses");
        mesh::JobBoard::fold_from_snapshot(
            [0; 32],
            &mesh::JobBoardSnapshot { jobs: fixture.jobs },
            std::iter::empty(),
        )
    }

    fn distillery_endpoint(
        session: ProjectionSession,
        generation: u64,
        revision: u64,
    ) -> distillery::ChronicleEndpoint {
        distillery::ChronicleEndpoint::new(
            session,
            &distillery_board(),
            &[
                distillery::ResidentReceipt::MaintenanceIdle,
                distillery::ResidentReceipt::StopRequested,
            ],
            distillery::ChronicleRevision::new(generation, 7, revision),
        )
    }

    #[test]
    fn generic_resumable_mount_rediscovery_resumes_by_diff_and_freezes_a_readable_table() {
        let mut mount = ChronicleMount::discover(LiveEndpoint::new()).expect("one offer");
        mount.mount().expect("initial snapshot");
        assert_eq!(
            mount
                .client()
                .acknowledgement(&mount.session)
                .unwrap()
                .revision
                .0,
            1
        );

        // This is deliberately a product-side mutation. Graphshell can only
        // observe it through the endpoint's resume contract.
        mount.endpoint_mut().append();
        mount
            .rediscover_same_session()
            .expect("descriptor on same-session rediscovery");
        let resumed = mount.resume().expect("contiguous diff");
        assert!(matches!(resumed, ResumeApplication::Applied(_)));

        let html = mount
            .frozen_html("Distillery Chronicle")
            .expect("frozen table");
        assert!(html.contains("<table class=\"frozen-alternate\">"));
        assert!(html.contains("Card 0"));
        assert!(html.contains("Card 1"));

        let receipt = mount.receipt("Distillery Chronicle").expect("receipt");
        assert_eq!(receipt.snapshot_revision, 1);
        assert!(receipt.rediscovered_same_session);
        assert!(receipt.resumed_by_diff);
        assert_eq!(receipt.resumed_revision, 2);
        assert_eq!(receipt.frozen_instances, 2);
        assert_eq!(receipt.frozen_table_rows, 2);
    }

    #[test]
    fn missing_offer_is_refused_before_endpoint_state_is_mounted() {
        let error = match ChronicleMount::from_offer(LiveEndpoint::new(), 1) {
            Ok(_) => panic!("the fixture has one offer"),
            Err(error) => error,
        };
        assert!(error.contains("no projection 1"));
    }

    #[test]
    fn actual_distillery_chronicle_mount_resumes_by_diff_and_freezes_job_table() {
        let admitted = ProjectionSession("admitted:chronicle-mount".into());
        let mut mount =
            ChronicleMount::discover(distillery_endpoint(admitted, 41, 11)).expect("one offer");
        mount.mount().expect("Chronicle snapshot");
        let initial = mount
            .frozen_scene("Distillery Chronicle")
            .expect("frozen scene");
        assert_eq!(initial.instances.len(), 3, "the W0 board has three jobs");
        assert!(
            initial
                .instances
                .iter()
                .all(|instance| instance.name.starts_with("Job "))
        );
        let html = mount
            .frozen_html("Distillery Chronicle")
            .expect("frozen table");
        assert_eq!(html.matches("<tr data-projection-instance=").count(), 3);
        assert!(html.contains("Every item and relationship in this projection"));
        assert!(html.contains("Observation tick: 2"));
        assert!(html.contains("Lease epoch:"));
        assert!(html.contains("Lease window:"));

        mount
            .endpoint_mut()
            .observe(
                &distillery_board(),
                &[
                    distillery::ResidentReceipt::MaintenanceIdle,
                    distillery::ResidentReceipt::StopRequested,
                    distillery::ResidentReceipt::MaintenanceIdle,
                ],
                distillery::ChronicleRevision::new(42, 7, 12),
            )
            .expect("later Chronicle observation");
        mount
            .rediscover_same_session()
            .expect("same-session rediscovery");
        let resumed = mount.resume().expect("contiguous Chronicle diff");
        assert!(matches!(resumed, ResumeApplication::Applied(_)));
        let receipt = mount.receipt("Distillery Chronicle").expect("receipt");
        let json = mount
            .receipt_json("Distillery Chronicle")
            .expect("machine receipt");
        let value: serde_json::Value = serde_json::from_str(&json).expect("receipt JSON");
        assert_eq!(value["endpoint_label"], "Distillery Chronicle");
        assert_eq!(
            json,
            include_str!("../docs/receipts/distillery_chronicle_w1.json").trim()
        );
        assert_eq!(receipt.snapshot_revision, 11);
        assert!(receipt.rediscovered_same_session);
        assert!(receipt.resumed_by_diff);
        assert_eq!(receipt.resumed_revision, 12);
        assert_eq!(receipt.frozen_instances, 3);
        assert_eq!(receipt.frozen_table_rows, 3);

        let resumed_html = mount
            .frozen_html("Distillery Chronicle")
            .expect("resumed frozen table");
        assert!(resumed_html.contains("Observation tick: 3"));
    }

    #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
    #[test]
    fn read_only_chronicle_can_be_registered_with_a_context_bound_factory() {
        let mut catalog = crate::native::endpoint_catalog::ResidentEndpointCatalog::new();
        let admitted_session = ProjectionSession("admitted:chronicle".into());
        let expected_session = admitted_session.clone();
        catalog
            .register_resumable_notifying(
                "distillery.chronicle",
                "Distillery Chronicle",
                move |context| {
                    assert_eq!(context.session(), &expected_session);
                    assert_eq!(context.subject(), [9; 32]);
                    Ok(distillery_endpoint(context.session().clone(), 41, 11))
                },
            )
            .expect("catalog registration");
        assert_eq!(catalog.offers()[0].id, "distillery.chronicle");
        assert_eq!(catalog.offers()[0].label, "Distillery Chronicle");
        let context = crate::lifecycle::AdmittedEndpointContext::new(admitted_session, [9; 32]);
        let session = catalog
            .open("distillery.chronicle", &context)
            .expect("open after admission");
        assert_eq!(session.describe().label, "Distillery Chronicle");
    }
}
