// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Versioned atomic sidecars, with a validated one-way legacy import.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{fs, io, path::Path};
#[derive(Serialize, Deserialize)]
struct Saved<T> {
    version: u32,
    layout: T,
}
pub fn invalid(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}
pub fn write<T: Serialize>(target: &Path, layout: T) -> io::Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&Saved { version: 1, layout }).map_err(invalid)?;
    let tmp = target.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(tmp, target)
}
fn read<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    let saved: Saved<T> = serde_json::from_str(&fs::read_to_string(path)?).map_err(invalid)?;
    if saved.version != 1 {
        return Err(invalid("Unsupported pane workspace version"));
    }
    Ok(saved.layout)
}
pub fn load<T: DeserializeOwned + Serialize>(
    dir: &Path,
    current: &str,
    legacy: &str,
    validate: impl Fn(T) -> io::Result<T>,
) -> io::Result<Option<T>> {
    let path = dir.join(current);
    if path.exists() {
        return validate(read(&path)?).map(Some);
    }
    let legacy = dir.join(legacy);
    if !legacy.exists() {
        return Ok(None);
    }
    let layout = serde_json::from_str(&fs::read_to_string(&legacy)?).map_err(invalid)?;
    let layout = validate(layout)?;
    // Preserve the recovery copy. Current-file errors always refuse rather
    // than falling through and silently resurrecting an obsolete layout.
    write(&path, &layout)?;
    Ok(Some(layout))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "outer-workspace-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[test]
    fn import_preserves_legacy_bytes_and_future_loads_use_current_choice() {
        let dir = dir("migration");
        let old = b"[3,null,9]\n";
        fs::write(dir.join("old"), old).unwrap();
        let value = load::<Vec<Option<u64>>>(&dir, "new", "old", Ok)
            .unwrap()
            .unwrap();
        assert_eq!(value, vec![Some(3), None, Some(9)]);
        assert_eq!(fs::read(dir.join("old")).unwrap(), old);
        write(&dir.join("new"), vec![Some(7), None]).unwrap();
        assert_eq!(
            load::<Vec<Option<u64>>>(&dir, "new", "old", Ok)
                .unwrap()
                .unwrap(),
            vec![Some(7), None]
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn corrupt_current_never_resurrects_the_legacy_copy() {
        let dir = dir("corrupt");
        fs::write(dir.join("old"), "[1]").unwrap();
        fs::write(dir.join("new"), "{bad").unwrap();
        assert!(load::<Vec<u64>>(&dir, "new", "old", Ok).is_err());
        assert_eq!(fs::read_to_string(dir.join("new")).unwrap(), "{bad");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unsupported_version_and_validation_failure_do_not_publish() {
        let dir = dir("version");
        fs::write(dir.join("new"), r#"{"version":2,"layout":[1]}"#).unwrap();
        assert!(load::<Vec<u64>>(&dir, "new", "old", Ok).is_err());
        fs::remove_file(dir.join("new")).unwrap();
        fs::write(dir.join("old"), "[1]").unwrap();
        assert!(
            load::<Vec<u64>>(&dir, "new", "old", |_| Err(invalid(
                "Invalid pane identity"
            )))
            .is_err()
        );
        assert!(!dir.join("new").exists());
        assert_eq!(fs::read_to_string(dir.join("old")).unwrap(), "[1]");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn absent_files_remain_absent() {
        let dir = dir("absent");
        assert!(load::<Vec<u64>>(&dir, "new", "old", Ok).unwrap().is_none());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::remove_dir_all(dir).unwrap();
    }
}
