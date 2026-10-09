// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Portable graph-selection transfer between independent Graphshell stores.
//!
//! The graph selection is an immutable Eidetic codicil. File bytes remain
//! separate Muniment blobs, addressed and verified by BLAKE3. Applying a
//! package is explicit about identity: replicate preserves container ids;
//! copy mints stable-per-transfer ids and records `CopiedFrom` provenance.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;

use chartulary::{AcceptAll, FacetId};
use chirograph::Sha256NamedInformation;
use eidetic::{
    BlobSource, Codicil, Hash, ManifestId, MereNativeFieldSpec, MereNativeSchemaBuilder,
    ModerationState, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, SchemaDefinition, SchemaRef,
    TimeBounds, Timestamp, TrustEnvelope, TrustLevel, TypedPayload, save_schema, save_typed,
    validate_payload,
};
use mere::kernel::geometry::PortablePoint;
use mere::kernel::graph::node_facets::{
    ARRANGEMENT_FRAME_LAYOUT, ARRANGEMENT_PIN, ARRANGEMENT_SPLIT_OFFER_SUPPRESSED,
    PROVENANCE_DERIVATIONS, PROVENANCE_IMPORT, VISIT_HISTORY,
};
use mere::kernel::graph::{Graph, NodeFacetStore};
use muniment::{Backend, BlobStore};
use pandect::graph_placement::materialize_snapshot;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::access::{
    ACCESS_HISTORY_FACET, AccessAction, AccessObservation, AccessRecord, AccessRecordFilter,
    AccessTransition, access_history, bootstrap_access_record_schema, query_access_records,
    record_observation, save_access_record,
};
use crate::mere_host::MereHost;
use crate::product::{
    CONTENT_FACET, ExportRequest, ProductCodicilV3, ProductError, ProfiledProductCodicil,
    SAVED_SCENE_FACET, SAVED_SCENE_FACET_V2, SAVED_SCENE_FACET_V3, SavedSceneV2, SavedSceneV3,
    decode_profiled_codicil,
};

pub const TRANSFER_MANIFEST_SCHEMA: &str = "graphshell.transfer-manifest/v1";
pub const TRANSFER_CONTENT_FACET: &str = "graphshell.transfer-content/v1";

static PRODUCT_CODICIL_SCHEMA_REF: LazyLock<SchemaRef> =
    LazyLock::new(|| schema_ref(&product_codicil_schema()));
static TRANSFER_RECEIPT_SCHEMA_REF: LazyLock<SchemaRef> =
    LazyLock::new(|| schema_ref(&transfer_receipt_schema()));

fn schema_ref(definition: &SchemaDefinition) -> SchemaRef {
    let bytes = serde_json::to_vec(definition).expect("Graphshell schema always serializes");
    SchemaRef::from_id(ManifestId::from_hash(Hash::of(&bytes)))
}

pub(crate) fn product_codicil_schema() -> SchemaDefinition {
    MereNativeSchemaBuilder::new("graphshell.GraphCodicil/v3")
        .description("A closed graph or scene selection with portable facets.")
        .field("schema", MereNativeFieldSpec::String, true)
        .field(
            "scope",
            MereNativeFieldSpec::Enum {
                values: vec![
                    "object-only".to_string(),
                    "direct-relations".to_string(),
                    "selected-subgraph".to_string(),
                    "saved-scene".to_string(),
                ],
            },
            true,
        )
        .field("exported_at_ms", MereNativeFieldSpec::U64, true)
        .field("graph", MereNativeFieldSpec::Object, true)
        .field("facets", MereNativeFieldSpec::Object, true)
        .field("scene", MereNativeFieldSpec::Object, false)
        .build()
}

fn transfer_receipt_schema() -> SchemaDefinition {
    MereNativeSchemaBuilder::new("graphshell.TransferReceipt/v1")
        .description("The durable result of applying one graph selection transfer.")
        .field("transfer_id", MereNativeFieldSpec::String, true)
        .field(
            "operation",
            MereNativeFieldSpec::Enum {
                values: vec!["replicate".to_string(), "copy".to_string()],
            },
            true,
        )
        .field("source", MereNativeFieldSpec::Object, true)
        .field("destination", MereNativeFieldSpec::Object, true)
        .field("route", MereNativeFieldSpec::Object, true)
        .field("authorization_grant", MereNativeFieldSpec::String, true)
        .field("manifest_hash", MereNativeFieldSpec::String, true)
        .field("selection_hash", MereNativeFieldSpec::String, true)
        .field("blob_hashes", MereNativeFieldSpec::Array, true)
        .field("id_map", MereNativeFieldSpec::Array, true)
        .field("nodes", MereNativeFieldSpec::U64, true)
        .field("relations", MereNativeFieldSpec::U64, true)
        .field(
            "destination_access_records",
            MereNativeFieldSpec::Array,
            true,
        )
        .field("completed_at_ms", MereNativeFieldSpec::U64, true)
        .field(
            "result",
            MereNativeFieldSpec::Enum {
                values: vec!["completed".to_string()],
            },
            true,
        )
        .build()
}

impl TypedPayload for ProductCodicilV3 {
    fn schema_ref() -> SchemaRef {
        *PRODUCT_CODICIL_SCHEMA_REF
    }
}

