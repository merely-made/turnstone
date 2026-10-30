// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn permission(id: u64, origin: &str) -> PermissionRequest {
    PermissionRequest {
        id: UserAgentRequestId::new(id),
        origin: origin.into(),
        descriptors: vec![PermissionDescriptor::Geolocation],
    }
}

fn challenge(id: u64) -> HttpAuthenticationChallenge {
    HttpAuthenticationChallenge {
        id: UserAgentRequestId::new(id),
        protection_space: HttpProtectionSpace {
            origin_url: "https://secure.example/private".into(),
            host: "secure.example".into(),
            port: 443,
            realm: Some("private".into()),
            scheme: "basic".into(),
            is_proxy: false,
        },
    }
}

fn temporary_policy_path(label: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!("turnstone-s2-{label}-{}", Uuid::new_v4()))
        .join("web-policy.json")
}

#[test]
fn grant_and_denial_are_origin_descriptor_scoped() {
    let node = Uuid::new_v4();
    let now = Instant::now();
    let mut service = WebPolicyService::new(
        PermissionRegistry::private("profile"),
        Duration::from_secs(5),
    );
    let grant = permission(1, "https://one.example/path");
    assert_eq!(
        service.receive_permission(node, grant.clone(), now),
        PermissionDisposition::Pending
    );
    service
        .complete_permission(
            node,
            grant.id,
            PermissionAnswer::Grant,
            PermissionRetention::Remember,
        )
        .unwrap();
    assert_eq!(
        service.receive_permission(node, permission(2, "https://one.example/other"), now),
        PermissionDisposition::Answer(PermissionAnswer::Grant)
    );

    let deny = permission(3, "https://two.example/");
    assert_eq!(
        service.receive_permission(node, deny.clone(), now),
        PermissionDisposition::Pending
    );
    service
        .complete_permission(
            node,
            deny.id,
            PermissionAnswer::Deny,
            PermissionRetention::Remember,
        )
        .unwrap();
    assert_eq!(
        service.receive_permission(node, permission(4, "https://two.example/path"), now),
        PermissionDisposition::Answer(PermissionAnswer::Deny)
    );
}

#[test]
fn dismissal_is_not_a_retained_denial() {
    let node = Uuid::new_v4();
    let now = Instant::now();
    let mut service = WebPolicyService::new(
        PermissionRegistry::private("profile"),
        Duration::from_secs(5),
    );
    let request = permission(1, "https://one.example/");
    service.receive_permission(node, request.clone(), now);
    service
        .complete_permission(
            node,
            request.id,
            PermissionAnswer::Dismiss,
            PermissionRetention::Remember,
        )
        .unwrap();
    assert_eq!(
        service
            .registry()
            .state("https://one.example/", &PermissionDescriptor::Geolocation),
        PermissionState::Prompt
    );
}

