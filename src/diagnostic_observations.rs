// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A redacted diagnostic copy at the existing app-event fanout. This neither
//! replaces Trail nor provides operation correlation absent from AppEvent.

use crate::observe::AppEvent;
use apparatus::{
    Batch, Cursor, Inspection, InspectionLimits, ObservationMetadata, ObservationStore,
    RetentionLimits, RunId, SourceId, StoreStats, inspect_batch,
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct EventCategory {
    /// Fixed vocabulary only; never format or clone the event's fields.
    pub kind: &'static str,
}

fn category(event: &AppEvent) -> EventCategory {
    use AppEvent::*;
    let kind = match event {
        AddressOpened(_) => "address-opened",
        ContentNavigated { .. } => "content-navigated",
        ContentTitleChanged { .. } => "content-title-changed",
        PermissionRequested { .. } => "permission-requested",
        PermissionAnswered { .. } => "permission-answered",
        AuthenticationRequested { .. } => "authentication-requested",
        AuthenticationAnswered { .. } => "authentication-answered",
        SmolwebInputRequested { .. } => "smolweb-input-requested",
        SmolwebInputSubmitted { .. } => "smolweb-input-submitted",
        SmolwebSubmissionStarted { .. } => "submission-started",
        SmolwebSubmissionSucceeded { .. } => "submission-succeeded",
        SmolwebSubmissionFailed { .. } => "submission-failed",
        DownloadStarted { .. } => "download-started",
        DownloadCompleted { .. } => "download-completed",
        DownloadFailed { .. } => "download-failed",
        SourceDocumentCaptured { .. } => "source-document-captured",
        SourceDocumentCaptureFailed { .. } => "source-document-capture-failed",
        SourceDocumentCaptureUnattached { .. } => "source-document-capture-unattached",
        FeedRefreshed { .. } => "feed-refreshed",
        FeedRefreshFailed { .. } => "feed-refresh-failed",
        WindowOpened => "window-opened",
        WindowClosed => "window-closed",
        // Explicit partial classification: every remaining event is counted,
        // but carries neither raw descriptions nor inferred operation meaning.
        _ => "other-app-event",
    };
    EventCategory { kind }
}

/// Count, accounted encoded bytes and age can be configured for each run.
/// Any zero disables retention. Invalid values fail the requested receipt.
pub fn limits_from(
    mut lookup: impl FnMut(&str) -> Option<String>,
) -> Result<RetentionLimits, &'static str> {
    fn value(
        lookup: &mut impl FnMut(&str) -> Option<String>,
        key: &str,
        fallback: usize,
    ) -> Result<usize, &'static str> {
        lookup(key).map_or(Ok(fallback), |text| {
            text.parse()
                .map_err(|_| "invalid diagnostic retention setting")
        })
    }
    Ok(RetentionLimits {
        max_records: value(&mut lookup, "TURNSTONE_DIAGNOSTIC_RECORDS", 128)?,
        max_bytes: value(&mut lookup, "TURNSTONE_DIAGNOSTIC_BYTES", 65_536)?,
        max_age: Duration::from_secs(
            value(&mut lookup, "TURNSTONE_DIAGNOSTIC_AGE_SECS", 300)? as u64
        ),
    })
}

pub struct DiagnosticObservations {
    store: ObservationStore<EventCategory>,
    started: Instant,
    failure: Option<&'static str>,
    inspection_limits: Result<InspectionLimits, &'static str>,
}

pub fn inspection_limits_from(
    mut lookup: impl FnMut(&str) -> Option<String>,
) -> Result<InspectionLimits, &'static str> {
    fn value(
        lookup: &mut impl FnMut(&str) -> Option<String>,
        key: &str,
        fallback: usize,
    ) -> Result<usize, &'static str> {
        lookup(key).map_or(Ok(fallback), |text| {
            text.parse()
                .map_err(|_| "invalid diagnostic inspection setting")
        })
    }
    Ok(InspectionLimits {
        max_records: value(&mut lookup, "TURNSTONE_DIAGNOSTIC_VIEW_RECORDS", 16)?,
        max_gaps: value(&mut lookup, "TURNSTONE_DIAGNOSTIC_VIEW_GAPS", 4)?,
        max_text_chars: value(&mut lookup, "TURNSTONE_DIAGNOSTIC_VIEW_CHARS", 256)?,
    })
}

