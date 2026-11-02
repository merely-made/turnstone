// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PresentationSource for TurnstoneEndpoint {
    type Error = String;

    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, Self::Error> {
        if request.session != self.session {
            return Err("resource request names the wrong session".into());
        }
        let bytes = self
            .resources
            .get(&request.resource)
            .cloned()
            .ok_or_else(|| "resource was not disclosed by this session".to_string())?;
        Ok(ResourceResponse {
            session: self.session.clone(),
            resource: request.resource,
            bytes,
        })
    }
}
