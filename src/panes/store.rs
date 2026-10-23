// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Versioned shared-Workspace sidecars beside graph.json. Legacy frame.json
//! and windows.json are read once, validated and retained as recovery copies.
//! A present invalid current file refuses; it never revives an older layout.

#[cfg(test)]
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::panes::FrisketLayout;

/// Current primary layout. The old public constant name remains for callers
/// that enumerate session sidecars; its value names the versioned format.
pub const FRAME_FILE: &str = "workspace.json";
pub const LEGACY_FRAME_FILE: &str = "frame.json";
pub const WINDOWS_FILE: &str = "window-workspaces.json";
pub const LEGACY_WINDOWS_FILE: &str = "windows.json";

use super::workspace_store::{invalid, load, write};
fn canonical(layout: &FrisketLayout) -> io::Result<FrisketLayout> {
    let mut layout = layout.clone();
    layout.adopt_workspace().map_err(invalid)?;
    layout.outer_workspace().map_err(invalid)?;
    Ok(layout)
}
pub fn frame_layout_path(session_dir: &Path) -> PathBuf {
    session_dir.join(FRAME_FILE)
}
pub fn save_frisket_layout(session_dir: &Path, layout: &FrisketLayout) -> io::Result<()> {
    write(&frame_layout_path(session_dir), canonical(layout)?)
}
pub fn load_frisket_layout(session_dir: &Path) -> io::Result<Option<FrisketLayout>> {
    load(
        session_dir,
        FRAME_FILE,
        LEGACY_FRAME_FILE,
        |layout: FrisketLayout| canonical(&layout),
    )
}
pub fn save_lens_spaces(session_dir: &Path, lenses: &[Option<FrisketLayout>]) -> io::Result<()> {
    let canonical: Vec<_> = lenses
        .iter()
        .map(|layout| layout.as_ref().map(canonical).transpose())
        .collect::<io::Result<_>>()?;
    write(&session_dir.join(WINDOWS_FILE), canonical)
}
pub fn load_lens_spaces(session_dir: &Path) -> io::Result<Option<Vec<Option<FrisketLayout>>>> {
    load(
        session_dir,
        WINDOWS_FILE,
        LEGACY_WINDOWS_FILE,
        |lenses: Vec<Option<FrisketLayout>>| {
            lenses
                .iter()
                .map(|layout| layout.as_ref().map(canonical).transpose())
                .collect::<io::Result<Vec<_>>>()
        },
    )
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
        // A real legacy tree has pane tags, without Workspace tab titles.
        // Replacing every matching string in today's saved layout also changes
        // a retained title, which migration correctly preserves as user text.
        let old = serde_json::json!({
            "id": "content", "label": "content",
            "root": { "Split": {
                "axis": "Horizontal", "ratio": 0.66,
                "first": { "Leaf": {
                    "pane_id": 0, "content": "Apparatus",
                    "graph_id": "00000000-0000-0000-0000-000000000001"
                } },
                "second": { "Leaf": {
                    "pane_id": 1, "content": "Steward",
                    "graph_id": "00000000-0000-0000-0000-000000000001"
                } }
            } }
        }).to_string();
        fs::write(dir.join(LEGACY_FRAME_FILE), &old).unwrap();
        fs::write(dir.join(LEGACY_WINDOWS_FILE), format!("[{old},null]")).unwrap();
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
        assert_eq!(restored.split_at(&[]).unwrap().1, 0.66);
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
        *layout.content_mut(PaneId(1)).unwrap() =
            PaneContent::Gloss(super::super::PaneComposition {
                sections: vec!["recent".into(), "downloads".into(), "removed".into()],
            });
        save_frisket_layout(&dir, &layout).unwrap();
        save_lens_spaces(&dir, &[Some(layout.clone())]).unwrap();
        assert_eq!(load_frisket_layout(&dir).unwrap(), Some(layout.clone()));
        assert_eq!(load_lens_spaces(&dir).unwrap(), Some(vec![Some(layout)]));
        fs::remove_dir_all(dir).unwrap();
    }
}
