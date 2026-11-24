// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use mere::{
    ContextCache,
    dataset::View,
    formats::{RdfFormat, parse::parse},
};
use mere_ontology::{Profile, RecordIndex, TermKind};
use oxrdf::{Graph, NamedNodeRef, NamedOrBlankNodeRef, TermRef};
use std::collections::{BTreeMap, BTreeSet};
use std::{fs::File, io::Read, path::Path};
use url::Url;

pub const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
const CONTEXT: &[u8] = include_bytes!("../assets/anno.jsonld");
const ANNOTATION: &str = "http://www.w3.org/ns/oa#Annotation";
const SELECTOR: &str = "http://www.w3.org/ns/oa#FragmentSelector";
const TARGET: &str = "http://www.w3.org/ns/oa#hasTarget";
const SOURCE: &str = "http://www.w3.org/ns/oa#hasSource";
const HAS_SELECTOR: &str = "http://www.w3.org/ns/oa#hasSelector";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const VALUE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#value";
const CONFORMS: &str = "http://purl.org/dc/terms/conformsTo";
const MEDIA_FRAGMENTS: &str = "http://www.w3.org/TR/media-frags/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Passage {
    pub note: String,
    pub title: Option<String>,
    pub address: Url,
}

impl Passage {
    pub fn source_address(&self) -> Url {
        let mut source = self.address.clone();
        source.set_fragment(None);
        source
    }

    pub fn time_range_ms(&self) -> Option<(u64, Option<u64>)> {
        temporal(self.address.fragment()?)
    }

    /// A listening note may have an audio body and no textual title.
    pub fn label(&self) -> String {
        let title = self.title.as_deref().unwrap_or("Open passage");
        let source = self.source_address();
        let time = self
            .address
            .fragment()
            .and_then(|value| value.strip_prefix("t="));
        match time {
            Some(range) => {
                let span = range.replace(',', " to ");
                format!("{title} · {span} s · {source}")
            },
            None => format!("{title} · {source}"),
        }
    }
}

/// The native action understands just the declared annotation and temporal selector terms.
fn enabled(profile: &Profile) -> bool {
    [
        (ANNOTATION, TermKind::Class),
        (SELECTOR, TermKind::Class),
        (TARGET, TermKind::Predicate),
        (SOURCE, TermKind::Predicate),
        (HAS_SELECTOR, TermKind::Predicate),
    ]
    .iter()
    .all(|(iri, kind)| {
        profile
            .terms
            .entry(iri)
            .is_some_and(|term| term.kind == *kind)
    })
}

fn only<'g>(
    graph: &'g Graph,
    subject: NamedOrBlankNodeRef<'_>,
    predicate: &'static str,
) -> Option<TermRef<'g>> {
    let mut objects =
        graph.objects_for_subject_predicate(subject, NamedNodeRef::new_unchecked(predicate));
    let object = objects.next()?;
    objects.next().is_none().then_some(object)
}

