// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Reticulum link behind NomadNet page and form requests. Only this module
//! names Retinue, so a build without the `nomadnet` feature carries none of it.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;

use retinue::endpoint::{Endpoint, ResourceReceiveLimits};
use retinue::hash::AddressHash;
use retinue::identity::{IDENTITY_LEN, PrivateIdentity};
use retinue::request::{Response, StringMapLimits, StringMapRequest};

use super::{NomadNetAddress, parse_address};

/// Independent native wire/envelope budgets and the exact application body cap.
/// The timeout covers discovery, transfer, decoding and graceful shutdown.
#[derive(Clone, Copy, Debug)]
pub struct MicronFetchPolicy {
    pub timeout: Duration,
    pub max_body_bytes: usize,
    pub receive: ResourceReceiveLimits,
}

impl Default for MicronFetchPolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_body_bytes: 4 * 1024 * 1024,
            receive: ResourceReceiveLimits::default(),
        }
    }
}

impl MicronFetchPolicy {
    /// Resolve product launch settings for each new request. Existing body and
    /// timeout knobs retain their defaults/ranges; new wire budgets are explicit.
    pub fn from_environment() -> Result<Self, String> {
        let defaults = Self::default();
        Ok(Self {
            timeout: Duration::from_secs(
                std::env::var("TURNSTONE_NOMADNET_TIMEOUT_SECS")
                    .ok()
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(30)
                    .clamp(1, 120),
            ),
            max_body_bytes: std::env::var("TURNSTONE_NOMADNET_MAX_PAGE_BYTES")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(defaults.max_body_bytes)
                .clamp(1, 64 * 1024 * 1024),
            receive: ResourceReceiveLimits {
                max_encoded_bytes: positive_budget_from_environment(
                    "TURNSTONE_NOMADNET_MAX_ENCODED_BYTES",
                    defaults.receive.max_encoded_bytes,
                )?,
                max_decoded_bytes: positive_budget_from_environment(
                    "TURNSTONE_NOMADNET_MAX_PACKED_BYTES",
                    defaults.receive.max_decoded_bytes,
                )?,
            },
        })
    }

    fn validate(&self) -> Result<(), String> {
        if self.timeout.is_zero()
            || self.max_body_bytes == 0
            || self.receive.max_encoded_bytes == 0
            || self.receive.max_decoded_bytes == 0
        {
            return Err("Micron request limits must be positive".into());
        }
        Ok(())
    }
}

fn positive_budget_from_environment(name: &str, fallback: usize) -> Result<usize, String> {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| format!("{name} must be a positive byte count")),
        Err(std::env::VarError::NotPresent) => Ok(fallback),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(format!("{name} must be a positive byte count"))
        },
    }
}

/// Fetch a NomadNet page with the endpoint address supplied by the host.
///
/// `TURNSTONE_NOMADNET_TCP` is the local or remote Reticulum TCP interface.
/// The destination's public identity is learned from an authenticated path
/// response, so ordinary navigation never asks users to paste peer keys.
pub async fn fetch_page(address: &str) -> Result<crate::action::FetchedPage, String> {
    let address = parse_address(address)
        .ok_or_else(|| "NomadNet address must be destinationhex:/absolute/page/path".to_string())?;
    let tcp = std::env::var("TURNSTONE_NOMADNET_TCP")
        .map_err(|_| "NomadNet is not configured: set TURNSTONE_NOMADNET_TCP".to_string())?
        .parse::<SocketAddr>()
        .map_err(|error| format!("invalid TURNSTONE_NOMADNET_TCP: {error}"))?;
    fetch_page_on_interface_with_policy(address, tcp, MicronFetchPolicy::from_environment()?).await
}

/// The ordinary page operation with caller-supplied interface settings. The
/// environment entry point above retains its existing validation and policy.
#[cfg(test)]
pub(crate) async fn fetch_page_on_interface(
    address: NomadNetAddress,
    tcp: SocketAddr,
    timeout: Duration,
    max_page_bytes: usize,
) -> Result<crate::action::FetchedPage, String> {
    fetch_page_on_interface_with_policy(
        address,
        tcp,
        MicronFetchPolicy {
            timeout,
            max_body_bytes: max_page_bytes,
            ..MicronFetchPolicy::default()
        },
    )
    .await
}

