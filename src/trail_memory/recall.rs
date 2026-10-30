// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn recall(
    store: &mut FjallStore,
    memory: &mut BrowsingMemory,
    index: &mut Option<RecallIndex>,
    stale: &mut bool,
    dir: &Path,
    request: RecallRequest<'_>,
) -> Result<Vec<RecallHit>, String> {
    if *stale || index.is_none() {
        flush(store, memory);
        let traces = traces_with_titles(memory, request.sources);
        let texts = stored_page_texts(store);
        *index = Some(RecallIndex::mint(dir, &traces, &texts)?);
        *stale = false;
    }
    let Some(index) = index.as_ref() else {
        return Err("no index".to_string());
    };
    index.search(request.query, request.limit, request.config)
}
