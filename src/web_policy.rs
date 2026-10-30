// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

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

#[cfg(test)]
#[path = "web_policy/tests.rs"]
mod tests;

#[path = "web_policy/canonical_origin.rs"]
mod canonical_origin;
#[path = "web_policy/impl_fmt_display_for_policy_error.rs"]
mod impl_fmt_display_for_policy_error;
#[path = "web_policy/impl_std_error_error_for_policy.rs"]
mod impl_std_error_error_for_policy;
#[path = "web_policy/permission_registry_load.rs"]
mod permission_registry_load;
pub(crate) use canonical_origin::canonical_origin;
#[path = "web_policy/authentication_summary.rs"]
mod authentication_summary;
#[path = "web_policy/credential_provider_insert.rs"]
mod credential_provider_insert;
#[path = "web_policy/impl_from_http_protection_space_for.rs"]
mod impl_from_http_protection_space_for;
#[path = "web_policy/web_policy_service_new.rs"]
mod web_policy_service_new;
use authentication_summary::authentication_summary;
#[path = "web_policy/default_policy_path.rs"]
mod default_policy_path;
pub use default_policy_path::default_policy_path;
