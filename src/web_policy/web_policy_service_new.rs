// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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

    pub(super) fn update_permission_summary(
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

    pub(super) fn update_authentication_summary(
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
