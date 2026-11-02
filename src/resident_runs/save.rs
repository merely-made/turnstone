// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn save(dir: &Path, state: &ResidentRuns) -> Result<(), String> {
    state.validate()?;
    let target = path(dir);
    let raw = serde_json::to_vec_pretty(&DiskState {
        version: VERSION,
        next_id: state.next_id,
        runs: state.runs.clone(),
    })
    .map_err(|e| format!("resident run state cannot encode: {e}"))?;
    if raw.len() as u64 > state.limits.max_bytes {
        return Err("resident run state exceeds its configured storage bound".into());
    }
    let parent = target
        .parent()
        .ok_or_else(|| "resident run state has no parent".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("cannot create resident run directory: {e}"))?;
    let temp = target.with_extension("json.new");
    let mut file = std::fs::File::create(&temp)
        .map_err(|e| format!("cannot create resident run state: {e}"))?;
    file.write_all(&raw)
        .map_err(|e| format!("cannot write resident run state: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("cannot sync resident run state: {e}"))?;
    drop(file);
    std::fs::rename(temp, target).map_err(|e| format!("cannot commit resident run state: {e}"))
}
