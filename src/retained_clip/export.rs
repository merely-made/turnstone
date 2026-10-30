// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

struct LimitedBytes {
    bytes: Vec<u8>,
    limit: usize,
}
impl std::io::Write for LimitedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let size = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|n| *n <= self.limit)
            .ok_or_else(|| {
                std::io::Error::other("Clip package exceeds its configured wire budget")
            })?;
        self.bytes
            .try_reserve(size - self.bytes.len())
            .map_err(std::io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) fn wire_limit(policy: ClipLimits) -> Result<usize, String> {
    usize::try_from(policy.max_bytes)
        .ok()
        .and_then(|n| n.checked_mul(16))
        .and_then(|n| n.checked_add(policy.max_record_bytes))
        .ok_or("Clip wire budget overflow".into())
}

pub(super) fn prepare(
    owner: &muniment::FjallStore,
    sealer: Option<&dyn eidetic::PayloadSealer>,
    id: ManifestId,
    policy: ClipLimits,
    limits: &crate::research_storage::StorageLimits,
) -> Result<Vec<u8>, String> {
    let mut store = crate::research_storage::BudgetStore {
        store: owner,
        budget: limits.budget(),
    };
    read_store(&mut store, sealer, id, policy).map_err(|error| error.to_string())?;
    let package = pollster::block_on(fleece::portable_clip::export_selected_clip(
        &mut store,
        sealer,
        id,
        &["body".into()],
        policy,
    ))
    .map_err(|e| e.to_string())?;
    let mut bytes = LimitedBytes {
        bytes: Vec::new(),
        limit: wire_limit(policy)?,
    };
    serde_json::to_writer(&mut bytes, &package).map_err(|e| e.to_string())?;
    Ok(bytes.bytes)
}

/// Caller has already claimed publication authority before any file exists.
pub(super) fn publish(bytes: &[u8], file: &Path, replace: bool) -> Result<(), String> {
    let parent = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temporary.write_all(bytes).map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    if replace {
        temporary.persist(file).map_err(|e| e.to_string())?;
    } else {
        temporary
            .persist_noclobber(file)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
