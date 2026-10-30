// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Worker {
    pub(crate) fn read(
        &self,
        root: PathBuf,
        id: eidetic::ManifestId,
        policy: crate::retained_clip::ClipLimits,
        sealer: Arc<dyn eidetic::PayloadSealer + Send + Sync>,
        limits: StorageLimits,
        budget: crate::research_storage::SharedReadBudget,
        queue_limits: QueueLimits,
    ) -> Result<Pending<Result<crate::retained_clip::ClipView, SourceReadError>>, &'static str>
    {
        // The retained result consists of at most the selected body and one
        // bounded serialized record. The shared ledger travels unchanged into
        // the operation; owner ceilings are separately fixed on the owner.
        // This is payload accounting, not allocator or process memory.
        let output = usize::try_from(policy.max_bytes)
            .ok()
            .and_then(|n| n.checked_add(policy.max_record_bytes))
            .and_then(|n| n.checked_add(root.as_os_str().len()))
            .ok_or("Retained clip queue limit exceeded")?;
        self.submit(queue_limits, output, move |owner, _cancelled| {
            let store = owner.store(&root, false, &limits)?;
            let mut bounded = crate::research_storage::BudgetStore { store, budget };
            crate::retained_clip::read_store(&mut bounded, Some(&*sealer), id, policy)
        })
    }
}

pub(crate) enum OwnedNative {
    Knot {
        export: serde_json::Value,
        backing: Vec<u8>,
    },
    Redshank {
        export: serde_json::Value,
        voice_body: Option<Vec<u8>>,
    },
}
impl OwnedNative {
    fn borrowed(&self) -> crate::retained_clip::NativeClip<'_> {
        match self {
            Self::Knot { export, backing } => {
                crate::retained_clip::NativeClip::Knot { export, backing }
            },
            Self::Redshank { export, voice_body } => crate::retained_clip::NativeClip::Redshank {
                export,
                voice_body: voice_body.as_deref(),
            },
        }
    }
    fn payload_bytes(&self, maximum: usize) -> Result<usize, &'static str> {
        let (export, body) = match self {
            Self::Knot { export, backing } => (export, backing.len()),
            Self::Redshank { export, voice_body } => {
                (export, voice_body.as_ref().map_or(0, Vec::len))
            },
        };
        struct Count {
            count: usize,
            max: usize,
        }
        impl std::io::Write for Count {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.count = self
                    .count
                    .checked_add(bytes.len())
                    .filter(|n| *n <= self.max)
                    .ok_or_else(|| std::io::Error::other("Retained clip queue limit exceeded"))?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut counter = Count {
            count: body,
            max: maximum,
        };
        if body > maximum {
            return Err("Retained clip queue limit exceeded");
        }
        serde_json::to_writer(&mut counter, export)
            .map_err(|_| "Retained clip queue limit exceeded")?;
        Ok(counter.count)
    }
}

// Reserve the selected body and retained record that can coexist with the
// queued input or serialized export. Internal storage/crypto allocations
// remain governed by their existing limits, not this payload accounting.
fn decoded_payload_limit(policy: crate::retained_clip::ClipLimits) -> Option<usize> {
    usize::try_from(policy.max_bytes)
        .ok()?
        .checked_add(policy.max_record_bytes)
}

impl Worker {
    pub(crate) fn retain(
        &self,
        root: PathBuf,
        source: OwnedNative,
        context: crate::retained_clip::CaptureContext,
        policy: crate::retained_clip::ClipLimits,
        access: crate::research::ClipAccess,
        epoch: u64,
        limits: StorageLimits,
    ) -> Result<Pending<Result<eidetic::ManifestId, String>>, &'static str> {
        let queue = (&limits.clip_queue).into();
        let payload = source
            .payload_bytes(limits.clip_queue.max_bytes)?
            .checked_add(root.as_os_str().len())
            .and_then(|n| n.checked_add(context.source.entity.len()))
            .and_then(|n| n.checked_add(context.capture_id.len()))
            .and_then(|n| decoded_payload_limit(policy).and_then(|decoded| n.checked_add(decoded)))
            .and_then(|n| n.checked_add(32))
            .ok_or("Retained clip queue limit exceeded")?;
        self.submit(queue, payload, move |owner, cancelled| {
            if cancelled.load(Ordering::SeqCst) || (access.epoch)() != Some(epoch) {
                return Err("Retained clip request is no longer current".into());
            }
            let source = source.borrowed();
            let (id, retained) =
                crate::retained_clip::stage_native(&source, &context, policy, &*access.sealer)?;
            if cancelled.load(Ordering::SeqCst) || (access.epoch)() != Some(epoch) {
                return Err("Retained clip request is no longer current".into());
            }
            let store = owner
                .store(&root, true, &limits)
                .map_err(|e| e.to_string())?;
            if cancelled.load(Ordering::SeqCst) || (access.epoch)() != Some(epoch) {
                return Err("Retained clip request is no longer current".into());
            }
            let id = crate::retained_clip::retain_staged(
                store,
                &source,
                &context,
                policy,
                &*access.sealer,
                &limits,
                id,
                retained,
            )?;
            store
                .sync_all()
                .map_err(|error| format!("Retained clip sync failed: {error}"))?;
            if (access.epoch)() != Some(epoch) {
                return Err("Profile custody changed during retention".into());
            }
            Ok(id)
        })
    }
}

