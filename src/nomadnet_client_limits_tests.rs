// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::sync::Arc;

#[tokio::test]
async fn native_page_and_submit_propagate_independent_encoded_packed_and_body_limits() {
    use retinue::destination::DestinationName;
    let body = vec![b'x'; 4096];
    let packed_len = Response {
        request_id: AddressHash::from_bytes([0; 16]),
        data: body.clone(),
    }
    .pack()
    .len();
    for submit in [false, true] {
        for rejected_budget in [None, Some("encoded"), Some("packed"), Some("body")] {
            let server = Arc::new(Endpoint::new(PrivateIdentity::from_secret_bytes(
                &[0x57; IDENTITY_LEN],
            )));
            let tcp = server.listen_tcp(([127, 0, 0, 1], 0).into()).await.unwrap();
            let name = DestinationName::new("nomadnetwork", ["node"]);
            let destination = name.destination_hash(server.identity());
            server.register_resource(name, &[]);
            let owner = Arc::clone(&server);
            let reply = body.clone();
            let handler = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(4), async move {
                    let mut accepted = owner.accept_resource().await.unwrap();
                    accepted
                        .session
                        .set_config(retinue::endpoint::ResourceTransferConfig {
                            timeout: Duration::from_secs(1),
                            ..Default::default()
                        });
                    let received = accepted.session.receive_raw_request().await.unwrap();
                    if submit {
                        let request =
                            StringMapRequest::unpack(&received.packed, StringMapLimits::default())
                                .unwrap();
                        assert_eq!(request.data.get("field").map(String::as_str), Some("sent"));
                    }
                    // Wire-budget refusal may end before sender completion;
                    // the success/exact-body case below still requires proof.
                    accepted
                        .session
                        .respond_auto(received.request_id, reply)
                        .await
                })
                .await
                .expect("native budget fixture joins")
            });
            let policy = MicronFetchPolicy {
                timeout: Duration::from_secs(2),
                max_body_bytes: body.len() - usize::from(rejected_budget == Some("body")),
                receive: ResourceReceiveLimits {
                    max_encoded_bytes: if rejected_budget == Some("encoded") {
                        1
                    } else {
                        ResourceReceiveLimits::default().max_encoded_bytes
                    },
                    max_decoded_bytes: packed_len - usize::from(rejected_budget == Some("packed")),
                },
            };
            let address = format!("{destination}:/page/read.mu");
            let result = if submit {
                submit_form_on_interface_with_policy(
                    &address,
                    BTreeMap::from([("field".into(), "sent".into())]),
                    tcp,
                    policy,
                )
                .await
            } else {
                fetch_page_on_interface_with_policy(parse_address(&address).unwrap(), tcp, policy)
                    .await
            };
            if rejected_budget.is_none() {
                assert_eq!(result.unwrap().bytes, body);
                handler
                    .await
                    .unwrap()
                    .expect("exact-fit Resource is proved");
            } else {
                assert!(
                    result.is_err(),
                    "{submit:?} {rejected_budget:?} must not return a truncated success"
                );
                let _ = handler.await.unwrap();
            }
            server.close();
        }
    }
}

#[tokio::test]
async fn invalid_native_policy_refuses_before_interface_contact() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let policy = MicronFetchPolicy {
        receive: ResourceReceiveLimits {
            max_encoded_bytes: 0,
            ..ResourceReceiveLimits::default()
        },
        ..MicronFetchPolicy::default()
    };
    let error = fetch_page_on_interface_with_policy(
        parse_address("00000000000000000000000000000000:/page/read.mu").unwrap(),
        listener.local_addr().unwrap(),
        policy,
    )
    .await
    .unwrap_err();
    assert!(error.contains("positive"));
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}
