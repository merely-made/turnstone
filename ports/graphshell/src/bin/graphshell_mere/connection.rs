// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The application door carries the shared panel's requests and notices.
//! Session validation, view state and lifecycle belong to MereRouteSession.
use chirograph::{
    CapabilityProfile, CarrierNotice, CarrierOutput, CarrierRequest, CarrierRequestBody,
    CarrierResponse, CarrierResponseBody, PresentationCapability, SessionOpened,
};
use graphshell_client::LineDriven;
use graphshell_endpoint::door::{
    app_admission::{AppId, AppRouteId},
    app_client::AppBrokerClient,
};
use mere_overmap::route_session::MereRouteSession;
use mere_view::MereViewRequest;

use super::Reading;

// The small transport seam lets native-adapter tests use real framed protocol
// responses without connecting to the user's running resident.
pub(super) trait RouteTransport {
    async fn request(&mut self, body: CarrierRequestBody) -> Result<CarrierResponseBody, String>;
    async fn take_notice(&mut self) -> Result<Option<CarrierNotice>, String>;
}

impl RouteTransport for AppBrokerClient {
    async fn request(&mut self, body: CarrierRequestBody) -> Result<CarrierResponseBody, String> {
        if let CarrierRequestBody::Resource(request) = body {
            // The shared controller asks for whole resources. The application
            // door has bounded native frames, so retain its chunked transfer
            // and address checks before returning the assembled resource.
            let bytes = self
                .resource(request.session.clone(), request.resource)
                .await
                .map_err(|error| error.to_string())?;
            return Ok(CarrierResponseBody::Resource(
                chirograph::ResourceResponse::new(request.session, bytes),
            ));
        }
        self.request_body(body)
            .await
            .map_err(|error| error.to_string())
    }

    async fn take_notice(&mut self) -> Result<Option<CarrierNotice>, String> {
        AppBrokerClient::take_notice(self)
            .await
            .map_err(|error| error.to_string())
    }
}

pub(super) struct Connection<T = AppBrokerClient> {
    transport: T,
    panel: MereRouteSession,
}

impl Connection {
    pub(super) async fn open(domain: &str) -> Result<Self, String> {
        let mut transport = AppBrokerClient::open_route(
            AppId::new("graphshell"),
            AppRouteId::new(format!("mere/{domain}")).map_err(|error| error.to_string())?,
        )
        .await
        .map_err(|error| error.to_string())?;
        let profile = CapabilityProfile::new([PresentationCapability::PortableCard]);
        let opened = transport
            .request(CarrierRequestBody::Open(Box::new(
                chirograph::SessionOpen {
                    version: chirograph::ProtocolVersion::V1,
                    capabilities: profile.clone(),
                },
            )))
            .await?;
        let CarrierResponseBody::Opened(opened) = opened else {
            return Err("The resident did not open the mere route.".into());
        };
        Self::from_opened(transport, *opened, profile)
    }
}

impl<T: RouteTransport> Connection<T> {
    fn from_opened(
        transport: T,
        opened: SessionOpened,
        profile: CapabilityProfile,
    ) -> Result<Self, String> {
        Ok(Self {
            transport,
            panel: MereRouteSession::from_descriptor(opened.descriptor, profile, "graphshell")?,
        })
    }

    pub(super) async fn read(&mut self) -> Result<Reading, String> {
        if let Err(reason) = self.settle().await {
            self.panel.fail(reason.clone());
            return Err(reason);
        }
        let saved_focus = self.panel.saved_view().focus.as_ref().and_then(|focus| {
            self.panel.records().iter().find_map(|(key, card)| {
                card.values
                    .iter()
                    .any(|value| value.label == "Address" && &value.value == focus)
                    .then(|| key.clone())
            })
        });
        // Browser hosts draw the panel's primary scene separately. This
        // native window paints its graph through MereView itself.
        let mut model = self.panel.model().clone();
        model.hide_graph = false;
        Ok(Reading {
            model,
            cards: self.panel.records().clone(),
            graph_epoch: self
                .panel
                .primary()
                .map_or(0, |primary| primary.scene.epoch.0),
            selected: self.panel.focus_key(),
            saved_focus,
            read_only: self.panel.read_only(),
            view_saved: self.panel.view_saved(),
        })
    }

    pub(super) fn take(&mut self, request: MereViewRequest) {
        if let Err(reason) = self.panel.request(request) {
            self.panel.show_notice(reason);
        }
    }

    pub(super) async fn take_notice(&mut self) -> Result<bool, String> {
        let result: Result<bool, String> = async {
            let Some(notice) = self.transport.take_notice().await? else {
                return Ok(false);
            };
            let line = chirograph::line::encode(&CarrierOutput::Notice(notice))
                .map_err(|error| error.to_string())?;
            LineDriven::on_line(&mut self.panel, &line)?;
            Ok(true)
        }
        .await;
        if let Err(reason) = &result {
            self.panel.fail(reason.clone());
        }
        result
    }

    async fn settle(&mut self) -> Result<(), String> {
        loop {
            let outgoing = self.panel.take_outgoing();
            if outgoing.is_empty() {
                if self.panel.in_flight() {
                    return Err("The shared mere is waiting without a carrier request.".into());
                }
                return match &self.panel.model().status {
                    mere_view::ViewStatus::Unavailable(note) => Err(note.message.clone()),
                    _ => Ok(()),
                };
            }
            for line in outgoing {
                let request = chirograph::line::decode::<CarrierRequest>(&line)
                    .map_err(|error| error.to_string())?
                    .ok_or("The shared mere emitted an empty request.")?;
                // The broker validates its own request/response ids. Restore
                // the driver's id when translating its body back into a line.
                let response = CarrierOutput::Response(CarrierResponse {
                    id: request.id,
                    body: Ok(self.transport.request(request.body).await?),
                });
                let line =
                    chirograph::line::encode(&response).map_err(|error| error.to_string())?;
                LineDriven::on_line(&mut self.panel, &line)?;
            }
        }
    }
}

#[cfg(test)]
#[path = "connection_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "connection_door_tests.rs"]
mod door_tests;
