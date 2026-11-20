// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::fs;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use inker::{
    DocumentSession, EngineDocument, EngineInput, SessionEngine, SessionError, SessionSpawnRequest,
};
use mere_document_lanes::{SmolwebDocument, SmolwebDocumentSession, SmolwebTheme};
use netrender::Scene;

#[path = "../ports/smolweb-pack/protocol.rs"]
mod protocol;

#[path = "engine_pack/install.rs"]
mod install;
use install::Transport;
pub use install::command;

const EXECUTABLE: &str = if cfg!(windows) {
    "turnstone-smolweb-pack.exe"
} else {
    "turnstone-smolweb-pack"
};
const PROCESS_TIMEOUT: Duration = Duration::from_secs(3);

use protocol::Parser;

impl Parser {
    const fn engine_id(self) -> &'static str {
        match self {
            Self::Gemtext => "nematic.gemtext.pack",
            Self::Markdown => "nematic.markdown.pack",
        }
    }
}

pub(crate) fn parser_label(engine: &str) -> Option<&'static str> {
    match engine {
        "nematic.gemtext.pack" => Some("Smolweb pack (Gemtext)"),
        "nematic.markdown.pack" => Some("Smolweb pack (Markdown)"),
        _ => None,
    }
}

fn parser_for(engine: &str) -> Option<Parser> {
    Parser::ALL
        .into_iter()
        .find(|parser| parser.engine_id() == engine)
}

#[derive(Clone)]
pub(crate) struct SmolwebPack {
    directory: PathBuf,
}

impl SmolwebPack {
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            directory: install::installed(root),
        }
    }

    pub(crate) fn register(&self, registry: &mut inker::SessionRegistry<Scene>) {
        for parser in Parser::ALL {
            registry.register(Box::new(PackParser {
                pack: self.clone(),
                parser,
            }));
        }
    }

    pub(crate) fn parser_availability(
        &self,
        engine: &str,
    ) -> Option<crate::content::EngineAvailability> {
        let parser = parser_for(engine)?;
        use crate::content::EngineAvailability;
        if !install::installed_kind(&self.directory).is_ok_and(|kind| kind.supports_parser(parser))
        {
            return Some(EngineAvailability::Unavailable {
                reason: "parser absent from pack".into(),
            });
        }
        if install::disabled(&self.directory, "parser", parser.name()) {
            Some(EngineAvailability::Disabled)
        } else {
            Some(EngineAvailability::Registered)
        }
    }

    pub(crate) fn lower(
        &self,
        engine: &str,
        input: &EngineInput,
    ) -> Result<EngineDocument, SessionError> {
        let parser = parser_for(engine)
            .ok_or_else(|| SessionError::SpawnFailed("unknown Smolweb pack parser".into()))?;
        let prepare = || {
            let kind = install::installed_kind(&self.directory)?;
            if !kind.supports_parser(parser)
                || install::disabled(&self.directory, "parser", parser.name())
            {
                return Err(io::Error::other(format!(
                    "{} parser is unavailable",
                    parser.name()
                )));
            }
            if !parser.accepts(input, kind.is_legacy()) {
                return Err(io::Error::other(format!(
                    "{} parser does not accept this response",
                    parser.name()
                )));
            }
            let command = if kind.is_legacy() {
                protocol::LEGACY_COMMAND
            } else {
                parser.command()
            };
            self.run(parser, command, input)
        };
        prepare()
            .map_err(|error| SessionError::SpawnFailed(format!("Smolweb engine pack: {error}")))
    }

    pub(crate) fn admit_transport(&self, scheme: &str) -> Result<(), String> {
        if let Some(transport) = Transport::from_scheme(scheme)
            && install::installed_kind(&self.directory)
                .is_ok_and(|kind| kind.supports_transport(transport))
            && install::disabled(&self.directory, "protocol", transport.name())
        {
            return Err(format!(
                "{} connections are disabled in the Smolweb pack",
                transport.name()
            ));
        }
        Ok(())
    }

    fn run(
        &self,
        parser: Parser,
        command_arg: &str,
        input: &EngineInput,
    ) -> io::Result<EngineDocument> {
        let request = serde_json::to_vec(input)?;
        if request.len() as u64 > protocol::REQUEST_LIMIT {
            return Err(io::Error::other("input exceeds its byte limit"));
        }
        // Anonymous files keep the protocol off the command line and avoid
        // blocking in a pipe operation before the child can be timed out.
        let mut stdin = tempfile::tempfile()?;
        std::io::Write::write_all(&mut stdin, &request)?;
        stdin.rewind()?;
        let mut stdout = tempfile::tempfile()?;
        let mut command = Command::new(fs::canonicalize(self.directory.join(EXECUTABLE))?);
        command
            .arg(command_arg)
            .env_clear()
            .stdin(stdin)
            .stdout(stdout.try_clone()?)
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn()?;
        let deadline = Instant::now() + PROCESS_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => break,
                Ok(Some(status)) => {
                    return Err(io::Error::other(format!("worker exited with {status}")));
                },
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5));
                },
                result => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(result
                        .err()
                        .unwrap_or_else(|| io::Error::other("worker timed out")));
                },
            }
        }
        stdout.rewind()?;
        let mut reply = Vec::new();
        stdout
            .take(protocol::REPLY_LIMIT + 1)
            .read_to_end(&mut reply)?;
        if reply.len() as u64 > protocol::REPLY_LIMIT {
            return Err(io::Error::other("output exceeds its byte limit"));
        }
        let document: EngineDocument = serde_json::from_slice(&reply)?;
        if document.address != input.address
            || document.provenance.source_kind.as_deref() != Some(parser.source_kind())
        {
            return Err(io::Error::other(
                "worker returned a document for the wrong source",
            ));
        }
        Ok(document)
    }
}

struct PackParser {
    pack: SmolwebPack,
    parser: Parser,
}

impl SessionEngine<Scene> for PackParser {
    fn engine_id(&self) -> &str {
        self.parser.engine_id()
    }

    fn spawn(
        &self,
        request: &SessionSpawnRequest,
    ) -> Result<Box<dyn DocumentSession<Scene>>, SessionError> {
        let body = request.body.as_ref().ok_or_else(|| {
            SessionError::SpawnFailed("Smolweb engine pack requires a host-fetched body".into())
        })?;
        let mut input = EngineInput::new(&request.address, body);
        input.content_type = request.content_type.clone();
        let document = SmolwebDocument::from_document_with_theme(
            self.pack.lower(self.engine_id(), &input)?,
            SmolwebTheme::default(),
        );
        Ok(Box::new(SmolwebDocumentSession::new(
            document,
            request.viewport,
        )))
    }
}
