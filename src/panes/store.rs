// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Pane-layout sidecar: the pane split tree ([`crate::panes::FrisketLayout`])
//! persisted beside `graph.json`, so a window's pane arrangement (which panes
//! are open, their split ratios) survives a restart.
//!
//! ```text
//! <session_dir>/
//! ├── graph.json            ← session_graph_store
//! ├── settings.json         ← settings_store
//! └── frame.json            ← this module (the on-disk tag is still `frame`)
//! ```
//!
//! Moved here from `pandect::frisket_store` at meerkat's deletion
//! (2026-07-18) — the pane-coupled half of pandect, relocated with
//! the pane model exactly as the boundary-pass plan parked it. v0 stores the
//! single content layout as one file; per-window files arrive with
//! multi-window (the `FrisketId` would key a subdirectory the way `views/`
//! keys view-intent).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::panes::FrisketLayout;

/// Filename for the pane layout's sidecar (beside `graph.json`).
///
/// The on-disk tag is still `frame`, deliberately. Renaming the file is a
/// format migration and it travels with the other one this rename left in
/// place, `PaneContent::Orrery` (a serde variant name, and so also on-disk).
/// Both are parked as a single vocabulary decision rather than two silent
/// breaks.
pub const FRAME_FILE: &str = "frame.json";

/// The frame sidecar path under `session_dir`.
pub fn frame_layout_path(session_dir: &Path) -> PathBuf {
    session_dir.join(FRAME_FILE)
}

/// Serialize `layout` to JSON and write it atomically (tmp + rename).
pub fn save_frisket_layout(session_dir: &Path, layout: &FrisketLayout) -> io::Result<()> {
    let target = frame_layout_path(session_dir);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(layout)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = target.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &target)?;
    Ok(())
}

/// Read + parse the frame sidecar. `Ok(None)` when it doesn't exist (fresh
/// session — the host falls back to its default single-pane layout).
pub fn load_frisket_layout(session_dir: &Path) -> io::Result<Option<FrisketLayout>> {
    let path = frame_layout_path(session_dir);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let layout: FrisketLayout =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(layout))
}

/// Filename for the lens-window sidecar (beside `frame.json`): each lens
/// window's frisket space by ordinal, `null` holding a closed window's slot
/// so ordinals stay stable. One file for the whole set (the per-window
/// subdirectory keyed by `FrisketId` is parked with multi-window, per the
/// module note above).
pub const WINDOWS_FILE: &str = "windows.json";

/// Serialize the lens spaces and write them atomically (tmp + rename).
pub fn save_lens_spaces(session_dir: &Path, lenses: &[Option<FrisketLayout>]) -> io::Result<()> {
    let target = session_dir.join(WINDOWS_FILE);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(lenses)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = target.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &target)?;
    Ok(())
}

/// Read + parse the lens-window sidecar. `Ok(None)` when it doesn't exist
/// (no lens windows were open — the common single-window session).
pub fn load_lens_spaces(session_dir: &Path) -> io::Result<Option<Vec<Option<FrisketLayout>>>> {
    let path = session_dir.join(WINDOWS_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let lenses: Vec<Option<FrisketLayout>> =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(lenses))
}

#[cfg(test)]
mod tests {
    use crate::panes::{FrisketId, GraphId, InsertSide, PaneContent, PaneId, PaneNode};

    use super::*;

    fn temp_session_dir(label: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("turnstone-frame-store-test-{label}-{pid}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_layout() -> FrisketLayout {
        let mut layout = FrisketLayout {
            id: FrisketId::new("content"),
            label: "content".into(),
            root: PaneNode::Leaf {
                pane_id: PaneId(0),
                content: PaneContent::Workbench,
                graph_id: GraphId::from_uuid(uuid::Uuid::from_u128(1)),
            },
        };
        layout.summon_leaf(
            &[],
            InsertSide::Right,
            PaneNode::Leaf {
                pane_id: PaneId(1),
                content: PaneContent::Roster,
                graph_id: GraphId::from_uuid(uuid::Uuid::from_u128(1)),
            },
        );
        layout.set_split_ratio(&[], 0.66);
        layout
    }

    #[test]
    fn save_then_load_round_trips_layout() {
        let dir = temp_session_dir("round-trip");
        let original = sample_layout();
        save_frisket_layout(&dir, &original).unwrap();
        let restored = load_frisket_layout(&dir)
            .unwrap()
            .expect("frame file present");
        assert_eq!(restored, original);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_returns_none_when_no_file() {
        let dir = temp_session_dir("no-file");
        assert!(load_frisket_layout(&dir).unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn malformed_json_is_invalid_data() {
        let dir = temp_session_dir("malformed");
        fs::write(frame_layout_path(&dir), "{ not json").unwrap();
        match load_frisket_layout(&dir) {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
            Ok(_) => panic!("expected malformed JSON to fail"),
        }
        fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn retired_panes_restore_their_content_and_arrangement_in_frame_and_lenses() {
        let dir = temp_session_dir("retired-pane-content");
        let old = serde_json::to_string(&sample_layout())
            .unwrap()
            .replace("\"Workbench\"", "\"Apparatus\"")
            .replace("\"Roster\"", "\"Steward\"");
        fs::write(frame_layout_path(&dir), &old).unwrap();
        fs::write(dir.join(WINDOWS_FILE), format!("[{old},null]")).unwrap();
        let restored = load_frisket_layout(&dir).unwrap().unwrap();
        let panes: Vec<_> = restored.iter_leaves().collect();
        assert_eq!(panes[0].0, PaneId(0));
        assert_eq!(panes[0].1, &PaneContent::Inspector);
        assert_eq!(panes[1].0, PaneId(1));
        assert_eq!(
            panes[1].1.composition().unwrap().sections,
            vec!["downloads"]
        );
        assert_eq!(panes[0].2, GraphId::from_uuid(uuid::Uuid::from_u128(1)));
        let PaneNode::Split { ratio, .. } = restored.root else {
            panic!("split retained")
        };
        assert_eq!(ratio, 0.66);
        let lenses = load_lens_spaces(&dir).unwrap().unwrap();
        assert_eq!(lenses, vec![Some(restored.clone()), None]);
        save_frisket_layout(&dir, &restored).unwrap();
        save_lens_spaces(&dir, &lenses).unwrap();
        for filename in [FRAME_FILE, WINDOWS_FILE] {
            let saved = fs::read_to_string(dir.join(filename)).unwrap();
            assert!(!saved.contains("Apparatus") && !saved.contains("Steward"));
            assert!(saved.contains("Inspector") && saved.contains("downloads"));
        }
        assert_eq!(load_frisket_layout(&dir).unwrap(), Some(restored));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn configured_gloss_section_order_survives_frame_and_lens_save() {
        let dir = temp_session_dir("gloss-order");
        let mut layout = sample_layout();
        *layout.content_mut(PaneId(1)).unwrap() = PaneContent::Gloss(super::super::PaneComposition {
            sections: vec!["recent".into(), "downloads".into(), "removed".into()],
        });
        save_frisket_layout(&dir, &layout).unwrap();
        save_lens_spaces(&dir, &[Some(layout.clone())]).unwrap();
        assert_eq!(load_frisket_layout(&dir).unwrap(), Some(layout.clone()));
        assert_eq!(load_lens_spaces(&dir).unwrap(), Some(vec![Some(layout)]));
        fs::remove_dir_all(dir).unwrap();
    }
}