pub async fn fetch_page_on_interface_with_policy(
    address: NomadNetAddress,
    tcp: SocketAddr,
    policy: MicronFetchPolicy,
) -> Result<crate::action::FetchedPage, String> {
    policy.validate()?;
    let timeout = policy.timeout;
    let destination = AddressHash::from_bytes(address.destination_bytes());
    let result = tokio::time::timeout(timeout, async {
        // Endpoint owns its interface and its Drop closes it. Keeping the full
        // operation in this future makes cancellation and the host deadline
        // release that transport even during connect or discovery.
        let mut secret = [0_u8; IDENTITY_LEN];
        getrandom::getrandom(&mut secret)
            .map_err(|error| format!("could not create ephemeral Reticulum identity: {error}"))?;
        let endpoint = Endpoint::new(PrivateIdentity::from_secret_bytes(&secret));
        secret.fill(0);
        endpoint
            .attach_tcp_client(tcp)
            .await
            .map_err(|error| format!("could not connect NomadNet interface: {error}"))?;
        endpoint.request_path(destination);
        let peer = loop {
            if let Some(peer) = endpoint.resolve(destination) {
                break peer;
            }
            endpoint
                .next_announcement()
                .await
                .map_err(|error| error.to_string())?;
        };
        let config = retinue::nomadnet::FetchPageConfig {
            max_page_bytes: policy.max_body_bytes,
            transfer: retinue::endpoint::ResourceTransferConfig {
                timeout,
                ..retinue::endpoint::ResourceTransferConfig::default()
            },
        };
        let result = retinue::nomadnet::fetch_page_with_limits(
            &endpoint,
            destination,
            peer,
            address.path().as_bytes(),
            config,
            policy.receive,
        )
        .await;
        // The completed request drops its session, queuing its Resource proof
        // and link close. Drain them before Endpoint::Drop aborts the writer;
        // the existing outer request deadline still bounds this shutdown.
        endpoint.shutdown(timeout).await;
        result.map_err(|error| format!("NomadNet page request failed: {error}"))
    })
    .await
    .map_err(|_| {
        format!(
            "NomadNet page request timed out after {}s",
            timeout.as_secs()
        )
    })?;
    let bytes = result?;
    Ok(crate::action::FetchedPage {
        content_type: None,
        content_disposition: None,
        body: String::from_utf8_lossy(&bytes).into_owned(),
        bytes,
    })
}

/// Submit one already-authorized map to a native Micron handler.  This is a
/// separate operation from page fetching: it never follows a response as a
/// navigation and accepts only the exact destination/path the form named.
pub async fn submit_form(
    address: &str,
    values: BTreeMap<String, String>,
) -> Result<crate::action::FetchedPage, String> {
    let tcp = std::env::var("TURNSTONE_NOMADNET_TCP")
        .map_err(|_| "NomadNet is not configured: set TURNSTONE_NOMADNET_TCP".to_string())?
        .parse::<SocketAddr>()
        .map_err(|error| format!("invalid TURNSTONE_NOMADNET_TCP: {error}"))?;
    submit_form_on_interface_with_policy(
        address,
        values,
        tcp,
        MicronFetchPolicy::from_environment()?,
    )
    .await
}

#[cfg(test)]
async fn submit_form_on_interface(
    address: &str,
    values: BTreeMap<String, String>,
    tcp: SocketAddr,
    timeout: Duration,
    max_response_bytes: usize,
) -> Result<crate::action::FetchedPage, String> {
    submit_form_on_interface_with_policy(
        address,
        values,
        tcp,
        MicronFetchPolicy {
            timeout,
            max_body_bytes: max_response_bytes,
            ..MicronFetchPolicy::default()
        },
    )
    .await
}

