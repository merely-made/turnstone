// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::collections::BTreeMap;
use std::io::Write;

use serde::{Deserialize, Serialize};

use super::*;

const SESSION_FILE: &str = "smolweb-pack-settings.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Switch {
    pub(crate) capability: Capability,
    pub(crate) profile_enabled: bool,
    pub(crate) session_override: Option<bool>,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct PackSettings {
    pub(crate) switches: Vec<Switch>,
    pub(crate) session: Option<u64>,
    pub(crate) session_error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    Profile(bool),
    Session {
        generation: u64,
        enabled: Option<bool>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PackEdit {
    pub(crate) capability: Capability,
    pub(crate) scope: Scope,
}

#[derive(Default)]
pub(super) struct SessionSwitches {
    generation: u64,
    path: Option<PathBuf>,
    values: BTreeMap<String, bool>,
    error: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SavedOverrides {
    version: u32,
    overrides: BTreeMap<String, bool>,
}

impl SavedOverrides {
    fn load(path: &Path) -> io::Result<BTreeMap<String, bool>> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(error) => return Err(error),
        };
        let saved: Self = serde_json::from_slice(&bytes)?;
        if saved.version != 1 {
            return Err(io::Error::other(
                "unsupported Smolweb session settings version",
            ));
        }
        Ok(saved.overrides)
    }

    fn save(path: &Path, overrides: BTreeMap<String, bool>) -> io::Result<()> {
        let directory = path.parent().expect("session settings directory");
        let mut temp = tempfile::NamedTempFile::new_in(directory)?;
        serde_json::to_writer_pretty(
            &mut temp,
            &Self {
                version: 1,
                overrides,
            },
        )?;
        temp.write_all(b"\n")?;
        temp.as_file().sync_all()?;
        temp.persist(path).map_err(|error| error.error)?;
        Ok(())
    }
}

impl Capability {
    fn key(self) -> String {
        let (group, name) = self.names();
        format!("{group}-{name}")
    }
}

impl SmolwebPack {
    /// Retire the old fetch scope before loading the adopted session's sidecar.
    pub(crate) fn bind_session(&self, directory: Option<&Path>) {
        let mut session = self.session.write().expect("Smolweb session lock");
        let generation = session
            .generation
            .checked_add(1)
            .expect("Smolweb session generation");
        let path = directory.map(|directory| directory.join(SESSION_FILE));
        let (values, error) = match path.as_deref().map(SavedOverrides::load).transpose() {
            Ok(values) => (values.unwrap_or_default(), None),
            Err(error) => {
                tracing::warn!(%error, "could not load Smolweb session settings");
                (BTreeMap::new(), Some(error.to_string()))
            },
        };
        *session = SessionSwitches {
            generation,
            path,
            values,
            error,
        };
    }

    pub(crate) fn settings(&self) -> PackSettings {
        let Ok(kind) = install::installed_kind(&self.directory) else {
            return PackSettings::default();
        };
        let session = self.session.read().expect("Smolweb session lock");
        PackSettings {
            switches: Capability::installed_in(kind)
                .map(|capability| Switch {
                    capability,
                    profile_enabled: self.profile_enabled(capability),
                    session_override: session.values.get(&capability.key()).copied(),
                })
                .collect(),
            session: session
                .path
                .as_ref()
                .filter(|_| session.error.is_none())
                .map(|_| session.generation),
            session_error: session.error.clone(),
        }
    }

    pub(crate) fn edit(&self, PackEdit { capability, scope }: PackEdit) -> io::Result<()> {
        let kind = install::installed_kind(&self.directory)?;
        if !capability.supported_by(kind) {
            return Err(io::Error::other(
                "capability absent from the installed Smolweb pack",
            ));
        }
        match scope {
            Scope::Profile(enabled) => install::set_enabled(&self.directory, capability, enabled),
            Scope::Session {
                generation,
                enabled,
            } => {
                // The shell serializes edits. Hold the guard through the save so
                // no hop can see an override before it is durable.
                let mut session = self.session.write().expect("Smolweb session lock");
                let path = session
                    .path
                    .as_deref()
                    .filter(|_| session.generation == generation && session.error.is_none())
                    .ok_or_else(|| {
                        io::Error::other("Smolweb session settings have changed or are unavailable")
                    })?;
                let mut values = session.values.clone();
                let key = capability.key();
                match enabled {
                    Some(enabled) => {
                        values.insert(key, enabled);
                    },
                    None => {
                        values.remove(&key);
                    },
                }
                if values != session.values {
                    SavedOverrides::save(path, values.clone())?;
                    session.values = values;
                }
                Ok(())
            },
        }
    }

    fn profile_enabled(&self, capability: Capability) -> bool {
        let (group, name) = capability.names();
        !install::disabled(&self.directory, group, name)
    }

    fn enabled(&self, session: &SessionSwitches, capability: Capability) -> bool {
        session
            .values
            .get(&capability.key())
            .copied()
            .unwrap_or_else(|| self.profile_enabled(capability))
    }

    pub(super) fn disabled(&self, capability: Capability) -> bool {
        let session = self.session.read().expect("Smolweb session lock");
        !self.enabled(&session, capability)
    }

    pub(crate) fn hop_admission(&self) -> Arc<fetch::SmolwebHopAdmission> {
        let generation = self
            .session
            .read()
            .expect("Smolweb session lock")
            .generation;
        let pack = self.clone();
        Arc::new(move |scheme| {
            let session = pack.session.read().expect("Smolweb session lock");
            if session.generation != generation {
                return Err("Smolweb fetch belongs to a retired session".into());
            }
            if let Some(transport) = Transport::from_scheme(scheme)
                && install::installed_kind(&pack.directory)
                    .is_ok_and(|kind| kind.supports_transport(transport))
                && !pack.enabled(&session, Capability::Protocol(transport))
            {
                return Err(format!(
                    "{} connections are disabled in the Smolweb pack",
                    transport.name()
                ));
            }
            Ok(())
        })
    }
}

// The manifest and a file at the worker path suffice for host policy tests.
#[cfg(test)]
impl SmolwebPack {
    pub(crate) fn install_test_fixture(&self) {
        fs::create_dir_all(&self.directory).unwrap();
        fs::write(self.directory.join(EXECUTABLE), []).unwrap();
        fs::write(
            self.directory.join("pack.json"),
            include_bytes!("../../ports/smolweb-pack/pack.json"),
        )
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::EngineAvailability;

    #[test]
    fn pack_settings_follow_profile_until_a_session_overrides_each_capability() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first-session");
        let second = root.path().join("second-session");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        let pack = SmolwebPack::new(root.path());
        pack.install_test_fixture();
        let spartan = Capability::Protocol(Transport::Spartan);
        let gemtext = Capability::Parser(Parser::Gemtext);
        let markdown = Capability::Parser(Parser::Markdown);

        pack.bind_session(Some(&first));
        let first_hops = pack.hop_admission();
        let generation = pack.settings().session.unwrap();
        let override_first = |capability, enabled| PackEdit {
            capability,
            scope: Scope::Session {
                generation,
                enabled,
            },
        };
        let profile = |capability, enabled| PackEdit {
            capability,
            scope: Scope::Profile(enabled),
        };
        assert!(first_hops("spartan").is_ok());
        pack.edit(profile(spartan, false)).unwrap();
        assert!(first_hops("spartan").is_err());
        assert!(first_hops("gemini").is_ok());
        pack.edit(override_first(spartan, Some(true))).unwrap();
        assert!(first_hops("spartan").is_ok());

        pack.edit(override_first(markdown, Some(false))).unwrap();
        assert_eq!(
            pack.parser_availability(Parser::Markdown.engine_id()),
            Some(EngineAvailability::Disabled)
        );
        assert_eq!(
            pack.parser_availability(Parser::Gemtext.engine_id()),
            Some(EngineAvailability::Registered)
        );

        pack.bind_session(Some(&second));
        let second_hops = pack.hop_admission();
        assert!(
            first_hops("gemini")
                .unwrap_err()
                .contains("retired session")
        );
        assert!(second_hops("spartan").is_err());
        assert!(second_hops("gemini").is_ok());
        assert_eq!(
            pack.parser_availability(Parser::Markdown.engine_id()),
            Some(EngineAvailability::Registered)
        );
        assert!(pack.edit(override_first(gemtext, Some(false))).is_err());
        assert!(!second.join(SESSION_FILE).exists());

        let reopened = SmolwebPack::new(root.path());
        reopened.bind_session(Some(&first));
        assert!(reopened.hop_admission()("spartan").is_ok());
        assert_eq!(
            reopened.parser_availability(Parser::Markdown.engine_id()),
            Some(EngineAvailability::Disabled)
        );
        let generation = reopened.settings().session.unwrap();
        reopened
            .edit(PackEdit {
                capability: spartan,
                scope: Scope::Session {
                    generation,
                    enabled: None,
                },
            })
            .unwrap();
        assert!(reopened.hop_admission()("spartan").is_err());
        assert_eq!(
            reopened.parser_availability(Parser::Markdown.engine_id()),
            Some(EngineAvailability::Disabled)
        );
    }
}
