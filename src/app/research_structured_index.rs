// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use eidetic::models::recall::RecallSourceId;
use eidetic::records::{PropertyRecordV1, RecordRow, SourceRecord};
use std::collections::BTreeMap;
#[cfg(test)]
#[path = "research_structured_index_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Row(usize),
    Record { row: usize, record: usize },
    Source { row: usize, source: usize },
}

/// Projection of one immutable result installation. Never used as authority.
pub struct Index {
    pub generation: uuid::Uuid,
    starts: Vec<usize>,
    records: Vec<usize>,
    source_versions: BTreeMap<RecallSourceId, Vec<usize>>,
    record_ids: BTreeMap<String, Vec<usize>>,
}
impl Index {
    pub fn new(
        generation: uuid::Uuid,
        rows: &[RecordRow],
        sources: &[SourceRecord],
        frozen_records: &[PropertyRecordV1],
    ) -> Result<Self, String> {
        let mut starts = Vec::with_capacity(rows.len().saturating_add(1));
        let mut records = Vec::with_capacity(rows.len());
        let mut total = 0usize;
        starts.push(total);
        for row in rows {
            total = total
                .checked_add(1)
                .and_then(|n| n.checked_add(row.record_ids.len()))
                .and_then(|n| n.checked_add(row.sources.len()))
                .ok_or("Result navigation limit exceeded")?;
            starts.push(total);
            records.push(row.record_ids.len());
        }
        let mut source_versions = BTreeMap::<RecallSourceId, Vec<usize>>::new();
        for (index, source) in sources.iter().enumerate() {
            source_versions
                .entry(source.source.clone())
                .or_default()
                .push(index);
        }
        let mut record_ids = BTreeMap::<String, Vec<usize>>::new();
        for (index, record) in frozen_records.iter().enumerate() {
            record_ids.entry(record.id.clone()).or_default().push(index);
        }
        Ok(Self {
            generation,
            starts,
            records,
            source_versions,
            record_ids,
        })
    }
    pub fn total(&self) -> usize {
        *self.starts.last().unwrap_or(&0)
    }
    pub fn source_versions(&self, source: &RecallSourceId) -> &[usize] {
        self.source_versions
            .get(source)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    pub fn record_index(&self, id: &str) -> Option<usize> {
        match self.record_ids.get(id)?.as_slice() {
            [index] => Some(*index),
            _ => None,
        }
    }
    pub fn locate(&self, position: usize) -> Option<Entry> {
        self.locate_counted(position, || {})
    }
    pub fn locate_counted(&self, position: usize, mut probe: impl FnMut()) -> Option<Entry> {
        if position >= self.total() {
            return None;
        }
        let (mut lo, mut hi) = (0, self.records.len());
        while lo < hi {
            probe();
            let mid = lo + (hi - lo) / 2;
            if self.starts[mid + 1] <= position {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let offset = position - self.starts[lo];
        if offset == 0 {
            Some(Entry::Row(lo))
        } else if offset <= self.records[lo] {
            Some(Entry::Record {
                row: lo,
                record: offset - 1,
            })
        } else {
            Some(Entry::Source {
                row: lo,
                source: offset - 1 - self.records[lo],
            })
        }
    }
}
