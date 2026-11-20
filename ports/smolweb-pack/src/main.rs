// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::io::{self, Read, Write};

use inker::{Engine, EngineInput};

#[path = "../protocol.rs"]
mod protocol;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() != Some(protocol::COMMAND) {
        return Err("expected the Gemini pack protocol version".into());
    }
    let mut request = Vec::new();
    io::stdin()
        .lock()
        .take(protocol::REQUEST_LIMIT + 1)
        .read_to_end(&mut request)?;
    if request.len() as u64 > protocol::REQUEST_LIMIT {
        return Err("Gemini pack input exceeds its byte limit".into());
    }
    let input: EngineInput = serde_json::from_slice(&request)?;
    if !protocol::accepts(&input) {
        return Err("Smolweb pack requires a Gemini Gemtext response".into());
    }
    let document = nematic::GemtextEngine::new().render(&input)?;
    let reply = serde_json::to_vec(&document)?;
    if reply.len() as u64 > protocol::REPLY_LIMIT {
        return Err("Gemini pack output exceeds its byte limit".into());
    }
    io::stdout().lock().write_all(&reply)?;
    Ok(())
}
