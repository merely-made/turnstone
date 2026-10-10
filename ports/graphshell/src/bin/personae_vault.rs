// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Terminal client for djinn's vault. This binary links no custody implementation.

use graphshell::native::app_admission::{AppId, configured_app_endpoint};
use graphshell::native::custody::{CustodyAnswer, CustodyCall};
use graphshell::native::custody_client::BlockingCustodyClient;
use personae::ProfileId;

const USAGE: &str = "usage: personae-vault [--app-endpoint <endpoint>] [--profile <name>] <command> [args]\n\
commands: profiles, new-profile, list, show, add-ssh, pub, remove, ca, mint, enroll-host, face, revoke, krl\n\
Unlock through djinn's own surface. Vault location and passphrase belong to djinn.";

fn main() {
    match run(std::env::args().skip(1).collect()) {
        Ok(text) => print!("{text}"),
        Err(error) => {
            eprintln!("personae-vault: {error}");
            std::process::exit(2);
        },
    }
}

fn run(args: Vec<String>) -> Result<String, String> {
    let mut endpoint = configured_app_endpoint();
    let mut profile = ProfileId("default".into());
    let mut args = args.into_iter();
    let command = loop {
        match args.next().as_deref() {
            Some("--app-endpoint") => {
                endpoint = args.next().ok_or("--app-endpoint needs a value")?
            },
            Some("--profile") => profile = ProfileId(args.next().ok_or("--profile needs a value")?),
            Some("--dir") => {
                return Err(
                    "djinn owns the vault directory; configure the resident's vault location"
                        .into(),
                );
            },
            Some("--help" | "-h") | None => return Ok(format!("{USAGE}\n")),
            Some(command) => break command.to_string(),
        }
    };
    let mut rest: Vec<String> = args.collect();
    // File arguments retain the caller's working directory when the resident
    // runs elsewhere. These are import/certificate paths, never vault roots.
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    if command == "add-ssh" {
        if let Some(path) = rest.iter_mut().find(|arg| arg.as_str() != "--per-use") {
            *path = cwd.join(&*path).display().to_string();
        }
    }
    for index in 0..rest.len().saturating_sub(1) {
        if rest[index] == "--out" {
            rest[index + 1] = cwd.join(&rest[index + 1]).display().to_string();
        }
    }
    let answer = BlockingCustodyClient::open_at(&endpoint, AppId::new("personae-vault"))
        .and_then(|mut client| {
            if command == "profiles" {
                return client.roster().map(CustodyAnswer::Roster);
            }
            client.call(CustodyCall::VaultCommand {
                profile,
                command,
                args: rest,
            })
        })
        .map_err(|error| {
            if error.is_pending() {
                format!("identity pending: {error}")
            } else {
                error.to_string()
            }
        })?;
    match answer {
        CustodyAnswer::VaultOutput { text } => Ok(text),
        CustodyAnswer::VaultEnrollment {
            target,
            script,
            confirmation,
        } => {
            use std::io::Write;
            use std::process::{Command, Stdio};
            let result = Command::new("ssh")
                .args(["-o", "BatchMode=yes", &target, "sh -s"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .and_then(|mut child| {
                    child
                        .stdin
                        .take()
                        .expect("piped stdin")
                        .write_all(script.as_bytes())?;
                    child.wait_with_output()
                })
                .map_err(|error| format!("run ssh {target}: {error}"))?;
            if !result.status.success()
                || !String::from_utf8_lossy(&result.stdout).contains("enrolled")
            {
                return Err(format!(
                    "enrollment did not confirm on {target}:\n{}",
                    String::from_utf8_lossy(&result.stderr)
                ));
            }
            Ok(confirmation)
        },
        CustodyAnswer::Roster(roster) => Ok(roster
            .entries
            .into_iter()
            .map(|entry| format!("{}  {:?}\n", entry.id.0, entry.display_name))
            .collect()),
        _ => Err("djinn did not answer the vault command".into()),
    }
}
