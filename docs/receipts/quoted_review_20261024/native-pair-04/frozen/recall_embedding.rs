// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Bounded, provider-bound projections over the source owner's exact revisions.
//! The resident lends its existing sealed journal as a cache, never source truth.
use super::recall::*;
use eidetic::models::embedding::EmbeddingCorpusV1;
use esp::embed::{
    EmbeddingProvider,
    compatible::{CompatibleIndex, CompatibleSearch},
};
use std::sync::Arc;

pub trait RecallEmbeddingCache: Send + Sync {
    fn load(
        &self,
        envelope: eidetic::ManifestId,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, String>;
    fn save(&self, envelope: eidetic::ManifestId, bytes: &[u8]) -> Result<(), String>;
}
pub struct RecallEmbedding {
    provider: Arc<dyn EmbeddingProvider>,
    cache: Arc<dyn RecallEmbeddingCache>,
    settings: EmbeddingRecallSettings,
}
pub struct ProjectedEmbedding {
    pub search: CompatibleSearch<String, Arc<dyn EmbeddingProvider>>,
    pub evidence: EmbeddingRecallEvidence,
}
impl RecallEmbedding {
    pub fn new(
        provider: Arc<dyn EmbeddingProvider>,
        cache: Arc<dyn RecallEmbeddingCache>,
        settings: EmbeddingRecallSettings,
    ) -> Result<Self, String> {
        settings.validate()?;
        let space = provider
            .compatibility_space()
            .ok_or("embedding provider has no compatibility identity")?;
        space.validate()?;
        if provider.embed_sparse(&[]).is_none() && space.dimensions > 384 {
            return Err(
                "resident recall permits sparse vectors or at most 384 dense dimensions".into(),
            );
        }
        Ok(Self {
            provider,
            cache,
            settings,
        })
    }
    pub fn project(
        &self,
        corpus: EmbeddingCorpusV1,
        items: &[(String, &str)],
    ) -> Result<ProjectedEmbedding, String> {
        let expected = esp::embed::compatible::envelope(&self.provider, corpus.clone())?;
        let envelope = expected.id();
        let key_bytes = items
            .iter()
            .try_fold(0usize, |total, (key, _)| total.checked_add(key.len()))
            .ok_or("embedding key byte overflow")?;
        let dense_bytes = if self.provider.embed_sparse(&[]).is_none() {
            items
                .len()
                .checked_mul(expected.space.dimensions)
                .and_then(|n| n.checked_mul(std::mem::size_of::<f32>()))
                .ok_or("embedding vector byte overflow")?
        } else {
            0
        };
        if key_bytes
            .checked_add(dense_bytes)
            .ok_or("embedding projection byte overflow")?
            > self.settings.max_index_bytes
        {
            return Err("embedding index exceeds configured byte limit before embedding".into());
        }
        let existing = self.cache.load(envelope, self.settings.max_index_bytes)?;
        let reopened = existing.is_some();
        let (search, bytes) = if let Some(bytes) = existing {
            if bytes.len() > self.settings.max_index_bytes {
                return Err("embedding index exceeds configured byte limit".into());
            }
            let saved: CompatibleIndex<String> = serde_json::from_slice(&bytes)
                .map_err(|e| format!("embedding cache format: {e}"))?;
            let search = CompatibleSearch::reopen(self.provider.clone(), corpus, saved)?;
            (search, bytes)
        } else {
            let search = CompatibleSearch::build(self.provider.clone(), corpus, items)?;
            let bytes = serde_json::to_vec(&search.snapshot()).map_err(|e| e.to_string())?;
            (search, bytes)
        };
        let actual: std::collections::BTreeSet<_> = search.keys().into_iter().collect();
        let requested: std::collections::BTreeSet<_> =
            items.iter().map(|(k, _)| k.clone()).collect();
        if actual != requested || actual.len() != items.len() {
            return Err("embedding index passage identities differ".into());
        }
        if !search.is_sparse() && search.envelope().space.dimensions > 384 {
            return Err(
                "resident recall permits at most 384 dense dimensions in stored projections".into(),
            );
        }
        let vector_bytes = search.vector_bytes();
        if bytes.len() > self.settings.max_index_bytes
            || vector_bytes > self.settings.max_index_bytes
        {
            return Err("embedding index exceeds configured byte limit".into());
        }
        let index = eidetic::ManifestId::of_blob(&bytes);
        if !reopened {
            self.cache.save(envelope, &bytes)?;
        }
        Ok(ProjectedEmbedding {
            search,
            evidence: EmbeddingRecallEvidence {
                envelope,
                index,
                cache: if reopened { "reopened" } else { "built" }.into(),
                vector_bytes,
                persisted_bytes: bytes.len(),
                lexical_weight: self.settings.lexical_weight,
                vector_weight: self.settings.vector_weight,
                rrf_k: self.settings.rrf_k,
            },
        })
    }
}

// Cache references hold only content ids. Bodies and their complete envelopes
// use the resident's existing private evidence seal and never acquire sources.
impl<B: muniment::Backend + Clone + Send + Sync + 'static> RecallEmbeddingCache
    for super::durable::PersonaRunJournal<B>
{
    fn load(
        &self,
        envelope: eidetic::ManifestId,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, String> {
        self.load_embedding_projection(envelope, max_bytes)
    }
    fn save(&self, envelope: eidetic::ManifestId, bytes: &[u8]) -> Result<(), String> {
        let saved: CompatibleIndex<String> =
            serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        saved.envelope.validate()?;
        if saved.envelope.id() != envelope {
            return Err("embedding cache envelope differs".into());
        }
        self.save_embedding_projection(envelope, bytes)
    }
}

/// Called on the resident's owned blocking startup task. Resolve only already
/// admitted local model components, release the library lock before tensor work.
pub fn open_embedding_recall<B: eidetic::Store + Send + 'static>(
    settings: EmbeddingRecallSettings,
    library: Arc<tokio::sync::Mutex<B>>,
    cache: Arc<dyn RecallEmbeddingCache>,
) -> Result<RecallEmbedding, String> {
    settings.validate()?;
    let provider: Arc<dyn EmbeddingProvider> = match &settings.provider {
        EmbeddingRecallProvider::LexicalSparse {
            dimensions,
            token_ngram_orders,
        } => Arc::new(
            esp::embed::LexicalEmbeddingProvider::with_token_ngram_orders(
                *dimensions,
                token_ngram_orders,
            )
            .map_err(|e| e.to_string())?,
        ),
        EmbeddingRecallProvider::Library { model } => {
            let resolved = {
                let mut store = library.blocking_lock();
                let id = super::parse_model_row(model)?;
                pollster::block_on(eidetic::ModelLibrary::resolve_components(
                    &mut *store,
                    &mut eidetic::NoFetcher,
                    id,
                ))
                .map_err(|e| e.to_string())?
                .ok_or("embedding model is unavailable in the admitted local library")?
            };
            let dimensions = resolved
                .manifest
                .config
                .get("hidden_size")
                .and_then(|v| v.as_u64())
                .ok_or("embedding model has no declared hidden size")?;
            if dimensions == 0 || dimensions > 384 {
                return Err("resident recall permits at most 384 dense dimensions".into());
            }
            Arc::from(
                esp::embed::bert::from_bytes_cpu(
                    &resolved.components.config_bytes,
                    &resolved.components.tokenizer_bytes,
                    &resolved.components.weight_bytes,
                )
                .map_err(|e| e.to_string())?,
            )
        },
    };
    RecallEmbedding::new(provider, cache, settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eidetic::models::{embedding::*, recall::*};
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    #[derive(Default)]
    struct Cache(Mutex<std::collections::HashMap<eidetic::ManifestId, Vec<u8>>>);
    impl RecallEmbeddingCache for Cache {
        fn load(&self, id: eidetic::ManifestId, _: usize) -> Result<Option<Vec<u8>>, String> {
            Ok(self.0.lock().unwrap().get(&id).cloned())
        }
        fn save(&self, id: eidetic::ManifestId, bytes: &[u8]) -> Result<(), String> {
            self.0.lock().unwrap().insert(id, bytes.to_vec());
            Ok(())
        }
    }
    struct Corpus {
        sources: Mutex<Vec<RetainedSourceText>>,
        reads: AtomicUsize,
    }
    impl RecallCorpus for Corpus {
        fn snapshot(&self) -> Result<Vec<RetainedSourceText>, String> {
            Ok(self.sources.lock().unwrap().clone())
        }
        fn current_version(&self, source: &RecallSourceId) -> Result<Option<String>, String> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(self
                .sources
                .lock()
                .unwrap()
                .iter()
                .find(|s| &s.source == source)
                .map(|s| s.version.clone()))
        }
    }
    fn fixture() -> Arc<Corpus> {
        Arc::new(Corpus {
            sources: Mutex::new(
                [
                    ("a", "owner-a", "rust async runtime"),
                    ("b", "owner-b", "rust other namespace"),
                ]
                .into_iter()
                .map(|(id, owner, text)| RetainedSourceText {
                    source: RecallSourceId {
                        kind: RecallSourceKind::PageText,
                        owner: Some(owner.into()),
                        source: id.into(),
                        node: None,
                    },
                    version: eidetic::Hash::of(text.as_bytes()).to_string(),
                    title: None,
                    text: text.into(),
                })
                .collect(),
            ),
            reads: AtomicUsize::new(0),
        })
    }
    #[test]
    fn ranking_ties_and_answer_inputs_ignore_source_identity_permutations() {
        let texts = [
            "The orchard opening day is Monday.",
            "The orchard opening day is Friday.",
            "The harbor access code is blue.",
            "The saved listener note is warm.",
        ];
        let question = "What is the orchard opening day?";
        let mut expected = None;
        for a in 0..4 {
            for b in 0..4 {
                for c in 0..4 {
                    for d in 0..4 {
                        let ids = [a, b, c, d];
                        if ids.iter().collect::<std::collections::BTreeSet<_>>().len() != 4 {
                            continue;
                        }
                        let corpus = Arc::new(Corpus {
                            sources: Mutex::new(
                                texts
                                    .iter()
                                    .zip(ids)
                                    .map(|(text, id)| RetainedSourceText {
                                        source: RecallSourceId {
                                            kind: RecallSourceKind::PageText,
                                            owner: Some(format!("owner-{id}")),
                                            source: format!("source-{id}"),
                                            node: None,
                                        },
                                        version: "v1".into(),
                                        title: None,
                                        text: (*text).into(),
                                    })
                                    .collect(),
                            ),
                            reads: AtomicUsize::new(0),
                        });
                        let config = RecallConfig {
                            max_passages: 4,
                            ..Default::default()
                        };
                        let lexical = RecallIndex::build(corpus.clone(), config.clone()).unwrap();
                        let runtime = runtime(Arc::new(Cache::default()));
                        let fused =
                            RecallIndex::build_with_embedding(corpus, config, &runtime).unwrap();
                        let mut views = Vec::new();
                        for index in [&lexical, &fused] {
                            let ranks = index.rankings(question, &[]).unwrap();
                            let (mut evidence, _) = index.prepare(question, &[]).unwrap();
                            let by_id: std::collections::BTreeMap<_, _> = evidence
                                .passages
                                .iter()
                                .map(|p| (p.passage.id().to_string(), p.passage.text.clone()))
                                .collect();
                            let text_order = |keys: &[String]| {
                                keys.iter()
                                    .map(|key| by_id[key].clone())
                                    .collect::<Vec<_>>()
                            };
                            evidence.answer_format = AnswerFormat::ClaimSupportV1;
                            let (contract, prompt, context) =
                                super::super::answer_contract::prepare(question, &evidence)
                                    .unwrap();
                            evidence.answer_contract = Some(contract);
                            let grammar = super::super::answer_contract::grammar(
                                &evidence,
                                "distillery.quoted-review/v7",
                            )
                            .unwrap();
                            // Provenance still follows its actual source; only equal-score
                            // presentation must ignore arbitrary owner and source identities.
                            for p in &evidence.passages {
                                let slot = texts
                                    .iter()
                                    .position(|text| *text == p.passage.text)
                                    .unwrap();
                                assert_eq!(
                                    p.passage.source.source,
                                    format!("source-{}", ids[slot])
                                );
                            }
                            views.push((
                                text_order(&ranks.lexical),
                                text_order(&ranks.vector),
                                text_order(&ranks.fused),
                                prompt,
                                context,
                                grammar,
                            ));
                        }
                        if let Some(expected) = &expected {
                            assert_eq!(&views, expected, "source identity permutation {ids:?}");
                        } else {
                            expected = Some(views);
                        }
                    }
                }
            }
        }
    }
    fn options() -> EmbeddingRecallSettings {
        EmbeddingRecallSettings {
            provider: EmbeddingRecallProvider::LexicalSparse {
                dimensions: 384,
                token_ngram_orders: vec![1, 2],
            },
            max_index_bytes: 8192,
            lexical_weight: 1,
            vector_weight: 1,
            rrf_k: 60,
        }
    }
    fn runtime(cache: Arc<Cache>) -> RecallEmbedding {
        RecallEmbedding::new(
            Arc::new(
                esp::embed::LexicalEmbeddingProvider::with_token_ngram_orders(384, [1, 2]).unwrap(),
            ),
            cache,
            options(),
        )
        .unwrap()
    }
    #[test]
    fn matching_projection_reopens_scopes_before_fusion_and_observes_each_head_once() {
        let corpus = fixture();
        let cache = Arc::new(Cache::default());
        let runtime = runtime(cache);
        let config = RecallConfig {
            chunk_chars: 8,
            ..Default::default()
        };
        let first =
            RecallIndex::build_with_embedding(corpus.clone(), config.clone(), &runtime).unwrap();
        let source = corpus.sources.lock().unwrap()[0].source.clone();
        corpus.reads.store(0, Ordering::SeqCst);
        let (e, _) = first.prepare("rust", &[source.clone()]).unwrap();
        assert_eq!(corpus.reads.load(Ordering::SeqCst), 1);
        assert!(e.passages.iter().all(|p| p.passage.source == source));
        assert_eq!(e.embedding.as_ref().unwrap().cache, "built");
        let second = RecallIndex::build_with_embedding(corpus.clone(), config, &runtime).unwrap();
        let (reopened, _) = second.prepare("rust", &[source]).unwrap();
        assert_eq!(reopened.embedding.as_ref().unwrap().cache, "reopened");
        assert_eq!(
            reopened.embedding.as_ref().unwrap().index,
            e.embedding.as_ref().unwrap().index
        );
        assert_eq!(reopened.passages, e.passages);
        corpus.sources.lock().unwrap()[0].version = "changed".into();
        let changed_source = corpus.sources.lock().unwrap()[0].source.clone();
        let (stale, _) = second.prepare("rust", &[changed_source]).unwrap();
        assert!(stale.passages.is_empty());
        assert_eq!(stale.stale.len(), 1);
    }
    #[test]
    fn private_cache_failure_and_byte_limits_do_not_silently_fall_back_to_lexical() {
        let corpus = fixture();
        let cache = Arc::new(Cache::default());
        let runtime = runtime(cache.clone());
        let first = RecallIndex::build_with_embedding(corpus.clone(), Default::default(), &runtime)
            .unwrap();
        let e = first.prepare("rust", &[]).unwrap().0.embedding.unwrap();
        let mut entries = cache.0.lock().unwrap();
        let bytes = entries.get_mut(&e.envelope).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        value["envelope"]["space"]["producer"]["revision"] = "foreign/v1".into();
        *bytes = serde_json::to_vec(&value).unwrap();
        drop(entries);
        assert!(
            RecallIndex::build_with_embedding(corpus.clone(), Default::default(), &runtime)
                .err()
                .unwrap()
                .contains("foreign embedding space")
        );
        let mut small = options();
        small.max_index_bytes = 1;
        let limited = RecallEmbedding::new(
            Arc::new(esp::embed::LexicalEmbeddingProvider::new(384).unwrap()),
            Arc::new(Cache::default()),
            small,
        )
        .unwrap();
        assert!(
            RecallIndex::build_with_embedding(corpus, Default::default(), &limited)
                .err()
                .unwrap()
                .contains("before embedding")
        );
    }
    #[test]
    fn a_sparse_provider_does_not_admit_an_oversized_stored_dense_representation() {
        let provider = Arc::new(esp::embed::LexicalEmbeddingProvider::new(512).unwrap());
        let corpus = EmbeddingCorpusV1 {
            chunker: "test/v1".into(),
            sources: vec![],
        };
        let envelope = esp::embed::compatible::envelope(&provider, corpus.clone()).unwrap();
        let mut index = esp::embed::VectorIndex::new(512, esp::embed::SimilarityMetric::Cosine);
        index.insert("p".to_string(), vec![1.0; 512]).unwrap();
        let saved = CompatibleIndex {
            envelope: envelope.clone(),
            vectors: esp::embed::compatible::CompatibleVectors::Dense(index),
        };
        let cache = Arc::new(Cache::default());
        cache
            .save(envelope.id(), &serde_json::to_vec(&saved).unwrap())
            .unwrap();
        let runtime = RecallEmbedding::new(provider, cache, options()).unwrap();
        assert!(
            runtime
                .project(corpus, &[("p".into(), "text")])
                .err()
                .unwrap()
                .contains("384 dense dimensions in stored projections")
        );
    }
}
