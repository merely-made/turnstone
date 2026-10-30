// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Structured reads share one operation-wide storage ledger.
use muniment::{Backend, FjallStore, StoreError, WriteOp};

pub(super) struct BoundedStore<'a> {
    pub inner: &'a FjallStore,
    pub max_bytes: usize,
    pub budget: crate::research_storage::SharedReadBudget,
    pub sync: &'a (dyn Fn(&FjallStore) -> Result<(), StoreError> + Sync),
}
// Recovery inventories need their domain limits before storage copies keys.
pub(super) trait StructuredStore: Backend {
    fn get_limited(&self, key: &str, max_value_bytes: usize)
    -> Result<Option<Vec<u8>>, StoreError>;
    fn list_inventory(
        &self,
        prefix: &str,
        max_items: usize,
        max_key_bytes: usize,
    ) -> Result<Vec<String>, StoreError>;
}

impl StructuredStore for BoundedStore<'_> {
    fn get_limited(
        &self,
        key: &str,
        max_value_bytes: usize,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        let mut limits = crate::research_storage::unrestricted_call_limits();
        limits.max_value_bytes = self.max_bytes.min(max_value_bytes);
        let mut budget = self
            .budget
            .lock()
            .map_err(|_| StoreError::Backend("storage budget unavailable".into()))?;
        self.inner
            .get_bounded(key, &limits, &mut budget)
            .map_err(Into::into)
    }
    fn list_inventory(
        &self,
        prefix: &str,
        max_items: usize,
        max_key_bytes: usize,
    ) -> Result<Vec<String>, StoreError> {
        let mut limits = crate::research_storage::unrestricted_call_limits();
        limits.max_output_items = max_items;
        limits.max_output_key_bytes = max_key_bytes;
        let mut budget = self
            .budget
            .lock()
            .map_err(|_| StoreError::Backend("storage budget unavailable".into()))?;
        self.inner
            .list_bounded(prefix, &limits, &mut budget)
            .map_err(Into::into)
    }
}

// Existing receipt unit fixtures use a real FjallStore directly.
#[cfg(test)]
impl StructuredStore for FjallStore {
    fn get_limited(
        &self,
        key: &str,
        max_value_bytes: usize,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        let value = pollster::block_on(self.get(key))?;
        if value.as_ref().is_some_and(|v| v.len() > max_value_bytes) {
            return Err(StoreError::LimitExceeded {
                kind: muniment::StoreLimitKind::ValueBytes,
                limit: u64::try_from(max_value_bytes).unwrap_or(u64::MAX),
            });
        }
        Ok(value)
    }
    fn list_inventory(
        &self,
        prefix: &str,
        max_items: usize,
        max_key_bytes: usize,
    ) -> Result<Vec<String>, StoreError> {
        let keys = pollster::block_on(self.list(prefix))?;
        if keys.len() > max_items
            || keys
                .iter()
                .try_fold(0usize, |n, k| n.checked_add(k.len()))
                .is_none_or(|n| n > max_key_bytes)
        {
            return Err(StoreError::Backend(
                "recovery inventory limit exceeded".into(),
            ));
        }
        Ok(keys)
    }
}

