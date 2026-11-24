// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use crate::sections::{SectionActivate, SectionRow};
use turnstone_ontology::{Reading, selected_profile};
use uuid::Uuid;

const PAGE: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
enum View {
    Records,
    Types,
    Members(String),
    Record(String),
}

/// The admission token expires on replacement, even at the same URL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkPage {
    pub(super) admission: Uuid,
    view: View,
    offset: usize,
    evidence: bool,
}

impl LinkPage {
    fn view(&self, view: View) -> Self {
        Self {
            admission: self.admission,
            view,
            offset: 0,
            evidence: false,
        }
    }

    fn button(self, text: impl Into<String>) -> SectionRow {
        SectionRow {
            id: None,
            text: text.into(),
            activate: Some(SectionActivate::BrowseLinks(self)),
        }
    }

    fn record(&self, reading: &Reading, iri: &str) -> SectionRow {
        let name = reading.name(iri);
        let n = reading.links.links(iri).len();
        let label = match n {
            0 => name.to_string(),
            1 => format!("{name} · 1 link"),
            n => format!("{name} · {n} links"),
        };
        self.view(View::Record(iri.to_string())).button(label)
    }
}

/// Keep the visible page until either the source admission or the chosen view changes.
#[derive(Default)]
pub(super) struct CachedRows {
    page: Option<LinkPage>,
    rows: Vec<SectionRow>,
}

impl CachedRows {
    pub(super) fn get(
        &mut self,
        admission: Uuid,
        source: &str,
        reading: &Reading,
        selected: Option<&LinkPage>,
    ) -> &[SectionRow] {
        let root = LinkPage {
            admission,
            view: View::Records,
            offset: 0,
            evidence: false,
        };
        let page = selected
            .filter(|p| p.admission == admission)
            .unwrap_or(&root);
        if self.page.as_ref() != Some(page) {
            self.rows = rows(admission, source, reading, Some(page));
            self.page = Some(page.clone());
        }
        &self.rows
    }
}

fn text(value: impl Into<String>) -> SectionRow {
    SectionRow {
        id: None,
        text: value.into(),
        activate: None,
    }
}

