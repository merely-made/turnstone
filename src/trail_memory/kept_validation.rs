// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::path::{Path, PathBuf};

use cambium::{DetailRow, DetailSection, ValidationEvidence, ValidationFinding};
use graphshell_host::pinned_shacl::PinnedValidation;
use muniment::{Backend, DirectoryBackend, FjallStore};
use oxrdf::NamedNode;
use pandect::graph_session::MereSessions;
use uuid::Uuid;

use super::{SourceCustody, SystemTime, sealed_capture};
use crate::action::{StoredSourceDocument, Update};
use crate::panes::PaneId;
use crate::research_storage::{BudgetStore, StorageLimits};
use crate::saved_sources::KeptSource;
use crate::source_capture::CaptureRequest;
use crate::swatch_pane::validation::{FindingView, Reply, Report, Response, Shapes, claim_preview};

const SHAPES_KEY: &str = "validation-shapes.json";

mod evidence;
pub use evidence::EvidenceRequest;
mod shapes;
use shapes::StoredShapes;

#[cfg(test)]
mod tests;

pub enum Command {
    Validate(ValidationRequest),
    OpenEvidence(EvidenceRequest),
}

impl Command {
    pub(super) fn run(
        self,
        directory: &Path,
        generation: Uuid,
        custody: Option<&SourceCustody>,
        store: Option<&FjallStore>,
        limits: &Result<StorageLimits, String>,
    ) -> Update {
        let reply = match self {
            Self::Validate(request) => {
                Reply::Report(request.run(directory, generation, custody, store, limits))
            },
            Self::OpenEvidence(request) => {
                Reply::Evidence(request.run(directory, generation, custody))
            },
        };
        Update::KeptSourceValidated(Box::new(reply))
    }
}

pub struct ValidationRequest {
    pub generation: Uuid,
    pub request: Uuid,
    pub pane: PaneId,
    pub source: KeptSource,
    pub capture: CaptureRequest,
    pub stored: StoredSourceDocument,
    pub directory: PathBuf,
    pub shapes: Shapes,
}

impl ValidationRequest {
    fn qualified(&self, directory: &Path, generation: Uuid) -> bool {
        self.generation == generation
            && self.directory == directory
            && in_session_directory(directory, &self.source)
            && self.capture.keep
            && self.capture.id == self.source.request
            && self.capture.session == self.source.key.session
            && self.capture.graph == self.source.key.graph
            && self.capture.node == self.source.key.node
            && self.stored.annotation_manifest == self.source.key.annotation_manifest
            && self
                .capture
                .source
                .as_ref()
                .is_some_and(|identity| identity.hash == self.source.capture_hash)
    }

    pub(super) fn run(
        self,
        directory: &Path,
        generation: Uuid,
        custody: Option<&SourceCustody>,
        store: Option<&FjallStore>,
        limits: &Result<StorageLimits, String>,
    ) -> Response {
        let result = self.run_scoped(directory, generation, custody, store, limits);
        Response {
            generation: self.generation,
            request: self.request,
            pane: self.pane,
            source: self.source,
            result,
        }
    }

    fn run_scoped(
        &self,
        directory: &Path,
        generation: Uuid,
        custody: Option<&SourceCustody>,
        store: Option<&FjallStore>,
        limits: &Result<StorageLimits, String>,
    ) -> Result<Report, String> {
        if !self.qualified(directory, generation) {
            return Err("The kept source belongs to another session opening or attachment.".into());
        }
        let custody = custody.ok_or("Unlock the profile to validate this kept source.")?;
        let epoch = custody.snapshot();
        if epoch.is_none() {
            return Err("Unlock the profile to validate this kept source.".into());
        }
        let mut store = BudgetStore {
            store: store.ok_or("The kept source store is unavailable.")?,
            budget: limits.as_ref().map_err(Clone::clone)?.budget(),
        };
        let document = sealed_capture::read_kept_document(
            &mut store,
            custody.sealer.as_ref(),
            &self.capture,
            &self.stored,
        )
        .map_err(|error| error.to_string())?;
        let journal_path = journal_path(directory, &self.stored.annotation_manifest);
        let result = pollster::block_on(validate_capture(
            self,
            document,
            &journal_path,
            custody,
            epoch,
        ));
        if !custody.remains_current(epoch) {
            return Err("Profile custody changed during validation.".into());
        }
        result
    }
}

fn in_session_directory(directory: &Path, source: &KeptSource) -> bool {
    directory
        .parent()
        .and_then(Path::file_name)
        .and_then(|parent| parent.to_str())
        == Some(source.key.session.as_uuid().to_string().as_str())
}

fn journal_path(directory: &Path, manifest: &str) -> PathBuf {
    let manifest = eidetic::Hash::of(manifest.as_bytes()).to_hex();
    directory.join("structured-validation").join(manifest)
}

