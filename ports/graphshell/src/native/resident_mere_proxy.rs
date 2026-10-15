// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Carrier-only bridge to an already admitted resident mere. Browser admission
//! happens before its host constructs this endpoint; browsers cannot choose an
//! application, persona, route, store or alternate authority through it.

use chirograph::{
    CarrierNotice, CarrierRequestBody, CarrierResponseBody, EndpointDescriptor, IntentInvocation,
    IntentResult, ProjectionRequest, ProjectionSnapshot, ResourceChunkRequest,
    ResourceChunkResponse, ResourceRequest, ResourceResponse, ResumeReply, ResumeRequest,
};
use graphshell_endpoint::door::app_admission::{AppId, AppRouteId};
use graphshell_endpoint::door::app_client::AppBrokerClient;
use graphshell_endpoint::{
    IntentSink, PresentationSource, ProjectionCatalog, ProjectionNoticeSource, ProjectionSource,
    ResumableProjectionSource,
};
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(5);
type Reply<T> = Sender<Result<T, String>>;
enum Command {
    Request(CarrierRequestBody, Reply<CarrierResponseBody>),
    Notice(Reply<Option<CarrierNotice>>),
    Stop,
}

/// A fixed `mere/divination` proxy, acting through the admitted Graphshell app.
/// Its worker owns only a carrier. All graphs and durable writes stay resident.
pub struct ResidentMereProxy {
    descriptor: EndpointDescriptor,
    send: Sender<Command>,
    worker: Option<JoinHandle<()>>,
}

impl ResidentMereProxy {
    pub fn divination(endpoint: &str) -> Result<Self, String> {
        let endpoint = endpoint.to_owned();
        let (send, receive) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        // Creating and dropping the current-thread runtime on its own thread
        // avoids nesting a blocking runtime in a WebRTC host's Tokio runtime.
        let worker = std::thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = ready.send(Err(error.to_string()));
                    return;
                },
            };
            let connected = runtime.block_on(async {
                tokio::time::timeout(
                    TIMEOUT,
                    AppBrokerClient::open_route_at(
                        &endpoint,
                        AppId::new("graphshell"),
                        AppRouteId::new("mere/divination").expect("fixed route"),
                    ),
                )
                .await
                .map_err(|_| "resident proxy attachment timed out".to_string())?
                .map_err(|e| e.to_string())
            });
            let mut client = match connected {
                Ok(client) => client,
                Err(error) => {
                    let _ = ready.send(Err(error));
                    return;
                },
            };
            let opened = runtime.block_on(async {
                tokio::time::timeout(TIMEOUT, client.open_session())
                    .await
                    .map_err(|_| "resident proxy open timed out".to_string())?
                    .map_err(|e| e.to_string())
            });
            let descriptor = match opened {
                Ok(opened) => opened.descriptor,
                Err(error) => {
                    let _ = ready.send(Err(error));
                    return;
                },
            };
            if ready.send(Ok(descriptor)).is_err() {
                return;
            }
            while let Ok(command) = receive.recv() {
                let failed = match command {
                    Command::Request(body, answer) => {
                        let result = runtime.block_on(async {
                            tokio::time::timeout(TIMEOUT, client.request_body(body))
                                .await
                                .map_err(|_| "resident proxy request timed out".to_string())?
                                .map_err(|e| e.to_string())
                        });
                        let failed = result.is_err();
                        let _ = answer.send(result);
                        failed
                    },
                    Command::Notice(answer) => {
                        let result = runtime.block_on(async {
                            tokio::time::timeout(TIMEOUT, client.take_notice())
                                .await
                                .map_err(|_| "resident proxy notice timed out".to_string())?
                                .map_err(|e| e.to_string())
                        });
                        let failed = result.is_err();
                        let _ = answer.send(result);
                        failed
                    },
                    Command::Stop => break,
                };
                if failed {
                    break;
                }
            }
            let _ = runtime.block_on(async { tokio::time::timeout(TIMEOUT, client.close()).await });
        });
        let descriptor = started
            .recv_timeout(TIMEOUT + Duration::from_secs(1))
            .map_err(|_| "resident proxy startup did not answer".to_string())??;
        Ok(Self {
            descriptor,
            send,
            worker: Some(worker),
        })
    }

    fn request(&self, body: CarrierRequestBody) -> Result<CarrierResponseBody, String> {
        let (send, reply) = mpsc::channel();
        self.send
            .send(Command::Request(body, send))
            .map_err(|_| "resident proxy closed".to_string())?;
        reply
            .recv_timeout(TIMEOUT + Duration::from_secs(1))
            .map_err(|_| "resident proxy response did not answer".to_string())?
    }
}

impl Drop for ResidentMereProxy {
    fn drop(&mut self) {
        let _ = self.send.send(Command::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl ProjectionCatalog for ResidentMereProxy {
    fn describe(&self) -> EndpointDescriptor {
        self.descriptor.clone()
    }
}
impl ProjectionSource for ResidentMereProxy {
    type Error = String;
    fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, String> {
        match self.request(CarrierRequestBody::Snapshot(request))? {
            CarrierResponseBody::Snapshot(snapshot) => Ok(*snapshot),
            other => Err(format!("unexpected resident snapshot response: {other:?}")),
        }
    }
}
impl PresentationSource for ResidentMereProxy {
    type Error = String;
    fn resource(&mut self, request: ResourceRequest) -> Result<ResourceResponse, String> {
        match self.request(CarrierRequestBody::Resource(request))? {
            CarrierResponseBody::Resource(resource) => Ok(resource),
            other => Err(format!("unexpected resident resource response: {other:?}")),
        }
    }
    fn resource_chunk(
        &mut self,
        request: ResourceChunkRequest,
    ) -> Result<ResourceChunkResponse, String> {
        match self.request(CarrierRequestBody::ResourceChunk(request))? {
            CarrierResponseBody::ResourceChunk(resource) => Ok(*resource),
            other => Err(format!("unexpected resident chunk response: {other:?}")),
        }
    }
}
impl IntentSink for ResidentMereProxy {
    type Error = String;
    fn invoke(&mut self, invocation: IntentInvocation) -> Result<IntentResult, String> {
        match self.request(CarrierRequestBody::Intent(invocation))? {
            CarrierResponseBody::Intent(result) => Ok(result),
            other => Err(format!("unexpected resident intent response: {other:?}")),
        }
    }
}
impl ProjectionNoticeSource for ResidentMereProxy {
    type Error = String;
    fn poll_notice(&mut self) -> Result<Option<CarrierNotice>, String> {
        let (send, reply) = mpsc::channel();
        self.send
            .send(Command::Notice(send))
            .map_err(|_| "resident proxy closed".to_string())?;
        reply
            .recv_timeout(TIMEOUT + Duration::from_secs(1))
            .map_err(|_| "resident proxy notice did not answer".to_string())?
    }
}
impl ResumableProjectionSource for ResidentMereProxy {
    type Error = String;
    fn resume(&mut self, request: ResumeRequest) -> Result<ResumeReply, String> {
        match self.request(CarrierRequestBody::Resume(request))? {
            CarrierResponseBody::Resume(reply) => Ok(reply),
            other => Err(format!("unexpected resident resume response: {other:?}")),
        }
    }
}
