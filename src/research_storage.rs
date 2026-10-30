// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Configurable limits for the retained research storage owners.

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecoveryLimits {
    pub max_value_bytes: usize,
    pub max_key_bytes: usize,
    pub max_replay_bytes: u64,
    pub max_replay_items: u64,
    pub max_metadata_bytes: u64,
    pub max_input_bytes: u64,
    pub max_alloc_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadLimits {
    pub max_value_bytes: usize,
    pub max_block_encoded_bytes: usize,
    pub max_block_decoded_bytes: usize,
    pub max_input_bytes: u64,
    pub max_decode_bytes: u64,
    pub max_metadata_bytes: u64,
    pub max_alloc_bytes: u64,
    pub max_visited_items: u64,
    pub max_key_bytes: usize,
    pub max_output_items: usize,
    pub max_output_key_bytes: usize,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StorageLimits {
    pub recovery: RecoveryLimits,
    pub owner_read: ReadLimits,
    pub operation_read: ReadLimits,
}
impl Default for RecoveryLimits {
    fn default() -> Self {
        Self {
            max_value_bytes: 64 * 1024 * 1024,
            max_key_bytes: 64 * 1024,
            max_replay_bytes: 1024 * 1024 * 1024,
            max_replay_items: 1_000_000,
            max_metadata_bytes: 128 * 1024 * 1024,
            max_input_bytes: 2 * 1024 * 1024 * 1024,
            max_alloc_bytes: 512 * 1024 * 1024,
        }
    }
}
impl Default for ReadLimits {
    fn default() -> Self {
        Self {
            max_value_bytes: 64 * 1024 * 1024,
            max_block_encoded_bytes: 64 * 1024 * 1024,
            max_block_decoded_bytes: 64 * 1024 * 1024,
            max_input_bytes: 512 * 1024 * 1024,
            max_decode_bytes: 512 * 1024 * 1024,
            max_metadata_bytes: 128 * 1024 * 1024,
            max_alloc_bytes: 512 * 1024 * 1024,
            max_visited_items: 1_000_000,
            max_key_bytes: 64 * 1024,
            max_output_items: 100_000,
            max_output_key_bytes: 32 * 1024 * 1024,
        }
    }
}
impl From<&RecoveryLimits> for muniment::FjallRecoveryLimits {
    fn from(s: &RecoveryLimits) -> Self {
        Self {
            max_value_bytes: s.max_value_bytes,
            max_key_bytes: s.max_key_bytes,
            max_replay_bytes: s.max_replay_bytes,
            max_replay_items: s.max_replay_items,
            max_metadata_bytes: s.max_metadata_bytes,
            max_input_bytes: s.max_input_bytes,
            max_alloc_bytes: s.max_alloc_bytes,
        }
    }
}
impl From<&ReadLimits> for muniment::FjallReadLimits {
    fn from(s: &ReadLimits) -> Self {
        Self {
            max_value_bytes: s.max_value_bytes,
            max_block_encoded_bytes: s.max_block_encoded_bytes,
            max_block_decoded_bytes: s.max_block_decoded_bytes,
            max_input_bytes: s.max_input_bytes,
            max_decode_bytes: s.max_decode_bytes,
            max_metadata_bytes: s.max_metadata_bytes,
            max_alloc_bytes: s.max_alloc_bytes,
            max_visited_items: s.max_visited_items,
            max_key_bytes: s.max_key_bytes,
            max_output_items: s.max_output_items,
            max_output_key_bytes: s.max_output_key_bytes,
        }
    }
}
impl StorageLimits {
    pub fn owner(&self) -> muniment::FjallOwnerLimits {
        muniment::FjallOwnerLimits {
            recovery: (&self.recovery).into(),
            read: (&self.owner_read).into(),
        }
    }
    pub fn budget(&self) -> SharedReadBudget {
        std::sync::Arc::new(std::sync::Mutex::new(muniment::FjallReadBudget::new(
            (&self.operation_read).into(),
        )))
    }
}
pub(crate) type SharedReadBudget = std::sync::Arc<std::sync::Mutex<muniment::FjallReadBudget>>;

impl StorageLimits {
    pub fn set(&mut self, field: &str, value: &str) -> Result<(), String> {
        let n: u64 = value
            .trim()
            .parse()
            .map_err(|_| "Enter a nonnegative whole number")?;
        let n_usize =
            || usize::try_from(n).map_err(|_| "The value is too large for this device".to_string());
        macro_rules! recovery {
            ($field:ident, usize) => {
                self.recovery.$field = n_usize()?
            };
            ($field:ident, u64) => {
                self.recovery.$field = n
            };
        }
        macro_rules! read {
            ($section:ident, $field:ident, usize) => {
                self.$section.$field = n_usize()?
            };
            ($section:ident, $field:ident, u64) => {
                self.$section.$field = n
            };
        }
        match field {
            "storage_recovery_value_bytes" => recovery!(max_value_bytes, usize),
            "storage_recovery_key_bytes" => recovery!(max_key_bytes, usize),
            "storage_recovery_replay_bytes" => recovery!(max_replay_bytes, u64),
            "storage_recovery_replay_items" => recovery!(max_replay_items, u64),
            "storage_recovery_metadata_bytes" => recovery!(max_metadata_bytes, u64),
            "storage_recovery_input_bytes" => recovery!(max_input_bytes, u64),
            "storage_recovery_alloc_bytes" => recovery!(max_alloc_bytes, u64),
            _ => {
                let (section, name) = field
                    .strip_prefix("storage_owner_")
                    .map(|s| (true, s))
                    .or_else(|| field.strip_prefix("storage_operation_").map(|s| (false, s)))
                    .ok_or("Unknown storage setting")?;
                macro_rules! assign {
                    ($field:ident, $ty:ident) => {
                        if section {
                            read!(owner_read, $field, $ty)
                        } else {
                            read!(operation_read, $field, $ty)
                        }
                    };
                }
                match name {
                    "value_bytes" => assign!(max_value_bytes, usize),
                    "block_encoded_bytes" => assign!(max_block_encoded_bytes, usize),
                    "block_decoded_bytes" => assign!(max_block_decoded_bytes, usize),
                    "input_bytes" => assign!(max_input_bytes, u64),
                    "decode_bytes" => assign!(max_decode_bytes, u64),
                    "metadata_bytes" => assign!(max_metadata_bytes, u64),
                    "alloc_bytes" => assign!(max_alloc_bytes, u64),
                    "visited_items" => assign!(max_visited_items, u64),
                    "key_bytes" => assign!(max_key_bytes, usize),
                    "output_items" => assign!(max_output_items, usize),
                    "output_key_bytes" => assign!(max_output_key_bytes, usize),
                    _ => return Err("Unknown storage setting".into()),
                }
            },
        }
        Ok(())
    }

    pub fn fields(&self) -> Vec<(String, String, String)> {
        let r = &self.recovery;
        let mut fields = vec![
            (
                "Open: value bytes",
                "storage_recovery_value_bytes",
                r.max_value_bytes.to_string(),
            ),
            (
                "Open: key bytes",
                "storage_recovery_key_bytes",
                r.max_key_bytes.to_string(),
            ),
            (
                "Open: replay bytes",
                "storage_recovery_replay_bytes",
                r.max_replay_bytes.to_string(),
            ),
            (
                "Open: recovery item visits",
                "storage_recovery_replay_items",
                r.max_replay_items.to_string(),
            ),
            (
                "Open: metadata bytes",
                "storage_recovery_metadata_bytes",
                r.max_metadata_bytes.to_string(),
            ),
            (
                "Open: input bytes",
                "storage_recovery_input_bytes",
                r.max_input_bytes.to_string(),
            ),
            (
                "Open: allocation bytes",
                "storage_recovery_alloc_bytes",
                r.max_alloc_bytes.to_string(),
            ),
        ]
        .into_iter()
        .map(|(a, b, c)| (a.to_string(), b.to_string(), c))
        .collect::<Vec<_>>();
        for (label, prefix, s) in [
            ("Each read", "storage_owner", &self.owner_read),
            ("Whole operation", "storage_operation", &self.operation_read),
        ] {
            for (name, key, value) in [
                ("value bytes", "value_bytes", s.max_value_bytes.to_string()),
                (
                    "encoded block bytes",
                    "block_encoded_bytes",
                    s.max_block_encoded_bytes.to_string(),
                ),
                (
                    "decoded block bytes",
                    "block_decoded_bytes",
                    s.max_block_decoded_bytes.to_string(),
                ),
                (
                    "encoded input bytes",
                    "input_bytes",
                    s.max_input_bytes.to_string(),
                ),
                (
                    "decoded bytes",
                    "decode_bytes",
                    s.max_decode_bytes.to_string(),
                ),
                (
                    "metadata bytes",
                    "metadata_bytes",
                    s.max_metadata_bytes.to_string(),
                ),
                (
                    "allocation bytes",
                    "alloc_bytes",
                    s.max_alloc_bytes.to_string(),
                ),
                (
                    "visited items",
                    "visited_items",
                    s.max_visited_items.to_string(),
                ),
                ("key bytes", "key_bytes", s.max_key_bytes.to_string()),
                (
                    "output items",
                    "output_items",
                    s.max_output_items.to_string(),
                ),
                (
                    "output key bytes",
                    "output_key_bytes",
                    s.max_output_key_bytes.to_string(),
                ),
            ] {
                fields.push((format!("{label}: {name}"), format!("{prefix}_{key}"), value));
            }
        }
        fields
    }
}

// The owner and operation ledger still apply when a call does not add a tighter cap.
pub(crate) fn unrestricted_call_limits() -> muniment::FjallReadLimits {
    muniment::FjallReadLimits {
        max_value_bytes: usize::MAX,
        max_block_encoded_bytes: usize::MAX,
        max_block_decoded_bytes: usize::MAX,
        max_input_bytes: u64::MAX,
        max_decode_bytes: u64::MAX,
        max_metadata_bytes: u64::MAX,
        max_alloc_bytes: u64::MAX,
        max_visited_items: u64::MAX,
        max_key_bytes: usize::MAX,
        max_output_items: usize::MAX,
        max_output_key_bytes: usize::MAX,
    }
}

/// All reads in an operation share this ledger, including across owner actors.
pub(crate) struct BudgetStore<'a> {
    pub store: &'a muniment::FjallStore,
    pub budget: SharedReadBudget,
}
impl BudgetStore<'_> {
    pub fn exists(&self, prefix: &str) -> Result<bool, muniment::StoreError> {
        let mut budget = self
            .budget
            .lock()
            .map_err(|_| muniment::StoreError::Backend("storage budget unavailable".into()))?;
        self.store
            .exists_bounded(prefix, &unrestricted_call_limits(), &mut budget)
            .map_err(store_error)
    }
}
fn store_error(error: muniment::FjallBoundError) -> muniment::StoreError {
    match error {
        muniment::FjallBoundError::Limit { kind, limit } => {
            muniment::StoreError::LimitExceeded { kind, limit }
        },
        muniment::FjallBoundError::Store(error) => error,
    }
}
#[async_trait::async_trait]
impl muniment::Backend for BudgetStore<'_> {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, muniment::StoreError> {
        let mut budget = self
            .budget
            .lock()
            .map_err(|_| muniment::StoreError::Backend("storage budget unavailable".into()))?;
        self.store
            .get_bounded(key, &unrestricted_call_limits(), &mut budget)
            .map_err(store_error)
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, muniment::StoreError> {
        let mut budget = self
            .budget
            .lock()
            .map_err(|_| muniment::StoreError::Backend("storage budget unavailable".into()))?;
        self.store
            .list_bounded(prefix, &unrestricted_call_limits(), &mut budget)
            .map_err(store_error)
    }
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, muniment::StoreError> {
        let mut budget = self
            .budget
            .lock()
            .map_err(|_| muniment::StoreError::Backend("storage budget unavailable".into()))?;
        self.store
            .scan_bounded(start, end, &unrestricted_call_limits(), &mut budget)
            .map_err(store_error)
    }
    async fn put(&self, key: &str, value: &[u8]) -> Result<(), muniment::StoreError> {
        muniment::Backend::put(self.store, key, value).await
    }
    async fn delete(&self, key: &str) -> Result<(), muniment::StoreError> {
        muniment::Backend::delete(self.store, key).await
    }
    async fn apply(&self, ops: &[muniment::WriteOp]) -> Result<(), muniment::StoreError> {
        muniment::Backend::apply(self.store, ops).await
    }
}

