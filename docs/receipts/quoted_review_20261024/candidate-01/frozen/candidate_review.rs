// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Constrain a claim to its model-authored, complete candidate review. This
//! enforces relationships between decisions, not their semantic correctness.
use super::recall::RecallEvidence;
use eidetic::models::requested::RequestedFactSet;
use esp::infer::OutputGrammar as G;

fn lit(value: impl Into<String>) -> G {
    G::Literal {
        value: value.into(),
    }
}
fn seq(items: Vec<G>) -> G {
    G::Sequence { items }
}
fn choice(alternatives: Vec<G>) -> G {
    G::Choice { alternatives }
}
fn wire(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap()
}

pub(super) fn grammar(
    e: &RecallEvidence,
    facts: &RequestedFactSet,
    quoted: bool,
) -> Result<G, String> {
    facts.validate_identity()?;
    if e.passages.len() > 8
        || e.passages
            .iter()
            .enumerate()
            .any(|(i, p)| e.passages[..i].iter().any(|q| p.handle == q.handle))
    {
        return Err("candidate review requires at most eight unique supplied handles".into());
    }
    let candidates: Vec<_> = e
        .passages
        .iter()
        .map(|p| {
            lit(wire(
                &serde_json::json!({"handle":p.handle,"quote":p.passage.text}),
            ))
        })
        .collect();
    if quoted {
        for (i, a) in e.passages.iter().enumerate() {
            if wire(&a.passage.text).len() > 386 {
                return Err("extractive answers require candidate text within the 384-byte claim bound; use shorter retained passages".into());
            }
            for b in &e.passages[i + 1..] {
                if a.passage.source != b.passage.source
                    && wire(&conflict_text(&a.passage.text, &b.passage.text)).len() > 386
                {
                    return Err("extractive conflict text exceeds the 384-byte claim bound; use shorter retained passages".into());
                }
            }
        }
    }
    fn extractive(agreement: &str, status: &str, text: &str, links: Vec<G>) -> G {
        seq(vec![
            lit(format!(
                "],\"agreement\":\"{agreement}\"}},\"status\":\"{status}\",\"text\":{},\"evidence\":[",
                wire(&text)
            )),
            seq(links),
            lit("]}"),
        ])
    }
    fn decisions(
        e: &RecallEvidence,
        cs: &[G],
        at: usize,
        chosen: &mut Vec<usize>,
        fact: &str,
        quoted: bool,
    ) -> G {
        if at < e.passages.len() {
            let mut branches = vec![];
            for answers in [false, true] {
                if answers {
                    chosen.push(at);
                }
                let next = decisions(e, cs, at + 1, chosen, fact, quoted);
                if answers {
                    chosen.pop();
                }
                branches.push(seq(vec![
                    lit(format!(
                        "{}{{\"handle\":{},{}\"answers\":{answers}}}",
                        if at == 0 { "" } else { "," },
                        e.passages[at].handle,
                        if quoted {
                            format!(
                                "\"requested\":{},\"quote\":{},",
                                wire(&fact),
                                wire(&e.passages[at].passage.text)
                            )
                        } else {
                            String::new()
                        },
                    )),
                    next,
                ]));
            }
            return choice(branches);
        }
        if quoted {
            if chosen.is_empty() {
                return extractive(
                    "none",
                    "insufficient",
                    &format!("The supplied evidence does not state {fact}."),
                    vec![],
                );
            }
            let agreement = if chosen.len() == 1 { "single" } else { "same" };
            let supported = choice(
                chosen
                    .iter()
                    .map(|i| {
                        extractive(
                            agreement,
                            "supported",
                            &e.passages[*i].passage.text,
                            vec![cs[*i].clone()],
                        )
                    })
                    .collect(),
            );
            let mut alternatives = vec![supported];
            for (at, i) in chosen.iter().enumerate() {
                for j in &chosen[at + 1..] {
                    if e.passages[*i].passage.source != e.passages[*j].passage.source {
                        alternatives.push(extractive(
                            "different",
                            "conflicting",
                            &conflict_text(
                                &e.passages[*i].passage.text,
                                &e.passages[*j].passage.text,
                            ),
                            vec![cs[*i].clone(), lit(","), cs[*j].clone()],
                        ));
                    }
                }
            }
            return choice(alternatives);
        }
        let body = |agreement: &str, status: &str, links: G| {
            seq(vec![
                lit(format!(
                    "],\"agreement\":\"{agreement}\"}},\"status\":\"{status}\",\"text\":"
                )),
                if status == "insufficient" {
                    lit(wire(&format!(
                        "The supplied evidence does not state {fact}."
                    )))
                } else {
                    G::JsonString { max_bytes: 384 }
                },
                lit(",\"evidence\":["),
                links,
                lit("]}"),
            ])
        };
        if chosen.is_empty() {
            return body("none", "insufficient", lit(""));
        }
        let supported = body(
            if chosen.len() == 1 { "single" } else { "same" },
            "supported",
            choice(chosen.iter().map(|i| cs[*i].clone()).collect()),
        );
        let mut pairs = vec![];
        for (a, i) in chosen.iter().enumerate() {
            for j in &chosen[a + 1..] {
                if e.passages[*i].passage.source != e.passages[*j].passage.source {
                    pairs.push(seq(vec![cs[*i].clone(), lit(","), cs[*j].clone()]));
                }
            }
        }
        if pairs.is_empty() {
            supported
        } else {
            choice(vec![
                supported,
                body("different", "conflicting", choice(pairs)),
            ])
        }
    }
    let mut items = vec![lit("{\"claims\":[")];
    for (i, fact) in facts.facts.iter().enumerate() {
        if wire(&format!(
            "The supplied evidence does not state {}.",
            fact.text
        ))
        .len()
            > 386
        {
            return Err("requested fact is too long for a bounded insufficiency statement".into());
        }
        if i > 0 {
            items.push(lit(","));
        }
        items.push(seq(vec![
            lit(format!(
                "{{\"fact\":{},\"review\":{{\"candidates\":[",
                wire(fact)
            )),
            decisions(e, &candidates, 0, &mut vec![], &fact.text, quoted),
        ]));
    }
    items.push(lit("]}"));
    let grammar = seq(items);
    // Match the decoder's resource envelope before admitting a retained task.
    // The explicit review tree is exponential but bounded; never truncate it.
    fn size(g: &G, depth: usize, nodes: &mut usize, bytes: &mut usize) -> Result<(), String> {
        if depth > 24 || *nodes > 100_000 || *bytes > 262_144 {
            return Err("candidate review exceeds the constrained grammar resource bound".into());
        }
        *nodes += 1;
        match g {
            G::Literal { value } => {
                *bytes += value.len();
                *nodes += value.len();
            },
            G::Sequence { items } => {
                for item in items {
                    size(item, depth + 1, nodes, bytes)?;
                }
            },
            G::Choice { alternatives } => {
                for item in alternatives {
                    size(item, depth + 1, nodes, bytes)?;
                }
            },
            G::JsonString { .. } => {},
        }
        Ok(())
    }
    let (mut nodes, mut bytes) = (0, 0);
    size(&grammar, 0, &mut nodes, &mut bytes)?;
    if nodes > 100_000 || bytes > 262_144 {
        return Err("candidate review exceeds the constrained grammar resource bound".into());
    }
    Ok(grammar)
}

pub(super) fn conflict_text(a: &str, b: &str) -> String {
    format!("The supplied sources disagree: {a} / {b}")
}
