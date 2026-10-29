// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Actual fresh native processes, without claiming headed UI qualification.

use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn probe(directory: &Path, stage: &str) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_graphshell-alembic"))
        .args(["--probe", stage])
        .arg(directory)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let limit = Instant::now() + Duration::from_secs(30);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= limit {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "{stage} timed out: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{stage}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn workshop_native_processes_reopen_proposal_accept_and_recover_one_destination_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let generated = probe(directory.path(), "generate");
    assert_eq!(generated["nodes"], 3);
    assert_eq!(generated["view"]["phase"], "Completed");
    let proposal = generated["view"]["proposal"].clone();
    assert_eq!(proposal["sources"].as_array().unwrap().len(), 3);
    assert!(
        proposal["sources"][1]["text"]
            .as_str()
            .unwrap()
            .contains("café")
    );
    let accepted = probe(directory.path(), "accept");
    assert_eq!(accepted["owner"], generated["owner"]);
    assert_eq!(accepted["nodes"], 4);
    let receipt = accepted["view"]["review"]["Added"]["receipt"].clone();
    assert!(receipt.is_object());
    let reopened = probe(directory.path(), "inspect");
    assert_eq!(reopened["view"]["proposal"], proposal);
    assert_eq!(reopened["view"]["review"]["Added"]["receipt"], receipt);
    assert_eq!(reopened["reading"], proposal["djot"]);
    let recovered = probe(directory.path(), "reconcile");
    assert_eq!(recovered["nodes"], 4);
    assert_eq!(recovered["view"]["review"]["Added"]["receipt"], receipt);
}