#[test]
fn remembered_permission_survives_registry_restart() {
    let path = temporary_policy_path("restart");
    let node = Uuid::new_v4();
    let request = permission(1, "https://one.example/");
    {
        let registry =
            PermissionRegistry::load("default", ProfileStorage::Persistent(path.clone())).unwrap();
        let mut service = WebPolicyService::new(registry, Duration::from_secs(5));
        service.receive_permission(node, request.clone(), Instant::now());
        service
            .complete_permission(
                node,
                request.id,
                PermissionAnswer::Grant,
                PermissionRetention::Remember,
            )
            .unwrap();
    }
    let registry =
        PermissionRegistry::load("default", ProfileStorage::Persistent(path.clone())).unwrap();
    assert_eq!(
        registry.state(
            "https://one.example/path",
            &PermissionDescriptor::Geolocation
        ),
        PermissionState::Granted
    );
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn private_profiles_are_isolated_and_never_create_a_registry_file() {
    let node = Uuid::new_v4();
    let now = Instant::now();
    let mut first = WebPolicyService::new(
        PermissionRegistry::private("private-a"),
        Duration::from_secs(5),
    );
    let mut second = WebPolicyService::new(
        PermissionRegistry::private("private-b"),
        Duration::from_secs(5),
    );
    let request = permission(1, "https://one.example/");
    first.receive_permission(node, request.clone(), now);
    first
        .complete_permission(
            node,
            request.id,
            PermissionAnswer::Grant,
            PermissionRetention::Remember,
        )
        .unwrap();
    assert_eq!(
        second.receive_permission(node, permission(1, "https://one.example/"), now),
        PermissionDisposition::Pending
    );
}

#[test]
fn unanswered_requests_expire_without_retaining_policy() {
    let node = Uuid::new_v4();
    let now = Instant::now();
    let timeout = Duration::from_millis(10);
    let mut service = WebPolicyService::new(PermissionRegistry::private("profile"), timeout);
    service.receive_permission(node, permission(1, "https://one.example/"), now);
    service.receive_authentication(node, challenge(2), now);
    let later = now + timeout / 2;
    service.receive_permission(node, permission(3, "https://two.example/"), later);
    let mut pending_challenge = challenge(4);
    pending_challenge.protection_space.origin_url = "https://other.example/private".into();
    pending_challenge.protection_space.host = "other.example".into();
    service.receive_authentication(node, pending_challenge, later);
    let expired = service.expire(now + timeout);
    assert_eq!(expired.len(), 2);
    assert!(expired.contains(&PendingRequest::Permission {
        node,
        id: UserAgentRequestId::new(1)
    }));
    assert!(expired.contains(&PendingRequest::Authentication {
        node,
        id: UserAgentRequestId::new(2)
    }));
    assert_eq!(
        service
            .registry()
            .state("https://one.example/", &PermissionDescriptor::Geolocation),
        PermissionState::Prompt
    );
    let summary = service.summary(node);
    assert_eq!(
        summary
            .permissions
            .iter()
            .find(|item| item.origin == "https://one.example")
            .unwrap()
            .disposition,
        "timed-out"
    );
    assert_eq!(
        summary
            .authentication
            .iter()
            .find(|item| item.host == "secure.example")
            .unwrap()
            .disposition,
        "timed-out"
    );
    assert_eq!(
        summary
            .permissions
            .iter()
            .find(|item| item.origin == "https://two.example")
            .unwrap()
            .disposition,
        "pending"
    );
    assert_eq!(
        summary
            .authentication
            .iter()
            .find(|item| item.host == "other.example")
            .unwrap()
            .disposition,
        "pending"
    );
}

#[test]
fn credential_provider_answers_without_secret_facet_leakage() {
    let node = Uuid::new_v4();
    let challenge = challenge(1);
    let credentials = HttpCredentials {
        username: "private-user".into(),
        password: "private-password".into(),
    };
    let mut service = WebPolicyService::new(
        PermissionRegistry::private("profile"),
        Duration::from_secs(5),
    );
    service
        .credentials_mut()
        .insert(challenge.protection_space.clone(), credentials.clone());
    let mut another_path = challenge.clone();
    another_path.protection_space.origin_url = "https://secure.example/other".into();
    assert_eq!(
        service.receive_authentication(node, another_path, Instant::now()),
        AuthenticationDisposition::Answer(HttpAuthenticationAnswer::Credentials(credentials))
    );
    let projection = service.facet_value(node).to_string();
    assert!(!projection.contains("private-user"));
    assert!(!projection.contains("private-password"));
    assert!(projection.contains("secure.example"));
    assert!(projection.contains("private"));
}

#[test]
fn navigation_withdrawal_removes_old_request_identity() {
    let node = Uuid::new_v4();
    let mut service = WebPolicyService::new(
        PermissionRegistry::private("profile"),
        Duration::from_secs(5),
    );
    service.receive_permission(node, permission(21, "https://old.example/"), Instant::now());
    service.receive_authentication(node, challenge(22), Instant::now());

    assert_eq!(service.pending_nodes(), vec![node]);

    let withdrawn = service.withdraw_node(node, "navigation-started");
    assert!(withdrawn.contains(&PendingRequest::Permission {
        node,
        id: UserAgentRequestId::new(21),
    }));
    assert!(withdrawn.contains(&PendingRequest::Authentication {
        node,
        id: UserAgentRequestId::new(22),
    }));
    assert!(
        service
            .permission_request(node, UserAgentRequestId::new(21))
            .is_none()
    );
    assert!(
        service
            .authentication_challenge(node, UserAgentRequestId::new(22))
            .is_none()
    );
    assert_eq!(
        service.summary(node).permissions[0].disposition,
        "navigation-started"
    );
    assert_eq!(
        service.summary(node).authentication[0].disposition,
        "navigation-started"
    );
    assert!(service.pending_nodes().is_empty());
}

#[test]
fn one_shot_and_process_credentials_do_not_survive_restart() {
    let path = temporary_policy_path("ephemeral-restart");
    let node = Uuid::new_v4();
    let request = permission(31, "https://one.example/");
    let auth = challenge(32);
    {
        let registry =
            PermissionRegistry::load("default", ProfileStorage::Persistent(path.clone())).unwrap();
        let mut service = WebPolicyService::new(registry, Duration::from_secs(5));
        service.receive_permission(node, request.clone(), Instant::now());
        service
            .complete_permission(
                node,
                request.id,
                PermissionAnswer::Grant,
                PermissionRetention::OneShot,
            )
            .unwrap();
        service.receive_authentication(node, auth.clone(), Instant::now());
        service
            .complete_authentication(
                node,
                auth.id,
                &HttpAuthenticationAnswer::Credentials(HttpCredentials {
                    username: "restart-user".into(),
                    password: "restart-password".into(),
                }),
                true,
            )
            .unwrap();
    }

    let registry =
        PermissionRegistry::load("default", ProfileStorage::Persistent(path.clone())).unwrap();
    let mut restarted = WebPolicyService::new(registry, Duration::from_secs(5));
    assert_eq!(
        restarted.registry().state(
            "https://one.example/path",
            &PermissionDescriptor::Geolocation,
        ),
        PermissionState::Prompt
    );
    assert_eq!(
        restarted.receive_authentication(node, auth, Instant::now()),
        AuthenticationDisposition::Pending
    );
    assert!(!path.exists());
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