impl Worker {
    pub(crate) fn import(
        &self,
        root: PathBuf,
        file: PathBuf,
        policy: crate::retained_clip::ClipLimits,
        now_ms: u64,
        access: crate::research::ClipAccess,
        epoch: u64,
        limits: StorageLimits,
    ) -> Result<Pending<Result<eidetic::ManifestId, String>>, &'static str> {
        let wire = usize::try_from(policy.max_bytes)
            .ok()
            .and_then(|n| n.checked_mul(16))
            .and_then(|n| n.checked_add(policy.max_record_bytes))
            .ok_or("Retained clip queue limit exceeded")?;
        let payload = wire
            .checked_add(decoded_payload_limit(policy).ok_or("Retained clip queue limit exceeded")?)
            .and_then(|n| n.checked_add(root.as_os_str().len()))
            .and_then(|n| n.checked_add(file.as_os_str().len()))
            .and_then(|n| n.checked_add(32))
            .ok_or("Retained clip queue limit exceeded")?;
        let queue = (&limits.clip_queue).into();
        self.submit(queue, payload, move |owner, cancelled| {
            if cancelled.load(Ordering::SeqCst) || (access.epoch)() != Some(epoch) {
                return Err("Clip import is no longer current".into());
            }
            let (package, context, id) =
                crate::retained_clip::stage_import(Some(&*access.sealer), &file, policy, now_ms)?;
            if cancelled.load(Ordering::SeqCst) || (access.epoch)() != Some(epoch) {
                return Err("Clip import is no longer current".into());
            }
            let store = owner
                .store(&root, true, &limits)
                .map_err(|e| e.to_string())?;
            if cancelled.load(Ordering::SeqCst) || (access.epoch)() != Some(epoch) {
                return Err("Clip import is no longer current".into());
            }
            let received = crate::retained_clip::import_staged(
                store,
                Some(&*access.sealer),
                package,
                context,
                policy,
                &limits,
                id,
            )?;
            store
                .sync_all()
                .map_err(|error| format!("Retained clip sync failed: {error}"))?;
            if (access.epoch)() != Some(epoch) {
                return Err("Profile custody changed during import".into());
            }
            Ok(received)
        })
    }
}

pub(crate) struct PreparedExport {
    bound: Arc<crate::custody_binding::BoundSealer>,
    epoch: u64,
    clip: eidetic::ManifestId,
    root: PathBuf,
    file: PathBuf,
    replace: bool,
    bytes: Vec<u8>,
}
impl PreparedExport {
    pub(crate) fn matches(
        &self,
        bound: &Arc<crate::custody_binding::BoundSealer>,
        epoch: u64,
        clip: eidetic::ManifestId,
        root: &Path,
        file: &Path,
        replace: bool,
    ) -> bool {
        Arc::ptr_eq(&self.bound, bound)
            && self.epoch == epoch
            && self.clip == clip
            && self.root == root
            && self.file == file
            && self.replace == replace
    }
}
impl Worker {
    pub(crate) fn prepare_export(
        &self,
        root: PathBuf,
        file: PathBuf,
        replace: bool,
        id: eidetic::ManifestId,
        policy: crate::retained_clip::ClipLimits,
        bound: Arc<crate::custody_binding::BoundSealer>,
        epoch: u64,
        limits: StorageLimits,
    ) -> Result<Pending<Result<PreparedExport, String>>, &'static str> {
        let wire = crate::retained_clip::export::wire_limit(policy)
            .map_err(|_| "Clip wire budget overflow")?;
        let reserved = wire
            .checked_add(decoded_payload_limit(policy).ok_or("Retained clip queue limit exceeded")?)
            .and_then(|n| n.checked_add(root.as_os_str().len()))
            .and_then(|n| n.checked_add(file.as_os_str().len()))
            .ok_or("Retained clip queue limit exceeded")?;
        let queue = (&limits.clip_queue).into();
        self.submit(queue, reserved, move |owner, cancelled| {
            if cancelled.load(Ordering::SeqCst) || bound.epoch() != Some(epoch) {
                return Err("Clip export is no longer current".into());
            }
            let store = owner
                .store(&root, false, &limits)
                .map_err(|e| e.to_string())?;
            let bytes =
                crate::retained_clip::export::prepare(store, Some(&*bound), id, policy, &limits)?;
            if cancelled.load(Ordering::SeqCst) || bound.epoch() != Some(epoch) {
                return Err("Profile custody changed while preparing the export".into());
            }
            Ok(PreparedExport {
                bound,
                epoch,
                clip: id,
                root,
                file,
                replace,
                bytes,
            })
        })
    }
    /// Only a claim for the exact prepared binding can publish this exact path.
    pub(crate) fn commit_export(
        &self,
        claim: crate::custody_binding::PublicationClaim,
        prepared: Response<Result<PreparedExport, String>>,
    ) -> Result<Pending<Result<(), String>>, String> {
        let Response {
            value,
            _reservation: reservation,
        } = prepared;
        let prepared = value?;
        if !Arc::ptr_eq(&reservation.shared, &self.shared) {
            return Err("Clip export reservation belongs to another owner".into());
        }
        if !claim.belongs_to(&prepared.bound) || claim.epoch() != prepared.epoch {
            return Err("Clip export claim does not match the prepared binding".into());
        }
        self.submit_reserved(reservation, move |_, _| {
            let _claim = claim;
            crate::retained_clip::export::publish(&prepared.bytes, &prepared.file, prepared.replace)
        })
        .map_err(str::to_string)
    }
}
