// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::io::{self, Read, Write};

use inker::{Engine, EngineInput};

#[path = "../protocol.rs"]
mod protocol;

use protocol::Parser;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let command = std::env::args()
        .nth(1)
        .ok_or("expected a Smolweb parser command")?;
    let legacy = command == protocol::LEGACY_COMMAND;
    let parser = if legacy {
        Parser::Gemtext
    } else {
        Parser::ALL
            .into_iter()
            .find(|parser| parser.command() == command)
            .ok_or("unsupported Smolweb parser command")?
    };
    let mut request = Vec::new();
    io::stdin()
        .lock()
        .take(protocol::REQUEST_LIMIT + 1)
        .read_to_end(&mut request)?;
    if request.len() as u64 > protocol::REQUEST_LIMIT {
        return Err("Smolweb pack input exceeds its byte limit".into());
    }
    let input: EngineInput = serde_json::from_slice(&request)?;
    if !parser.accepts(&input, legacy) {
        return Err(format!("Smolweb {} does not accept this response", parser.name()).into());
    }
    let document = match parser {
        Parser::Gemtext => nematic::GemtextEngine::new().render(&input)?,
        Parser::Markdown => nematic::MarkdownEngine::new().render(&input)?,
    };
    if document.provenance.source_kind.as_deref() != Some(parser.source_kind()) {
        return Err("Smolweb parser returned unexpected provenance".into());
    }
    let reply = serde_json::to_vec(&document)?;
    if reply.len() as u64 > protocol::REPLY_LIMIT {
        return Err("Smolweb pack output exceeds its byte limit".into());
    }
    io::stdout().lock().write_all(&reply)?;
    Ok(())
}