fn node(term: TermRef<'_>) -> Option<NamedOrBlankNodeRef<'_>> {
    match term {
        TermRef::NamedNode(id) => Some(id.into()),
        TermRef::BlankNode(id) => Some(id.into()),
        _ => None,
    }
}

// Redshank expresses Normal Play Time in seconds with millisecond precision.
fn milliseconds(seconds: &str) -> Option<u64> {
    let (whole, fraction) = match seconds.split_once('.') {
        None => (seconds, ""),
        Some((_, "")) => return None,
        Some(parts) => parts,
    };
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 3
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let mut ms = whole.parse::<u64>().ok()?.checked_mul(1000)?;
    for (byte, weight) in fraction.bytes().zip([100, 10, 1]) {
        ms = ms.checked_add(u64::from(byte - b'0') * weight)?;
    }
    Some(ms)
}

fn temporal(fragment: &str) -> Option<(u64, Option<u64>)> {
    let range = fragment.strip_prefix("t=")?;
    let (start, end) = range
        .split_once(',')
        .map_or((range, None), |(s, e)| (s, Some(e)));
    let start = milliseconds(start)?;
    let end = match end {
        Some(value) => Some(milliseconds(value)?),
        None => None,
    };
    if end.is_some_and(|end| end <= start) {
        return None;
    }
    Some((start, end))
}

fn passage(profile: &Profile, graph: &Graph, note: NamedNodeRef<'_>) -> Option<Passage> {
    let target = node(only(graph, note.into(), TARGET)?)?;
    let selector = node(only(graph, target, HAS_SELECTOR)?)?;
    if !graph
        .objects_for_subject_predicate(selector, NamedNodeRef::new_unchecked(TYPE))
        .any(|ty| ty == NamedNodeRef::new_unchecked(SELECTOR).into())
        || only(graph, selector, CONFORMS)? != NamedNodeRef::new_unchecked(MEDIA_FRAGMENTS).into()
    {
        return None;
    }
    let TermRef::Literal(value) = only(graph, selector, VALUE)? else {
        return None;
    };
    let fragment = value.value();
    temporal(fragment)?;
    let TermRef::NamedNode(source) = only(graph, target, SOURCE)? else {
        return None;
    };
    let mut address = Url::parse(source.as_str()).ok()?;
    // A received record may only offer an ordinary web navigation. In particular,
    // a note received from the web cannot ask the host to open a local file.
    if !matches!(address.scheme(), "http" | "https")
        || address.fragment().is_some()
        || !address.username().is_empty()
        || address.password().is_some()
    {
        return None;
    }
    address.set_fragment(Some(fragment));
    const HAS_BODY: &str = "http://www.w3.org/ns/oa#hasBody";
    const TEXT_BODY: &str = "http://www.w3.org/ns/oa#TextualBody";
    let title = profile
        .terms
        .entry(HAS_BODY)
        .filter(|entry| entry.kind == TermKind::Predicate)
        .and_then(|_| profile.terms.entry(TEXT_BODY))
        .filter(|entry| entry.kind == TermKind::Class)
        .and_then(|_| node(only(graph, note.into(), HAS_BODY)?))
        .filter(|body| {
            graph
                .objects_for_subject_predicate(*body, NamedNodeRef::new_unchecked(TYPE))
                .any(|ty| ty == NamedNodeRef::new_unchecked(TEXT_BODY).into())
        })
        .and_then(|body| match only(graph, body, VALUE)? {
            TermRef::Literal(text) if !text.value().trim().is_empty() => {
                Some(text.value().chars().take(160).collect())
            },
            _ => None,
        });
    Some(Passage {
        title,
        note: note.as_str().into(),
        address,
    })
}

/// Projections of one document's default graph. Received graph names do not establish custody.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    pub passages: Vec<Passage>,
    pub links: RecordIndex,
    names: BTreeMap<String, String>,
}

impl Reading {
    pub fn name<'a>(&'a self, iri: &'a str) -> &'a str {
        self.names.get(iri).map_or(iri, String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.passages.is_empty() && self.links.records().len() == 0
    }
}

fn json_ld() -> RdfFormat {
    RdfFormat::JsonLd {
        profile: Default::default(),
    }
}

fn default_graph(bytes: &[u8], format: RdfFormat, base: Option<&str>) -> Result<Graph, String> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err("Annotation document exceeds 2 MiB".into());
    }
    let cache = ContextCache::new()
        .with("http://www.w3.org/ns/anno.jsonld", CONTEXT)
        .with("https://www.w3.org/ns/anno.jsonld", CONTEXT);
    let dataset = parse(format, bytes, base, cache)
        .map_err(|error| format!("Could not read linked document: {error}"))?;
    Ok(dataset
        .quads(View::Live)
        .filter(|q| q.graph_name.is_default_graph())
        .map(|q| oxrdf::Triple::new(q.subject, q.predicate, q.object))
        .collect())
}

pub fn read(profile: &Profile, bytes: &[u8]) -> Result<Reading, String> {
    read_format(profile, bytes, json_ld(), None)
}

