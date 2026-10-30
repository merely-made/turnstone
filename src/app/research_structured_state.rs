// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use crate::research_records::{Binding, ManualProperty};
use eidetic::records::{
    Cardinality, CivilDate, OffsetTimestamp, PropertyDefinitionV1, PropertyValue, RecordFilters,
    RecordOperation, RecordQuery, TimeInterval, ValueType,
};
#[path = "research_structured_index.rs"]
pub(crate) mod index;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    Sources,
    Records,
    Definitions,
    Lifecycle,
    Exclusions,
    Query,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrozenTarget {
    Source(usize),
    Record(usize),
    Definition(usize),
    Lifecycle(usize),
    Exclusion(usize),
    Query,
    Inspected,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Home,
    Inputs,
    InputList(InputKind),
    RowSource {
        generation: uuid::Uuid,
        row: usize,
        source: usize,
    },
    Frozen {
        generation: Option<uuid::Uuid>,
        target: FrozenTarget,
    },
    FrozenPart {
        generation: Option<uuid::Uuid>,
        target: FrozenTarget,
        field: usize,
        start: usize,
    },
    OpenResultSource {
        generation: uuid::Uuid,
        source: usize,
    },
    OpenFrozenEvidence {
        generation: uuid::Uuid,
        record: usize,
    },
    Group {
        generation: uuid::Uuid,
        index: usize,
    },
    GroupPart {
        generation: uuid::Uuid,
        index: usize,
        field: usize,
        start: usize,
    },
    Operations,
    Scope,
    Filters,
    FilterSources,
    Author,
    Entities,
    Evidence,
    Settings,
    Saved,
    History,
    Recovery,
    RefreshRecovery,
    RetryRecovery(uuid::Uuid),
    AbandonRecovery(uuid::Uuid),
    Operation(RecordOperation),
    Type(ValueType),
    Cardinality(Cardinality),
    Entity(crate::panes::GraphId, uuid::Uuid),
    EvidenceTarget(crate::research::Target),
    FilterSource(Option<eidetic::models::recall::RecallSourceId>),
    Input(String),
    Submit(String),
    Run,
    SaveProperty,
    Inspect(Binding),
    OpenEvidence(Binding),
    Withdraw(Binding),
    Replay(eidetic::ManifestId),
    Page(isize),
    Back,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    Home,
    Inputs,
    InputList(InputKind),
    RowSource {
        generation: uuid::Uuid,
        row: usize,
        source: usize,
    },
    Frozen {
        generation: Option<uuid::Uuid>,
        target: FrozenTarget,
    },
    FrozenPart {
        generation: Option<uuid::Uuid>,
        target: FrozenTarget,
        field: usize,
        start: usize,
    },
    Group {
        generation: uuid::Uuid,
        index: usize,
    },
    GroupPart {
        generation: uuid::Uuid,
        index: usize,
        field: usize,
        start: usize,
    },
    Operations,
    Filters,
    FilterSources,
    Author,
    Entities,
    Evidence,
    Settings,
    Saved,
    History,
    Recovery,
    Input(String),
}