pub async fn submit_form_on_interface_with_policy(
    address: &str,
    values: BTreeMap<String, String>,
    tcp: SocketAddr,
    policy: MicronFetchPolicy,
) -> Result<crate::action::FetchedPage, String> {
    let address = parse_address(address)
        .ok_or_else(|| "Micron form target must be destinationhex:/absolute/path".to_string())?;
    policy.validate()?;
    let timeout = policy.timeout;
    let max_response_bytes = policy.max_body_bytes;
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?
        .as_secs_f64();
    let packed = StringMapRequest::new(address.path().as_bytes(), values, time)
        .pack(StringMapLimits::default())
        .map_err(|_| "Micron form exceeds the native request limit".to_string())?;
    let destination = AddressHash::from_bytes(address.destination_bytes());
    let result = tokio::time::timeout(timeout, async {
        let mut secret = [0_u8; IDENTITY_LEN];
        getrandom::getrandom(&mut secret)
            .map_err(|error| format!("could not create ephemeral Reticulum identity: {error}"))?;
        let endpoint = Endpoint::new(PrivateIdentity::from_secret_bytes(&secret));
        secret.fill(0);
        endpoint
            .attach_tcp_client(tcp)
            .await
            .map_err(|error| format!("could not connect NomadNet interface: {error}"))?;
        endpoint.request_path(destination);
        let peer = loop {
            if let Some(peer) = endpoint.resolve(destination) {
                break peer;
            }
            endpoint
                .next_announcement()
                .await
                .map_err(|error| error.to_string())?;
        };
        let mut session = endpoint
            .open_resource(destination, peer)
            .await
            .map_err(|error| format!("Micron form link failed: {error}"))?;
        session.set_config(retinue::endpoint::ResourceTransferConfig {
            timeout,
            ..Default::default()
        });
        let response = session
            .request_raw_with_limits(&packed, policy.receive)
            .await;
        drop(session);
        // The session queues its close before shutdown. Let the queued
        // Resource proof and link close reach the peer before Endpoint::Drop
        // aborts the interface writer. The outer request deadline still applies.
        endpoint.shutdown(timeout).await;
        let response = response.map_err(|error| format!("Micron form response failed: {error}"))?;
        let response =
            Response::unpack_with_limit(&response.packed, max_response_bytes).map_err(|error| {
                match error {
                    retinue::Error::CapacityExceeded => {
                        "Micron form response exceeds TURNSTONE_NOMADNET_MAX_PAGE_BYTES".to_string()
                    },
                    _ => "Micron form response envelope was invalid".to_string(),
                }
            })?;
        Ok::<_, String>(response.data)
    })
    .await
    .map_err(|_| {
        format!(
            "Micron form request timed out after {}s; remote outcome may be unknown",
            timeout.as_secs()
        )
    })?;
    let bytes = result?;
    if bytes.len() > max_response_bytes {
        return Err("Micron form response exceeds TURNSTONE_NOMADNET_MAX_PAGE_BYTES".to_string());
    }
    Ok(crate::action::FetchedPage {
        content_type: None,
        content_disposition: None,
        body: String::from_utf8_lossy(&bytes).into_owned(),
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn micron_form_explicit_interface_sends_map_and_bounds_resource_reply() {
        use retinue::destination::DestinationName;
        use std::sync::Arc;
        for limit in [8192, 8] {
            let server = Arc::new(Endpoint::new(PrivateIdentity::from_secret_bytes(
                &[0x45; IDENTITY_LEN],
            )));
            let tcp = server.listen_tcp(([127, 0, 0, 1], 0).into()).await.unwrap();
            let name = DestinationName::new("nomadnetwork", ["node"]);
            let destination = name.destination_hash(server.identity());
            server.register_resource(name, &[]);
            let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
            let handler_endpoint = Arc::clone(&server);
            let handler = tokio::spawn(async move {
                let mut accepted = handler_endpoint.accept_resource().await.unwrap();
                let received = accepted.session.receive_raw_request().await.unwrap();
                let request =
                    StringMapRequest::unpack(&received.packed, StringMapLimits::default()).unwrap();
                assert_eq!(
                    request.path_hash,
                    retinue::hash::AddressHash::of(b"/page/capture.mu")
                );
                assert_eq!(
                    request.data,
                    BTreeMap::from([("field_note".into(), "café 雪".into())])
                );
                accepted
                    .session
                    .respond_auto(received.request_id, vec![b'x'; 4096])
                    .await
                    .unwrap();
                // Keep the response session live until the client has decoded
                // its Resource, not merely until its proof was received.
                let _ = done_rx.await;
            });
            let result = submit_form_on_interface(
                &format!("{destination}:/page/capture.mu"),
                BTreeMap::from([("field_note".into(), "café 雪".into())]),
                tcp,
                Duration::from_secs(10),
                limit,
            )
            .await;
            let _ = done_tx.send(());
            if limit == 8192 {
                assert_eq!(result.unwrap().bytes, vec![b'x'; 4096]);
            } else {
                assert!(result.unwrap_err().contains("MAX_PAGE_BYTES"));
            }
            tokio::time::timeout(Duration::from_secs(10), handler)
                .await
                .unwrap()
                .unwrap();
            server.close();
        }
    }
}

#[cfg(test)]
#[path = "../nomadnet_client_limits_tests.rs"]
mod client_limits_tests;
