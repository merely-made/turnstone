// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Live graph assertion writes and the existing session snapshot boundary.
use super::App;
use crate::research_record_graph::{self, GraphBinding};
use crate::research_records::Binding;
use mere::kernel::graph::Author;

impl App {
    pub(super) fn structured_check_assertion_author(
        &self,
        assertion: &serde_json::Value,
    ) -> Result<(), String> {
        let current = self.structured_author_identity()?.asserter_iri();
        if assertion
            .get("provenance_iri")
            .and_then(|value| value.as_str())
            != Some(current.as_str())
        {
            return Err("Profile identity changed before graph save".into());
        }
        Ok(())
    }
    pub(super) fn structured_author_identity(&self) -> Result<Author, String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or("Unlock the profile to author a property")?;
        let key = identity::IdentityProvider::master_public_key(identity.as_ref()).to_bytes();
        let encoded: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(Author::person(format!("personae:{encoded}")))
    }
    pub(super) fn structured_attach_binding(
        &mut self,
        graph_id: crate::panes::GraphId,
        entity: uuid::Uuid,
        manifest: eidetic::ManifestId,
        result_binding: bool,
        expected_author: &str,
    ) -> Result<(Binding, serde_json::Value), String> {
        let author = self.structured_author_identity()?;
        if author.asserter_iri() != expected_author {
            return Err("Profile identity changed before property commit".into());
        }
        let (work, bytes) = research_record_graph::attachment_reservation(
            result_binding,
            expected_author,
            manifest,
        );
        self.structured_scope_reserved(work, bytes)?;
        self.journal
            .lock()
            .map_err(|_| "Graph journal unavailable")?
            .execution_error()
            .map_or(Ok(()), |e| Err(e.to_string()))?;
        let directory = self.session_dir();
        let guard = crate::host_journal::AuthorRestore::enter(self.journal.clone(), author.clone());
        let runtime = self
            .graph_runtimes
            .get_mut(graph_id)
            .ok_or("Property graph unavailable")?;
        if runtime.session != Some(self.session_id)
            || !runtime.persistence.owns_directory(&directory)
            || runtime.canvas.graph().resource(entity).is_none()
        {
            return Err("Property graph or writer lease changed".into());
        }
        let mut outcome: Result<GraphBinding, String> =
            Err("Property graph mutation did not run".into());
        runtime.canvas.ingest_graph(|graph| {
            outcome = graph.write_as(author, |graph| {
                if result_binding {
                    research_record_graph::attach_result(graph, entity, manifest)
                } else {
                    research_record_graph::attach(graph, entity, manifest)
                }
            });
            outcome.is_ok()
        });
        let binding = outcome?;
        drop(guard);
        self.journal
            .lock()
            .map_err(|_| "Graph journal unavailable")?
            .execution_error()
            .map_or(Ok(()), |e| Err(e.to_string()))?;
        let assertion = research_record_graph::assertion_for(runtime.canvas.graph(), &binding)?;
        Ok((
            Binding {
                graph: graph_id,
                entity: binding.entity,
                statement_id: binding.statement_id,
                manifest: binding.manifest,
            },
            assertion,
        ))
    }
    pub(super) fn structured_save_graph(
        &self,
        graph_id: crate::panes::GraphId,
    ) -> Result<(), String> {
        let directory = self.session_dir();
        let runtime = self
            .graph_runtimes
            .get(graph_id)
            .ok_or("Property graph unavailable")?;
        if runtime.session != Some(self.session_id)
            || !runtime.persistence.owns_directory(&directory)
        {
            return Err("Property writer lease changed".into());
        }
        runtime
            .persistence
            .save_graph_with(
                &directory,
                runtime.canvas.graph(),
                || pandect::save_node_facets(&directory, runtime.canvas.graph().facets()),
                |placement| {
                    crate::session_graph_file::save_profiled(
                        &directory.join(pandect::session_graph_store::GRAPH_FILE),
                        runtime.canvas.graph(),
                        placement,
                    )
                },
            )
            .map_err(|e| format!("Property snapshot save failed; recovery is required: {e}"))
    }
    pub(super) fn structured_assertion_matches(
        &self,
        binding: &Binding,
        assertion: &serde_json::Value,
        result: bool,
    ) -> Result<(), String> {
        let runtime = self
            .graph_runtimes
            .get(binding.graph)
            .ok_or("Property graph unavailable")?;
        let graph = runtime.canvas.graph();
        let candidates = if result {
            research_record_graph::result_bindings(graph)?
        } else {
            research_record_graph::bindings(graph, false)?
        };
        let held = candidates
            .into_iter()
            .find(|held| {
                held.entity == binding.entity
                    && held.statement_id == binding.statement_id
                    && held.manifest == binding.manifest
            })
            .ok_or("Property assertion changed before save")?;
        if research_record_graph::assertion_for(graph, &held)? != *assertion {
            return Err("Property assertion changed before save".into());
        }
        Ok(())
    }
    pub(super) fn structured_commit_withdrawal(&mut self, binding: &Binding) -> Result<(), String> {
        let author = self.structured_author_identity()?;
        self.journal
            .lock()
            .map_err(|_| "Graph journal unavailable")?
            .execution_error()
            .map_or(Ok(()), |error| Err(error.to_string()))?;
        let directory = self.session_dir();
        let guard = crate::host_journal::AuthorRestore::enter(self.journal.clone(), author.clone());
        let runtime = self
            .graph_runtimes
            .get_mut(binding.graph)
            .ok_or("Property graph unavailable")?;
        if runtime.session != Some(self.session_id)
            || !runtime.persistence.owns_directory(&directory)
        {
            return Err("Property graph or writer lease changed".into());
        }
        let mut outcome = Err("Property withdrawal did not run".to_owned());
        runtime.canvas.ingest_graph(|graph| {
            outcome = graph.write_as(author, |graph| {
                research_record_graph::withdraw_owned(graph, &binding.statement_id)
            });
            outcome.as_ref().copied().unwrap_or(false)
        });
        let changed = outcome?;
        drop(guard);
        self.journal
            .lock()
            .map_err(|_| "Graph journal unavailable")?
            .execution_error()
            .map_or(Ok(()), |error| Err(error.to_string()))?;
        if !changed {
            return Err("Property was already withdrawn".into());
        }
        runtime
            .persistence
            .save_graph_with(
                &directory,
                runtime.canvas.graph(),
                || pandect::save_node_facets(&directory, runtime.canvas.graph().facets()),
                |placement| {
                    crate::session_graph_file::save_profiled(
                        &directory.join(pandect::session_graph_store::GRAPH_FILE),
                        runtime.canvas.graph(),
                        placement,
                    )
                },
            )
            .map_err(|e| format!("Withdrawal snapshot save failed; recovery is required: {e}"))?;
        Ok(())
    }
}

