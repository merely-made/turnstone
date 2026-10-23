// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Request identity and order, anchored to exact Unicode ranges in a question.
//! This is a bounded request notation, not general natural-language parsing.
use super::recall::PassageSelector;
use crate::{ManifestId, SchemaRef, TypedPayload};
use serde::{Deserialize, Serialize};

pub const EXPLICIT_METHOD: &str = "eidetic.requested-fact-spans/v1";
pub const DERIVED_METHOD: &str = "eidetic.explicit-question-list/v1";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestedFact {
    pub id: ManifestId,
    pub selector: PassageSelector,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestedFactSet {
    pub method: String,
    pub question: ManifestId,
    pub facts: Vec<RequestedFact>,
}
impl RequestedFactSet {
    /// Caller order is authoritative. Ranges must be disjoint, nonblank exact
    /// excerpts. Repeated/missing/overlapping identities cannot name new slots.
    pub fn from_spans(question: &str, spans: &[PassageSelector]) -> Result<Self, String> {
        if question.chars().count() > 4096 || !(1..=8).contains(&spans.len()) {
            return Err("requested facts exceed question or count bounds".into());
        }
        let chars: Vec<char> = question.chars().collect();
        let qid = ManifestId::of_blob(question.as_bytes());
        let mut facts = Vec::new();
        for span in spans {
            if span.start >= span.end
                || span.end > chars.len()
                || spans
                    .iter()
                    .filter(|s| s.start < span.end && span.start < s.end)
                    .count()
                    != 1
            {
                return Err(
                    "requested fact ranges must be nonempty, disjoint and within the question"
                        .into(),
                );
            }
            let text: String = chars[span.start..span.end].iter().collect();
            if text.trim() != text || text.is_empty() || text.len() > 512 {
                return Err("requested fact must be a bounded, nonpadding question excerpt".into());
            }
            let bytes = zeroize::Zeroizing::new(
                serde_json::to_vec(&(qid, span, &text)).map_err(|e| e.to_string())?,
            );
            facts.push(RequestedFact {
                id: ManifestId::of_blob(&bytes),
                selector: span.clone(),
                text,
            });
        }
        Ok(Self {
            method: EXPLICIT_METHOD.into(),
            question: qid,
            facts,
        })
    }
    /// Existing question-only calls accept a single fact or an explicit list:
    /// `What are A, B, and C? Give one claim for each, in that order.`
    /// Quoted/nested lists, multiple questions and unmarked conjunctions require
    /// caller-provided spans. No corpus text, answers or fixture names are used.
    pub fn derive(question: &str) -> Result<Self, String> {
        let error = || "ambiguous requested facts; provide explicit question ranges".to_string();
        let (head, tail) = question
            .split_once('?')
            .map_or((question, ""), |(a, b)| (a, b));
        if tail.contains('?') || head.contains(['"', '(', ')', '[', ']', ';', '\n']) {
            return Err(error());
        }
        let list = tail
            .trim()
            .eq_ignore_ascii_case("Give one claim for each, in that order.");
        let prefix = if list {
            "What are "
        } else if head
            .get(..8)
            .is_some_and(|s| s.eq_ignore_ascii_case("What is "))
        {
            "What is "
        } else {
            ""
        };
        if list
            && !head
                .get(..prefix.len())
                .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
        {
            return Err(error());
        }
        let body = head[prefix.len()..].trim();
        let parts: Vec<&str> = if list {
            if let Some((first, last)) = body.rsplit_once(", and ") {
                let mut p: Vec<_> = first.split(", ").collect();
                p.push(last);
                p
            } else if let Some((first, last)) = body.split_once(" and ") {
                vec![first, last]
            } else {
                return Err(error());
            }
        } else {
            vec![body]
        };
        if list && parts.len() < 2 {
            return Err(error());
        }
        let mut cursor = prefix.len();
        let mut spans = Vec::new();
        for part in parts {
            let lower = zeroize::Zeroizing::new(part.to_ascii_lowercase());
            if part.is_empty()
                || part.trim() != part
                || part.contains(',')
                || lower.contains(" and ")
                || lower.contains(" or ")
            {
                return Err(error());
            }
            let start = cursor + question[cursor..head.len()].find(part).ok_or_else(error)?;
            let end = start + part.len();
            spans.push(PassageSelector {
                start: question[..start].chars().count(),
                end: question[..end].chars().count(),
            });
            cursor = end;
        }
        let mut set = Self::from_spans(question, &spans)?;
        set.method = DERIVED_METHOD.into();
        Ok(set)
    }
    pub fn validate_identity(&self) -> Result<(), String> {
        if !(1..=8).contains(&self.facts.len())
            || !matches!(self.method.as_str(), EXPLICIT_METHOD | DERIVED_METHOD)
        {
            return Err("unsupported requested-fact set or count".into());
        }
        for f in &self.facts {
            if f.text.is_empty()
                || f.text.trim() != f.text
                || f.text.len() > 512
                || f.selector.end.checked_sub(f.selector.start) != Some(f.text.chars().count())
                || self
                    .facts
                    .iter()
                    .filter(|b| {
                        b.selector.start < f.selector.end && f.selector.start < b.selector.end
                    })
                    .count()
                    != 1
            {
                return Err("invalid requested-fact range or text".into());
            }
            let bytes = zeroize::Zeroizing::new(
                serde_json::to_vec(&(self.question, &f.selector, &f.text))
                    .map_err(|e| e.to_string())?,
            );
            if ManifestId::of_blob(&bytes) != f.id {
                return Err("invalid requested-fact identity".into());
            }
        }
        Ok(())
    }
    pub fn validate(&self, question: &str) -> Result<(), String> {
        self.validate_identity()?;
        let rebuilt = Self::from_spans(
            question,
            &self
                .facts
                .iter()
                .map(|f| f.selector.clone())
                .collect::<Vec<_>>(),
        )?;
        if self.question != rebuilt.question || self.facts != rebuilt.facts {
            return Err("requested fact identity does not match its question".into());
        }
        match self.method.as_str() {
            EXPLICIT_METHOD => Ok(()),
            DERIVED_METHOD if Self::derive(question)? == *self => Ok(()),
            _ => Err("unsupported or inconsistent requested-fact derivation".into()),
        }
    }
    pub fn id(&self) -> ManifestId {
        ManifestId::of_blob(&zeroize::Zeroizing::new(
            serde_json::to_vec(self).expect("requested facts serialization"),
        ))
    }
}
impl TypedPayload for RequestedFactSet {
    fn schema_ref() -> SchemaRef {
        SchemaRef::from_id(ManifestId::of_blob(br#"{"format":"mere-native","schema_id":"eidetic.RequestedFactSet/v1","body":{"version":1,"description":"Ordered exact question excerpts and stable requested-fact identities. No answer or source judgment."}}"#))
    }
}
impl Drop for RequestedFact {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.text.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_list_is_ordered_unicode_question_identity() {
        let q = "What are the café hours, the 🦉 habitat, and the ferry duration? Give one claim for each, in that order.";
        let s = RequestedFactSet::derive(q).unwrap();
        assert_eq!(
            s.facts.iter().map(|f| f.text.as_str()).collect::<Vec<_>>(),
            ["the café hours", "the 🦉 habitat", "the ferry duration"]
        );
        s.validate(q).unwrap();
        let mut wrong = s.clone();
        wrong.facts.swap(0, 1);
        assert!(wrong.validate(q).is_err());
        assert!(s.validate(&q.replace("hours", "owner")).is_err());
        let restored: RequestedFactSet =
            serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(s, restored);
    }
    #[test]
    fn ambiguous_requests_require_explicit_ranges_and_never_read_evidence() {
        for q in [
            "When does the museum open and the ferry depart?",
            "What are A, B?",
            "Who made it? What does it cost?",
            "What are A, B, and C?",
        ] {
            assert!(RequestedFactSet::derive(q).is_err(), "{q}");
        }
        let q = "A and B";
        let s = RequestedFactSet::from_spans(
            q,
            &[
                PassageSelector { start: 6, end: 7 },
                PassageSelector { start: 0, end: 1 },
            ],
        )
        .unwrap();
        s.validate(q).unwrap();
        assert_eq!(s.facts[0].text, "B");
        assert!(
            RequestedFactSet::from_spans(
                q,
                &[
                    PassageSelector { start: 0, end: 3 },
                    PassageSelector { start: 2, end: 4 }
                ]
            )
            .is_err()
        );
        let s =
            RequestedFactSet::derive("What is the museum designer? The sources may not name one.")
                .unwrap();
        assert_eq!(s.facts[0].text, "the museum designer");
    }
}
