//! Turnstone admission for the reusable Knot document surface.

use std::io;
use std::path::{Path, PathBuf};

use cambium::{DomHandle, RetainedSurfaceSession};
use genet_host_api::{SurfaceDescriptor, SurfaceUnavailableReason};
use knot_document::{
    DocumentFormat, KNOT_DOCUMENT_CSS, KnotDocumentSession, KnotDocumentSurfaceState,
    knot_document_descriptor, knot_document_surface,
};
use serde::{Deserialize, Serialize};

use crate::contributed_surface::{SurfaceAdmissionError, SurfaceProvider};
use crate::panes::{PaneKindId, PaneSource, SerializedSource, SourceRef, SourceSchemaId};

pub const PANE_KIND: &str = "turnstone.knot-document";
pub const SOURCE_SCHEMA: &str = "knot.document.v1";
pub const SOURCE_VERSION: u32 = 1;

/// The durable source payload for a local Knot document pane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KnotDocumentAccessV1 {
    #[default]
    ReadWrite,
    ReadOnly,
}

/// The durable source payload for a local Knot document pane.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotDocumentFileSourceV1 {
    pub path: PathBuf,
    #[serde(default)]
    pub access: KnotDocumentAccessV1,
}

pub fn file_source(path: impl Into<PathBuf>) -> PaneSource {
    file_source_with_access(path, KnotDocumentAccessV1::ReadWrite)
}

pub fn read_only_file_source(path: impl Into<PathBuf>) -> PaneSource {
    file_source_with_access(path, KnotDocumentAccessV1::ReadOnly)
}

fn file_source_with_access(path: impl Into<PathBuf>, access: KnotDocumentAccessV1) -> PaneSource {
    let payload = KnotDocumentFileSourceV1 {
        path: path.into(),
        access,
    };
    PaneSource::Fixed(SourceRef::External {
        schema: SourceSchemaId::new(SOURCE_SCHEMA),
        payload: SerializedSource {
            version: SOURCE_VERSION,
            payload: serde_json::to_value(payload)
                .expect("a path-only Knot document source is serializable"),
        },
    })
}

/// The product-neutral registry adapter for one local Knot document.
pub struct KnotDocumentProvider {
    pane_kind: PaneKindId,
    source_schema: SourceSchemaId,
    descriptor: SurfaceDescriptor,
}

impl Default for KnotDocumentProvider {
    fn default() -> Self {
        Self {
            pane_kind: PaneKindId::new(PANE_KIND),
            source_schema: SourceSchemaId::new(SOURCE_SCHEMA),
            descriptor: knot_document_descriptor(),
        }
    }
}

impl SurfaceProvider for KnotDocumentProvider {
    fn pane_kind(&self) -> &PaneKindId {
        &self.pane_kind
    }

    fn source_schema(&self) -> &SourceSchemaId {
        &self.source_schema
    }

    fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }

    fn stylesheet(&self) -> &str {
        KNOT_DOCUMENT_CSS
    }

    fn admit(
        &self,
        source: &PaneSource,
        dom: DomHandle,
    ) -> Result<Box<dyn RetainedSurfaceSession>, SurfaceAdmissionError> {
        let PaneSource::Fixed(SourceRef::External { schema, payload }) = source else {
            return Err(SurfaceAdmissionError::InvalidSource {
                expected: self.source_schema.clone(),
                actual: None,
            });
        };
        if schema != &self.source_schema {
            return Err(SurfaceAdmissionError::InvalidSource {
                expected: self.source_schema.clone(),
                actual: Some(schema.clone()),
            });
        }
        if payload.version != SOURCE_VERSION {
            return Err(invalid_payload(format!(
                "version {} is not supported; expected {SOURCE_VERSION}",
                payload.version
            )));
        }
        let source: KnotDocumentFileSourceV1 = serde_json::from_value(payload.payload.clone())
            .map_err(|error| invalid_payload(error.to_string()))?;
        admit_file(&source.path)?;
        let session = open_session(&source)?;
        Ok(knot_document_surface(
            dom,
            KnotDocumentSurfaceState::new(session),
        ))
    }
}

