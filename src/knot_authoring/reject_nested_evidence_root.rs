// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn reject_nested_evidence_root(
    source: &PathBuf,
    evidence: Option<&std::path::Path>,
) -> Result<(), String> {
    let Some(evidence) = evidence else {
        return Ok(());
    };
    let source = normalized_absolute(source)?;
    let evidence = normalized_absolute(evidence)?;
    if evidence.starts_with(&source) {
        return Err(format!(
            "TURNSTONE_KNOT_EVIDENCE_ROOT must be outside the served Knot directory {}",
            source.display()
        ));
    }
    Ok(())
}
