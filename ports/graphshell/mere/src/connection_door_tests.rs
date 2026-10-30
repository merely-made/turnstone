// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use chirograph::{
    CarrierRequest, CarrierResponse, CarrierResponseBody, ContentHash, MAX_RESOURCE_CHUNK_BYTES,
    ProjectionSession, ResourceChunkResponse, ResourceRequest,
};
use djinn::door::app_admission::{AppHello, AppId, AppRouteId};
use djinn::door::frame::{read_native_message_async, write_native_message_async};
use djinn::door::wire::{
        APP_CONNECT_SCHEMA, AppHostMessage as WireHostMessage, AppMessage as WireMessage, BROWSER_CHALLENGE_SCHEMA, BrowserChallenge,
    };
use std::time::Duration;
type AppMessage = WireMessage<chirograph::CarrierRequest, ()>;
type AppHostMessage = WireHostMessage<chirograph::CarrierResponse, chirograph::CarrierNotice, ()>;
use tokio::{net::UnixListener, time::timeout};

#[tokio::test(flavor = "multi_thread")]
async fn application_door_resource_is_assembled_from_bounded_chunk_frames() {
    let directory = tempfile::tempdir_in("/tmp").unwrap();
    let socket = directory.path().join("mere-route-test.sock");
    let socket_path = socket.to_string_lossy().into_owned();
    let listener = UnixListener::bind(&socket).unwrap();
    let session = ProjectionSession("djinn.mere/v1/graph".into());
    let bytes = vec![0x5a; 1_300_123];
    let resource = ContentHash::of(&bytes);
    let expected = bytes.clone();
    let client_session = session.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = timeout(Duration::from_secs(10), listener.accept())
            .await
            .expect("client connect timed out")
            .unwrap();
        let hello: AppHello = read_native_message_async(&mut stream)
            .await
            .unwrap()
            .unwrap();
        let admitted = hello.clone().accept().unwrap();
        assert_eq!(admitted.route.as_str(), "mere/native-test");
        write_native_message_async(
            &mut stream,
            &AppHostMessage::Challenge {
                challenge: BrowserChallenge {
                    schema: BROWSER_CHALLENGE_SCHEMA.into(),
                    host_nonce: "fixture-host-nonce".into(),
                },
            },
        )
        .await
        .unwrap();
        let connect: AppMessage = read_native_message_async(&mut stream)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            connect,
            AppMessage::Connect { schema, .. } if schema == APP_CONNECT_SCHEMA
        ));
        write_native_message_async(
            &mut stream,
            &AppHostMessage::Connected {
                app: hello.app,
                session: "fixture-session".into(),
            },
        )
        .await
        .unwrap();

        let mut offset = 0u64;
        let mut count = 0;
        while offset < bytes.len() as u64 {
            let message: AppMessage = timeout(
                Duration::from_secs(10),
                read_native_message_async(&mut stream),
            )
            .await
            .expect("resource request timed out")
            .unwrap()
            .unwrap();
            let AppMessage::Request { request } = message else {
                panic!("expected resource chunk request, got {message:?}");
            };
            let CarrierRequest { id, body } = request;
            let chirograph::CarrierRequestBody::ResourceChunk(chunk_request) = body else {
                panic!("the native route must receive chunks, got {body:?}");
            };
            assert_eq!(chunk_request.session, session);
            assert_eq!(chunk_request.resource, resource);
            assert_eq!(chunk_request.offset, offset);
            assert_eq!(chunk_request.length, MAX_RESOURCE_CHUNK_BYTES);
            let chunk = ResourceChunkResponse::from_slice(
                session.clone(),
                resource,
                &bytes,
                offset,
                chunk_request.length,
            );
            offset += chunk.decode().expect("fixture chunk should hash").len() as u64;
            count += 1;
            write_native_message_async(
                &mut stream,
                &AppHostMessage::Response {
                    response: CarrierResponse {
                        id,
                        body: Ok(CarrierResponseBody::ResourceChunk(Box::new(chunk))),
                    },
                },
            )
            .await
            .unwrap();
        }
        assert_eq!(count, 3);
    });

    let transfer = async {
        let mut client = graphshell_endpoint::door::projection::AppBrokerClient::open_route_at(
            &socket_path,
            AppId::new("graphshell"),
            AppRouteId::new("mere/native-test").unwrap(),
        )
        .await
        .unwrap();
        let body = RouteTransport::request(
            &mut client,
            chirograph::CarrierRequestBody::Resource(ResourceRequest {
                session: client_session,
                resource,
            }),
        )
        .await
        .unwrap();
        let CarrierResponseBody::Resource(assembled) = body else {
            panic!("expected assembled resource bytes");
        };
        assert_eq!(assembled.bytes, expected);
        assert!(assembled.has_valid_address());
    };
    timeout(Duration::from_secs(15), transfer)
        .await
        .expect("client resource transfer timed out");
    timeout(Duration::from_secs(10), server)
        .await
        .expect("fake resident timed out")
        .unwrap();
}