fn open_session(
    source: &KnotDocumentFileSourceV1,
) -> Result<KnotDocumentSession, SurfaceAdmissionError> {
    let result = match source.access {
        KnotDocumentAccessV1::ReadWrite => KnotDocumentSession::open(&source.path),
        KnotDocumentAccessV1::ReadOnly => KnotDocumentSession::open_read_only(&source.path),
    };
    result.map_err(|message| SurfaceAdmissionError::Unavailable {
        reason: SurfaceUnavailableReason::Other(message),
    })
}

fn invalid_payload(message: String) -> SurfaceAdmissionError {
    SurfaceAdmissionError::InvalidPayload {
        schema: SourceSchemaId::new(SOURCE_SCHEMA),
        message,
    }
}

fn admit_file(path: &Path) -> Result<(), SurfaceAdmissionError> {
    if !matches!(
        DocumentFormat::from_path(path),
        Some(DocumentFormat::Djot | DocumentFormat::Knot)
    ) {
        return Err(SurfaceAdmissionError::Unavailable {
            reason: SurfaceUnavailableReason::Unsupported,
        });
    }
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(SurfaceAdmissionError::Unavailable {
            reason: SurfaceUnavailableReason::Unsupported,
        }),
        Err(error) => Err(SurfaceAdmissionError::Unavailable {
            reason: match error.kind() {
                io::ErrorKind::NotFound => SurfaceUnavailableReason::Absent,
                io::ErrorKind::PermissionDenied => SurfaceUnavailableReason::Denied,
                _ => SurfaceUnavailableReason::Other(error.to_string()),
            },
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contributed_surface::SurfaceProviderRegistry;
    use tempfile::tempdir;

    #[test]
    fn registry_admits_the_published_knot_surface_from_a_versioned_file_source() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        std::fs::write(&path, "# Field\n").unwrap();
        let mut registry = SurfaceProviderRegistry::new();
        registry
            .register_provider(KnotDocumentProvider::default())
            .expect("Knot provider");

        let pane = registry
            .admit(&PaneKindId::new(PANE_KIND), &file_source(&path))
            .expect("Knot document admission");
        assert_eq!(pane.descriptor(), &knot_document_descriptor());
        assert!(pane.availability().is_available());
    }

    #[test]
    fn absent_file_is_a_generic_typed_unavailable_surface() {
        let temp = tempdir().unwrap();
        let mut registry = SurfaceProviderRegistry::new();
        registry
            .register_provider(KnotDocumentProvider::default())
            .expect("Knot provider");

        let pane = registry
            .admit(
                &PaneKindId::new(PANE_KIND),
                &file_source(temp.path().join("missing.djot")),
            )
            .expect("generic unavailable surface");
        assert_eq!(
            pane.availability(),
            genet_host_api::SurfaceAvailability::Unavailable(SurfaceUnavailableReason::Absent)
        );
    }

    #[test]
    fn read_only_source_opens_a_read_only_document_session() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("field.djot");
        std::fs::write(&path, "# Field\n").unwrap();
        let source = KnotDocumentFileSourceV1 {
            path,
            access: KnotDocumentAccessV1::ReadOnly,
        };

        let session = open_session(&source).expect("read-only session");
        assert_eq!(
            session.snapshot().write_posture,
            knot_document::KnotDocumentWritePostureV1::ReadOnly
        );
    }

    #[test]
    fn payload_version_is_checked_before_file_authority_is_opened() {
        let mut source = file_source("field.djot");
        let PaneSource::Fixed(SourceRef::External { payload, .. }) = &mut source else {
            unreachable!()
        };
        payload.version = SOURCE_VERSION + 1;
        let mut registry = SurfaceProviderRegistry::new();
        registry
            .register_provider(KnotDocumentProvider::default())
            .expect("Knot provider");

        assert!(matches!(
            registry.admit(&PaneKindId::new(PANE_KIND), &source),
            Err(SurfaceAdmissionError::InvalidPayload { .. })
        ));
    }
}
