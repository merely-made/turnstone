// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

#![cfg(feature = "native")]
use graphshell_endpoint::door::app_admission::AppId;
use graphshell_endpoint::door::custody_client::CustodyClient;

#[tokio::test]
#[ignore = "run scripts/dr_c_receipts.py against the isolated receipt keeper"]
async fn dr_c_graphshell_identity_stays_pending() {
    let mode = std::env::var("DR_C_RECEIPT_MODE").expect("receipt mode");
    match CustodyClient::open(AppId::new("graphshell")).await {
        Err(error) => {
            assert_eq!(mode, "absent");
            assert!(error.is_pending());
        },
        Ok(mut client) => {
            assert_eq!(mode, "locked");
            let status = client.status().await.unwrap();
            assert_eq!(
                status.lock,
                graphshell_endpoint::door::identity::VaultLockView::Locked
            );
            assert!(status.persona_public_key.is_none());
            assert!(
                !client.roster().await.unwrap().entries.is_empty(),
                "public roster remains readable"
            );
            let result = graphshell_endpoint::door::profile::GraphshellIdentity::from_djinn(
                &mut client,
                &["dr-c"],
            )
            .await;
            assert!(
                matches!(result, Err(error) if error.is_pending()),
                "Graphshell must have no session key"
            );
        },
    }
}