async fn validate_capture(
    request: &ValidationRequest,
    document: crate::content::FetchedDocument,
    journal_path: &Path,
    custody: &SourceCustody,
    epoch: Option<u64>,
) -> Result<Report, String> {
    let origin = document
        .effective_url
        .as_deref()
        .ok_or("The kept source has no effective address.")?;
    let address = url::Url::parse(origin).map_err(|error| error.to_string())?;
    let dom = genet_static_dom::StaticDocument::parse(&document.body);
    let base = crate::swatch_pane::document_base(&dom, &address);
    let blocks = fleece::extract_json_ld_blocks(&dom);
    let source = NamedNode::new(format!(
        "urn:mere:kept-source:{}",
        eidetic::Hash::of(request.stored.annotation_manifest.as_bytes()).to_hex(),
    ))
    .map_err(|error| error.to_string())?;

    if request.shapes == Shapes::Reopen && !journal_path.is_dir() {
        return Err("This kept source has no saved validation.".into());
    }
    let disk = DirectoryBackend::open(journal_path).map_err(|error| error.to_string())?;
    let backend = sporran::WalletSealedBackend::with_sealer(disk, custody.sealer.clone());
    let (shapes, checked_shapes) = match &request.shapes {
        Shapes::File(path) => StoredShapes::read_turtle(path)?,
        Shapes::Pack(path) => StoredShapes::read_pack(path)?,
        Shapes::Reopen => {
            let bytes = backend
                .get(SHAPES_KEY)
                .await
                .map_err(|error| error.to_string())?
                .ok_or("This kept source has no saved validation.")?;
            let shapes: StoredShapes =
                serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            let checked = shapes.check()?;
            (shapes, checked)
        },
        Shapes::Choose | Shapes::ChoosePack => return Err("Choose a shapes file first.".into()),
    };
    let sessions = MereSessions::new(backend.clone());
    let author = kernel::graph::Author::engine("turnstone-structured-source", "1");
    let mut session = match sessions
        .latest_live()
        .await
        .map_err(|error| error.to_string())?
    {
        Some(manifest) => sessions
            .open(manifest.session_id)
            .await
            .map_err(|error| error.to_string())?,
        None if request.shapes == Shapes::Reopen => {
            return Err("The saved validation journal is unavailable.".into());
        },
        None => sessions.begin(author.clone(), None),
    };
    if !custody.remains_current(epoch) {
        return Err("Profile custody changed before validation.".into());
    }
    if request.shapes != Shapes::Reopen {
        let claims = crate::swatch_pane::interpret_blocks(&blocks, &base);
        session
            .admit_page_claims(author, &source, &claims, SystemTime::now())
            .await
            .map_err(|error| error.to_string())?;
    }
    let pin = session
        .pin(session.journal().live_cursor())
        .map_err(|error| error.to_string())?;
    let projection = PinnedValidation::validate(pin, &source, &checked_shapes.graph)
        .map_err(|error| format!("SHACL validation failed: {error}"))?;
    if !custody.remains_current(epoch) {
        return Err("Profile custody changed during validation.".into());
    }
    if request.shapes != Shapes::Reopen {
        let bytes = serde_json::to_vec(&shapes).map_err(|error| error.to_string())?;
        backend
            .put(SHAPES_KEY, &bytes)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(report_view(
        request.request,
        projection,
        checked_shapes.pack_label,
        origin,
        &request.source.capture_hash,
        blocks,
    ))
}

fn report_view(
    id: Uuid,
    projection: PinnedValidation,
    pack_label: Option<String>,
    origin: &str,
    capture: &str,
    blocks: Vec<fleece::EmbeddedJsonLdBlock>,
) -> Report {
    let report = projection.report();
    let kept = projection.kept();
    let findings = report
        .findings
        .iter()
        .map(|finding| {
            let path = finding.path.as_ref().and_then(|path| path.predicate());
            let evidence = kept
                .evidence_for(&finding.focus_node, path)
                .into_iter()
                .map(|block| {
                    let mut rows = vec![
                        DetailRow::new("Source", origin),
                        DetailRow::new("Capture", capture),
                    ];
                    rows.extend(
                        block
                            .quads
                            .iter()
                            .filter(|quad| {
                                quad.object == finding.focus_node
                                    || match (&quad.subject, &finding.focus_node) {
                                        (
                                            oxrdf::NamedOrBlankNode::NamedNode(subject),
                                            oxrdf::Term::NamedNode(node),
                                        ) => subject == node,
                                        (
                                            oxrdf::NamedOrBlankNode::BlankNode(subject),
                                            oxrdf::Term::BlankNode(node),
                                        ) => subject == node,
                                        _ => false,
                                    }
                            })
                            .take(8)
                            .map(|quad| DetailRow::new("Claim", quad.to_string())),
                    );
                    ValidationEvidence {
                        id: block.order,
                        detail: DetailSection::new(
                            format!("JSON-LD block {}", block.order.saturating_add(1)),
                            rows,
                        ),
                    }
                })
                .collect();
            FindingView {
                finding: ValidationFinding {
                    focus_node: finding.focus_node.to_string(),
                    path: finding.path.as_ref().map(ToString::to_string),
                    severity: finding.severity.clone(),
                    message: finding.message.clone(),
                },
                evidence,
            }
        })
        .collect();
    let claims = claim_preview(&kept.claims);
    let diagnostics = kept
        .claims
        .diagnostics
        .iter()
        .map(|issue| {
            format!(
                "JSON-LD block {}: {}",
                issue.order.saturating_add(1),
                issue.message
            )
        })
        .collect();
    Report {
        id,
        origin: origin.into(),
        blocks,
        conforms: report.conforms,
        projection,
        pack_label,
        claims,
        diagnostics,
        findings,
    }
}
