// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Product chrome placement, independent of pane topology and document identity.
use serde::{Deserialize, Serialize};
use std::{io, path::Path};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabEdge {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placement_choices_survive_a_cold_load() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            ChromeLayout::load(root.path()).unwrap(),
            ChromeLayout::default()
        );
        for document_tabs in [TabEdge::Top, TabEdge::Bottom, TabEdge::Left, TabEdge::Right] {
            let layout = ChromeLayout {
                toolbar_bottom: true,
                document_tabs,
            };
            layout.save(root.path()).unwrap();
            assert_eq!(ChromeLayout::load(root.path()).unwrap(), layout);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChromeLayout {
    pub toolbar_bottom: bool,
    pub document_tabs: TabEdge,
}

impl ChromeLayout {
    pub fn load(root: &Path) -> io::Result<Self> {
        match std::fs::read(root.join("chrome-layout.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }
    pub fn save(self, root: &Path) -> io::Result<()> {
        std::fs::create_dir_all(root)?;
        let path = root.join("chrome-layout.json");
        let temp = root.join(format!("chrome-layout.{}.tmp", std::process::id()));
        std::fs::write(
            &temp,
            serde_json::to_vec_pretty(&self).map_err(io::Error::other)?,
        )?;
        std::fs::rename(temp, path)
    }
}
