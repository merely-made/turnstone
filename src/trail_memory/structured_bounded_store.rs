// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Post-fetch guard for structured operations. Fjall may allocate before this guard.
use muniment::{Backend, FjallStore, StoreError, WriteOp};

pub(super) struct BoundedStore<'a> {
    pub inner: &'a FjallStore,
    pub max_bytes: usize,
    pub sync: &'a (dyn Fn(&FjallStore) -> Result<(), String> + Sync),
}
#[async_trait::async_trait]
impl Backend for BoundedStore<'_> {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let value = self.inner.get(key).await?;
        if value
            .as_ref()
            .is_some_and(|bytes| bytes.len() > self.max_bytes)
        {
            return Err(StoreError::Backend(
                "structured stored byte limit exceeded".into(),
            ));
        }
        Ok(value)
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
            (self.sync)(self.inner).map_err(StoreError::Backend)?;
        }
        self.inner.delete(key).await
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        self.inner.list(prefix).await
    }
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        self.inner.scan(start, end).await
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