pub struct State {
    pub controller: super::research_record_owner::Controller,
    pub index: Option<index::Index>,
    pub indexed_generation: Option<uuid::Uuid>,
    pub index_error: Option<String>,
    pub view: View,
    pub draft: Draft,
    pub operation: RecordOperation,
    pub filters: RecordFilters,
    pub filter_start: String,
    pub filter_end: String,
    pub input_return: View,
    pub page: usize,
    pub error: Option<String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            controller: Default::default(),
            index: None,
            indexed_generation: None,
            index_error: None,
            view: View::Home,
            draft: Draft::default(),
            operation: RecordOperation::SourceLookup,
            filters: RecordFilters::default(),
            filter_start: String::new(),
            filter_end: String::new(),
            input_return: View::Home,
            page: 0,
            error: None,
        }
    }
}
#[derive(Default)]
pub struct Draft {
    pub entity: Option<(crate::panes::GraphId, uuid::Uuid)>,
    pub definition: String,
    pub revision: String,
    pub value_type: Option<ValueType>,
    pub cardinality: Option<Cardinality>,
    pub unit: String,
    pub statuses: String,
    pub value: String,
    pub context: String,
    pub status: String,
    pub valid_start: String,
    pub valid_end: String,
    pub evidence: Option<crate::research::Target>,
}
impl Draft {
    pub fn property(&self) -> Result<ManualProperty, String> {
        let (graph, entity) = self.entity.ok_or("Choose an existing entity")?;
        let value_type = self.value_type.clone().ok_or("Choose a value type")?;
        let cardinality = self.cardinality.clone().ok_or("Choose cardinality")?;
        let definition = PropertyDefinitionV1 {
            id: self.definition.trim().into(),
            revision: self.revision.trim().into(),
            value_type: value_type.clone(),
            unit: optional(&self.unit),
            cardinality,
            statuses: self
                .statuses
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
        };
        definition
            .validate()
            .map_err(|e| format!("Invalid definition: {e:?}"))?;
        let value = parse_value(&value_type, &self.value, &self.unit)?;
        let status = optional(&self.status);
        if status
            .as_ref()
            .is_some_and(|s| !definition.statuses.contains(s))
        {
            return Err("Status must be listed in the definition".into());
        }
        if self.context.trim().is_empty() {
            return Err("Enter a context".into());
        }
        let evidence = self.evidence.clone().ok_or("Choose an exact passage")?;
        if !evidence
            .selector()
            .as_ref()
            .is_some_and(|s| s.start < s.end)
        {
            return Err("Choose an exact nonempty passage".into());
        }
        Ok(ManualProperty {
            graph,
            entity,
            definition,
            value,
            context: self.context.trim().into(),
            valid_time: parse_interval(&self.valid_start, &self.valid_end)?,
            status,
            evidence,
        })
    }
}
pub fn optional(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}
fn timestamp(s: &str) -> Result<OffsetTimestamp, String> {
    let s = s.trim();
    // jiff accepts only an offset-qualified timestamp here; never consult local time.
    let ts: jiff::Timestamp = s
        .parse()
        .map_err(|_| "Enter YYYY-MM-DDTHH:MM:SS with Z or an explicit offset".to_string())?;
    let date = s.get(..10).ok_or("Invalid date")?;
    let time = s.get(11..19).ok_or("Invalid time")?;
    let offset = s.get(19..).ok_or("Explicit offset required")?;
    let minutes: i16 = if offset == "Z" {
        0
    } else {
        if offset.len() != 6 || &offset[3..4] != ":" || !matches!(&offset[..1], "+" | "-") {
            return Err("Explicit offset required, for example +02:00".into());
        }
        let h: i16 = offset[1..3].parse().map_err(|_| "Invalid offset")?;
        let m: i16 = offset[4..6].parse().map_err(|_| "Invalid offset")?;
        if h > 23 || m > 59 {
            return Err("Invalid offset".into());
        }
        (h * 60 + m) * if &offset[..1] == "-" { -1 } else { 1 }
    };
    let d = civil_date(date)?;
    let p: Vec<_> = time.split(':').collect();
    if p.len() != 3 {
        return Err("Invalid time".into());
    }
    let result = OffsetTimestamp {
        date: d,
        hour: p[0].parse().map_err(|_| "Invalid hour")?,
        minute: p[1].parse().map_err(|_| "Invalid minute")?,
        second: p[2].parse().map_err(|_| "Invalid second")?,
        offset_minutes: minutes,
    };
    let seconds = result
        .utc_seconds()
        .map_err(|e| format!("Invalid timestamp: {e:?}"))?;
    if seconds != ts.as_second() {
        return Err("Invalid timestamp".into());
    }
    Ok(result)
}
fn civil_date(s: &str) -> Result<CivilDate, String> {
    let p: Vec<_> = s.split('-').collect();
    if p.len() != 3 || p[0].len() != 4 || p[1].len() != 2 || p[2].len() != 2 {
        return Err("Enter YYYY-MM-DD".into());
    }
    let date = CivilDate {
        year: p[0].parse().map_err(|_| "Invalid year")?,
        month: p[1].parse().map_err(|_| "Invalid month")?,
        day: p[2].parse().map_err(|_| "Invalid day")?,
    };
    date.validate()
        .map_err(|e| format!("Invalid date: {e:?}"))?;
    Ok(date)
}
pub fn parse_interval(start: &str, end: &str) -> Result<Option<TimeInterval>, String> {
    if start.trim().is_empty() && end.trim().is_empty() {
        return Ok(None);
    }
    let interval = TimeInterval {
        start: timestamp(start)?
            .utc_seconds()
            .map_err(|e| format!("{e:?}"))?,
        end: timestamp(end)?
            .utc_seconds()
            .map_err(|e| format!("{e:?}"))?,
    };
    interval
        .validate()
        .map_err(|e| format!("Invalid interval: {e:?}"))?;
    Ok(Some(interval))
}
pub fn parse_value(kind: &ValueType, text: &str, unit: &str) -> Result<PropertyValue, String> {
    let original = text;
    let text = text.trim();
    Ok(match kind {
        ValueType::Text if !text.is_empty() => PropertyValue::Text(original.into()),
        ValueType::Text => return Err("Enter text".into()),
        ValueType::Boolean => PropertyValue::Boolean(match text {
            "true" => true,
            "false" => false,
            _ => return Err("Enter true or false".into()),
        }),
        ValueType::Integer => PropertyValue::Integer(text.parse().map_err(|_| "Enter an integer")?),
        ValueType::Decimal => {
            let unit = optional(unit).ok_or("Decimal requires a unit")?;
            let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
            if fraction.len() > 18
                || !fraction.chars().all(|c| c.is_ascii_digit())
                || whole.is_empty()
            {
                return Err("Enter a decimal with at most 18 fractional digits".into());
            }
            let digits = format!("{whole}{fraction}");
            PropertyValue::Decimal {
                coefficient: digits.parse().map_err(|_| "Invalid decimal")?,
                scale: fraction.len() as u8,
                unit,
            }
        },
        ValueType::CivilDate => PropertyValue::CivilDate(civil_date(text)?),
        ValueType::Timestamp => PropertyValue::Timestamp(timestamp(text)?),
    })
}
impl State {
    pub fn query(&self) -> Result<RecordQuery, String> {
        let mut filters = self.filters.clone();
        filters.valid_time = parse_interval(&self.filter_start, &self.filter_end)?;
        Ok(RecordQuery {
            operation: self.operation.clone(),
            filters,
        })
    }
}
