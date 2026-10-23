// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Reproducible claim-to-passage evidence. These are structural and verbatim
//! checks, never an entailment judgment or an owner assertion.
use super::recall::{InferencePassage, PassageSelector};
use super::requested::{RequestedFact, RequestedFactSet};
use crate::{ManifestId, SchemaRef, TypedPayload};
use serde::{Deserialize, Serialize};

pub const SUPPORT_METHOD: &str = "eidetic.claim-support/v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Supported,
    Insufficient,
    Conflicting,
}

/// Model-authored interpretation. An insufficient claim does not prove absence
/// from the corpus; a conflicting claim does not establish a true contradiction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerDocument {
    pub claims: Vec<AnswerClaim>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerClaim {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact: Option<RequestedFact>,
    pub status: ClaimStatus,
    pub text: String,
    pub evidence: Vec<ClaimEvidence>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimEvidence {
    pub handle: u32,
    pub quote: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PassageSupport {
    pub handle: u32,
    pub quote: String,
    pub passage: Option<ManifestId>,
    /// Absolute Unicode scalar range in the source's canonical text stream.
    pub selector: Option<PassageSelector>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessedClaim {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact: Option<RequestedFact>,
    pub status: ClaimStatus,
    pub text: String,
    pub evidence: Vec<PassageSupport>,
    pub issues: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticReview {
    Unreviewed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerSupport {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_facts: Option<ManifestId>,
    pub method: String,
    pub raw_output: ManifestId,
    pub semantic_review: SemanticReview,
    pub claims: Vec<AssessedClaim>,
    pub error: Option<String>,
}
impl AnswerSupport {
    /// Strict whole-document parse; no extraction of a plausible JSON fragment
    /// from otherwise unassessed output. Code fences must enclose the whole body.
    pub fn assess(raw: &str, passages: &[(u32, &InferencePassage)]) -> Self {
        Self::assess_requested(raw, passages, None)
    }
    /// Validate exact count and ordered fact identities before admitting claims.
    /// The caller validates the fact set against the retained question first.
    pub fn assess_requested(
        raw: &str,
        passages: &[(u32, &InferencePassage)],
        facts: Option<&RequestedFactSet>,
    ) -> Self {
        let mut result = Self {
            requested_facts: facts.map(|s| s.id()),
            method: SUPPORT_METHOD.into(),
            raw_output: ManifestId::of_blob(raw.as_bytes()),
            semantic_review: SemanticReview::Unreviewed,
            claims: vec![],
            error: None,
        };
        if facts.is_some_and(|s| s.validate_identity().is_err()) {
            result.error = Some("Invalid requested-fact set.".into());
            return result;
        }
        let body = raw.trim();
        let body = body
            .strip_prefix("```json")
            .or_else(|| body.strip_prefix("```"))
            .and_then(|body| body.trim().strip_suffix("```"))
            .unwrap_or(body)
            .trim();
        let document: AnswerDocument = match serde_json::from_str(body) {
            Ok(document) => document,
            Err(_) => {
                result.error = Some("The model did not return a complete claim-support document. Raw output is retained.".into());
                return result;
            }
        };
        if document.claims.is_empty() || document.claims.len() > 16 {
            result.error = Some("An answer must contain between one and sixteen claims.".into());
            return result;
        }
        if facts.is_some_and(|s| {
            s.facts.len() != document.claims.len()
                || document
                    .claims
                    .iter()
                    .zip(&s.facts)
                    .any(|(c, f)| c.fact.as_ref() != Some(f))
        }) || (facts.is_none() && document.claims.iter().any(|c| c.fact.is_some()))
        {
            result.error=Some("Answer claim count or ordered requested-fact identities do not match the retained request.".into());
            return result;
        }
        for claim in &document.claims {
            let mut assessed = AssessedClaim {
                fact: claim.fact.clone(),
                status: claim.status,
                text: claim.text.clone(),
                evidence: vec![],
                issues: vec![],
            };
            if claim.text.trim().is_empty()
                || claim.text.chars().count() > 4096
                || claim.evidence.len() > 8
            {
                result.error = Some("Claim text or evidence exceeds answer bounds.".into());
                result.claims.clear();
                return result;
            }
            let mut sources = std::collections::BTreeSet::new();
            for link in &claim.evidence {
                let mut support = PassageSupport {
                    handle: link.handle,
                    quote: link.quote.clone(),
                    passage: None,
                    selector: None,
                    error: None,
                };
                if let Some((_, passage)) =
                    passages.iter().find(|(handle, _)| *handle == link.handle)
                {
                    support.passage = Some(passage.id());
                    if passage.selector.end.checked_sub(passage.selector.start)
                        != Some(passage.text.chars().count())
                    {
                        support.error =
                            Some("Supplied passage source range is inconsistent.".into());
                    } else if link.quote.trim().chars().count() < 16 {
                        support.error =
                            Some("Evidence quotation is shorter than sixteen characters.".into());
                    } else if let Some(offset) = passage.text.find(&link.quote) {
                        let start = passage.selector.start + passage.text[..offset].chars().count();
                        support.selector = Some(PassageSelector {
                            start,
                            end: start + link.quote.chars().count(),
                        });
                        sources.insert(passage.source.clone());
                    } else {
                        support.error = Some(
                            "Quotation does not occur verbatim in the supplied passage.".into(),
                        );
                    }
                } else {
                    support.error =
                        Some("Evidence handle does not name a supplied passage.".into());
                }
                if let Some(error) = &support.error {
                    assessed.issues.push(error.clone());
                }
                assessed.evidence.push(support);
            }
            match claim.status {
                ClaimStatus::Supported if sources.is_empty() => assessed.issues.push("Supported claim has no checked quotation.".into()),
                ClaimStatus::Conflicting if sources.len() < 2 => assessed.issues.push("Conflicting claim needs checked quotations from at least two distinct sources.".into()),
                ClaimStatus::Insufficient if !claim.evidence.is_empty() => assessed.issues.push("An insufficient-evidence statement must not assert supporting passages.".into()),
                _ => {}
            }
            if result
                .claims
                .iter()
                .any(|previous| previous.text.trim() == assessed.text.trim())
            {
                assessed
                    .issues
                    .push("The model repeated an earlier claim.".into());
            }
            result.claims.push(assessed);
        }
        result
    }
    pub fn id(&self) -> ManifestId {
        let bytes =
            zeroize::Zeroizing::new(serde_json::to_vec(self).expect("support serialization"));
        ManifestId::of_blob(&bytes)
    }
}
impl TypedPayload for AnswerSupport {
    fn schema_ref() -> SchemaRef {
        SchemaRef::from_id(ManifestId::of_blob(br#"{"format":"mere-native","schema_id":"eidetic.AnswerSupport/v1","body":{"version":1,"description":"Model-declared claims and reproducible exact quotation links. Semantic support remains unreviewed."}}"#))
    }
}
impl Drop for AnswerClaim {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.text.zeroize();
    }
}
impl Drop for ClaimEvidence {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.quote.zeroize();
    }
}
impl Drop for AssessedClaim {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.text.zeroize();
    }
}
impl Drop for PassageSupport {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.quote.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::recall::{RecallSourceId, RecallSourceKind};
    fn passage(source: &str, text: &str) -> InferencePassage {
        InferencePassage {
            source: RecallSourceId {
                kind: RecallSourceKind::Clip,
                owner: Some("owner".into()),
                source: source.into(),
                node: None,
            },
            version: "v1".into(),
            text_hash: crate::Hash::of(text.as_bytes()),
            selector: PassageSelector {
                start: 7,
                end: 7 + text.chars().count(),
            },
            chunker: "test/v1".into(),
            text: text.into(),
        }
    }
    #[test]
    fn requested_facts_refuse_missing_extra_and_reordered_claims() {
        let q = "What are the opening time and the ticket cost? Give one claim for each, in that order.";
        let facts = RequestedFactSet::derive(q).unwrap();
        let claims:Vec<_>=facts.facts.iter().map(|f|serde_json::json!({"fact":f,"status":"insufficient","text":format!("No supplied {}.",f.text),"evidence":[]})).collect();
        let raw = serde_json::json!({"claims":claims}).to_string();
        let support = AnswerSupport::assess_requested(&raw, &[], Some(&facts));
        assert!(support.error.is_none());
        assert_eq!(support.requested_facts, Some(facts.id()));
        assert_eq!(support.claims[0].fact.as_ref(), Some(&facts.facts[0]));
        assert!(AnswerSupport::assess(&raw, &[]).error.is_some());
        for bad in [
            vec![claims[0].clone()],
            vec![claims[1].clone(), claims[0].clone()],
            vec![claims[0].clone(), claims[1].clone(), claims[1].clone()],
        ] {
            let s = AnswerSupport::assess_requested(
                &serde_json::json!({"claims":bad}).to_string(),
                &[],
                Some(&facts),
            );
            assert!(s.error.is_some());
            assert!(s.claims.is_empty());
        }
    }

    #[test]
    fn exact_links_use_unicode_offsets_and_do_not_certify_entailment() {
        let p = passage("a", "🦉 élan: the launch is on Thursday.");
        let raw = r#"{"claims":[{"status":"supported","text":"An unrelated interpretation","evidence":[{"handle":1,"quote":"the launch is on Thursday."}]}]}"#;
        let result = AnswerSupport::assess(raw, &[(1, &p)]);
        assert_eq!(result.semantic_review, SemanticReview::Unreviewed);
        assert!(result.claims[0].issues.is_empty());
        assert_eq!(
            result.claims[0].evidence[0]
                .selector
                .as_ref()
                .unwrap()
                .start,
            15
        );
        assert_eq!(result.raw_output, ManifestId::of_blob(raw.as_bytes()));
    }
    #[test]
    fn conflict_requires_distinct_sources_and_keeps_both_versions() {
        let a = passage("a", "The launch is on Thursday.");
        let b = passage("b", "The launch is on Saturday.");
        let raw = r#"{"claims":[{"status":"conflicting","text":"Dates disagree","evidence":[{"handle":1,"quote":"The launch is on Thursday."},{"handle":2,"quote":"The launch is on Saturday."}]}]}"#;
        let result = AnswerSupport::assess(raw, &[(1, &a), (2, &b)]);
        assert!(result.claims[0].issues.is_empty());
        assert_ne!(
            result.claims[0].evidence[0].passage,
            result.claims[0].evidence[1].passage
        );
        assert!(
            !AnswerSupport::assess(raw, &[(1, &a), (2, &a)]).claims[0]
                .issues
                .is_empty()
        );
    }
    #[test]
    fn malformed_unknown_and_invented_support_fail_closed() {
        let p = passage("a", "The launch is on Thursday.");
        for raw in [
            r#"prefix {"claims":[]}"#,
            r#"{"claims":[]}"#,
            r#"{"claims":["#,
        ] {
            assert!(AnswerSupport::assess(raw, &[]).error.is_some());
        }
        let raw = r#"{"claims":[{"status":"supported","text":"Saturday","evidence":[{"handle":99,"quote":"The launch is on Saturday."},{"handle":1,"quote":"The launch is on Saturday."}]}]}"#;
        let result = AnswerSupport::assess(raw, &[(1, &p)]);
        assert_eq!(result.claims[0].issues.len(), 3);
        assert!(
            result.claims[0]
                .evidence
                .iter()
                .all(|e| e.selector.is_none())
        );
    }
    #[test]
    fn chunk_quotes_preserve_the_full_source_hash_and_absolute_range() {
        let mut p = passage("chunk", "The launch is on Thursday.");
        p.text_hash = crate::Hash::of(b"A larger canonical source containing the retained chunk.");
        let raw = r#"{"claims":[{"status":"supported","text":"Thursday launch","evidence":[{"handle":1,"quote":"The launch is on Thursday."}]}]}"#;
        let result = AnswerSupport::assess(raw, &[(1, &p)]);
        assert!(result.claims[0].issues.is_empty());
        assert_eq!(
            result.claims[0].evidence[0]
                .selector
                .as_ref()
                .unwrap()
                .start,
            7
        );
        assert_eq!(result.claims[0].evidence[0].passage, Some(p.id()));
    }
    #[test]
    fn corrupt_passage_ranges_and_repeated_claims_are_reported() {
        let mut p = passage("a", "The launch is on Thursday.");
        p.selector.start = usize::MAX;
        let claim = r#"{"status":"supported","text":"Thursday launch","evidence":[{"handle":1,"quote":"The launch is on Thursday."}]}"#;
        let raw = format!("{{\"claims\":[{claim},{claim}]}}");
        let result = AnswerSupport::assess(&raw, &[(1, &p)]);
        assert!(result.claims[0].evidence[0].selector.is_none());
        assert!(
            result.claims[1]
                .issues
                .iter()
                .any(|i| i.contains("repeated"))
        );
    }
    #[test]
    fn insufficient_is_a_retained_model_judgment_not_an_absence_proof() {
        let raw = r#"{"claims":[{"status":"insufficient","text":"No budget was supplied.","evidence":[]}]}"#;
        let result = AnswerSupport::assess(raw, &[]);
        assert!(result.claims[0].issues.is_empty());
        let reopened: AnswerSupport =
            serde_json::from_slice(&serde_json::to_vec(&result).unwrap()).unwrap();
        assert_eq!(reopened, result);
        assert_eq!(reopened.id(), result.id());
    }
}
