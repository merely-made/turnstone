// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::servo_profile_directory;
use std::path::PathBuf;

#[test]
fn named_process_profile_has_no_node_component_and_can_move() {
    let root = std::env::current_dir().unwrap();
    assert_eq!(
        servo_profile_directory(&root, "Research", None).unwrap(),
        root.join("servo/profiles/Research")
    );
    let location = root.join("custom-browser-data");
    assert_eq!(
        servo_profile_directory(&root, "Research", Some(location.clone())).unwrap(),
        location
    );
}

#[test]
fn ambiguous_or_escaping_profile_bindings_are_refused() {
    let root = std::env::current_dir().unwrap();
    for name in [
        "", " ", ".", "..", "../other", "x\\y", "a:b", "a.", "NUL", "COM1.txt",
    ] {
        assert!(
            servo_profile_directory(&root, name, None).is_err(),
            "{name}"
        );
    }
    assert!(servo_profile_directory(&root, "Research", Some(PathBuf::from("relative"))).is_err());
}
