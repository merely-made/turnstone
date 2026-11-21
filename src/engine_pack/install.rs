// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{EXECUTABLE, Parser};

const MANIFEST: &str = "pack.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Transport {
    Gemini,
    Spartan,
}

impl Transport {
    const ALL: [Self; 2] = [Self::Gemini, Self::Spartan];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Gemini => "gemini",
            Self::Spartan => "spartan",
        }
    }

    pub(crate) fn from_scheme(scheme: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|transport| scheme.eq_ignore_ascii_case(transport.name()))
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum PackKind {
    GeminiV1,
    SmolwebV2,
}

impl PackKind {
    pub(super) fn is_legacy(self) -> bool {
        matches!(self, Self::GeminiV1)
    }

    pub(super) fn supports_parser(self, parser: Parser) -> bool {
        !self.is_legacy() || parser == Parser::Gemtext
    }

    pub(super) fn supports_transport(self, transport: Transport) -> bool {
        !self.is_legacy() || transport == Transport::Gemini
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Manifest {
    Legacy(LegacyManifest),
    Current(CurrentManifest),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyManifest {
    version: u32,
    engine: String,
    scheme: String,
    content_type: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrentManifest {
    version: u32,
    protocols: Vec<String>,
    parsers: Vec<String>,
}

fn read_manifest(directory: &Path) -> io::Result<(PackKind, Vec<u8>)> {
    let mut bytes = Vec::new();
    fs::File::open(directory.join(MANIFEST))?
        .take(1025)
        .read_to_end(&mut bytes)?;
    if bytes.len() <= 1024 {
        match serde_json::from_slice::<Manifest>(&bytes) {
            Ok(Manifest::Legacy(entry))
                if entry.version == 1
                    && entry.engine == Parser::Gemtext.engine_id()
                    && entry.scheme == Transport::Gemini.name()
                    && entry.content_type == "text/gemini" =>
            {
                return Ok((PackKind::GeminiV1, bytes));
            },
            Ok(Manifest::Current(entry))
                if entry.version == 2
                    && entry.protocols == Transport::ALL.map(Transport::name)
                    && entry.parsers == Parser::ALL.map(Parser::name) =>
            {
                return Ok((PackKind::SmolwebV2, bytes));
            },
            _ => {},
        }
    }
    Err(io::Error::other("unsupported Smolweb pack manifest"))
}

pub(super) fn installed_kind(directory: &Path) -> io::Result<PackKind> {
    let (kind, _) = read_manifest(directory)?;
    if directory.join(EXECUTABLE).is_file() {
        Ok(kind)
    } else {
        Err(io::Error::other("Smolweb pack executable is missing"))
    }
}

fn pack_parent(root: &Path) -> PathBuf {
    root.join("engine-packs")
}

pub(super) fn installed(root: &Path) -> PathBuf {
    pack_parent(root).join("smolweb")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Capability {
    Protocol(Transport),
    Parser(Parser),
}

impl Capability {
    fn parse(kind: PackKind, group: &OsStr, name: &OsStr) -> io::Result<Self> {
        Self::installed_in(kind)
            .find(|item| {
                let (item_group, item_name) = item.names();
                group == OsStr::new(item_group) && name == OsStr::new(item_name)
            })
            .ok_or_else(|| io::Error::other("capability absent from the installed Smolweb pack"))
    }

    pub(super) fn installed_in(kind: PackKind) -> impl Iterator<Item = Self> {
        Transport::ALL
            .into_iter()
            .map(Self::Protocol)
            .chain(Parser::ALL.into_iter().map(Self::Parser))
            .filter(move |item| item.supported_by(kind))
    }

    pub(super) fn supported_by(self, kind: PackKind) -> bool {
        match self {
            Self::Protocol(item) => kind.supports_transport(item),
            Self::Parser(item) => kind.supports_parser(item),
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Protocol(Transport::Gemini) => "Gemini protocol",
            Self::Protocol(Transport::Spartan) => "Spartan protocol",
            Self::Parser(Parser::Gemtext) => "Gemtext parser",
            Self::Parser(Parser::Markdown) => "Markdown parser",
        }
    }

    pub(super) fn names(self) -> (&'static str, &'static str) {
        match self {
            Self::Protocol(transport) => ("protocol", transport.name()),
            Self::Parser(parser) => ("parser", parser.name()),
        }
    }
}

fn marker(directory: &Path, group: &str, name: &str) -> PathBuf {
    directory.join(format!("{group}-{name}.off"))
}

pub(super) fn disabled(directory: &Path, group: &str, name: &str) -> bool {
    marker(directory, group, name).try_exists().unwrap_or(true)
}

fn toggle(directory: &Path, group: &OsStr, name: &OsStr, enabled: bool) -> io::Result<()> {
    let capability = Capability::parse(installed_kind(directory)?, group, name)?;
    set_enabled(directory, capability, enabled)
}

pub(super) fn set_enabled(
    directory: &Path,
    capability: Capability,
    enabled: bool,
) -> io::Result<()> {
    let (group, name) = capability.names();
    let path = marker(directory, group, name);
    let result = if enabled {
        fs::remove_file(path)
    } else {
        fs::File::create_new(path).map(drop)
    };
    let already_set = if enabled {
        io::ErrorKind::NotFound
    } else {
        io::ErrorKind::AlreadyExists
    };
    result.or_else(|error| {
        if error.kind() == already_set {
            Ok(())
        } else {
            Err(error)
        }
    })
}

fn status(directory: &Path) -> io::Result<()> {
    let kind = installed_kind(directory)?;
    for capability in Capability::installed_in(kind) {
        let (group, name) = capability.names();
        let setting = if disabled(directory, group, name) {
            "off"
        } else {
            "on"
        };
        println!("{group} {name} {setting}");
    }
    Ok(())
}

/// Manage the first-party local Smolweb pack before opening the native app.
pub fn command(args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let args: Vec<_> = args.collect();
    let root = crate::session::default_turnstone_root();
    let directory = installed(&root);
    let pack = OsStr::new("smolweb");
    let result = match args.as_slice() {
        [verb, bundle] if verb == OsStr::new("install") => install(&root, Path::new(bundle)),
        [verb, name] if name == pack && verb == OsStr::new("remove") => remove(&root),
        [verb, name] if name == pack && verb == OsStr::new("status") => status(&directory),
        [verb, pack_name, group, name] if pack_name == pack
            && matches!(verb.to_str(), Some("enable" | "disable")) =>
        {
            toggle(&directory, group, name, verb == OsStr::new("enable"))
        },
        _ => return Err(
            "Usage: turnstone engine-pack install <bundle> | remove smolweb | status smolweb | enable|disable smolweb protocol|parser <name>".into()
        ),
    };
    result.map_err(|error| format!("Smolweb engine pack: {error}"))
}

fn install(root: &Path, bundle: &Path) -> io::Result<()> {
    let (_, manifest) = read_manifest(bundle)?;
    let parent = pack_parent(root);
    fs::create_dir_all(&parent)?;
    let staging = tempfile::tempdir_in(parent)?;
    fs::copy(bundle.join(EXECUTABLE), staging.path().join(EXECUTABLE))?;
    fs::write(staging.path().join(MANIFEST), manifest)?;
    fs::rename(staging.path(), installed(root))
}

fn remove(root: &Path) -> io::Result<()> {
    let staging = tempfile::tempdir_in(pack_parent(root))?;
    fs::rename(installed(root), staging.path().join("removed"))?;
    staging.close()
}