impl DiagnosticObservations {
    pub fn new(run: RunId, limits: RetentionLimits) -> Self {
        Self {
            store: ObservationStore::new(run, SourceId::from("turnstone.app-event-fanout"), limits),
            started: Instant::now(),
            failure: None,
            inspection_limits: inspection_limits_from(|_| None),
        }
    }

    pub fn from_env() -> Self {
        let limits = limits_from(|key| std::env::var(key).ok());
        let mut observations = Self::new(
            RunId(uuid::Uuid::new_v4().to_string()),
            limits.unwrap_or(RetentionLimits {
                max_records: 0,
                max_bytes: 0,
                max_age: Duration::ZERO,
            }),
        );
        observations.failure = limits.err();
        observations.inspection_limits = inspection_limits_from(|key| std::env::var(key).ok());
        observations
    }

    /// Independent read for the optional Gloss section. It neither consumes the
    /// receipt cursor nor formats raw AppEvent fields, and enforces age on reads.
    pub fn inspection(&mut self) -> Result<Inspection, String> {
        if let Some(failure) = self.failure {
            return Err(failure.to_owned());
        }
        let display = self.inspection_limits.map_err(str::to_owned)?;
        let retention = self.store.limits();
        let mut cursor = self.cursor();
        let batch = self
            .read(&mut cursor, retention.max_records.max(1))
            .map_err(|_| "diagnostic inspection read failed".to_owned())?;
        Ok(inspect_batch(&batch, retention, display, |payload| {
            payload.kind
        }))
    }

    pub fn record(&mut self, event: &AppEvent) {
        let payload = category(event);
        // This small static category has a bounded JSON encoding. Its encoded
        // length is the actual serializer result, not raw event size or a guess.
        let encoded_bytes = serde_json::to_vec(&payload)
            .expect("static category serializes")
            .len();
        if self
            .store
            .record(
                payload,
                encoded_bytes,
                ObservationMetadata::default(),
                self.started.elapsed(),
            )
            .is_err()
        {
            self.failure = Some("diagnostic observation admission failed");
        }
    }

    /// UI consumers receive independent cursors; automation keeps its own stream.
    pub fn cursor(&self) -> Cursor {
        self.store.cursor()
    }
    pub fn stats(&mut self) -> Result<StoreStats, apparatus::StoreError> {
        self.store.expire(self.started.elapsed())
    }
    pub fn read(
        &mut self,
        cursor: &mut Cursor,
        max_records: usize,
    ) -> Result<Batch<EventCategory>, apparatus::StoreError> {
        self.store.read(cursor, self.started.elapsed(), max_records)
    }

    fn export(&mut self, directory: &Path) -> Result<(), String> {
        if let Some(failure) = self.failure {
            return Err(failure.to_owned());
        }
        let mut cursor = self.cursor();
        // One bounded batch includes all retained records and any earlier loss.
        let batch = self
            .read(&mut cursor, self.store.limits().max_records.max(1))
            .map_err(|_| "diagnostic observation read failed")?;
        let bytes = serde_json::to_vec_pretty(&batch)
            .map_err(|_| "diagnostic export serialization failed")?;
        std::fs::write(directory.join("diagnostics.json"), bytes)
            .map_err(|_| "diagnostic export write failed".to_owned())
    }