pub(super) fn rows(
    admission: Uuid,
    source: &str,
    reading: &Reading,
    selected: Option<&LinkPage>,
) -> Vec<SectionRow> {
    let root = LinkPage {
        admission,
        view: View::Records,
        offset: 0,
        evidence: false,
    };
    let page = selected
        .filter(|p| p.admission == admission)
        .unwrap_or(&root);
    if reading.links.records().len() == 0 {
        return Vec::new();
    }
    let Ok(profile) = selected_profile() else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    let count = match &page.view {
        View::Records => {
            if reading.links.classes().len() > 0 {
                rows.push(page.view(View::Types).button("Browse by type"));
            }
            let records = reading.links.records();
            let count = records.len();
            rows.extend(
                records
                    .skip(page.offset)
                    .take(PAGE)
                    .map(|iri| page.record(reading, iri)),
            );
            count
        },
        View::Types => {
            rows.push(root.clone().button("All linked records"));
            let classes = reading.links.classes();
            let count = classes.len();
            for class in classes.skip(page.offset).take(PAGE) {
                let label = profile
                    .terms
                    .entry(class)
                    .map_or(class, |term| term.display_label("en"));
                let n = reading.links.members(class).len();
                rows.push(
                    page.view(View::Members(class.to_owned()))
                        .button(format!("{label} ({n})")),
                );
            }
            count
        },
        View::Members(class) => {
            rows.push(page.view(View::Types).button("All types"));
            let members = reading.links.members(class);
            let label = profile
                .terms
                .entry(class)
                .map_or(class.as_str(), |t| t.display_label("en"));
            rows.push(text(format!("{label} ({})", members.len())));
            rows.extend(
                members
                    .iter()
                    .skip(page.offset)
                    .take(PAGE)
                    .map(|iri| page.record(reading, iri)),
            );
            members.len()
        },
        View::Record(record) => {
            rows.push(root.clone().button("All linked records"));
            rows.push(text(reading.name(record)));
            let types = reading.links.types(record);
            let visible_types = types.len().saturating_sub(page.offset).min(PAGE);
            for class in types.iter().skip(page.offset).take(PAGE) {
                let label = profile
                    .terms
                    .entry(class)
                    .map_or(class.as_str(), |t| t.display_label("en"));
                rows.push(
                    page.view(View::Members(class.clone()))
                        .button(format!("Type · {label}")),
                );
            }
            let links = reading.links.links(record);
            let count = types.len() + links.len();
            for link in links
                .skip(page.offset.saturating_sub(types.len()))
                .take(PAGE - visible_types)
            {
                let Some(term) = profile.terms.entry(&link.predicate) else {
                    continue;
                };
                let (direction, other) = if &link.subject == record {
                    ("Outgoing", &link.object)
                } else {
                    ("Incoming", &link.subject)
                };
                rows.push(page.view(View::Record(other.clone())).button(format!(
                    "{direction} · {} · {}",
                    term.display_label("en"),
                    reading.name(other)
                )));
                if page.evidence {
                    rows.push(text(format!(
                        "{} → {} → {}",
                        link.subject, link.predicate, link.object
                    )));
                    rows.push(text(format!("Vocabulary · {}", term.pack)));
                }
            }
            rows.push(
                LinkPage {
                    evidence: !page.evidence,
                    ..page.clone()
                }
                .button(if page.evidence {
                    "Hide evidence"
                } else {
                    "Show evidence"
                }),
            );
            if page.evidence {
                rows.push(text(format!("Record · {record}")));
                rows.push(text(format!("Source document · {source}")));
            }
            count
        },
    };
    if page.offset >= PAGE {
        rows.push(
            LinkPage {
                offset: page.offset - PAGE,
                ..page.clone()
            }
            .button("Previous"),
        );
    }
    if page.offset.saturating_add(PAGE) < count {
        rows.push(
            LinkPage {
                offset: page.offset + PAGE,
                ..page.clone()
            }
            .button("More"),
        );
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ontology_browsing_follows_an_incoming_link_and_expires_on_reload() {
        let profile = selected_profile().unwrap();
        let data = br#"{"@graph":[{"@id":"urn:line","http://purl.org/spar/cito/cites":{"@id":"urn:take"}},{"@id":"urn:take","@type":"http://www.w3.org/ns/oa#TextualBody"}]}"#;
        let reading = turnstone_ontology::read(profile, data).unwrap();
        let token = Uuid::new_v4();
        let follow = |page: Option<&LinkPage>, label: &str| {
            let list = rows(token, "urn:export", &reading, page);
            let row = list.into_iter().find(|r| r.text.contains(label)).unwrap();
            let Some(SectionActivate::BrowseLinks(page)) = row.activate else {
                panic!("missing local drill-down")
            };
            page
        };
        let types = follow(None, "Browse by type");
        let class = follow(Some(&types), "(1)");
        let take = follow(Some(&class), "1 link");
        let evidence = follow(Some(&take), "Show evidence");
        let list = rows(token, "urn:export", &reading, Some(&evidence));
        assert!(
            list.iter()
                .any(|r| r.text == "urn:line → http://purl.org/spar/cito/cites → urn:take")
        );
        assert!(
            list.iter()
                .any(|r| r.text == "Source document · urn:export")
        );
        let line = follow(Some(&take), "Incoming · cites");
        assert!(
            rows(token, "urn:export", &reading, Some(&line))
                .iter()
                .any(|r| r.text.starts_with("Outgoing · cites"))
        );
        let new = rows(Uuid::new_v4(), "urn:export", &reading, Some(&line));
        assert!(new.iter().any(|r| r.text == "Browse by type"));
    }

    #[test]
    fn ontology_large_type_and_backlink_sets_page_without_losing_members() {
        let profile = selected_profile().unwrap();
        let triples: Vec<_> = (0..257)
            .map(|i| {
                serde_json::json!({
                    "@id": format!("urn:line:{i:03}"),
                    "@type": "http://www.w3.org/ns/oa#TextualBody",
                    "http://purl.org/spar/cito/cites": {"@id":"urn:take"}
                })
            })
            .collect();
        let input = serde_json::to_vec(&serde_json::json!({"@graph": triples})).unwrap();
        let reading = turnstone_ontology::read(profile, &input).unwrap();
        let token = Uuid::new_v4();
        for view in [
            View::Record("urn:take".into()),
            View::Members("http://www.w3.org/ns/oa#TextualBody".into()),
        ] {
            let mut page = LinkPage {
                admission: token,
                view,
                offset: 0,
                evidence: false,
            };
            let mut visited = std::collections::BTreeSet::new();
            loop {
                let listed = rows(token, "urn:source", &reading, Some(&page));
                let mut next = None;
                assert!(listed.len() <= PAGE + 6);
                for row in listed {
                    if let Some(SectionActivate::BrowseLinks(target)) = row.activate {
                        if row.text == "More" {
                            next = Some(target);
                        } else if let View::Record(record) = target.view
                            && record.starts_with("urn:line:")
                        {
                            assert!(visited.insert(record));
                        }
                    }
                }
                let Some(target) = next else {
                    break;
                };
                page = target;
            }
            assert_eq!(visited.len(), 257);
        }
    }
}
