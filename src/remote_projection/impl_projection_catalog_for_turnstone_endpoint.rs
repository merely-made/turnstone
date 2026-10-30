// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ProjectionCatalog for TurnstoneEndpoint {
    fn describe(&self) -> EndpointDescriptor {
        EndpointDescriptor {
            label: "Turnstone".into(),
            projections: vec![ProjectionOffer {
                label: "Browsing graph".into(),
                request: ProjectionRequest {
                    version: ProtocolVersion::V1,
                    session: self.session.clone(),
                    score: Score::new(Arrangement::Spiral(Spiral::default())),
                },
            }],
        }
    }
}