pub(super) fn read_format(
    profile: &Profile,
    bytes: &[u8],
    format: RdfFormat,
    base: Option<&str>,
) -> Result<Reading, String> {
    let graph = default_graph(bytes, format, base)?;
    let links = RecordIndex::new(
        profile,
        graph.iter().filter_map(|triple| {
            let NamedOrBlankNodeRef::NamedNode(subject) = triple.subject else {
                return None;
            };
            let TermRef::NamedNode(object) = triple.object else {
                return None;
            };
            Some((subject.as_str(), triple.predicate.as_str(), object.as_str()))
        }),
    );
    let mut names = BTreeMap::new();
    const LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
    for iri in links.records() {
        // Conflicting labels do not silently choose which name to show.
        if let Some(TermRef::Literal(value)) =
            only(&graph, NamedNodeRef::new_unchecked(iri).into(), LABEL)
            && !value.value().trim().is_empty()
        {
            names.insert(iri.to_owned(), value.value().chars().take(160).collect());
        } else if let Some(class) = links
            .types(iri)
            .first()
            .and_then(|class| profile.terms.entry(class))
        {
            let hint = iri
                .rsplit(['/', '#', ':'])
                .find(|part| !part.is_empty())
                .unwrap_or(iri);
            let hint: String = hint.chars().take(48).collect();
            names.insert(
                iri.to_string(),
                format!("{} · {hint}", class.display_label("en")),
            );
        }
    }
    // A source can give the same title to a work and an occurrence of it.
    // Qualify shared names with the type first. Preserve the IRI when ambiguity remains.
    let repeated = repeated_names(&links, &names);
    if !repeated.is_empty() {
        for (iri, name) in &mut names {
            if repeated.contains(name)
                && let Some(class) = links
                    .types(iri)
                    .first()
                    .and_then(|iri| profile.terms.entry(iri))
            {
                name.push_str(" · ");
                name.push_str(class.display_label("en"));
            }
        }
        let repeated = repeated_names(&links, &names);
        for (iri, name) in &mut names {
            if repeated.contains(name) {
                name.push_str(" · ");
                name.push_str(iri);
            }
        }
    }
    Ok(Reading {
        passages: timed_notes(profile, &graph),
        links,
        names,
    })
}

fn repeated_names(links: &RecordIndex, names: &BTreeMap<String, String>) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut repeated = BTreeSet::new();
    for iri in links.records() {
        let name = names.get(iri).map_or(iri, String::as_str);
        if !seen.insert(name) {
            repeated.insert(name.to_owned());
        }
    }
    repeated
}

/// Select timed notes without constructing the relationship browsing index.
pub fn passages(profile: &Profile, bytes: &[u8]) -> Result<Vec<Passage>, String> {
    if !enabled(profile) {
        return Ok(Vec::new());
    }
    Ok(timed_notes(
        profile,
        &default_graph(bytes, json_ld(), None)?,
    ))
}

fn timed_notes(profile: &Profile, graph: &Graph) -> Vec<Passage> {
    if !enabled(profile) {
        return Vec::new();
    }
    let mut found: Vec<_> = graph
        .subjects_for_predicate_object(
            NamedNodeRef::new_unchecked(TYPE),
            NamedNodeRef::new_unchecked(ANNOTATION),
        )
        .filter_map(|subject| match subject {
            NamedOrBlankNodeRef::NamedNode(note) => passage(profile, graph, note),
            _ => None,
        })
        .collect();
    found.sort_by(|a, b| a.note.cmp(&b.note));
    found
}

/// Read timed selections from an explicitly chosen local export.
pub fn passages_from_file(profile: &Profile, path: &Path) -> Result<Vec<Passage>, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|f| {
            f.take(MAX_DOCUMENT_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !enabled(profile) {
        return Ok(Vec::new());
    }
    let format = path
        .extension()
        .and_then(|s| s.to_str())
        .and_then(RdfFormat::from_extension)
        .unwrap_or_else(json_ld);
    Ok(timed_notes(profile, &default_graph(&bytes, format, None)?))
}

/// Resolve one named annotation from a local export and return its standard media-fragment address.
pub fn open_annotation(profile: &Profile, path: &Path, note: &str) -> Result<Url, String> {
    passages_from_file(profile, path)?
        .into_iter()
        .find(|p| p.note == note)
        .map(|p| p.address)
        .ok_or_else(|| format!("No supported timed target for {note} under the selected lens"))
}
