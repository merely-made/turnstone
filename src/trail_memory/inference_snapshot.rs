// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn inference_snapshot(
    store: Option<&FjallStore>,
    dir: &Path,
    owner: &str,
) -> Result<Vec<eidetic::models::recall::RetainedSourceText>, String> {
    use eidetic::models::recall::{RecallSourceId, RecallSourceKind, RetainedSourceText};
    if inference_owner(dir)? != owner {
        return Err("This answer belongs to another browsing session.".into());
    }
    let store = store.ok_or("The retained-text owner is not open.")?;
    pollster::block_on(async {
        let texts = PageTextStore::new(store);
        let mut out = Vec::new();
        for entry in texts.references().await.map_err(|e| e.to_string())? {
            let text = texts
                .text_at(&entry)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("A retained source version is unavailable.")?;
            out.push(RetainedSourceText {
                source: RecallSourceId {
                    kind: RecallSourceKind::PageText,
                    owner: Some(owner.into()),
                    source: entry.url,
                    node: None,
                },
                version: format!("blake3:{}", entry.blob),
                title: None,
                text,
            });
        }
        Ok(out)
    })
}
