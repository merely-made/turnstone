// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn ensure_binding(
    retained: &mut RetainedEndpointSession,
    mounted: &mut Option<ProjectionSession>,
    bindings: &mut BTreeMap<String, DocumentBinding>,
    address: &str,
) -> Result<DocumentBinding, String> {
    if let Some(binding) = bindings.get(address) {
        return Ok(binding.clone());
    }
    let session = match mounted {
        Some(session) => session.clone(),
        None => {
            let session = retained.mount(0)?;
            *mounted = Some(session.clone());
            session
        },
    };
    let binding = resolve_binding(retained, &session, address)?;
    bindings.insert(address.to_string(), binding.clone());
    Ok(binding)
}
