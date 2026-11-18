// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use cambium::{DetailRow, DetailSection};

use super::LinkTarget;

/// Text quote and half-open code-point range in the capture's canonical text.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OccurrenceText {
    pub start: u64,
    pub end: u64,
    pub prefix: String,
    pub exact: String,
    pub suffix: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ObservationLocator {
    Hyperlink {
        order: usize,
        href: String,
        rel: Option<String>,
        text: String,
        anchor: Option<OccurrenceText>,
    },
    JsonLd {
        order: u64,
        named_graph: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ObservationSource {
    Response,
    ObservedDocument,
}

/// One directed observation from an admitted capture. These are never held
/// statements and have no write or retention authority.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LinkObservation {
    pub from: String,
    pub to: String,
    pub predicate: String,
    pub source: String,
    pub capture: String,
    pub acquisition: ObservationSource,
    pub locator: ObservationLocator,
}

pub struct ObservedLinkInspection {
    pub target: LinkTarget,
    pub observations: Vec<LinkObservation>,
    pub total: usize,
}

impl ObservedLinkInspection {
    pub fn details(&self) -> Vec<DetailSection> {
        let mut sections: Vec<_> = self
            .observations
            .iter()
            .map(LinkObservation::details)
            .collect();
        if self.total > sections.len() {
            sections.push(DetailSection::new(
                format!("{} more observations", self.total - sections.len()),
                Vec::new(),
            ));
        }
        sections
    }
}

impl LinkObservation {
    fn details(&self) -> DetailSection {
        let mut rows = vec![
            DetailRow::new("From", &self.from),
            DetailRow::new("Predicate", &self.predicate),
            DetailRow::new("To", &self.to),
            DetailRow::new("Source", &self.source),
            DetailRow::new("Capture", &self.capture),
            DetailRow::new(
                "Acquisition",
                match self.acquisition {
                    ObservationSource::Response => "Fetched response",
                    ObservationSource::ObservedDocument => "Observed document",
                },
            ),
        ];
        let title = match &self.locator {
            ObservationLocator::Hyperlink {
                order,
                href,
                rel,
                text,
                anchor,
            } => {
                rows.push(DetailRow::new("Href", href));
                if let Some(rel) = rel {
                    rows.push(DetailRow::new("Rel", rel));
                }
                if let Some(anchor) = anchor {
                    rows.push(DetailRow::new(
                        "Context",
                        format!(
                            "{}[{}]{}",
                            anchor.prefix,
                            excerpt(&anchor.exact),
                            anchor.suffix
                        ),
                    ));
                    rows.push(DetailRow::new(
                        "Text range",
                        format!("{}–{}", anchor.start, anchor.end),
                    ));
                } else {
                    rows.push(DetailRow::new("Anchor text", excerpt(text)));
                    rows.push(DetailRow::new("Text range", "Unavailable"));
                }
                format!("HTML link {}", order + 1)
            },
            ObservationLocator::JsonLd { order, named_graph } => {
                if let Some(graph) = named_graph {
                    rows.push(DetailRow::new("RDF graph", graph));
                }
                format!("JSON-LD block at DOM position {order}")
            },
        };
        DetailSection::new(title, rows)
    }
}

fn excerpt(text: &str) -> String {
    let mut chars = text.chars();
    let mut excerpt: String = chars.by_ref().take(240).collect();
    if chars.next().is_some() {
        excerpt.push('…');
    }
    excerpt
}
