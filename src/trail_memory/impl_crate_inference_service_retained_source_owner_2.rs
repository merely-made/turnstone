// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl crate::inference_service::RetainedSourceOwner for CaptureInferenceOwner {
    fn snapshot(
        &self,
        owner: &str,
    ) -> Result<Vec<eidetic::models::recall::RetainedSourceText>, String> {
        let context = self.bridge.context()?;
        if owner != crate::research::namespace(context.session) {
            return Err("The answer belongs to another saved-source owner.".into());
        }
        let (reply, receive) = std::sync::mpsc::sync_channel(1);
        if !self.trail.command(TrailCommand::ResearchSnapshot {
            context: context.clone(),
            selected: None,
            max_chars: None,
            max_metadata_chars: None,
            reply,
        }) {
            return Err("The saved-source owner is closed.".into());
        }
        let entries = receive
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "The saved-source owner did not answer.")??;

        let mut sources = Vec::new();
        for entry in &entries {
            if let Some(content) = &entry.content {
                let target = crate::saved_sources::Target {
                    key: entry.admission.key.clone(),
                    text_hash: content.text_hash.clone(),
                    selector: None,
                };
                let source = crate::research::source_id(&target.into())?;
                if !sources
                    .iter()
                    .any(|entry: &eidetic::models::recall::RetainedSourceText| {
                        entry.source == source
                    })
                {
                    sources.push(eidetic::models::recall::RetainedSourceText {
                        source,
                        version: content.text_hash.clone(),
                        title: Some(content.title.clone()),
                        text: content.text.clone(),
                    });
                }
            }
        }
        let mut clips = Vec::new();
        for admission in &context.clips {
            match self.bridge.clip_text(&context, admission) {
                Ok((title, text)) => {
                    let text_hash = eidetic::Hash::of(text.as_bytes()).to_string();
                    let target = crate::research::Target::Clip(crate::research::ClipTarget {
                        clip: admission.clone(),
                        text_hash: text_hash.clone(),
                        selector: None,
                    });
                    sources.push(eidetic::models::recall::RetainedSourceText {
                        source: crate::research::source_id(&target)?,
                        version: text_hash,
                        title: Some(title.clone()),
                        text,
                    });
                    clips.push(crate::research::Choice {
                        target: Some(target),
                        label: format!(
                            "{title} · retained text · {}",
                            admission
                                .manifest
                                .0
                                .to_hex()
                                .chars()
                                .take(12)
                                .collect::<String>()
                        ),
                        error: None,
                    });
                },
                Err(error) => clips.push(crate::research::Choice {
                    target: None,
                    label: format!("Retained clip {}", admission.manifest),
                    error: Some(error),
                }),
            }
        }
        self.bridge.loaded(&context, &entries, clips)?;
        Ok(sources)
    }
    fn resolve(
        &self,
        source: &eidetic::models::recall::RecallSourceId,
        version: &str,
    ) -> Result<String, String> {
        let target = crate::research::target(source, version)?;
        let context = self.bridge.context()?;
        if source.owner.as_deref() != Some(crate::research::namespace(context.session).as_str()) {
            return Err("The citation belongs to another saved-source owner.".into());
        }
        let target = match target {
            crate::research::Target::Clip(target) => {
                let (_, text) = self.bridge.clip_text(&context, &target.clip)?;
                if target.text_hash != eidetic::Hash::of(text.as_bytes()).to_string() {
                    return Err("The retained clip text version changed.".into());
                }
                return Ok(text);
            },
            crate::research::Target::Capture(target) => target,
        };
        let (reply, receive) = std::sync::mpsc::sync_channel(1);
        if !self.trail.command(TrailCommand::ResearchSource {
            context: context.clone(),
            target,
            reply,
        }) {
            return Err("The saved-source owner is closed.".into());
        }
        let content = receive
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "The saved-source owner did not answer.")??;
        if self.bridge.context()? != context {
            return Err("The cited source session or admissions changed.".into());
        }
        Ok(content.text)
    }
    fn open_source(
        &self,
        opened: &crate::inference_service::OpenSource,
        generation: Option<uuid::Uuid>,
    ) -> Result<(), String> {
        let context = self.bridge.context()?;
        if Some(context.generation) != generation
            || opened.retained.source.owner.as_deref()
                != Some(crate::research::namespace(context.session).as_str())
        {
            return Err("The citation belongs to an earlier session opening.".into());
        }
        let mut target =
            crate::research::target(&opened.retained.source, &opened.retained.version)?;
        if target.text_hash() != eidetic::Hash::of(opened.retained.text.as_bytes()).to_string()
            || opened.selector.start >= opened.selector.end
            || opened.selector.end > opened.retained.text.chars().count()
        {
            return Err("The cited source text or Unicode range is invalid.".into());
        }
        target.set_selector(crate::saved_sources::Selector {
            start: opened.selector.start,
            end: opened.selector.end,
        });
        if !crate::research::admitted(&context, &target) {
            return Err("The cited saved source was withdrawn from this session.".into());
        }
        self.bridge
            .send(crate::research::Intent::Citation { context, target });
        Ok(())
    }
    fn choices(&self) -> Vec<crate::research::Choice> {
        self.bridge.choices()
    }
    fn generation(&self) -> Option<uuid::Uuid> {
        self.bridge.context().ok().map(|c| c.generation)
    }
}
