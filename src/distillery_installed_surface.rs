//! Turnstone admission for Distillery's installed authority surface.
//!
//! This is the second independent provider over the contributed-surface seam
//! (the Knot shared-surface plan's P0): the same descriptor and erased
//! retained-session mechanism, registered through the same
//! `SurfaceProviderRegistry`, with no Distillery-specific renderer arm. The
//! versioned source carries only the projection facts the installed bootstrap
//! already selected — mounting the pane grants projection, never Distillery
//! authority.

use std::path::PathBuf;
use std::time::Duration;

use cambium::{DomHandle, RetainedSurfaceSession};
use distillery::{
    DISTILLERY_INSTALLED_CSS, DistilleryInstalledSnapshotV1, DistilleryInstalledSurfaceState,
    DistilleryResidentSnapshotV1, ResidentSettings, RetentionSettings,
    distillery_installed_descriptor, distillery_installed_surface,
};
use genet_host_api::SurfaceDescriptor;
use serde::{Deserialize, Serialize};

use crate::contributed_surface::{SurfaceAdmissionError, SurfaceProvider};
use crate::panes::{PaneKindId, PaneSource, SerializedSource, SourceRef, SourceSchemaId};

pub const PANE_KIND: &str = "turnstone.distillery-installed";
pub const SOURCE_SCHEMA: &str = "distillery.installed.v1";
pub const SOURCE_VERSION: u32 = 1;

/// Resident cadence facts, serialized in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistilleryResidentSourceV1 {
    pub tick_every_ms: u64,
    #[serde(default)]
    pub maintenance_every_ms: Option<u64>,
    pub blob_gc_every_ms: u64,
    pub collect_after_checkpoint: bool,
}

/// The durable source payload for one installed Distillery mesh pane.
///
/// Every field is a projection fact the installed bootstrap already selected;
/// nothing here opens a vault, store, or resident.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistilleryInstalledSourceV1 {
    pub profile: String,
    pub protection: String,
    /// The mesh id as 64 lowercase hex characters.
    pub mesh_id: String,
    pub mesh_root: PathBuf,
    pub mesh_store_path: PathBuf,
    pub blob_store_root: PathBuf,
    #[serde(default)]
    pub resident: Option<DistilleryResidentSourceV1>,
}

/// Mint the versioned pane source for one installed Distillery projection.
pub fn installed_source(payload: DistilleryInstalledSourceV1) -> PaneSource {
    PaneSource::Fixed(SourceRef::External {
        schema: SourceSchemaId::new(SOURCE_SCHEMA),
        payload: SerializedSource {
            version: SOURCE_VERSION,
            payload: serde_json::to_value(payload)
                .expect("a data-only installed Distillery source is serializable"),
        },
    })
}

/// The product-neutral registry adapter for one installed Distillery mesh.
pub struct DistilleryInstalledProvider {
    pane_kind: PaneKindId,
    source_schema: SourceSchemaId,
    descriptor: SurfaceDescriptor,
}

impl Default for DistilleryInstalledProvider {
    fn default() -> Self {
        Self {
            pane_kind: PaneKindId::new(PANE_KIND),
            source_schema: SourceSchemaId::new(SOURCE_SCHEMA),
            descriptor: distillery_installed_descriptor(),
        }
    }
}

impl SurfaceProvider for DistilleryInstalledProvider {
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
        DISTILLERY_INSTALLED_CSS
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
        let source: DistilleryInstalledSourceV1 =
            serde_json::from_value(payload.payload.clone())
                .map_err(|error| invalid_payload(error.to_string()))?;
        let snapshot = snapshot_from_source(source)?;
        Ok(distillery_installed_surface(
            dom,
            DistilleryInstalledSurfaceState::new(snapshot),
        ))
    }
}

fn snapshot_from_source(
    source: DistilleryInstalledSourceV1,
) -> Result<DistilleryInstalledSnapshotV1, SurfaceAdmissionError> {
    let resident = source
        .resident
        .map(|resident| {
            let settings = ResidentSettings {
                tick_every: Duration::from_millis(resident.tick_every_ms),
                maintenance_every: resident.maintenance_every_ms.map(Duration::from_millis),
                blob_gc_every: Duration::from_millis(resident.blob_gc_every_ms),
                retention: RetentionSettings {
                    collect_after_checkpoint: resident.collect_after_checkpoint,
                },
            };
            settings
                .validate()
                .map_err(|error| invalid_payload(error.to_string()))?;
            Ok(DistilleryResidentSnapshotV1 { settings })
        })
        .transpose()?;
    Ok(DistilleryInstalledSnapshotV1 {
        profile: source.profile,
        protection: source.protection,
        mesh_id: mesh_id(&source.mesh_id)?,
        mesh_root: source.mesh_root,
        mesh_store_path: source.mesh_store_path,
        blob_store_root: source.blob_store_root,
        resident,
    })
}

fn mesh_id(hex: &str) -> Result<[u8; 32], SurfaceAdmissionError> {
    let bytes = hex.as_bytes();
    if bytes.len() != 64 {
        return Err(invalid_payload(format!(
            "mesh id must be 64 hex characters, got {}",
            bytes.len()
        )));
    }
    let mut id = [0u8; 32];
    for (index, chunk) in bytes.chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk).map_err(|_| bad_mesh_hex())?;
        id[index] = u8::from_str_radix(text, 16).map_err(|_| bad_mesh_hex())?;
    }
    Ok(id)
}