#[cfg(test)]
#[path = "research_storage_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceReadError {
    Storage(muniment::StoreError),
    StorageOpen {
        owner: &'static str,
        error: muniment::StoreError,
    },
    StorageOpenRetry {
        owner: &'static str,
        error: muniment::StoreError,
    },
    Context {
        label: String,
        cause: Box<SourceReadError>,
    },
    Other(String),
}
impl SourceReadError {
    pub(crate) fn context(self, context: &str) -> Self {
        Self::Context {
            label: context.into(),
            cause: Box::new(self),
        }
    }
    pub(crate) fn open(owner: &'static str, error: muniment::StoreError) -> Self {
        Self::StorageOpen { owner, error }
    }
    pub(crate) fn open_retry(owner: &'static str, error: muniment::StoreError) -> Self {
        Self::StorageOpenRetry { owner, error }
    }
    pub fn storage_error(&self) -> Option<&muniment::StoreError> {
        match self {
            Self::Storage(error)
            | Self::StorageOpen { error, .. }
            | Self::StorageOpenRetry { error, .. } => Some(error),
            Self::Context { cause, .. } => cause.storage_error(),
            Self::Other(_) => None,
        }
    }
}
impl From<String> for SourceReadError {
    fn from(value: String) -> Self {
        Self::Other(value)
    }
}
impl From<&str> for SourceReadError {
    fn from(value: &str) -> Self {
        Self::Other(value.into())
    }
}
impl From<eidetic::Error> for SourceReadError {
    fn from(value: eidetic::Error) -> Self {
        match value.storage_error() {
            Some(error) => Self::Storage(error.clone()),
            None => Self::Other(value.to_string()),
        }
    }
}
impl From<muniment::StoreError> for SourceReadError {
    fn from(value: muniment::StoreError) -> Self {
        Self::Storage(value)
    }
}
impl std::fmt::Display for SourceReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(error) => error.fmt(f),
            Self::StorageOpen { owner, error } => format_open(f, owner, error, true),
            Self::StorageOpenRetry { owner, error } => format_open(f, owner, error, false),
            Self::Context { label, cause } => write!(f, "{label}: {cause}"),
            Self::Other(error) => f.write_str(error),
        }
    }
}