impl App {
    pub(super) fn structured_recover_assertion(
        &mut self,
        item: &crate::trail_memory::record_actor::RecoveryItem,
    ) -> Result<(Binding, serde_json::Value, bool), String> {
        use crate::trail_memory::record_actor::RecoveryItem;
        let author = self.structured_author_identity()?;
        let author_iri = author.asserter_iri();
        let (session, graph_id, entity, manifest, statement, assertion, result_kind, withdrawing) =
            match item {
                RecoveryItem::Property(receipt) => {
                    if receipt.request.record.author != author_iri {
                        return Err("Pending property belongs to another identity".into());
                    }
                    (
                        receipt.request.session,
                        crate::panes::GraphId(receipt.request.graph),
                        receipt
                            .request
                            .record
                            .entity
                            .parse()
                            .map_err(|_| "Invalid recovery entity")?,
                        receipt
                            .manifest
                            .ok_or("Pending property manifest unavailable")?,
                        receipt.statement_id.clone(),
                        receipt.assertion.clone(),
                        false,
                        receipt.withdrawal.as_ref().is_some_and(|w| !w.saved),
                    )
                },
                RecoveryItem::Result(receipt) => (
                    receipt.request.session,
                    crate::panes::GraphId(receipt.request.graph),
                    receipt.request.entity,
                    receipt.request.manifest,
                    receipt.statement_id.clone(),
                    receipt.assertion.clone(),
                    true,
                    false,
                ),
            };
        if session != *self.session_id.as_uuid() {
            return Err("Pending save belongs to another session".into());
        }
        let missing = statement.as_ref().is_some_and(|id| {
            self.graph_runtimes
                .get(graph_id)
                .is_none_or(|runtime| runtime.canvas.graph().find_assertion(id).is_none())
        });
        if missing {
            let (work, bytes) =
                research_record_graph::attachment_reservation(result_kind, &author_iri, manifest);
            self.structured_scope_reserved(work, bytes)?;
        }
        let directory = self.session_dir();
        let settings = self.research_search.preferences.records.clone();
        let guard = crate::host_journal::AuthorRestore::enter(self.journal.clone(), author.clone());
        let runtime = self
            .graph_runtimes
            .get_mut(graph_id)
            .ok_or("Pending graph unavailable")?;
        if runtime.session != Some(self.session_id)
            || !runtime.persistence.owns_directory(&directory)
            || runtime.canvas.graph().resource(entity).is_none()
        {
            return Err("Pending graph or writer lease unavailable".into());
        }
        let mut budget = research_record_graph::Budget::new(&settings);
        let candidates = if result_kind {
            research_record_graph::result_bindings_limited(runtime.canvas.graph(), &mut budget)?
        } else {
            research_record_graph::bindings_limited(runtime.canvas.graph(), true, &mut budget)?
        };
        let mut same = candidates
            .into_iter()
            .filter(|candidate| candidate.entity == entity && candidate.manifest == manifest);
        let existing = same.next();
        if same.next().is_some() {
            return Err("Pending graph binding is ambiguous".into());
        }
        let (binding, assertion, withdrawn) = if let Some(existing) = existing {
            if existing.author != author_iri
                || statement
                    .as_ref()
                    .is_some_and(|id| id != &existing.statement_id)
            {
                return Err("Pending graph binding differs from receipt".into());
            }
            let actual = research_record_graph::assertion_for(runtime.canvas.graph(), &existing)?;
            let recorded = assertion.unwrap_or_else(|| actual.clone());
            if withdrawing && existing.withdrawn {
                let mut a = actual.clone();
                let mut b = recorded.clone();
                if let Some(object) = a.as_object_mut() {
                    object.remove("withdrawn");
                }
                if let Some(object) = b.as_object_mut() {
                    object.remove("withdrawn");
                }
                if a != b {
                    return Err("Withdrawn assertion differs from its receipt".into());
                }
            } else if actual != recorded || existing.withdrawn {
                return Err("Pending assertion differs from its receipt".into());
            }
            let binding = Binding {
                graph: graph_id,
                entity,
                statement_id: existing.statement_id,
                manifest,
            };
            (binding, recorded, existing.withdrawn)
        } else {
            let statement_id =
                statement.ok_or("This unbound deposit has no durable assertion; set it aside")?;
            let assertion = assertion.ok_or("Pending assertion content unavailable")?;
            let expected = GraphBinding {
                entity,
                statement_id: statement_id.clone(),
                manifest,
                withdrawn: false,
                author: author_iri,
            };
            let mut outcome = Err("Recovery graph mutation did not run".to_owned());
            runtime.canvas.ingest_graph(|graph| {
                outcome = graph.write_as(author, |graph| {
                    research_record_graph::restore(
                        graph,
                        &expected,
                        &assertion,
                        result_kind,
                        &settings,
                    )
                });
                outcome.is_ok()
            });
            outcome?;
            (
                Binding {
                    graph: graph_id,
                    entity,
                    statement_id,
                    manifest,
                },
                assertion,
                false,
            )
        };
        drop(guard);
        Ok((binding, assertion, withdrawn))
    }
}