impl TypedPayload for ProfiledProductCodicil {
    fn schema_ref() -> SchemaRef {
        *PRODUCT_CODICIL_SCHEMA_REF
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransferOperation {
    Replicate,
    Copy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccessTransferPolicy {
    ExcludeSourceHistory,
    IncludeSourceHistory,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferEndpointV1 {
    pub graph: String,
    pub persona: String,
    pub device: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferRouteV1 {
    pub carrier: String,
    pub peer: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferAuthorization {
    pub grant_id: String,
    pub revoked: bool,
}

#[derive(Clone, Debug)]
pub struct TransferRequest {
    pub transfer_id: Uuid,
    pub operation: TransferOperation,
    pub source: TransferEndpointV1,
    pub destination: TransferEndpointV1,
    pub route: TransferRouteV1,
    pub selection: ExportRequest,
    pub access_policy: AccessTransferPolicy,
    pub privacy: PrivacyClass,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferBlobInput {
    pub node_id: Uuid,
    pub role: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferBlobV1 {
    pub node_id: Uuid,
    pub role: String,
    pub media_type: String,
    pub content_hash: Hash,
    pub byte_len: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferManifestV1 {
    pub schema: String,
    pub transfer_id: Uuid,
    pub operation: TransferOperation,
    pub source: TransferEndpointV1,
    pub destination: TransferEndpointV1,
    pub route: TransferRouteV1,
    pub selection_schema: SchemaDefinition,
    pub selection: Codicil,
    pub blobs: Vec<TransferBlobV1>,
    pub access_policy: AccessTransferPolicy,
    pub access_records: Vec<AccessRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferredIdV1 {
    pub source: Uuid,
    pub destination: Uuid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransferResult {
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferReceiptV1 {
    pub transfer_id: Uuid,
    pub operation: TransferOperation,
    pub source: TransferEndpointV1,
    pub destination: TransferEndpointV1,
    pub route: TransferRouteV1,
    pub authorization_grant: String,
    pub manifest_hash: Hash,
    pub selection_hash: Hash,
    pub blob_hashes: Vec<Hash>,
    pub id_map: Vec<TransferredIdV1>,
    pub nodes: u64,
    pub relations: u64,
    pub destination_access_records: Vec<Uuid>,
    pub completed_at_ms: u64,
    pub result: TransferResult,
}

impl TypedPayload for TransferReceiptV1 {
    fn schema_ref() -> SchemaRef {
        *TRANSFER_RECEIPT_SCHEMA_REF
    }
}

#[derive(Clone, Debug)]
pub struct ApplyTransferContext {
    pub authorization: TransferAuthorization,
    pub application: String,
    pub handler: String,
    pub completed_at_ms: u64,
    pub access_privacy: PrivacyClass,
}

#[derive(Debug)]
pub enum TransferError {
    Product(ProductError),
    Store(muniment::StoreError),
    Eidetic(eidetic::Error),
    InvalidManifest(String),
    MissingBlob(Hash),
    BlobHashMismatch(Hash),
    ContentMismatch { node_id: Uuid, reason: String },
    Revoked(String),
    PartialCopy,
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Product(error) => write!(formatter, "{error}"),
            Self::Store(error) => write!(formatter, "{error}"),
            Self::Eidetic(error) => write!(formatter, "{error}"),
            Self::InvalidManifest(error) => write!(formatter, "invalid transfer manifest: {error}"),
            Self::MissingBlob(hash) => write!(formatter, "transfer blob {hash} is unavailable"),
            Self::BlobHashMismatch(hash) => {
                write!(formatter, "transfer blob {hash} failed its BLAKE3 check")
            },
            Self::ContentMismatch { node_id, reason } => {
                write!(formatter, "content for {node_id} is invalid: {reason}")
            },
            Self::Revoked(grant) => write!(formatter, "transfer grant {grant} is revoked"),
            Self::PartialCopy => formatter.write_str(
                "copy destination contains only part of this transfer's deterministic id set",
            ),
        }
    }
}

impl std::error::Error for TransferError {}

impl From<ProductError> for TransferError {
    fn from(value: ProductError) -> Self {
        Self::Product(value)
    }
}

impl From<muniment::StoreError> for TransferError {
    fn from(value: muniment::StoreError) -> Self {
        Self::Store(value)
    }
}

impl From<eidetic::Error> for TransferError {
    fn from(value: eidetic::Error) -> Self {
        Self::Eidetic(value)
    }
}

/// Build a carrier-ready manifest and stage its external blobs in the source
/// blob store. Local file locations are always removed from the selection.
pub async fn prepare_transfer<HB: Backend, AB: Backend, BB: Backend>(
    host: &MereHost<HB>,
    source_authority: &mut AB,
    source_blobs: &BlobStore<BB>,
    mut request: TransferRequest,
    blob_inputs: Vec<TransferBlobInput>,
) -> Result<TransferManifestV1, TransferError> {
    if host.selected_persona().persona != request.source.persona {
        return Err(TransferError::InvalidManifest(format!(
            "source persona {} is not the host's selected persona {}",
            request.source.persona,
            host.selected_persona().persona
        )));
    }
    if request.operation == TransferOperation::Replicate
        && request.source.persona != request.destination.persona
    {
        return Err(TransferError::InvalidManifest(
            "replicate requires one persona on both endpoints".to_string(),
        ));
    }
    request.selection.include_local_file_locations = false;
    let selection_bytes = host.export_product_codicil(request.selection)?;
    let mut product = decode_profiled_codicil(&selection_bytes)?;
    let selected_ids = selected_ids(&product)?;

    for node_id in &selected_ids {
        product
            .facets
            .remove(node_id, &FacetId::new(ACCESS_HISTORY_FACET));
    }

    let mut blobs = Vec::with_capacity(blob_inputs.len());
    let mut seen = HashSet::new();
    for input in blob_inputs {
        if !selected_ids.contains(&input.node_id) {
            return Err(TransferError::ContentMismatch {
                node_id: input.node_id,
                reason: "blob is outside the selected graph closure".to_string(),
            });
        }
        if !seen.insert((input.node_id, input.role.clone())) {
            return Err(TransferError::ContentMismatch {
                node_id: input.node_id,
                reason: format!("duplicate blob role {}", input.role),
            });
        }
        verify_content_facet(&product.facets, &input)?;
        let muniment_hash = source_blobs.put(&input.bytes).await?;
        let content_hash = Hash::of(&input.bytes);
        debug_assert_eq!(muniment_hash.to_hex(), content_hash.to_hex());
        blobs.push(TransferBlobV1 {
            node_id: input.node_id,
            role: input.role,
            media_type: input.media_type,
            content_hash,
            byte_len: input.bytes.len() as u64,
        });
    }
    require_selected_content_blobs(&product.facets, &selected_ids, &blobs)?;
    blobs.sort_by(|left, right| {
        (left.node_id, left.role.as_str()).cmp(&(right.node_id, right.role.as_str()))
    });

    let access_records = match request.access_policy {
        AccessTransferPolicy::ExcludeSourceHistory => Vec::new(),
        AccessTransferPolicy::IncludeSourceHistory => {
            query_access_records(source_authority, &AccessRecordFilter::default())
                .await?
                .into_iter()
                .filter(|record| selected_ids.contains(&record.container_id))
                .collect()
        },
    };

    let selection_payload = product.serialize_to_bytes()?;
    let definition = product_codicil_schema();
    validate_payload(&definition, &selection_payload)?;
    let selection = Codicil::new(
        ProductCodicilV3::schema_ref(),
        selection_payload,
        request.privacy,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some(format!("graphshell-transfer/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp(product.exported_at_ms),
        },
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        },
        TimeBounds::at(Timestamp(product.exported_at_ms)),
    );

    Ok(TransferManifestV1 {
        schema: TRANSFER_MANIFEST_SCHEMA.to_string(),
        transfer_id: request.transfer_id,
        operation: request.operation,
        source: request.source,
        destination: request.destination,
        route: request.route,
        selection_schema: definition,
        selection,
        blobs,
        access_policy: request.access_policy,
        access_records,
    })
}

/// Apply a verified package to another host and store. The authorization check
/// is deliberately first, so a revoked transfer cannot fetch bytes or mutate
/// destination truth.
pub async fn apply_transfer<HB, SB, DB, AB>(
    host: &mut MereHost<HB>,
    source_blobs: &BlobStore<SB>,
    destination_blobs: &BlobStore<DB>,
    destination_authority: &mut AB,
    manifest: &TransferManifestV1,
    context: &ApplyTransferContext,
) -> Result<TransferReceiptV1, TransferError>
where
    HB: Backend,
    SB: Backend,
    DB: Backend,
    AB: Backend,
{
    if context.authorization.revoked {
        return Err(TransferError::Revoked(
            context.authorization.grant_id.clone(),
        ));
    }
    if host.selected_persona().persona != manifest.destination.persona {
        return Err(TransferError::InvalidManifest(format!(
            "destination persona {} is not the host's selected persona {}",
            manifest.destination.persona,
            host.selected_persona().persona
        )));
    }
    let product = verify_manifest(manifest)?;
    let source_addresses: HashMap<Uuid, String> = product
        .graph
        .nodes
        .iter()
        .filter_map(|node| {
            let id = Uuid::parse_str(&node.node_id).ok()?;
            let from_address = node.address.as_url_str();
            Some((
                id,
                if from_address.is_empty() {
                    node.url.clone()
                } else {
                    from_address.to_string()
                },
            ))
        })
        .collect();

    let (target_product, id_map) = match manifest.operation {
        TransferOperation::Replicate => {
            let mut source_ids: Vec<_> = selected_ids(&product)?.into_iter().collect();
            source_ids.sort_unstable();
            let ids = source_ids
                .into_iter()
                .map(|id| TransferredIdV1 {
                    source: id,
                    destination: id,
                })
                .collect();
            (product, ids)
        },
        TransferOperation::Copy => copied_product(&product, manifest)?,
    };
    let existing = id_map
        .iter()
        .filter(|mapping| host.graph().get_node_by_id(mapping.destination).is_some())
        .count();
    if manifest.operation == TransferOperation::Copy && existing > 0 && existing < id_map.len() {
        return Err(TransferError::PartialCopy);
    }
    let import_required =
        manifest.operation == TransferOperation::Replicate || existing != id_map.len();
    if import_required {
        crate::product::product_import_edits(
            host.graph(),
            target_product.graph.clone(),
            target_product.facets.clone(),
            target_product.placement,
        )?;
    }

    for descriptor in &manifest.blobs {
        let hash = muniment_hash(descriptor.content_hash)?;
        if let Some(existing) = destination_blobs.get(&hash).await? {
            if Hash::of(&existing) != descriptor.content_hash
                || existing.len() as u64 != descriptor.byte_len
            {
                return Err(TransferError::BlobHashMismatch(descriptor.content_hash));
            }
            continue;
        }
        let bytes = source_blobs
            .get(&hash)
            .await?
            .ok_or(TransferError::MissingBlob(descriptor.content_hash))?;
        if Hash::of(&bytes) != descriptor.content_hash || bytes.len() as u64 != descriptor.byte_len
        {
            return Err(TransferError::BlobHashMismatch(descriptor.content_hash));
        }
        let stored = destination_blobs.put(&bytes).await?;
        if stored.to_hex() != descriptor.content_hash.to_hex() {
            return Err(TransferError::BlobHashMismatch(descriptor.content_hash));
        }
    }

    if import_required {
        let bytes = target_product.serialize_to_bytes()?;
        host.import_product_codicil(&bytes)?;
    }

    attach_transfer_content(host, manifest, &id_map)?;

    bootstrap_access_record_schema(destination_authority).await?;
    for record in &manifest.access_records {
        save_access_record(destination_authority, record).await?;
        if manifest.operation == TransferOperation::Replicate {
            attach_existing_access_record(host, record)?;
        }
    }

    let mut destination_access_records = Vec::with_capacity(id_map.len());
    for mapping in &id_map {
        let key = host
            .graph()
            .get_node_key_by_id(mapping.destination)
            .ok_or_else(|| {
                TransferError::InvalidManifest(format!(
                    "destination object {} was not imported",
                    mapping.destination
                ))
            })?;
        let source_address = source_addresses.get(&mapping.source).cloned();
        let record_id = Uuid::new_v5(
            &manifest.transfer_id,
            format!("import:{}:{}", mapping.source, mapping.destination).as_bytes(),
        );
        let observation = AccessObservation {
            record_id,
            action: AccessAction::Import,
            persona: manifest.destination.persona.clone(),
            device: manifest.destination.device.clone(),
            application: context.application.clone(),
            handler: context.handler.clone(),
            at_ms: context.completed_at_ms,
            dwell_ms: None,
            referring_container_id: Some(mapping.source),
            referring_address: source_address,
            transition: AccessTransition::Imported,
            capture_source: "graphshell.transfer".to_string(),
            source_event_id: Some(manifest.transfer_id.to_string()),
            privacy: context.access_privacy,
        };
        let record = host
            .mutate_product_graph(|graph| {
                let (candidate, inserted) = record_observation(graph, key, &observation)?;
                if inserted {
                    return Ok(candidate);
                }
                access_history(graph, key)?
                    .records
                    .into_iter()
                    .find(|existing| existing.record_id == record_id)
                    .ok_or(crate::access::AccessError::UnknownNode)
            })
            .map_err(|error| TransferError::InvalidManifest(error.to_string()))?
            .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;
        save_access_record(destination_authority, &record).await?;
        destination_access_records.push(record.record_id);
    }

    host.persist(context.completed_at_ms / 1_000)
        .await
        .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;

    let selection_schema_id = save_schema(
        destination_authority,
        &manifest.selection_schema,
        PrivacyClass::PublicPortable,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Imported {
                source: manifest.source.graph.clone(),
            },
            upstream: Vec::new(),
            tooling: Some(format!("graphshell-transfer/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp(context.completed_at_ms),
        },
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        },
        Timestamp(context.completed_at_ms),
    )
    .await?;
    if selection_schema_id != manifest.selection.schema.0 {
        return Err(TransferError::InvalidManifest(
            "selection schema id changed while saving".to_string(),
        ));
    }
    bootstrap_transfer_receipt_schema(destination_authority).await?;

    let manifest_bytes = serde_json::to_vec(manifest)
        .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;
    let receipt = TransferReceiptV1 {
        transfer_id: manifest.transfer_id,
        operation: manifest.operation,
        source: manifest.source.clone(),
        destination: manifest.destination.clone(),
        route: manifest.route.clone(),
        authorization_grant: context.authorization.grant_id.clone(),
        manifest_hash: Hash::of(&manifest_bytes),
        selection_hash: manifest.selection.content_hash,
        blob_hashes: manifest
            .blobs
            .iter()
            .map(|blob| blob.content_hash)
            .collect(),
        id_map,
        nodes: target_product.graph.nodes.len() as u64,
        relations: target_product.graph.edges.len() as u64,
        destination_access_records,
        completed_at_ms: context.completed_at_ms,
        result: TransferResult::Completed,
    };
    save_transfer_receipt(destination_authority, &receipt).await?;
    Ok(receipt)
}

async fn bootstrap_transfer_receipt_schema<B: Backend>(
    store: &mut B,
) -> Result<(), eidetic::Error> {
    if eidetic::manifest::load_manifest(store, TRANSFER_RECEIPT_SCHEMA_REF.0)
        .await?
        .is_some()
    {
        return Ok(());
    }
    let id = save_schema(
        store,
        &transfer_receipt_schema(),
        PrivacyClass::PublicPortable,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some(format!("graphshell-transfer/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp::ZERO,
        },
        TrustEnvelope {
            level: TrustLevel::CheckpointAccepted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Accepted,
        },
        Timestamp::ZERO,
    )
    .await?;
    debug_assert_eq!(id, TRANSFER_RECEIPT_SCHEMA_REF.0);
    Ok(())
}

async fn save_transfer_receipt<B: Backend>(
    store: &mut B,
    receipt: &TransferReceiptV1,
) -> Result<ManifestId, eidetic::Error> {
    save_typed(
        store,
        receipt,
        Vec::<BlobSource>::new(),
        PrivacyClass::LocalOnly,
        ProvenanceRecord {
            origin: ProvenanceOrigin::Generated,
            upstream: vec![ManifestId::from_hash(receipt.selection_hash)],
            tooling: Some(format!("graphshell-transfer/{}", env!("CARGO_PKG_VERSION"))),
            generated_at: Timestamp(receipt.completed_at_ms),
        },
        TrustEnvelope {
            level: TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: ModerationState::Unreviewed,
        },
        Timestamp(receipt.completed_at_ms),
    )
    .await
}

pub(crate) fn verify_manifest(
    manifest: &TransferManifestV1,
) -> Result<ProfiledProductCodicil, TransferError> {
    if manifest.schema != TRANSFER_MANIFEST_SCHEMA {
        return Err(TransferError::InvalidManifest(format!(
            "expected {TRANSFER_MANIFEST_SCHEMA}, found {}",
            manifest.schema
        )));
    }
    manifest.selection.verify_integrity()?;
    let actual_schema_ref = schema_ref(&manifest.selection_schema);
    let schema_id = manifest.selection_schema.schema_id.as_str();
    // Version 3 carries a version-4 scene; version 2 and the engram are read
    // and their scene converted (dynamics grammar plan, F159).
    let supported_schema = schema_id == "graphshell.GraphCodicil/v3"
        || schema_id == "graphshell.GraphCodicil/v2"
        || schema_id == "graphshell.GraphEngram/v1";
    if actual_schema_ref != manifest.selection.schema || !supported_schema {
        return Err(TransferError::InvalidManifest(
            "selection schema reference does not match its definition".to_string(),
        ));
    }
    validate_payload(&manifest.selection_schema, &manifest.selection.payload)?;
    let product = decode_profiled_codicil(&manifest.selection.payload)?;
    let selected = selected_ids(&product)?;
    if manifest.operation == TransferOperation::Replicate
        && manifest.source.persona != manifest.destination.persona
    {
        return Err(TransferError::InvalidManifest(
            "replicate requires one persona on both endpoints".to_string(),
        ));
    }
    let mut seen = HashSet::new();
    for blob in &manifest.blobs {
        if !selected.contains(&blob.node_id) {
            return Err(TransferError::InvalidManifest(format!(
                "blob {} names an object outside the selection",
                blob.content_hash
            )));
        }
        if !seen.insert((blob.node_id, blob.role.as_str())) {
            return Err(TransferError::InvalidManifest(format!(
                "duplicate blob role {} for {}",
                blob.role, blob.node_id
            )));
        }
    }
    let mut access_ids = HashSet::new();
    if manifest.access_policy == AccessTransferPolicy::ExcludeSourceHistory
        && !manifest.access_records.is_empty()
    {
        return Err(TransferError::InvalidManifest(
            "source access records are present under the exclusion policy".to_string(),
        ));
    }
    for record in &manifest.access_records {
        if !selected.contains(&record.container_id) {
            return Err(TransferError::InvalidManifest(format!(
                "access record {} names an object outside the selection",
                record.record_id
            )));
        }
        if !access_ids.insert(record.record_id) {
            return Err(TransferError::InvalidManifest(format!(
                "duplicate access record id {}",
                record.record_id
            )));
        }
    }
    require_selected_content_blobs(&product.facets, &selected, &manifest.blobs)?;
    Ok(product)
}

fn selected_ids(product: &ProductCodicilV3) -> Result<HashSet<Uuid>, TransferError> {
    product
        .graph
        .nodes
        .iter()
        .map(|node| {
            Uuid::parse_str(&node.node_id).map_err(|error| {
                TransferError::InvalidManifest(format!(
                    "object id {} is not a UUID: {error}",
                    node.node_id
                ))
            })
        })
        .collect()
}

/// Lowercase hex of a SHA-256 digest.
///
/// Written out rather than `format!("{:x}", …)`: on the digest 0.11 row a
/// digest is a `hybrid_array::Array`, which does not implement `LowerHex`
/// the way `generic_array::GenericArray` did on 0.10. The test below already
/// spelled it this way after the row bump; these call sites did not compile
/// until the sha2 feature was actually turned on, so they kept the old form.
fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn verify_content_facet(
    facets: &NodeFacetStore,
    input: &TransferBlobInput,
) -> Result<(), TransferError> {
    let Some(content) = facets.get(&input.node_id, &FacetId::new(CONTENT_FACET)) else {
        return Ok(());
    };
    if let Some(expected) = content.get("byte_len").and_then(serde_json::Value::as_u64)
        && expected != input.bytes.len() as u64
    {
        return Err(TransferError::ContentMismatch {
            node_id: input.node_id,
            reason: format!(
                "declared {expected} bytes but selected {}",
                input.bytes.len()
            ),
        });
    }
    if let Some(expected) = content
        .get("media_type")
        .and_then(serde_json::Value::as_str)
        && !expected.is_empty()
        && expected != input.media_type
    {
        return Err(TransferError::ContentMismatch {
            node_id: input.node_id,
            reason: format!(
                "declared media type {expected} but selected {}",
                input.media_type
            ),
        });
    }
    if let Some(expected) = content
        .get("portable_id")
        .and_then(serde_json::Value::as_str)
    {
        let portable = expected
            .parse::<Sha256NamedInformation>()
            .map_err(|error| TransferError::ContentMismatch {
                node_id: input.node_id,
                reason: format!("portable content identity is malformed: {error}"),
            })?;
        portable
            .verify(&input.bytes)
            .map_err(|_| TransferError::ContentMismatch {
                node_id: input.node_id,
                reason: "RFC 6920 SHA-256 does not match the graph content facet".to_string(),
            })?;
    } else if let Some(expected) = content.get("sha256").and_then(serde_json::Value::as_str) {
        // Compatibility for graph content facets authored before C1 moved new
        // records to RFC 6920 names.
        let actual = hex_digest(&input.bytes);
        if !expected.eq_ignore_ascii_case(&actual) {
            return Err(TransferError::ContentMismatch {
                node_id: input.node_id,
                reason: "SHA-256 does not match the graph content facet".to_string(),
            });
        }
    }
    Ok(())
}

fn require_selected_content_blobs(
    facets: &NodeFacetStore,
    selected: &HashSet<Uuid>,
    blobs: &[TransferBlobV1],
) -> Result<(), TransferError> {
    for node_id in selected {
        if facets.get(node_id, &FacetId::new(CONTENT_FACET)).is_some()
            && !blobs.iter().any(|blob| blob.node_id == *node_id)
        {
            return Err(TransferError::ContentMismatch {
                node_id: *node_id,
                reason: "portable content metadata has no transferred blob".to_string(),
            });
        }
    }
    Ok(())
}

fn copied_product(
    source: &ProfiledProductCodicil,
    manifest: &TransferManifestV1,
) -> Result<(ProfiledProductCodicil, Vec<TransferredIdV1>), TransferError> {
    let mut donor = materialize_snapshot(&source.graph, source.placement)
        .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;
    donor.overlay_facets(source.facets.clone());
    donor
        .validate_active_resource_assertion_handles()
        .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;
    let mut copy = Graph::new();
    let mut id_map = Vec::with_capacity(source.graph.nodes.len());
    let mut id_by_source = HashMap::with_capacity(source.graph.nodes.len());

    for persisted in &source.graph.nodes {
        let source_id = Uuid::parse_str(&persisted.node_id)
            .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;
        let (_, node) = donor.get_node_by_id(source_id).ok_or_else(|| {
            TransferError::InvalidManifest(format!("source object {source_id} did not load"))
        })?;
        let destination_id = Uuid::new_v5(&manifest.transfer_id, source_id.as_bytes());
        copy.copy_node_from_with_id(
            destination_id,
            node,
            Some(manifest.source.graph.clone()),
            PortablePoint::new(0.0, 0.0),
        );
        id_by_source.insert(source_id, destination_id);
        id_map.push(TransferredIdV1 {
            source: source_id,
            destination: destination_id,
        });
    }

    let mut copied_facets = copy.facets().clone();
    for (source_id, source_facets) in source.facets.iter() {
        let Some(destination_id) = id_by_source.get(source_id).copied() else {
            continue;
        };
        for (facet, value) in source_facets.iter() {
            if copy_excludes_facet(facet.as_str()) {
                continue;
            }
            let value = remap_scene_facet(facet.as_str(), value, &id_by_source);
            copied_facets
                .set(destination_id, facet.clone(), value, &AcceptAll)
                .expect("AcceptAll cannot reject a copied facet");
        }
    }

    let mut snapshot = copy.to_snapshot();
    snapshot.edges = source
        .graph
        .edges
        .iter()
        .cloned()
        .map(|mut edge| {
            edge.from_node_id = remapped_id(&edge.from_node_id, &id_by_source)?;
            edge.to_node_id = remapped_id(&edge.to_node_id, &id_by_source)?;
            Ok(edge)
        })
        .collect::<Result<_, TransferError>>()?;
    snapshot.resources = source.graph.resources.clone();
    snapshot.resource_edges = source.graph.resource_edges.clone();
    snapshot.shown_resources = source
        .graph
        .shown_resources
        .iter()
        .cloned()
        .map(|mut shown| {
            shown.surface_id = remapped_id(&shown.surface_id, &id_by_source)?;
            Ok(shown)
        })
        .collect::<Result<_, TransferError>>()?;
    snapshot.timestamp_secs = source.graph.timestamp_secs;

    Ok((
        ProfiledProductCodicil {
            placement: source.placement,
            product: ProductCodicilV3 {
                schema: source.schema.clone(),
                scope: source.scope,
                exported_at_ms: source.exported_at_ms,
                graph: snapshot,
                facets: copied_facets,
                scene: source
                    .scene
                    .as_ref()
                    .map(|scene| remap_scene(scene, &id_by_source)),
            },
        },
        id_map,
    ))
}

fn copy_excludes_facet(facet: &str) -> bool {
    matches!(
        facet,
        ACCESS_HISTORY_FACET
            | PROVENANCE_IMPORT
            | PROVENANCE_DERIVATIONS
            | VISIT_HISTORY
            | ARRANGEMENT_PIN
            | ARRANGEMENT_FRAME_LAYOUT
            | ARRANGEMENT_SPLIT_OFFER_SUPPRESSED
    )
}

fn remapped_id(source: &str, id_by_source: &HashMap<Uuid, Uuid>) -> Result<String, TransferError> {
    let source = Uuid::parse_str(source)
        .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;
    id_by_source
        .get(&source)
        .map(Uuid::to_string)
        .ok_or_else(|| {
            TransferError::InvalidManifest(format!("relation leaves selected closure at {source}"))
        })
}

/// A spec's target item roles under a copy's new ids, as the scene's own
/// item roles are: an item the copy leaves out is dropped (dynamics grammar
/// plan, G4a; F105's item roles are keyed by node UUID).
/// A spec this reader refuses rides on unchanged, to be refused where the
/// scene opens (F114).
fn remap_dynamics(
    dynamics: &crate::product::SavedDynamics,
    remap: &impl Fn(Uuid) -> Option<Uuid>,
) -> crate::product::SavedDynamics {
    match dynamics.spec() {
        Ok(spec) => crate::product::SavedDynamics::from_spec(&remap_spec(&spec, remap)),
        Err(_) => dynamics.clone(),
    }
}

fn remap_spec(
    spec: &mere::canvas::dynamics_spec::DynamicsSpec,
    remap: &impl Fn(Uuid) -> Option<Uuid>,
) -> mere::canvas::dynamics_spec::DynamicsSpec {
    let mut spec = spec.clone();
    if let Some(target) = spec.target.as_mut() {
        target.items = std::mem::take(&mut target.items)
            .into_iter()
            .filter_map(|(id, role)| Some((remap(Uuid::parse_str(&id).ok()?)?.to_string(), role)))
            .collect();
    }
    spec
}

/// A scene's representations under a copy's new ids; a member the copy
/// leaves out is dropped.
fn remap_cartography(
    cartography: &mere::canvas::CartographyGeometry,
    remap: &impl Fn(Uuid) -> Option<Uuid>,
) -> mere::canvas::CartographyGeometry {
    mere::canvas::CartographyGeometry::from_positions(
        cartography
            .iter()
            .filter_map(|(id, position)| remap(id).map(|new_id| (new_id, position))),
    )
    .with_sizes(
        cartography
            .size_iter()
            .filter_map(|(id, size)| remap(id).map(|new_id| (new_id, size))),
    )
    .with_size_by_degree(cartography.size_by_degree())
    .with_size_by_importance(cartography.size_by_importance())
    .with_importance_metric(cartography.importance_metric())
    .with_sprites(
        cartography
            .sprite_iter()
            .filter_map(|(id, uri)| remap(id).map(|new_id| (new_id, uri.to_string()))),
    )
    .with_sprite_hulls(
        cartography
            .sprite_hull_iter()
            .filter_map(|(id, hull)| remap(id).map(|new_id| (new_id, hull))),
    )
    .with_materials(
        cartography
            .material_iter()
            .filter_map(|(id, material)| remap(id).map(|new_id| (new_id, material))),
    )
    .with_faces(
        cartography
            .face_iter()
            .filter_map(|(id, face)| remap(id).map(|new_id| (new_id, face.to_string()))),
    )
}

/// A version-4 scene under a copy's new ids: the selection, the
/// representations and the spec's target items (F159).
fn remap_scene(scene: &SavedSceneV3, ids: &HashMap<Uuid, Uuid>) -> SavedSceneV3 {
    let remap = |id: Uuid| ids.get(&id).copied();
    SavedSceneV3 {
        name: scene.name.clone(),
        selected: scene.selected.iter().filter_map(|id| remap(*id)).collect(),
        physics_paused: scene.physics_paused,
        dynamics: remap_dynamics(&scene.dynamics, &remap),
        camera_offset: scene.camera_offset,
        camera_zoom: scene.camera_zoom,
        default_handler: scene.default_handler.clone(),
        cartography: remap_cartography(&scene.cartography, &remap),
    }
}

/// A version-3 or version-2 scene facet a copied node carries, under the
/// copy's new ids, kept in its own version (F159).
fn remap_scene_v2(scene: &SavedSceneV2, ids: &HashMap<Uuid, Uuid>) -> SavedSceneV2 {
    let remap = |id: Uuid| ids.get(&id).copied();
    SavedSceneV2 {
        selected: scene.selected.iter().filter_map(|id| remap(*id)).collect(),
        arrangement_roles: scene.arrangement_roles.as_ref().map(|roles| {
            crate::product::SavedRolesV1 {
                items: roles
                    .items
                    .iter()
                    .filter_map(|(id, role)| Some((remap(*id)?, role.clone())))
                    .collect(),
                ..roles.clone()
            }
        }),
        dynamics: scene
            .dynamics
            .as_ref()
            .map(|dynamics| remap_dynamics(dynamics, &remap)),
        cartography: remap_cartography(&scene.cartography, &remap),
        ..scene.clone()
    }
}

/// A copied node's facet: a saved scene under the copy's new ids, anything
/// else as it was. A scene this reader cannot read is copied unchanged.
fn remap_scene_facet(
    facet: &str,
    value: &serde_json::Value,
    ids: &HashMap<Uuid, Uuid>,
) -> serde_json::Value {
    let remapped = if facet == SAVED_SCENE_FACET {
        serde_json::from_value::<SavedSceneV3>(value.clone())
            .ok()
            .and_then(|scene| serde_json::to_value(remap_scene(&scene, ids)).ok())
    } else if facet == SAVED_SCENE_FACET_V3 || facet == SAVED_SCENE_FACET_V2 {
        serde_json::from_value::<SavedSceneV2>(value.clone())
            .ok()
            .and_then(|scene| serde_json::to_value(remap_scene_v2(&scene, ids)).ok())
    } else {
        None
    };
    remapped.unwrap_or_else(|| value.clone())
}

fn attach_transfer_content<B: Backend>(
    host: &mut MereHost<B>,
    manifest: &TransferManifestV1,
    id_map: &[TransferredIdV1],
) -> Result<(), TransferError> {
    let destinations: HashMap<_, _> = id_map
        .iter()
        .map(|mapping| (mapping.source, mapping.destination))
        .collect();
    let mut by_node = BTreeMap::<Uuid, Vec<&TransferBlobV1>>::new();
    for blob in &manifest.blobs {
        by_node.entry(blob.node_id).or_default().push(blob);
    }
    for (source, descriptors) in by_node {
        let destination = destinations[&source];
        let key = host
            .graph()
            .get_node_key_by_id(destination)
            .ok_or_else(|| {
                TransferError::InvalidManifest(format!(
                    "destination object {destination} was not imported"
                ))
            })?;
        host.set_facet(
            key,
            TRANSFER_CONTENT_FACET,
            serde_json::to_value(descriptors)
                .map_err(|error| TransferError::InvalidManifest(error.to_string()))?,
        )
        .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;
    }
    Ok(())
}

fn attach_existing_access_record<B: Backend>(
    host: &mut MereHost<B>,
    record: &AccessRecord,
) -> Result<(), TransferError> {
    let Some(key) = host.graph().get_node_key_by_id(record.container_id) else {
        return Ok(());
    };
    let observation = AccessObservation {
        record_id: record.record_id,
        action: record.action,
        persona: record.persona.clone(),
        device: record.device.clone(),
        application: record.application.clone(),
        handler: record.handler.clone(),
        at_ms: record.at_ms,
        dwell_ms: record.dwell_ms,
        referring_container_id: record.referring_container_id,
        referring_address: record.referring_address.clone(),
        transition: record.transition,
        capture_source: record.capture_source.clone(),
        source_event_id: record.source_event_id.clone(),
        privacy: record.privacy,
    };
    let (projected, _) = host
        .mutate_product_graph(|graph| record_observation(graph, key, &observation))
        .map_err(|error| TransferError::InvalidManifest(error.to_string()))?
        .map_err(|error| TransferError::InvalidManifest(error.to_string()))?;
    if &projected != record {
        return Err(TransferError::InvalidManifest(format!(
            "access record {} does not name its selected container",
            record.record_id
        )));
    }
    Ok(())
}

fn muniment_hash(hash: Hash) -> Result<muniment::Hash, TransferError> {
    muniment::Hash::from_hex(&hash.to_hex()).ok_or_else(|| {
        TransferError::InvalidManifest(format!("{} is not a Muniment BLAKE3 hash", hash))
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use eidetic::{NoFetcher, load_typed};
    use mere::kernel::graph::{ProvenanceSubKind, RelationKind, SemanticSubKind};
    use muniment::MemoryBackend;

    use super::*;
    use crate::access::{
        AccessContext, bootstrap_access_record_schema, query_access_records, save_access_record,
    };
    use crate::mere_host::{
        FIXTURE_DEVICE_ONE_ADDRESS, FIXTURE_DEVICE_TWO_ADDRESS, FIXTURE_PERSONA_ADDRESS,
        SelectedPersonaRef, fixture_handlers,
    };
    use crate::product::{EditableRelation, LocalFileMetadata, TransferScope};

    fn selected(persona: &str) -> SelectedPersonaRef {
        SelectedPersonaRef {
            persona: persona.to_string(),
            profile: "profile:graphshell-h6".to_string(),
        }
    }

    fn endpoint(graph: &str, persona: &str, device: &str) -> TransferEndpointV1 {
        TransferEndpointV1 {
            graph: graph.to_string(),
            persona: persona.to_string(),
            device: device.to_string(),
        }
    }

    fn apply_context(revoked: bool) -> ApplyTransferContext {
        ApplyTransferContext {
            authorization: TransferAuthorization {
                grant_id: "grant:h6-transfer".to_string(),
                revoked,
            },
            application: "graphshell".to_string(),
            handler: "graphshell.transfer/v1".to_string(),
            completed_at_ms: 1_700_000_001_000,
            access_privacy: PrivacyClass::LocalOnly,
        }
    }

    struct Fixture {
        host: MereHost<MemoryBackend>,
        backend: MemoryBackend,
        url: Uuid,
        file: Uuid,
        file_bytes: Vec<u8>,
    }

    async fn source_fixture() -> Fixture {
        let backend = MemoryBackend::new();
        let mut host = MereHost::empty(
            backend.clone(),
            selected(FIXTURE_PERSONA_ADDRESS),
            fixture_handlers(),
            AccessContext {
                persona: FIXTURE_PERSONA_ADDRESS.to_string(),
                device: FIXTURE_DEVICE_ONE_ADDRESS.to_string(),
                at_ms: 1_700_000_000_000,
            },
        );
        let url = host
            .create_address("https://example.test/h6", "H6 transfer notes")
            .unwrap();
        host.edit_node(url, "H6 transfer notes", ["transport".to_string()])
            .unwrap();

        let path =
            std::env::temp_dir().join(format!("graphshell-h6-{}-reference.txt", Uuid::new_v4()));
        fs::write(&path, b"real file bytes for Graphshell H6\n").unwrap();
        let file_bytes = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        let sha256 = hex_digest(&file_bytes);
        let file = host
            .create_file_metadata(LocalFileMetadata {
                content_hash: sha256,
                name: "h6-reference.txt".to_string(),
                media_type: "text/plain".to_string(),
                byte_len: file_bytes.len() as u64,
                last_modified_ms: 1_700_000_000_000,
            })
            .unwrap();
        host.edit_node(
            file,
            "H6 real file",
            ["file".to_string(), "transport".to_string()],
        )
        .unwrap();
        host.assert_product_relation(file, url, EditableRelation::Cites)
            .unwrap();

        let key = host.graph().get_node_key_by_id(url).unwrap();
        let observation = AccessObservation {
            record_id: Uuid::new_v4(),
            action: AccessAction::Examine,
            persona: FIXTURE_PERSONA_ADDRESS.to_string(),
            device: FIXTURE_DEVICE_ONE_ADDRESS.to_string(),
            application: "graphshell".to_string(),
            handler: "system.default".to_string(),
            at_ms: 1_700_000_000_100,
            dwell_ms: Some(500),
            referring_container_id: None,
            referring_address: None,
            transition: AccessTransition::UrlTyped,
            capture_source: "graphshell.test".to_string(),
            source_event_id: Some("h6-source-access".to_string()),
            privacy: PrivacyClass::LocalOnly,
        };
        let (record, _) = host
            .mutate_product_graph(|graph| record_observation(graph, key, &observation))
            .unwrap()
            .unwrap();
        let mut authority = backend.clone();
        bootstrap_access_record_schema(&mut authority)
            .await
            .unwrap();
        save_access_record(&mut authority, &record).await.unwrap();
        host.persist(1_700_000_000).await.unwrap();

        Fixture {
            host,
            backend,
            url,
            file,
            file_bytes,
        }
    }

    async fn package(
        fixture: &Fixture,
        operation: TransferOperation,
        destination_persona: &str,
    ) -> TransferManifestV1 {
        let mut authority = fixture.backend.clone();
        let blobs = BlobStore::new(fixture.backend.clone());
        prepare_transfer(
            &fixture.host,
            &mut authority,
            &blobs,
            TransferRequest {
                transfer_id: Uuid::new_v4(),
                operation,
                source: endpoint(
                    "graph:source",
                    FIXTURE_PERSONA_ADDRESS,
                    FIXTURE_DEVICE_ONE_ADDRESS,
                ),
                destination: endpoint(
                    "graph:destination",
                    destination_persona,
                    FIXTURE_DEVICE_TWO_ADDRESS,
                ),
                route: TransferRouteV1 {
                    carrier: "local.two-store".to_string(),
                    peer: "device:destination".to_string(),
                },
                selection: ExportRequest {
                    focused: fixture.file,
                    selected: vec![fixture.file, fixture.url],
                    scope: TransferScope::SelectedSubgraph,
                    exported_at_ms: 1_700_000_000_500,
                    include_local_file_locations: true,
                    scene: None,
                },
                access_policy: AccessTransferPolicy::IncludeSourceHistory,
                privacy: PrivacyClass::TrustedPeersOnly,
            },
            vec![TransferBlobInput {
                node_id: fixture.file,
                role: "primary".to_string(),
                media_type: "text/plain".to_string(),
                bytes: fixture.file_bytes.clone(),
            }],
        )
        .await
        .unwrap()
    }

    fn exact_resource_edges(graph: &Graph) -> Vec<String> {
        let mut edges: Vec<_> = graph
            .to_snapshot()
            .resource_edges
            .iter()
            .map(|edge| serde_json::to_string(edge).unwrap())
            .collect();
        edges.sort();
        edges
    }

    fn assert_resource_cites(graph: &Graph, from: Uuid, to: Uuid) {
        let from = graph.get_node_key_by_id(from).unwrap();
        let to = graph.get_node_key_by_id(to).unwrap();
        assert!(graph.find_edge_key(from, to).is_none());
        let (handle, payload) = graph.projected_relations_between(from, to).next().unwrap();
        assert!(matches!(
            handle,
            mere::kernel::graph::RelationKey::Resource(_)
        ));
        assert!(
            payload.has_relation(mere::kernel::graph::RelationSelector::Semantic(
                SemanticSubKind::Cites
            ))
        );
        assert_eq!(payload.semantic_statements().len(), 1);
        assert!(payload.semantic_statements()[0].provenance_iri.is_some());
        assert_eq!(
            payload.semantic_statements()[0].graph_scope,
            mere::kernel::types::GraphScope::Default
        );
    }

    #[test]
    fn h6_replicate_preserves_ids_relations_tags_blobs_and_access_authority() {
        pollster::block_on(async {
            let source = source_fixture().await;
            let manifest = package(
                &source,
                TransferOperation::Replicate,
                FIXTURE_PERSONA_ADDRESS,
            )
            .await;
            assert_eq!(manifest.access_records.len(), 1);
            assert!(
                !String::from_utf8(manifest.selection.payload.clone())
                    .unwrap()
                    .contains(crate::product::LOCAL_FILE_FACET)
            );

            let destination_backend = MemoryBackend::new();
            let mut destination = MereHost::empty(
                destination_backend.clone(),
                selected(FIXTURE_PERSONA_ADDRESS),
                fixture_handlers(),
                AccessContext {
                    persona: FIXTURE_PERSONA_ADDRESS.to_string(),
                    device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                    at_ms: 1_700_000_001_000,
                },
            );
            let source_blobs = BlobStore::new(source.backend.clone());
            let destination_blobs = BlobStore::new(destination_backend.clone());
            let mut destination_authority = destination_backend.clone();
            let receipt = apply_transfer(
                &mut destination,
                &source_blobs,
                &destination_blobs,
                &mut destination_authority,
                &manifest,
                &apply_context(false),
            )
            .await
            .unwrap();

            assert_eq!(receipt.nodes, 2);
            assert_eq!(
                receipt.relations, 0,
                "the receipt counts Surface relation records"
            );
            assert_eq!(
                receipt.relations,
                verify_manifest(&manifest).unwrap().graph.edges.len() as u64
            );
            assert!(
                receipt
                    .id_map
                    .iter()
                    .all(|ids| ids.source == ids.destination)
            );
            assert!(destination.graph().get_node_by_id(source.url).is_some());
            assert!(destination.graph().get_node_by_id(source.file).is_some());
            let file_key = destination.graph().get_node_key_by_id(source.file).unwrap();
            assert!(
                destination
                    .graph()
                    .node_content_tags(file_key)
                    .unwrap()
                    .contains("file")
            );
            assert!(destination.graph().node_tags(file_key).unwrap().is_empty());
            assert_resource_cites(destination.graph(), source.file, source.url);
            assert_eq!(
                exact_resource_edges(destination.graph()),
                exact_resource_edges(source.host.graph()),
                "replicate preserves all Resource claim IDs, endpoints, asserters, scopes and metadata"
            );
            let descriptor = &manifest.blobs[0];
            let stored = destination_blobs
                .get(&muniment_hash(descriptor.content_hash).unwrap())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(stored, source.file_bytes);
            assert_eq!(
                destination
                    .facet_value(
                        &Sha256NamedInformation::of(&source.file_bytes).to_string(),
                        TRANSFER_CONTENT_FACET
                    )
                    .unwrap()[0]["content_hash"],
                serde_json::to_value(descriptor.content_hash).unwrap()
            );

            let records =
                query_access_records(&mut destination_authority, &AccessRecordFilter::default())
                    .await
                    .unwrap();
            assert_eq!(records.len(), 3, "source, URL import, and file import");
            assert_eq!(
                records
                    .iter()
                    .filter(|record| record.action == AccessAction::Import)
                    .count(),
                2
            );
            assert_eq!(
                destination
                    .access_history_for("https://example.test/h6")
                    .unwrap()
                    .records
                    .len(),
                2,
                "source examination and destination import"
            );

            let receipt_id =
                ManifestId::from_hash(Hash::of(&receipt.serialize_to_bytes().unwrap()));
            let stored_receipt = load_typed::<TransferReceiptV1>(
                &mut destination_authority,
                &mut NoFetcher,
                receipt_id,
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(stored_receipt, receipt);
        });
    }

    #[test]
    fn h6_copy_mints_ids_and_keeps_copied_from_provenance() {
        pollster::block_on(async {
            let source = source_fixture().await;
            let destination_persona = "personae://persona/bob";
            let manifest = package(&source, TransferOperation::Copy, destination_persona).await;
            let destination_backend = MemoryBackend::new();
            let mut destination = MereHost::empty(
                destination_backend.clone(),
                selected(destination_persona),
                fixture_handlers(),
                AccessContext {
                    persona: destination_persona.to_string(),
                    device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                    at_ms: 1_700_000_001_000,
                },
            );
            let mut authority = destination_backend.clone();
            let receipt = apply_transfer(
                &mut destination,
                &BlobStore::new(source.backend.clone()),
                &BlobStore::new(destination_backend.clone()),
                &mut authority,
                &manifest,
                &apply_context(false),
            )
            .await
            .unwrap();

            assert!(
                receipt
                    .id_map
                    .iter()
                    .all(|ids| ids.source != ids.destination)
            );
            for mapping in &receipt.id_map {
                let key = destination
                    .graph()
                    .get_node_key_by_id(mapping.destination)
                    .unwrap();
                let derivations = destination.graph().node_derivations(key).unwrap();
                assert_eq!(derivations.len(), 1);
                assert_eq!(derivations[0].sub_kind, ProvenanceSubKind::CopiedFrom);
                assert_eq!(derivations[0].source_node, mapping.source.to_string());
                assert_eq!(derivations[0].source_graph.as_deref(), Some("graph:source"));
            }
            assert_eq!(destination.graph().relations().count(), 0);
            assert_eq!(
                exact_resource_edges(destination.graph()),
                exact_resource_edges(source.host.graph()),
                "copy preserves Resource claim handles and changes only Surface identity"
            );
            let copied_file = receipt
                .id_map
                .iter()
                .find(|ids| ids.source == source.file)
                .unwrap()
                .destination;
            let copied_url = receipt
                .id_map
                .iter()
                .find(|ids| ids.source == source.url)
                .unwrap()
                .destination;
            assert_resource_cites(destination.graph(), copied_file, copied_url);
            let copied_key = destination.graph().get_node_key_by_id(copied_file).unwrap();
            let source_key = source.host.graph().get_node_key_by_id(source.file).unwrap();
            assert_eq!(
                destination.graph().shown_resource_id(copied_key),
                source.host.graph().shown_resource_id(source_key)
            );
            assert!(
                destination
                    .graph()
                    .node_content_tags(copied_key)
                    .unwrap()
                    .contains("transport")
            );
            assert!(
                destination
                    .graph()
                    .node_tags(copied_key)
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                destination
                    .graph()
                    .facets()
                    .get(&copied_file, &FacetId::new(TRANSFER_CONTENT_FACET))
                    .unwrap()[0]["byte_len"],
                source.file_bytes.len() as u64
            );
        });
    }

    #[test]
    fn copy_keeps_resource_identity_and_remaps_only_shown_surfaces() {
        pollster::block_on(async {
            use mere::kernel::persistence::{
                PersistedEdge, PersistedEdgeFamily, PersistedResourceRecord,
                PersistedSemanticEdgeData, PersistedSemanticStatement, PersistedShownResource,
            };
            let source = source_fixture().await;
            let manifest =
                package(&source, TransferOperation::Copy, "personae://persona/bob").await;
            let mut product = verify_manifest(&manifest).unwrap();
            let iri = "https://copy-resource.test/page";
            let resource_id = chartulary::resource_id_from_canonical_iri(iri).to_string();
            let tag = "https://copy-resource.test/vocab#Tag";
            let tag_id = chartulary::resource_id_from_canonical_iri(tag).to_string();
            product.graph.resources = [iri, tag]
                .into_iter()
                .map(|iri| PersistedResourceRecord {
                    canonical_iri: iri.into(),
                    facets: vec![],
                })
                .collect();
            product.graph.resource_edges = vec![PersistedEdge {
                from_node_id: resource_id.clone(),
                to_node_id: tag_id,
                families: vec![PersistedEdgeFamily::Semantic],
                semantic: Some(PersistedSemanticEdgeData {
                    statements: vec![PersistedSemanticStatement {
                        statement_id: "copied-tag-handle".into(),
                        predicate: mere::kernel::graph::resource::TAGGED_WITH_IRI.into(),
                        recognized_sub_kind: None,
                        label: Some("tag".into()),
                        graph_scope: mere::kernel::types::GraphScope::Default,
                        provenance_iri: Some("https://tagger.test/".into()),
                        asserted_at_ms: Some(23),
                    }],
                    ..Default::default()
                }),
                traversal: None,
                containment: None,
                arrangement: None,
                imported: None,
                provenance: None,
            }];
            product.graph.shown_resources = product
                .graph
                .nodes
                .iter()
                .map(|surface| PersistedShownResource {
                    surface_id: surface.node_id.clone(),
                    resource_id: resource_id.clone(),
                })
                .collect();
            let (copied, ids) = copied_product(&product, &manifest).unwrap();
            assert_eq!(
                product.placement,
                Some(pandect::graph_placement::PlacementProfile::RecordedStrataV1)
            );
            assert_eq!(copied.placement, product.placement);
            assert_eq!(copied.graph.resources, product.graph.resources);
            assert_eq!(copied.graph.resource_edges, product.graph.resource_edges);
            assert_eq!(
                copied.graph.resource_edges.len(),
                1,
                "the carried assertion is a positive control"
            );
            assert_eq!(
                copied.graph.shown_resources.len(),
                product.graph.shown_resources.len()
            );
            for shown in &copied.graph.shown_resources {
                assert_eq!(shown.resource_id, resource_id);
                assert!(ids.iter().any(|mapping| mapping.destination.to_string()
                    == shown.surface_id
                    && mapping.source != mapping.destination));
            }
            assert!(Graph::try_from_snapshot(&copied.graph).is_ok());
            product.graph.shown_resources[0].resource_id = Uuid::nil().to_string();
            assert!(
                matches!(
                    copied_product(&product, &manifest),
                    Err(TransferError::InvalidManifest(_))
                ),
                "invalid source is refused before copying"
            );
        });
    }

    #[test]
    fn transfer_profiles_preserve_held_claims_and_refuse_legacy_before_blob_writes() {
        use pandect::graph_placement::PlacementProfile;
        pollster::block_on(async {
            let source = source_fixture().await;
            let mut manifest = package(
                &source,
                TransferOperation::Replicate,
                FIXTURE_PERSONA_ADDRESS,
            )
            .await;
            let mut product = verify_manifest(&manifest).unwrap();
            assert_eq!(product.placement, Some(PlacementProfile::RecordedStrataV1));
            let mut held = crate::mere_host::placement_test_graph()
                .to_snapshot()
                .edges
                .remove(0);
            held.from_node_id = source.file.to_string();
            held.to_node_id = source.url.to_string();
            product.graph.edges = vec![held.clone()];
            let replace_payload =
                |manifest: &mut TransferManifestV1, product: &ProfiledProductCodicil| {
                    let old = &manifest.selection;
                    manifest.selection = Codicil::new(
                        old.schema,
                        product.serialize_to_bytes().unwrap(),
                        old.privacy,
                        old.provenance.clone(),
                        old.trust.clone(),
                        old.bounds,
                    );
                };
            replace_payload(&mut manifest, &product);
            assert_eq!(
                verify_manifest(&manifest).unwrap().graph.edges,
                vec![held.clone()]
            );
            let mut copy_manifest = manifest.clone();
            copy_manifest.operation = TransferOperation::Copy;
            copy_manifest.destination.persona = "personae://persona/bob".into();
            let (copied, mapping) = copied_product(&product, &copy_manifest).unwrap();
            assert_eq!(copied.placement, product.placement);
            let carried = &copied.graph.edges[0];
            assert_eq!(carried.semantic, held.semantic);
            assert!(
                mapping.iter().any(|id| id.source == source.file
                    && id.destination.to_string() == carried.from_node_id)
            );
            assert!(
                mapping.iter().any(|id| id.source == source.url
                    && id.destination.to_string() == carried.to_node_id)
            );
            product.placement = None;
            replace_payload(&mut manifest, &product);
            assert_eq!(verify_manifest(&manifest).unwrap().placement, None);
            assert_eq!(
                copied_product(&product, &copy_manifest)
                    .unwrap()
                    .0
                    .placement,
                None
            );

            let destination_backend = MemoryBackend::new();
            let mut destination = MereHost::empty(
                destination_backend.clone(),
                selected(FIXTURE_PERSONA_ADDRESS),
                fixture_handlers(),
                AccessContext {
                    persona: FIXTURE_PERSONA_ADDRESS.into(),
                    device: FIXTURE_DEVICE_TWO_ADDRESS.into(),
                    at_ms: 1_700_000_001_000,
                },
            );
            product.placement = Some(PlacementProfile::LegacySurfaceV1);
            replace_payload(&mut manifest, &product);
            assert!(verify_manifest(&manifest).is_err());
            let mut authority = destination_backend.clone();
            assert!(
                apply_transfer(
                    &mut destination,
                    &BlobStore::new(source.backend.clone()),
                    &BlobStore::new(destination_backend.clone()),
                    &mut authority,
                    &manifest,
                    &apply_context(false)
                )
                .await
                .is_err()
            );
            assert!(destination_backend.is_empty());
            assert_eq!(destination.graph().node_count(), 0);
            assert!(destination.graph_session().journal().entries().is_empty());
            product.placement = Some(PlacementProfile::RecordedStrataV1);
            let original_facets = product.facets.clone();
            let mut property =
                mere::kernel::types::NodeProperty::new("urn:test:predicate".into(), "exact".into())
                    .with_metadata(Some("urn:test:source".into()), Some(42));
            property.statement_id = product
                .graph
                .resource_edges
                .iter()
                .filter_map(|edge| edge.semantic.as_ref())
                .flat_map(|bucket| &bucket.statements)
                .next()
                .unwrap()
                .statement_id
                .clone();
            product
                .facets
                .set(
                    source.file,
                    FacetId::new(mere::kernel::graph::node_facets::SEMANTIC_PROPERTIES),
                    serde_json::to_value(vec![property]).unwrap(),
                    &AcceptAll,
                )
                .unwrap();
            replace_payload(&mut manifest, &product);
            assert!(verify_manifest(&manifest).is_err());
            assert!(copied_product(&product, &copy_manifest).is_err());
            assert!(
                apply_transfer(
                    &mut destination,
                    &BlobStore::new(source.backend.clone()),
                    &BlobStore::new(destination_backend.clone()),
                    &mut authority,
                    &manifest,
                    &apply_context(false)
                )
                .await
                .is_err()
            );
            assert!(
                destination_backend.is_empty(),
                "sidecar refusal precedes blob and authority writes"
            );
            assert_eq!(destination.graph().node_count(), 0);
            assert!(destination.graph_session().journal().entries().is_empty());
            product.facets = original_facets;
            let collision_backend = MemoryBackend::new();
            let mut collision_destination = MereHost::empty(
                collision_backend.clone(),
                selected(FIXTURE_PERSONA_ADDRESS),
                fixture_handlers(),
                AccessContext {
                    persona: FIXTURE_PERSONA_ADDRESS.into(),
                    device: FIXTURE_DEVICE_TWO_ADDRESS.into(),
                    at_ms: 17,
                },
            );
            let destination_id = collision_destination
                .create_address("https://destination.test/page", "existing")
                .unwrap();
            let key = collision_destination
                .graph()
                .get_node_key_by_id(destination_id)
                .unwrap();
            let mut property =
                mere::kernel::types::NodeProperty::new("urn:test:predicate".into(), "exact".into())
                    .with_metadata(Some("urn:test:source".into()), Some(42));
            property.statement_id = "destination-resource-only".into();
            assert!(
                collision_destination
                    .mutate_product_graph(|graph| {
                        graph.append_node_properties(key, vec![property.clone()])
                    })
                    .expect("valid collision fixture edit")
            );
            product
                .facets
                .set(
                    source.file,
                    FacetId::new(mere::kernel::graph::node_facets::SEMANTIC_PROPERTIES),
                    serde_json::to_value(vec![property]).unwrap(),
                    &AcceptAll,
                )
                .unwrap();
            replace_payload(&mut manifest, &product);
            assert!(
                verify_manifest(&manifest).is_ok(),
                "valid input has no internal collision"
            );
            let before = collision_destination.graph().to_snapshot();
            let facets = collision_destination.graph().facets().clone();
            let journal = collision_destination
                .graph_session()
                .journal()
                .entries()
                .len();
            assert!(
                apply_transfer(
                    &mut collision_destination,
                    &BlobStore::new(source.backend.clone()),
                    &BlobStore::new(collision_backend.clone()),
                    &mut collision_backend.clone(),
                    &manifest,
                    &apply_context(false)
                )
                .await
                .is_err()
            );
            assert!(
                collision_backend.is_empty(),
                "destination-only collision precedes blob and authority writes"
            );
            let mut after = collision_destination.graph().to_snapshot();
            after.timestamp_secs = before.timestamp_secs;
            assert_eq!(
                serde_json::to_value(after).unwrap(),
                serde_json::to_value(before).unwrap()
            );
            assert_eq!(collision_destination.graph().facets(), &facets);
            assert_eq!(
                collision_destination
                    .graph_session()
                    .journal()
                    .entries()
                    .len(),
                journal
            );
            product.facets.remove(
                &source.file,
                &FacetId::new(mere::kernel::graph::node_facets::SEMANTIC_PROPERTIES),
            );
            replace_payload(&mut manifest, &product);
            apply_transfer(
                &mut destination,
                &BlobStore::new(source.backend.clone()),
                &BlobStore::new(destination_backend.clone()),
                &mut authority,
                &manifest,
                &apply_context(false),
            )
            .await
            .unwrap();
            let restored = destination.graph().to_snapshot();
            assert!(restored.edges.contains(&held));
            assert_eq!(
                destination.snapshot_placement(),
                Some(PlacementProfile::RecordedStrataV1)
            );
        });
    }

    #[test]
    fn h6_revoked_grant_refuses_before_blob_or_graph_mutation() {
        pollster::block_on(async {
            let source = source_fixture().await;
            let manifest = package(
                &source,
                TransferOperation::Replicate,
                FIXTURE_PERSONA_ADDRESS,
            )
            .await;
            let destination_backend = MemoryBackend::new();
            let mut destination = MereHost::empty(
                destination_backend.clone(),
                selected(FIXTURE_PERSONA_ADDRESS),
                fixture_handlers(),
                AccessContext {
                    persona: FIXTURE_PERSONA_ADDRESS.to_string(),
                    device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                    at_ms: 1_700_000_001_000,
                },
            );
            let mut authority = destination_backend.clone();
            let error = apply_transfer(
                &mut destination,
                &BlobStore::new(source.backend.clone()),
                &BlobStore::new(destination_backend.clone()),
                &mut authority,
                &manifest,
                &apply_context(true),
            )
            .await
            .unwrap_err();
            assert!(matches!(error, TransferError::Revoked(_)));
            assert_eq!(destination.graph().node_count(), 0);
            assert!(destination_backend.is_empty());
        });
    }

    #[test]
    fn repeated_replication_merges_resource_truth_with_existing_surface_ids() {
        pollster::block_on(async {
            let source = source_fixture().await;
            let mut manifest = package(
                &source,
                TransferOperation::Replicate,
                FIXTURE_PERSONA_ADDRESS,
            )
            .await;
            let backend = MemoryBackend::new();
            let mut destination = MereHost::empty(
                backend.clone(),
                selected(FIXTURE_PERSONA_ADDRESS),
                fixture_handlers(),
                AccessContext {
                    persona: FIXTURE_PERSONA_ADDRESS.into(),
                    device: FIXTURE_DEVICE_TWO_ADDRESS.into(),
                    at_ms: 17,
                },
            );
            let mut authority = backend.clone();
            apply_transfer(
                &mut destination,
                &BlobStore::new(source.backend.clone()),
                &BlobStore::new(backend.clone()),
                &mut authority,
                &manifest,
                &apply_context(false),
            )
            .await
            .unwrap();
            let mut product = verify_manifest(&manifest).unwrap();
            let old_claim = product
                .graph
                .resource_edges
                .iter()
                .filter_map(|edge| edge.semantic.as_ref())
                .flat_map(|bucket| &bucket.statements)
                .next()
                .unwrap()
                .clone();
            let mut carried = old_claim.clone();
            carried.statement_id = "replicated-new-resource-handle".into();
            carried.provenance_iri = Some("urn:mere:replication-peer".into());
            carried.asserted_at_ms = Some(77);
            carried.label = Some("new carried assertion".into());
            product
                .graph
                .resource_edges
                .iter_mut()
                .find_map(|edge| edge.semantic.as_mut())
                .unwrap()
                .statements
                .push(carried.clone());
            let record = mere::kernel::persistence::PersistedResourceRecord {
                canonical_iri: "urn:mere:replicated-note".into(),
                facets: vec![mere::kernel::persistence::PersistedResourceFacet {
                    facet: "extension.origin-note".into(),
                    value_json: "{\"exact\":true}".into(),
                }],
            };
            product.graph.resources.push(record.clone());
            let replace_payload =
                |manifest: &mut TransferManifestV1, product: &ProfiledProductCodicil| {
                    let old = &manifest.selection;
                    manifest.selection = Codicil::new(
                        old.schema,
                        product.serialize_to_bytes().unwrap(),
                        old.privacy,
                        old.provenance.clone(),
                        old.trust.clone(),
                        old.bounds,
                    );
                };
            replace_payload(&mut manifest, &product);
            assert!(product.graph.nodes.iter().all(|node| {
                destination
                    .graph()
                    .get_node_by_id(Uuid::parse_str(&node.node_id).unwrap())
                    .is_some()
            }));
            let node_count = destination.graph().node_count();
            apply_transfer(
                &mut destination,
                &BlobStore::new(source.backend.clone()),
                &BlobStore::new(backend.clone()),
                &mut authority,
                &manifest,
                &apply_context(false),
            )
            .await
            .unwrap();
            let snapshot = destination.graph().to_snapshot();
            assert_eq!(destination.graph().node_count(), node_count);
            assert!(snapshot.resources.contains(&record));
            let claims: Vec<_> = snapshot
                .resource_edges
                .iter()
                .filter_map(|edge| edge.semantic.as_ref())
                .flat_map(|bucket| &bucket.statements)
                .collect();
            assert!(claims.contains(&&carried));
            assert!(claims.contains(&&old_claim));
            let facets = destination.graph().facets().clone();
            let journal = destination.graph_session().journal().entries().len();
            let records = query_access_records(&mut authority, &AccessRecordFilter::default())
                .await
                .unwrap()
                .len();
            apply_transfer(
                &mut destination,
                &BlobStore::new(source.backend.clone()),
                &BlobStore::new(backend.clone()),
                &mut authority,
                &manifest,
                &apply_context(false),
            )
            .await
            .unwrap();
            let mut repeated = destination.graph().to_snapshot();
            repeated.timestamp_secs = snapshot.timestamp_secs;
            assert_eq!(
                serde_json::to_value(repeated).unwrap(),
                serde_json::to_value(&snapshot).unwrap()
            );
            assert_eq!(destination.graph().facets(), &facets);
            assert_eq!(
                destination.graph_session().journal().entries().len(),
                journal
            );
            assert_eq!(
                query_access_records(&mut authority, &AccessRecordFilter::default())
                    .await
                    .unwrap()
                    .len(),
                records
            );

            // Input is valid alone; the reused handle conflicts only with destination truth.
            let bucket = product
                .graph
                .resource_edges
                .iter_mut()
                .find_map(|edge| edge.semantic.as_mut())
                .unwrap();
            bucket
                .statements
                .iter_mut()
                .find(|claim| claim.statement_id == carried.statement_id)
                .unwrap()
                .provenance_iri = Some("urn:mere:conflicting-peer".into());
            replace_payload(&mut manifest, &product);
            assert!(verify_manifest(&manifest).is_ok());
            let refused_backend = MemoryBackend::new();
            let mut refused = MereHost::empty(
                refused_backend.clone(),
                selected(FIXTURE_PERSONA_ADDRESS),
                fixture_handlers(),
                AccessContext {
                    persona: FIXTURE_PERSONA_ADDRESS.into(),
                    device: FIXTURE_DEVICE_TWO_ADDRESS.into(),
                    at_ms: 17,
                },
            );
            refused
                .begin_profiled_session(
                    destination.graph().clone(),
                    Some(pandect::graph_placement::PlacementProfile::RecordedStrataV1),
                )
                .unwrap();
            let before = refused.graph().to_snapshot();
            let before_facets = refused.graph().facets().clone();
            let before_journal = refused.graph_session().journal().entries().len();
            assert!(
                apply_transfer(
                    &mut refused,
                    &BlobStore::new(source.backend.clone()),
                    &BlobStore::new(refused_backend.clone()),
                    &mut refused_backend.clone(),
                    &manifest,
                    &apply_context(false)
                )
                .await
                .is_err()
            );
            assert!(
                refused_backend.is_empty(),
                "existing Surface IDs cannot bypass preflight before blob writes"
            );
            let mut after = refused.graph().to_snapshot();
            after.timestamp_secs = before.timestamp_secs;
            assert_eq!(
                serde_json::to_value(after).unwrap(),
                serde_json::to_value(before).unwrap()
            );
            assert_eq!(refused.graph().facets(), &before_facets);
            assert_eq!(
                refused.graph_session().journal().entries().len(),
                before_journal
            );
        });
    }

    #[test]
    fn h6_retry_reuses_verified_blobs_and_deterministic_copy_ids() {
        pollster::block_on(async {
            let source = source_fixture().await;
            let manifest =
                package(&source, TransferOperation::Copy, "personae://persona/bob").await;
            let destination_backend = MemoryBackend::new();
            let mut destination = MereHost::empty(
                destination_backend.clone(),
                selected("personae://persona/bob"),
                fixture_handlers(),
                AccessContext {
                    persona: "personae://persona/bob".to_string(),
                    device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                    at_ms: 1_700_000_001_000,
                },
            );
            let source_blobs = BlobStore::new(source.backend.clone());
            let destination_blobs = BlobStore::new(destination_backend.clone());
            let mut authority = destination_backend.clone();
            let first = apply_transfer(
                &mut destination,
                &source_blobs,
                &destination_blobs,
                &mut authority,
                &manifest,
                &apply_context(false),
            )
            .await
            .unwrap();
            let node_count = destination.graph().node_count();
            let relation_count = destination.graph().relations().count();
            let record_count = query_access_records(&mut authority, &AccessRecordFilter::default())
                .await
                .unwrap()
                .len();
            let mut retry_context = apply_context(false);
            retry_context.completed_at_ms += 9_000;
            let second = apply_transfer(
                &mut destination,
                &source_blobs,
                &destination_blobs,
                &mut authority,
                &manifest,
                &retry_context,
            )
            .await
            .unwrap();
            assert_eq!(second.id_map, first.id_map);
            assert_eq!(destination.graph().node_count(), node_count);
            assert_eq!(destination.graph().relations().count(), relation_count);
            assert_eq!(
                query_access_records(&mut authority, &AccessRecordFilter::default())
                    .await
                    .unwrap()
                    .len(),
                record_count,
                "retry reuses the original destination access records"
            );
        });
    }

    #[test]
    fn h6_resume_uses_an_already_verified_destination_blob() {
        pollster::block_on(async {
            let source = source_fixture().await;
            let manifest = package(
                &source,
                TransferOperation::Replicate,
                FIXTURE_PERSONA_ADDRESS,
            )
            .await;
            let destination_backend = MemoryBackend::new();
            let mut destination = MereHost::empty(
                destination_backend.clone(),
                selected(FIXTURE_PERSONA_ADDRESS),
                fixture_handlers(),
                AccessContext {
                    persona: FIXTURE_PERSONA_ADDRESS.to_string(),
                    device: FIXTURE_DEVICE_TWO_ADDRESS.to_string(),
                    at_ms: 1_700_000_001_000,
                },
            );
            let source_blobs = BlobStore::new(source.backend.clone());
            let destination_blobs = BlobStore::new(destination_backend.clone());
            let descriptor = &manifest.blobs[0];
            let hash = muniment_hash(descriptor.content_hash).unwrap();

            destination_blobs.put(&source.file_bytes).await.unwrap();
            source_blobs
                .backend()
                .delete(&format!("blob/{}", hash.to_hex()))
                .await
                .unwrap();
            assert!(source_blobs.get(&hash).await.unwrap().is_none());

            let mut authority = destination_backend.clone();
            let receipt = apply_transfer(
                &mut destination,
                &source_blobs,
                &destination_blobs,
                &mut authority,
                &manifest,
                &apply_context(false),
            )
            .await
            .unwrap();
            assert_eq!(receipt.result, TransferResult::Completed);
            assert_eq!(
                destination_blobs.get(&hash).await.unwrap(),
                Some(source.file_bytes)
            );
        });
    }

    #[cfg(all(feature = "personal-sync", not(target_arch = "wasm32")))]
    #[test]
    fn an_offer_summarizes_the_manifest_it_names() {
        use crate::personal_sync::PersonalGraphEvent;

        pollster::block_on(async {
            let source = source_fixture().await;
            let manifest = package(&source, TransferOperation::Copy, FIXTURE_PERSONA_ADDRESS).await;
            let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
            let offer = crate::transfer_offer::offer_for(
                &manifest,
                Hash::of(&manifest_bytes),
                manifest_bytes.len() as u64,
                "pairing-1",
                1_700_000_002_000,
            )
            .unwrap();

            // The counts are advisory, so they are worth pinning against what
            // applying the same manifest produces. The protocol counts Surface
            // records; Resource content remains in the immutable selection.
            assert_eq!(offer.nodes, 2);
            let product = verify_manifest(&manifest).unwrap();
            assert_eq!(offer.relations, 0);
            assert_eq!(offer.relations, product.graph.edges.len() as u64);
            assert!(!product.graph.resource_edges.is_empty());
            assert!(product.graph.resource_edges.iter().any(|edge| {
                edge.semantic.as_ref().is_some_and(|bucket| {
                    bucket.statements.iter().any(|statement| {
                        statement.recognized_sub_kind
                            == Some(mere::kernel::persistence::PersistedSemanticSubKind::Cites)
                    })
                })
            }));
            assert_eq!(offer.blobs, manifest.blobs.len() as u64);
            assert_eq!(offer.blob_bytes, source.file_bytes.len() as u64);
            assert_eq!(offer.transfer_id, manifest.transfer_id);
            assert_eq!(offer.destination.device, FIXTURE_DEVICE_TWO_ADDRESS);

            let events = crate::transfer_offer::offer_events(&offer).unwrap();
            assert!(matches!(
                &events[0],
                PersonalGraphEvent::AddNode { id, address, .. }
                    if *id == manifest.transfer_id && address.contains("phone/laptop")
            ));
        });
    }

    /// G4a: a copy carries a scene's dynamics spec, its target's item roles
    /// under the copy's new ids, as the scene's own item roles are (F105).
    #[test]
    fn a_copy_remaps_the_spec_targets_item_roles() {
        use mere::canvas::Role;
        use mere::canvas::dynamics_spec::{DynamicsSpec, Node, Target};
        let (kept, left, minted) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let mut spec = DynamicsSpec::new(Node::preset("spring.rapier"));
        spec.target = Some(Target {
            arrangement: "grid.default".into(),
            anchored_pull: 12.0,
            default_role: Role::Seeded,
            groups: None,
            items: [(kept, Role::Pinned), (left, Role::Anchored)]
                .into_iter()
                .map(|(id, role)| (id.to_string(), role))
                .collect(),
        });
        let ids = HashMap::from([(kept, minted)]);
        let copied = remap_spec(&spec, &|id| ids.get(&id).copied());
        let items = &copied.target.as_ref().unwrap().items;
        assert_eq!(items.len(), 1, "an item the copy leaves out is dropped");
        assert_eq!(items.get(&minted.to_string()), Some(&Role::Pinned));
        assert_eq!(copied.root, spec.root, "the rest is carried as it is");
    }

    /// F159: a copied node's saved scene, version 4 or earlier, has its
    /// selection, representations and item roles under the copy's new ids;
    /// any other facet is copied as it was.
    #[test]
    fn a_copied_scene_facet_is_remapped() {
        use mere::canvas::Role;
        use mere::canvas::dynamics_spec::{DynamicsSpec, Node, Target};
        let (kept, minted) = (Uuid::from_u128(1), Uuid::from_u128(3));
        let ids = HashMap::from([(kept, minted)]);
        let mut spec = DynamicsSpec::new(Node::preset("spring.rapier"));
        spec.target = Some(Target {
            arrangement: "grid.default".into(),
            anchored_pull: 12.0,
            default_role: Role::Seeded,
            groups: None,
            items: [(kept.to_string(), Role::Pinned)].into_iter().collect(),
        });
        let v4 = SavedSceneV3 {
            name: "copied".into(),
            selected: vec![kept],
            physics_paused: false,
            dynamics: crate::product::SavedDynamics::from_spec(&spec),
            camera_offset: (0.0, 0.0),
            camera_zoom: 1.0,
            default_handler: "system.default".into(),
            cartography: mere::canvas::CartographyGeometry::from_positions([(kept, (1.0, 2.0))]),
        };
        let value = serde_json::to_value(&v4).unwrap();
        let copied: SavedSceneV3 =
            serde_json::from_value(remap_scene_facet(SAVED_SCENE_FACET, &value, &ids)).unwrap();
        assert_eq!(copied.selected, vec![minted]);
        assert_eq!(
            copied
                .cartography
                .iter()
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            vec![minted]
        );
        assert_eq!(
            copied
                .dynamics
                .spec()
                .unwrap()
                .target
                .unwrap()
                .items
                .keys()
                .collect::<Vec<_>>(),
            vec![&minted.to_string()]
        );
        let mut v3 = serde_json::json!({
            "name": "flat",
            "selected": [kept],
            "layout_strategy": "grid.default",
            "physics_paused": true,
            "physics_damping": 0.7,
            "arrangement_pull": 0.4,
            "arrangement_roles": { "default": "seeded", "items": { kept.to_string(): "pinned" } },
            "camera_offset": [0.0, 0.0],
            "camera_zoom": 1.0,
            "default_handler": "system.default",
            "cartography": mere::canvas::CartographyGeometry::default(),
        });
        let copied: SavedSceneV2 =
            serde_json::from_value(remap_scene_facet(SAVED_SCENE_FACET_V3, &v3, &ids)).unwrap();
        assert_eq!(copied.selected, vec![minted]);
        assert_eq!(
            copied
                .arrangement_roles
                .unwrap()
                .items
                .keys()
                .collect::<Vec<_>>(),
            vec![&minted]
        );
        v3["selected"] = serde_json::json!("not a scene");
        assert_eq!(
            remap_scene_facet(SAVED_SCENE_FACET_V3, &v3, &ids),
            v3,
            "a scene this reader cannot read is copied unchanged"
        );
        let other = serde_json::json!({ "selected": [kept] });
        assert_eq!(remap_scene_facet("example.notes/v1", &other, &ids), other);
    }
}