fn format_open(
    f: &mut std::fmt::Formatter<'_>,
    owner: &str,
    error: &muniment::StoreError,
    restart: bool,
) -> std::fmt::Result {
    write!(f, "{owner} storage open: {error}")?;
    if let muniment::StoreError::LimitExceeded { kind, .. } = error {
        let setting = match kind {
            muniment::StoreLimitKind::ValueBytes => "Open: value bytes",
            muniment::StoreLimitKind::KeyBytes => "Open: key bytes",
            muniment::StoreLimitKind::ReplayBytes => "Open: replay bytes",
            muniment::StoreLimitKind::ReplayItems => "Open: recovery item visits",
            muniment::StoreLimitKind::MetadataBytes => "Open: metadata bytes",
            muniment::StoreLimitKind::InputBytes => "Open: input bytes",
            muniment::StoreLimitKind::AllocationBytes => "Open: allocation bytes",
            _ => "Open storage limit",
        };
        if restart {
            write!(
                f,
                ". Raise {setting} in settings and restart Turnstone to retry."
            )?;
        } else {
            write!(
                f,
                ". Raise {setting} in settings and retry; restart Turnstone if the changed settings are not active."
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
impl SourceReadError {
    pub(crate) fn contains(&self, text: impl AsRef<str>) -> bool {
        self.to_string().contains(text.as_ref())
    }
}
