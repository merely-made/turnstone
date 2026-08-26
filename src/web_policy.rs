//! Profile-scoped browser policy and credential-provider state.
//!
//! Permission decisions are durable profile/origin facts. Credentials are
//! process-memory provider values and never enter the serialized registry or
//! the node-facing summary projection.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use inker::{
    HttpAuthenticationAnswer, HttpAuthenticationChallenge, HttpCredentials, HttpProtectionSpace,
    PermissionAnswer, PermissionDescriptor, PermissionRequest, PermissionState, UserAgentRequestId,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const USER_AGENT_POLICY_FACET: &str = "web.user-agent-policy";
const POLICY_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyError {
    Io(String),
    InvalidData(String),
    ProfileMismatch { expected: String, found: String },
    RequestNotPending,
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(reason) => write!(f, "policy registry I/O failed: {reason}"),
            Self::InvalidData(reason) => write!(f, "invalid policy registry: {reason}"),
            Self::ProfileMismatch { expected, found } => {
                write!(
                    f,
                    "policy registry belongs to profile {found}, not {expected}"
                )
            }
            Self::RequestNotPending => f.write_str("user-agent request is no longer pending"),
        }
    }
}

impl std::error::Error for PolicyError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileStorage {
    Persistent(PathBuf),
    Private,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionRetention {
    OneShot,
    Remember,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct PermissionKey {
    origin: String,
    descriptor: PermissionDescriptor,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PermissionRecord {
    key: PermissionKey,
    state: PermissionState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedPolicy {
    version: u32,
    profile_id: String,
    permissions: Vec<PermissionRecord>,
}

/// The durable permission authority for one browser profile.
pub struct PermissionRegistry {
    profile_id: String,
    storage: ProfileStorage,
    remembered: BTreeMap<PermissionKey, PermissionState>,
}

impl PermissionRegistry {
    pub fn load(
        profile_id: impl Into<String>,
        storage: ProfileStorage,
    ) -> Result<Self, PolicyError> {
        let profile_id = profile_id.into();
        let mut registry = Self {
            profile_id,
            storage,
            remembered: BTreeMap::new(),
        };
        let ProfileStorage::Persistent(path) = &registry.storage else {
            return Ok(registry);
        };
        if !path.exists() {
            return Ok(registry);
        }
        let bytes = std::fs::read(path).map_err(|error| PolicyError::Io(error.to_string()))?;
        let persisted: PersistedPolicy = serde_json::from_slice(&bytes)
            .map_err(|error| PolicyError::InvalidData(error.to_string()))?;
        if persisted.version != POLICY_VERSION {
            return Err(PolicyError::InvalidData(format!(
                "unsupported version {}",
                persisted.version
            )));
        }
        if persisted.profile_id != registry.profile_id {
            return Err(PolicyError::ProfileMismatch {
                expected: registry.profile_id,
                found: persisted.profile_id,
            });
        }
        registry.remembered = persisted
            .permissions
            .into_iter()
            .map(|record| (record.key, record.state))
            .collect();
        Ok(registry)
    }

    pub fn private(profile_id: impl Into<String>) -> Self {
        Self::load(profile_id, ProfileStorage::Private)
            .expect("a private profile performs no registry I/O")
    }

    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn state(&self, origin: &str, descriptor: &PermissionDescriptor) -> PermissionState {
        let key = PermissionKey {
            origin: canonical_origin(origin),
            descriptor: descriptor.clone(),
        };
        self.remembered
            .get(&key)
            .copied()
            .unwrap_or(PermissionState::Prompt)
    }

    fn remember(
        &mut self,
        origin: &str,
        descriptors: &[PermissionDescriptor],
        answer: PermissionAnswer,
        retention: PermissionRetention,
    ) -> Result<(), PolicyError> {
        if retention == PermissionRetention::OneShot || answer == PermissionAnswer::Dismiss {
            return Ok(());
        }
        let state = match answer {
            PermissionAnswer::Grant => PermissionState::Granted,
            PermissionAnswer::Deny => PermissionState::Denied,
            PermissionAnswer::Dismiss => unreachable!(),
        };
        let origin = canonical_origin(origin);
        for descriptor in descriptors {
            self.remembered.insert(
                PermissionKey {
                    origin: origin.clone(),
                    descriptor: descriptor.clone(),
                },
                state,
            );
        }
        self.save()
    }

    fn save(&self) -> Result<(), PolicyError> {
        let ProfileStorage::Persistent(path) = &self.storage else {
            return Ok(());
        };
        let persisted = PersistedPolicy {
            version: POLICY_VERSION,
            profile_id: self.profile_id.clone(),
            permissions: self
                .remembered
                .iter()
                .map(|(key, state)| PermissionRecord {
                    key: key.clone(),
                    state: *state,
                })
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&persisted)
            .map_err(|error| PolicyError::InvalidData(error.to_string()))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| PolicyError::Io(error.to_string()))?;
        }
        std::fs::write(path, bytes).map_err(|error| PolicyError::Io(error.to_string()))
    }
}

pub(crate) fn canonical_origin(input: &str) -> String {
    match url::Url::parse(input) {
        Ok(url) => {
            let origin = url.origin().ascii_serialization();
            if origin == "null" {
                input.trim().to_ascii_lowercase()
            } else {
                origin
            }
        }
        Err(_) => input.trim().to_ascii_lowercase(),
    }
}

/// Process-memory credential provider. This type has no serialization API.
#[derive(Default)]
pub struct CredentialProvider {
    credentials: BTreeMap<ProtectionSpaceKey, HttpCredentials>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ProtectionSpaceKey {
    host: String,
    port: u16,
    realm: Option<String>,
    scheme: String,
    is_proxy: bool,
}

impl From<&HttpProtectionSpace> for ProtectionSpaceKey {
    fn from(space: &HttpProtectionSpace) -> Self {
        Self {
            host: space.host.to_ascii_lowercase(),
            port: space.port,
            realm: space.realm.clone(),
            scheme: space.scheme.to_ascii_lowercase(),
            is_proxy: space.is_proxy,
        }
    }
}

impl CredentialProvider {
    pub fn insert(&mut self, space: HttpProtectionSpace, credentials: HttpCredentials) {
        self.credentials
            .insert(ProtectionSpaceKey::from(&space), credentials);
    }

    pub fn get(&self, space: &HttpProtectionSpace) -> Option<&HttpCredentials> {
        self.credentials.get(&ProtectionSpaceKey::from(space))
    }

    pub fn remove(&mut self, space: &HttpProtectionSpace) {
        self.credentials.remove(&ProtectionSpaceKey::from(space));
    }
}

#[derive(Clone, Debug)]
struct PendingPermission {
    request: PermissionRequest,
    deadline: Instant,
}

#[derive(Clone, Debug)]
struct PendingAuthentication {
    challenge: HttpAuthenticationChallenge,
    deadline: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermissionDisposition {
    Answer(PermissionAnswer),
    Pending,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthenticationDisposition {
    Answer(HttpAuthenticationAnswer),
    Pending,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingRequest {
    Permission { node: Uuid, id: UserAgentRequestId },
    Authentication { node: Uuid, id: UserAgentRequestId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionSummary {
    pub origin: String,
    pub descriptors: Vec<PermissionDescriptor>,
    pub state: PermissionState,
    pub disposition: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticationSummary {
    pub host: String,
    pub port: u16,
    pub realm: Option<String>,
    pub scheme: String,
    pub is_proxy: bool,
    pub disposition: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodePolicySummary {
    pub permissions: Vec<PermissionSummary>,
    pub authentication: Vec<AuthenticationSummary>,
}

/// Pending request coordinator around one profile registry and credential
/// provider. `node + id` is the correlation key because inker ids are scoped
/// to their emitting surface.
pub struct WebPolicyService {
    registry: PermissionRegistry,
    credentials: CredentialProvider,
    timeout: Duration,
    permissions: BTreeMap<(Uuid, UserAgentRequestId), PendingPermission>,
    authentication: BTreeMap<(Uuid, UserAgentRequestId), PendingAuthentication>,
    summaries: BTreeMap<Uuid, NodePolicySummary>,
}

impl WebPolicyService {
    pub fn new(registry: PermissionRegistry, timeout: Duration) -> Self {
        Self {
            registry,
            credentials: CredentialProvider::default(),
            timeout,
            permissions: BTreeMap::new(),
            authentication: BTreeMap::new(),
            summaries: BTreeMap::new(),
        }
    }

    pub fn registry(&self) -> &PermissionRegistry {
        &self.registry
    }

    pub fn credentials_mut(&mut self) -> &mut CredentialProvider {
        &mut self.credentials
    }

    pub fn receive_permission(
        &mut self,
        node: Uuid,
        request: PermissionRequest,
        now: Instant,
    ) -> PermissionDisposition {
        let states = request
            .descriptors
            .iter()
            .map(|descriptor| self.registry.state(&request.origin, descriptor))
            .collect::<Vec<_>>();
        let answer = if !states.is_empty()
            && states
                .iter()
                .all(|state| *state == PermissionState::Granted)
        {
            Some(PermissionAnswer::Grant)
        } else if states.iter().any(|state| *state == PermissionState::Denied) {
            Some(PermissionAnswer::Deny)
        } else {
            None
        };
        let disposition = answer
            .map(|answer| format!("answered:{answer:?}").to_ascii_lowercase())
            .unwrap_or_else(|| "pending".into());
        self.summaries
            .entry(node)
            .or_default()
            .permissions
            .push(PermissionSummary {
                origin: canonical_origin(&request.origin),
                descriptors: request.descriptors.clone(),
                state: answer.map_or(PermissionState::Prompt, |answer| match answer {
                    PermissionAnswer::Grant => PermissionState::Granted,
                    PermissionAnswer::Deny => PermissionState::Denied,
                    PermissionAnswer::Dismiss => PermissionState::Prompt,
                }),
                disposition,
            });
        if let Some(answer) = answer {
            PermissionDisposition::Answer(answer)
        } else {
            self.permissions.insert(
                (node, request.id),
                PendingPermission {
                    request,
                    deadline: now + self.timeout,
                },
            );
            PermissionDisposition::Pending
        }
    }

    pub fn permission_request(
        &self,
        node: Uuid,
        id: UserAgentRequestId,
    ) -> Option<&PermissionRequest> {
        self.permissions
            .get(&(node, id))
            .map(|pending| &pending.request)
    }

    pub fn complete_permission(
        &mut self,
        node: Uuid,
        id: UserAgentRequestId,
        answer: PermissionAnswer,
        retention: PermissionRetention,
    ) -> Result<(), PolicyError> {
        let pending = self
            .permissions
            .get(&(node, id))
            .ok_or(PolicyError::RequestNotPending)?;
        let origin = pending.request.origin.clone();
        let descriptors = pending.request.descriptors.clone();
        let remember_result = self.registry.remember(
            &pending.request.origin,
            &pending.request.descriptors,
            answer,
            retention,
        );
        self.permissions.remove(&(node, id));
        self.update_permission_summary(node, &origin, &descriptors, answer, None);
        remember_result
    }

    pub fn receive_authentication(
        &mut self,
        node: Uuid,
        challenge: HttpAuthenticationChallenge,
        now: Instant,
    ) -> AuthenticationDisposition {
        let credentials = self.credentials.get(&challenge.protection_space).cloned();
        let disposition = if credentials.is_some() {
            "answered:credentials"
        } else {
            "pending"
        };
        self.summaries
            .entry(node)
            .or_default()
            .authentication
            .push(authentication_summary(&challenge, disposition));
        if let Some(credentials) = credentials {
            AuthenticationDisposition::Answer(HttpAuthenticationAnswer::Credentials(credentials))
        } else {
            self.authentication.insert(
                (node, challenge.id),
                PendingAuthentication {
                    challenge,
                    deadline: now + self.timeout,
                },
            );
            AuthenticationDisposition::Pending
        }
    }

    pub fn authentication_challenge(
        &self,
        node: Uuid,
        id: UserAgentRequestId,
    ) -> Option<&HttpAuthenticationChallenge> {
        self.authentication
            .get(&(node, id))
            .map(|pending| &pending.challenge)
    }

    pub fn complete_authentication(
        &mut self,
        node: Uuid,
        id: UserAgentRequestId,
        answer: &HttpAuthenticationAnswer,
        remember: bool,
    ) -> Result<(), PolicyError> {
        let pending = self
            .authentication
            .get(&(node, id))
            .ok_or(PolicyError::RequestNotPending)?;
        let space = pending.challenge.protection_space.clone();
        if remember && let HttpAuthenticationAnswer::Credentials(credentials) = answer {
            self.credentials.insert(
                pending.challenge.protection_space.clone(),
                credentials.clone(),
            );
        }
        self.authentication.remove(&(node, id));
        self.update_authentication_summary(node, &space, answer, None);
        Ok(())
    }

    pub fn expire(&mut self, now: Instant) -> Vec<PendingRequest> {
        let expired_permissions = self
            .permissions
            .iter()
            .filter_map(|(&(node, id), pending)| (pending.deadline <= now).then_some((node, id)))
            .collect::<Vec<_>>();
        let expired_authentication = self
            .authentication
            .iter()
            .filter_map(|(&(node, id), pending)| (pending.deadline <= now).then_some((node, id)))
            .collect::<Vec<_>>();
        let mut expired =
            Vec::with_capacity(expired_permissions.len() + expired_authentication.len());
        for (node, id) in expired_permissions {
            if let Some(pending) = self.permissions.remove(&(node, id)) {
                self.update_permission_summary(
                    node,
                    &pending.request.origin,
                    &pending.request.descriptors,
                    PermissionAnswer::Dismiss,
                    Some("timed-out"),
                );
            }
            expired.push(PendingRequest::Permission { node, id });
        }
        for (node, id) in expired_authentication {
            let space = self
                .authentication
                .remove(&(node, id))
                .map(|pending| pending.challenge.protection_space);
            if let Some(space) = space {
                self.update_authentication_summary(
                    node,
                    &space,
                    &HttpAuthenticationAnswer::Cancel,
                    Some("timed-out"),
                );
            }
            expired.push(PendingRequest::Authentication { node, id });
        }
        expired
    }

    /// Withdraw every callback held for a surface before that surface starts a
    /// later navigation or is destroyed. Request ids are surface-scoped, so
    /// removing the old generation here prevents a reused id from attaching a
    /// stale visible decision to the later document.
    pub fn withdraw_node(&mut self, node: Uuid, reason: &str) -> Vec<PendingRequest> {
        let permissions = self
            .permissions
            .keys()
            .filter_map(|&(candidate, id)| (candidate == node).then_some(id))
            .collect::<Vec<_>>();
        let authentication = self
            .authentication
            .keys()
            .filter_map(|&(candidate, id)| (candidate == node).then_some(id))
            .collect::<Vec<_>>();
        let mut withdrawn = Vec::with_capacity(permissions.len() + authentication.len());
        for id in permissions {
            if let Some(pending) = self.permissions.remove(&(node, id)) {
                self.update_permission_summary(
                    node,
                    &pending.request.origin,
                    &pending.request.descriptors,
                    PermissionAnswer::Dismiss,
                    Some(reason),
                );
            }
            withdrawn.push(PendingRequest::Permission { node, id });
        }
        for id in authentication {
            let space = self
                .authentication
                .remove(&(node, id))
                .map(|pending| pending.challenge.protection_space);
            if let Some(space) = space {
                self.update_authentication_summary(
                    node,
                    &space,
                    &HttpAuthenticationAnswer::Cancel,
                    Some(reason),
                );
            }
            withdrawn.push(PendingRequest::Authentication { node, id });
        }
        withdrawn
    }

    pub fn pending_nodes(&self) -> Vec<Uuid> {
        let mut nodes = self
            .permissions
            .keys()
            .chain(self.authentication.keys())
            .map(|(node, _)| *node)
            .collect::<Vec<_>>();
        nodes.sort_unstable();
        nodes.dedup();
        nodes
    }

    pub fn summary(&self, node: Uuid) -> NodePolicySummary {
        self.summaries.get(&node).cloned().unwrap_or_default()
    }

    pub fn facet_value(&self, node: Uuid) -> serde_json::Value {
        serde_json::to_value(self.summary(node)).expect("policy summary is serializable")
    }

    fn update_permission_summary(
        &mut self,
        node: Uuid,
        origin: &str,
        descriptors: &[PermissionDescriptor],
        answer: PermissionAnswer,
        disposition: Option<&str>,
    ) {
        let Some(summary) = self.summaries.get_mut(&node) else {
            return;
        };
        let state = match answer {
            PermissionAnswer::Grant => PermissionState::Granted,
            PermissionAnswer::Deny => PermissionState::Denied,
            PermissionAnswer::Dismiss => PermissionState::Prompt,
        };
        let origin = canonical_origin(origin);
        if let Some(item) = summary
            .permissions
            .iter_mut()
            .rev()
            .find(|item| item.origin == origin && item.descriptors == descriptors)
        {
            item.state = state;
            item.disposition = disposition
                .map(str::to_owned)
                .unwrap_or_else(|| match answer {
                    PermissionAnswer::Dismiss => "dismissed".into(),
                    _ => format!("answered:{answer:?}").to_ascii_lowercase(),
                });
        }
    }

    fn update_authentication_summary(
        &mut self,
        node: Uuid,
        space: &HttpProtectionSpace,
        answer: &HttpAuthenticationAnswer,
        disposition: Option<&str>,
    ) {
        if let Some(item) = self.summaries.get_mut(&node).and_then(|summary| {
            summary.authentication.iter_mut().rev().find(|item| {
                item.host == space.host
                    && item.port == space.port
                    && item.realm == space.realm
                    && item.scheme == space.scheme
                    && item.is_proxy == space.is_proxy
            })
        }) {
            item.disposition = disposition
                .map(str::to_owned)
                .unwrap_or_else(|| match answer {
                    HttpAuthenticationAnswer::Credentials(_) => "answered:credentials".into(),
                    HttpAuthenticationAnswer::Cancel => "cancelled".into(),
                });
        }
    }
}

fn authentication_summary(
    challenge: &HttpAuthenticationChallenge,
    disposition: &str,
) -> AuthenticationSummary {
    AuthenticationSummary {
        host: challenge.protection_space.host.clone(),
        port: challenge.protection_space.port,
        realm: challenge.protection_space.realm.clone(),
        scheme: challenge.protection_space.scheme.clone(),
        is_proxy: challenge.protection_space.is_proxy,
        disposition: disposition.into(),
    }
}

pub fn default_policy_path(data_root: &Path) -> PathBuf {
    data_root
        .join("profiles")
        .join("default")
        .join("web-policy.json")
}

#[cfg(test)]
mod tests {
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
                PermissionRegistry::load("default", ProfileStorage::Persistent(path.clone()))
                    .unwrap();
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
                PermissionRegistry::load("default", ProfileStorage::Persistent(path.clone()))
                    .unwrap();
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
}
