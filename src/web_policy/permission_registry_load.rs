// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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

    pub(super) fn remember(
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

    pub(super) fn save(&self) -> Result<(), PolicyError> {
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