    /// Called only for an explicitly requested scenario receipt. An export
    /// failure changes the sentinel result; no ordinary run creates a file.
    pub fn write_scenario_receipt(
        &mut self,
        directory: &Path,
        outcome: &taproot::Outcome,
    ) -> std::io::Result<()> {
        std::fs::create_dir_all(directory)?;
        let export = self.export(directory);
        let ok = outcome.ok && export.is_ok();
        let mut body = format!("RESULT {}\n", if ok { "ok" } else { "fail" });
        for line in &outcome.log {
            body.push_str(line);
            body.push('\n');
        }
        if let Err(error) = export {
            body.push_str("FAIL: ");
            body.push_str(&error);
            body.push('\n');
        }
        std::fs::write(directory.join("scenario.done"), body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspection_keeps_loss_redaction_and_receipt_reader_independent() {
        let mut observed = DiagnosticObservations::new(RunId::from("inspection"), limits(1));
        let mut receipt = observed.cursor();
        observed.record(&AppEvent::AddressOpened(
            "https://secret.test/?token=secret".into(),
        ));
        observed.record(&AppEvent::WindowClosed);
        let inspection = observed.inspection().unwrap();
        let texts = inspection
            .lines
            .iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(texts.contains("Loss evicted: 1"));
        assert!(texts.contains("Unavailable records: [1, 2)"));
        assert!(texts.contains("#2: window-closed"));
        assert!(texts.contains("Operation: unavailable"));
        assert!(!texts.contains("secret"));
        let batch = observed.read(&mut receipt, 1).unwrap();
        assert_eq!(batch.records[0].envelope.reference.sequence, 2);
        assert_eq!(batch.gaps[0].first_sequence, 1);
    }
    #[test]
    fn inspection_age_and_disabled_status_are_actual_store_readings() {
        let mut expired = DiagnosticObservations::new(
            RunId::from("age"),
            RetentionLimits {
                max_age: Duration::from_secs(1),
                ..limits(1)
            },
        );
        expired.record(&AppEvent::WindowOpened);
        expired.started = Instant::now() - Duration::from_secs(2);
        let inspection = expired.inspection().unwrap();
        assert!(
            inspection
                .lines
                .iter()
                .any(|row| row.text == "Loss expired: 1")
        );
        assert!(
            inspection
                .lines
                .iter()
                .any(|row| row.text == "Retained: 0 records, 0 accounted bytes")
        );
        let mut disabled = DiagnosticObservations::new(RunId::from("disabled-view"), limits(0));
        disabled.record(&AppEvent::WindowOpened);
        assert!(
            disabled
                .inspection()
                .unwrap()
                .lines
                .iter()
                .any(|row| row.text == "Retention disabled")
        );
    }
    #[test]
    fn inspection_settings_are_independent_of_retention() {
        let display = inspection_limits_from(|key| {
            (key == "TURNSTONE_DIAGNOSTIC_VIEW_RECORDS").then(|| "0".into())
        })
        .unwrap();
        assert_eq!(display.max_records, 0);
        assert!(inspection_limits_from(|_| Some("invalid".into())).is_err());
        let mut observed = DiagnosticObservations::new(RunId::from("view-budget"), limits(1));
        observed.inspection_limits = Ok(display);
        observed.record(&AppEvent::WindowOpened);
        assert!(
            !observed
                .inspection()
                .unwrap()
                .lines
                .iter()
                .any(|row| row.text.starts_with('#'))
        );
        assert_eq!(observed.stats().unwrap().retained_records, 1);
    }
    fn limits(count: usize) -> RetentionLimits {
        RetentionLimits {
            max_records: count,
            max_bytes: 4096,
            max_age: Duration::from_secs(300),
        }
    }
    #[test]
    fn event_copy_omits_urls_paths_titles_prompts_and_errors() {
        let mut observed = DiagnosticObservations::new(RunId::from("redaction"), limits(16));
        let node = uuid::Uuid::nil();
        for event in [
            AppEvent::AddressOpened("https://secret.test/?password=secret".into()),
            AppEvent::ContentTitleChanged {
                node,
                title: "secret title".into(),
            },
            AppEvent::DownloadCompleted {
                node,
                destination: "C:\\secret-path".into(),
                content_hash: "secret-hash".into(),
            },
            AppEvent::SmolwebInputRequested {
                node,
                prompt: "secret prompt".into(),
                sensitive: true,
            },
            AppEvent::DownloadFailed {
                node,
                error: "secret error".into(),
            },
            AppEvent::SessionSwitched("secret session".into()),
        ] {
            observed.record(&event);
        }
        let mut cursor = observed.cursor();
        let batch = observed.read(&mut cursor, 16).unwrap();
        let encoded = serde_json::to_string(&batch).unwrap();
        assert!(!encoded.contains("secret"));
        assert_eq!(batch.records.len(), 6);
        assert!(
            batch
                .records
                .iter()
                .all(|record| record.envelope.metadata == ObservationMetadata::default())
        );
        assert_eq!(
            batch.records.last().unwrap().payload.kind,
            "other-app-event"
        );
    }
    #[test]
    fn overflow_is_visible_to_independent_readers() {
        let mut observed = DiagnosticObservations::new(RunId::from("overflow"), limits(1));
        let mut first = observed.cursor();
        let mut second = observed.cursor();
        observed.record(&AppEvent::WindowOpened);
        observed.record(&AppEvent::WindowClosed);
        let a = observed.read(&mut first, 1).unwrap();
        let b = observed.read(&mut second, 1).unwrap();
        assert_eq!(a.records, b.records);
        assert_eq!(a.gaps, b.gaps);
        assert_eq!(a.gaps[0].first_sequence, 1);
        assert_eq!(a.stats.loss.evicted, 1);
        assert_eq!(a.records[0].payload.kind, "window-closed");
    }
    #[test]
    fn byte_limit_accounts_the_encoded_category_and_envelope() {
        let mut configured = limits(16);
        let mut measured = DiagnosticObservations::new(RunId::from("bytes"), configured);
        measured.record(&AppEvent::WindowOpened);
        configured.max_bytes = measured.stats().unwrap().retained_bytes;
        let mut observed = DiagnosticObservations::new(RunId::from("bytes"), configured);
        observed.record(&AppEvent::WindowOpened);
        observed.record(&AppEvent::WindowClosed);
        let mut cursor = observed.cursor();
        let batch = observed.read(&mut cursor, 16).unwrap();
        assert_eq!(batch.records.len(), 1);
        assert_eq!(batch.stats.loss.evicted, 1);
        assert!(batch.stats.retained_bytes <= configured.max_bytes);
        let record = &batch.records[0];
        assert!(record.accounted_bytes > serde_json::to_vec(&record.payload).unwrap().len());

        configured.max_bytes = serde_json::to_vec(&record.payload).unwrap().len();
        let mut payload_only = DiagnosticObservations::new(RunId::from("bytes"), configured);
        payload_only.record(&AppEvent::WindowClosed);
        assert_eq!(payload_only.stats().unwrap().loss.rejected_oversized, 1);
        assert_eq!(payload_only.stats().unwrap().retained_records, 0);
    }

    #[test]
    fn successful_requested_receipt_exports_records_and_loss() {
        let directory = tempfile::tempdir().unwrap();
        let mut observed = DiagnosticObservations::new(RunId::from("receipt"), limits(1));
        observed.record(&AppEvent::WindowOpened);
        observed.record(&AppEvent::WindowClosed);
        observed
            .write_scenario_receipt(
                directory.path(),
                &taproot::Outcome {
                    ok: true,
                    log: vec!["existing assertion".into()],
                },
            )
            .unwrap();
        let receipt = std::fs::read_to_string(directory.path().join("scenario.done")).unwrap();
        assert!(receipt.starts_with("RESULT ok\nexisting assertion\n"));
        let bytes = std::fs::read(directory.path().join("diagnostics.json")).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["stats"]["loss"]["evicted"], 1);
        assert_eq!(json["records"][0]["payload"]["kind"], "window-closed");
        assert_eq!(json["gaps"][0]["first_sequence"], 1);
    }

    #[test]
    fn zero_configuration_reports_disabled_retention() {
        for key in [
            "TURNSTONE_DIAGNOSTIC_RECORDS",
            "TURNSTONE_DIAGNOSTIC_BYTES",
            "TURNSTONE_DIAGNOSTIC_AGE_SECS",
        ] {
            let configured = limits_from(|name| (name == key).then(|| "0".into())).unwrap();
            let mut observed = DiagnosticObservations::new(RunId::from("disabled"), configured);
            observed.record(&AppEvent::WindowOpened);
            let mut cursor = observed.cursor();
            let batch = observed.read(&mut cursor, 1).unwrap();
            assert!(batch.records.is_empty());
            assert_eq!(batch.stats.loss.rejected_disabled, 1);
            assert_eq!(batch.gaps.len(), 1);
        }
        assert!(limits_from(|_| Some("invalid".into())).is_err());
    }
    #[test]
    fn requested_export_failure_fails_the_scenario_sentinel() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("diagnostics.json")).unwrap();
        let mut observed = DiagnosticObservations::new(RunId::from("export"), limits(2));
        observed.record(&AppEvent::WindowOpened);
        observed
            .write_scenario_receipt(
                directory.path(),
                &taproot::Outcome {
                    ok: true,
                    log: Vec::new(),
                },
            )
            .unwrap();
        let receipt = std::fs::read_to_string(directory.path().join("scenario.done")).unwrap();
        assert!(receipt.starts_with("RESULT fail\n"));
        assert!(receipt.contains("diagnostic export write failed"));
    }
}