impl BoundedStore<'_> {
    pub fn ensure_no_legacy_receipts(
        &self,
    ) -> Result<(), crate::research_storage::SourceReadError> {
        let store = crate::research_storage::BudgetStore {
            store: self.inner,
            budget: self.budget.clone(),
        };
        for (prefix, kind) in [
            ("turnstone/sealed-property-request/v1/", "property"),
            ("turnstone/sealed-result-request/v1/", "result"),
        ] {
            if store.exists(prefix)? {
                return Err(format!(
                    "Unsupported prior {kind} receipt format; preserved for recovery"
                )
                .into());
            }
        }
        Ok(())
    }
}
#[async_trait::async_trait]
impl Backend for BoundedStore<'_> {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.get_limited(key, usize::MAX)
    }
    async fn put(&self, key: &str, value: &[u8]) -> Result<(), StoreError> {
        if value.len() > self.max_bytes {
            return Err(StoreError::Backend(
                "structured stored byte limit exceeded".into(),
            ));
        }
        self.inner.put(key, value).await
    }
    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        if key.starts_with("turnstone/sealed-property-pending/v2/")
            || key.starts_with("turnstone/sealed-result-pending/v2/")
        {
            (self.sync)(self.inner)?;
        }
        self.inner.delete(key).await
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        crate::research_storage::BudgetStore {
            store: self.inner,
            budget: self.budget.clone(),
        }
        .list(prefix)
        .await
    }
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        crate::research_storage::BudgetStore {
            store: self.inner,
            budget: self.budget.clone(),
        }
        .scan(start, end)
        .await
    }
    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        if ops
            .iter()
            .any(|op| matches!(op, WriteOp::Put { value, .. } if value.len() > self.max_bytes))
        {
            return Err(StoreError::Backend(
                "structured stored byte limit exceeded".into(),
            ));
        }
        self.inner.apply(ops).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_free_structured_stored_value_limit_is_checked_by_storage() {
        for (length, succeeds) in [(8, true), (9, false)] {
            let dir = tempfile::tempdir().unwrap();
            let limits = crate::research_storage::StorageLimits::default();
            let store = FjallStore::open_with_limits(dir.path(), limits.owner()).unwrap();
            pollster::block_on(store.put("entry", &vec![42; length])).unwrap();
            let bounded = BoundedStore {
                inner: &store,
                max_bytes: 8,
                budget: limits.budget(),
                sync: &|_| Ok(()),
            };
            let result = pollster::block_on(bounded.get("entry"));
            if succeeds {
                assert_eq!(result.unwrap(), Some(vec![42; length]));
            } else {
                assert!(matches!(
                    result,
                    Err(StoreError::LimitExceeded {
                        kind: muniment::StoreLimitKind::ValueBytes,
                        limit: 8,
                    })
                ));
            }
        }
    }

    #[test]
    fn model_free_per_read_remaining_envelope_ceiling_is_typed() {
        let dir = tempfile::tempdir().unwrap();
        let limits = crate::research_storage::StorageLimits::default();
        let store = FjallStore::open_with_limits(dir.path(), limits.owner()).unwrap();
        pollster::block_on(store.put("envelope", &[7; 8])).unwrap();
        let bounded = BoundedStore {
            inner: &store,
            max_bytes: 32,
            budget: limits.budget(),
            sync: &|_| Ok(()),
        };
        assert_eq!(
            bounded.get_limited("envelope", 8).unwrap(),
            Some(vec![7; 8])
        );
        assert!(matches!(
            bounded.get_limited("envelope", 7),
            Err(StoreError::LimitExceeded {
                kind: muniment::StoreLimitKind::ValueBytes,
                limit: 7
            })
        ));
    }

    #[test]
    fn model_free_pending_inventory_caps_keys_before_materialization() {
        let dir = tempfile::tempdir().unwrap();
        let limits = crate::research_storage::StorageLimits::default();
        let store = FjallStore::open_with_limits(dir.path(), limits.owner()).unwrap();
        let prefix = "turnstone/sealed-property-pending/v2/";
        for nonce in [uuid::Uuid::new_v4(), uuid::Uuid::new_v4()] {
            pollster::block_on(store.put(&format!("{prefix}{nonce}"), b"pending")).unwrap();
        }
        let budget = limits.budget();
        let bounded = BoundedStore {
            inner: &store,
            max_bytes: 4096,
            budget: budget.clone(),
            sync: &|_| Ok(()),
        };
        assert!(matches!(
            bounded.list_inventory(prefix, 1, usize::MAX),
            Err(StoreError::LimitExceeded {
                kind: muniment::StoreLimitKind::OutputItems,
                ..
            })
        ));
        let bounded = BoundedStore {
            budget: limits.budget(),
            ..bounded
        };
        assert!(matches!(
            bounded.list_inventory(prefix, 2, prefix.len() + 35),
            Err(StoreError::LimitExceeded {
                kind: muniment::StoreLimitKind::OutputKeyBytes,
                ..
            })
        ));
        let bounded = BoundedStore {
            budget: limits.budget(),
            ..bounded
        };
        assert_eq!(
            bounded.list_inventory(prefix, 2, usize::MAX).unwrap().len(),
            2
        );
    }
}