fn bad_mesh_hex() -> SurfaceAdmissionError {
    invalid_payload("mesh id is not valid hex".to_owned())
}

fn invalid_payload(message: String) -> SurfaceAdmissionError {
    SurfaceAdmissionError::InvalidPayload {
        schema: SourceSchemaId::new(SOURCE_SCHEMA),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contributed_surface::SurfaceProviderRegistry;
    use accesskit::Role;
    use layout_dom_api::LayoutDom;

    fn text_present(dom: &genet_scripted_dom::ScriptedDom, needle: &str) -> bool {
        fn contains(
            dom: &genet_scripted_dom::ScriptedDom,
            node: genet_scripted_dom::NodeId,
            needle: &str,
        ) -> bool {
            dom.text(node).is_some_and(|text| text.contains(needle))
                || dom
                    .dom_children(node)
                    .any(|child| contains(dom, child, needle))
        }
        contains(dom, dom.document(), needle)
    }

    fn source_payload() -> DistilleryInstalledSourceV1 {
        DistilleryInstalledSourceV1 {
            profile: "research".into(),
            protection: "sealed passphrase".into(),
            mesh_id: "d5".repeat(32),
            mesh_root: PathBuf::from("C:/data/distillery/meshes/d5"),
            mesh_store_path: PathBuf::from("C:/data/distillery/meshes/d5/mesh.redb"),
            blob_store_root: PathBuf::from("C:/data/distillery/meshes/d5/blobs"),
            resident: None,
        }
    }

    fn registry() -> SurfaceProviderRegistry {
        let mut registry = SurfaceProviderRegistry::new();
        registry
            .register_provider(DistilleryInstalledProvider::default())
            .expect("Distillery installed provider");
        registry
    }

    #[test]
    fn registry_admits_the_published_distillery_surface_from_a_versioned_source() {
        let pane = registry()
            .admit(
                &PaneKindId::new(PANE_KIND),
                &installed_source(source_payload()),
            )
            .expect("Distillery installed admission");
        assert_eq!(pane.descriptor(), &distillery_installed_descriptor());
        assert!(pane.availability().is_available());
    }

    #[test]
    fn admitted_surface_renders_the_projected_profile_and_paths() {
        let mut pane = registry()
            .admit(
                &PaneKindId::new(PANE_KIND),
                &installed_source(source_payload()),
            )
            .expect("Distillery installed admission");
        let _ = pane.scene(480, 320, 1.0);
        let dom = pane.dom_ref();
        assert!(text_present(&dom, "Profile: research"));
        assert!(text_present(&dom, "Protection: sealed passphrase"));
        assert!(text_present(&dom, &"d5".repeat(32)));
        assert!(text_present(&dom, "Resident: not bound"));
        assert!(text_present(&dom, "Resident receipt: none observed"));
    }

    #[test]
    fn resident_cadences_round_trip_through_the_versioned_source() {
        let mut payload = source_payload();
        payload.resident = Some(DistilleryResidentSourceV1 {
            tick_every_ms: 250,
            maintenance_every_ms: None,
            blob_gc_every_ms: 60_000,
            collect_after_checkpoint: true,
        });
        let mut pane = registry()
            .admit(&PaneKindId::new(PANE_KIND), &installed_source(payload))
            .expect("Distillery installed admission");
        let _ = pane.scene(480, 320, 1.0);
        assert!(text_present(
            &pane.dom_ref(),
            "Resident: tick 250 ms; maintenance explicit only; blob collection 60000 ms; \
             release settled custody enabled"
        ));
    }

    #[test]
    fn malformed_mesh_ids_and_spinning_cadences_are_refused_before_admission() {
        let mut short_mesh = source_payload();
        short_mesh.mesh_id = "d5d5".into();
        assert!(matches!(
            registry().admit(&PaneKindId::new(PANE_KIND), &installed_source(short_mesh)),
            Err(SurfaceAdmissionError::InvalidPayload { .. })
        ));

        let mut spinning = source_payload();
        spinning.resident = Some(DistilleryResidentSourceV1 {
            tick_every_ms: 0,
            maintenance_every_ms: None,
            blob_gc_every_ms: 60_000,
            collect_after_checkpoint: false,
        });
        assert!(matches!(
            registry().admit(&PaneKindId::new(PANE_KIND), &installed_source(spinning)),
            Err(SurfaceAdmissionError::InvalidPayload { .. })
        ));
    }

    #[test]
    fn payload_version_is_checked_before_the_snapshot_is_built() {
        let mut source = installed_source(source_payload());
        let PaneSource::Fixed(SourceRef::External { payload, .. }) = &mut source else {
            unreachable!()
        };
        payload.version = SOURCE_VERSION + 1;
        assert!(matches!(
            registry().admit(&PaneKindId::new(PANE_KIND), &source),
            Err(SurfaceAdmissionError::InvalidPayload { .. })
        ));
    }

    #[test]
    fn retained_accessibility_projects_a_generic_status_surface() {
        let mut pane = registry()
            .admit(
                &PaneKindId::new(PANE_KIND),
                &installed_source(source_payload()),
            )
            .expect("Distillery installed admission");
        let _ = pane.scene(480, 320, 1.0);
        let (tree, _) = pane.accessibility_tree().expect("retained projection");
        let status = tree
            .nodes
            .iter()
            .map(|(_, node)| node)
            .find(|node| node.role() == Role::Status)
            .expect("status role from the contributed surface");
        assert_eq!(status.label(), Some("Distillery installed authority"));
    }
}
