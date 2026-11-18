// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use scenomise::{
    contexts::CommandDisclosure,
    projection::{ProjectionFieldType, ProjectionOccurrence, ProjectionValue},
};
use std::collections::BTreeMap;

enum Route {
    Host(String),
    Projection(String),
    Block {
        resource: String,
        block: String,
        command: String,
    },
    #[cfg(feature = "applets")]
    Applet(String),
}

struct Offer {
    data: CommandDisclosure,
    route: Route,
}

fn row<'a>(data: &'a ProjectionDataset, subject: &Subject) -> Option<&'a ProjectionOccurrence> {
    matches!(subject.kind, SubjectKind::Node | SubjectKind::Selection)
        .then(|| {
            data.occurrences
                .iter()
                .find(|row| row.occurrence_id == subject.id)
        })
        .flatten()
}

fn text(row: &ProjectionOccurrence, field: &str) -> Option<String> {
    row.values.get(field)?.text().map(str::to_owned)
}

fn address(row: &ProjectionOccurrence) -> String {
    text(row, "url").unwrap_or_else(|| row.source.id.clone())
}

fn offer(id: String, label: String, route: Route) -> Offer {
    Offer {
        data: CommandDisclosure {
            id,
            label,
            description: String::new(),
            shortcut: None,
            disabled_reason: None,
        },
        route,
    }
}

fn offers(page: &TreePage, data: &ProjectionDataset, subject: &Subject) -> Vec<Offer> {
    let mut commands: Vec<_> = crate::web_tree::commands::catalog(page)
        .commands()
        .iter()
        .map(|command| Offer {
            data: CommandDisclosure {
                id: format!("host:{}", command.id),
                label: command.label.clone(),
                description: command.category.clone(),
                shortcut: command.shortcut.clone(),
                disabled_reason: command.disabled_reason.clone(),
            },
            route: Route::Host(command.id.clone()),
        })
        .collect();
    if page.shared.remote_is_shown() {
        commands.extend(
            page.shared
                .remote
                .borrow()
                .actions()
                .into_iter()
                .map(|action| {
                    let key = action.intent.0;
                    let mut offer = offer(
                        format!("projection:{key}"),
                        action.label,
                        Route::Projection(key),
                    );
                    offer.data.description = action.explanation;
                    offer
                }),
        );
    }
    if let Some(row) = row(data, subject) {
        if let (Some(resource), Some(block)) = (text(row, "document_id"), text(row, "block_id")) {
            commands.extend(
                page.blocks
                    .context_claim(&resource, &block)
                    .into_iter()
                    .map(|command| {
                        let mut offer = offer(
                            format!("block:{}", command.id),
                            command.label,
                            Route::Block {
                                resource: resource.clone(),
                                block: block.clone(),
                                command: command.id,
                            },
                        );
                        if command.requires_editor {
                            offer.data.disabled_reason =
                                Some("Continue in the block editor".into());
                        }
                        offer
                    }),
            );
        }
        #[cfg(feature = "applets")]
        {
            let url = address(row);
            if page.shared.applet.borrow().offers_context_open(&url) {
                commands.push(offer(
                    "applet:open-address".into(),
                    "Open shared capsule".into(),
                    Route::Applet(url),
                ));
            }
        }
    }
    commands
}

pub(super) fn invoke(page: &mut TreePage, active: &Active, context_id: &str, id: &str) {
    let Some(p) = page.product.as_ref().map(|product| &product.projection) else {
        return;
    };
    let scenes = definitions(page, p, &active.subject);
    if p.contexts.active.as_ref() != Some(active) {
        return;
    }
    let Some(scene) = scenes.iter().find(|scene| {
        scene.id == context_id
            && scene.trigger == active.trigger
            && scene.subject == active.subject.kind
            && scene.input == "commands"
    }) else {
        return;
    };
    if scene
        .commands
        .as_ref()
        .is_some_and(|choices| !choices.iter().any(|c| c.id == id))
    {
        return;
    }
    let Some(data) = &p.dataset else { return };
    let Some(offer) = offers(page, data, &active.subject)
        .into_iter()
        .find(|offer| offer.data.id == id && offer.data.disabled_reason.is_none())
    else {
        return;
    };
    match offer.route {
        Route::Host(id) => crate::web_tree::commands::run(page, &id),
        Route::Projection(key) => {
            let index = page
                .shared
                .remote
                .borrow()
                .actions()
                .iter()
                .position(|action| action.intent.0 == key);
            if let Some(index) = index {
                page.invoke(index);
            }
        },
        Route::Block {
            resource,
            block,
            command,
        } => {
            page.blocks.invoke_context(&resource, &block, &command);
        },
        #[cfg(feature = "applets")]
        Route::Applet(url) => page.shared.applet.borrow_mut().context_open(&url),
    }
}

pub(super) fn read(
    page: &TreePage,
    p: &ProjectionControls,
    scene: &ContextScene,
    subject: &Subject,
) -> Option<ProjectionDataset> {
    let data = p.dataset.as_ref()?;
    let selected = row(data, subject);
    match scene.input.as_str() {
        "linked" => {
            let linked = p
                .relationships
                .iter()
                .filter_map(|edge| {
                    if edge.from_occurrence == subject.id {
                        Some(&edge.to_occurrence)
                    } else if edge.to_occurrence == subject.id {
                        Some(&edge.from_occurrence)
                    } else {
                        None
                    }
                })
                .collect::<std::collections::BTreeSet<_>>();
            Some(ProjectionDataset {
                source: data.source.clone(),
                revision: data.revision.clone(),
                fields: data.fields.clone(),
                occurrences: data
                    .occurrences
                    .iter()
                    .filter(|row| linked.contains(&row.occurrence_id))
                    .cloned()
                    .collect(),
            })
        },
        "resource" => {
            let selected = selected?;
            let mut row = selected.clone();
            row.values
                .insert("address".into(), ProjectionValue::Text(address(selected)));
            let mut fields = data.fields.clone();
            fields.insert("address".into(), ProjectionFieldType::Text);
            Some(ProjectionDataset {
                source: data.source.clone(),
                revision: data.revision.clone(),
                fields,
                occurrences: vec![row],
            })
        },
        "subject" => {
            let label = match subject.kind {
                SubjectKind::Relation => p
                    .relationships
                    .iter()
                    .find(|edge| edge.id == subject.id)?
                    .label
                    .clone(),
                SubjectKind::Field => {
                    data.fields.get(&subject.id)?;
                    subject.id.clone()
                },
                _ => {
                    return Some(ProjectionDataset {
                        source: data.source.clone(),
                        revision: data.revision.clone(),
                        fields: data.fields.clone(),
                        occurrences: vec![selected?.clone()],
                    });
                },
            };
            Some(ProjectionDataset {
                source: data.source.clone(),
                revision: data.revision.clone(),
                fields: BTreeMap::from([
                    ("occurrence_id".into(), ProjectionFieldType::Text),
                    ("label".into(), ProjectionFieldType::Text),
                    ("column".into(), ProjectionFieldType::Number),
                    ("order".into(), ProjectionFieldType::Number),
                ]),
                occurrences: vec![ProjectionOccurrence::new(
                    subject.id.clone(),
                    sceno::SourceRef::new("context.subject", subject.id.clone()),
                    BTreeMap::from([
                        (
                            "occurrence_id".into(),
                            ProjectionValue::Text(subject.id.clone()),
                        ),
                        ("label".into(), ProjectionValue::Text(label)),
                        ("column".into(), ProjectionValue::Number(0.0)),
                        ("order".into(), ProjectionValue::Number(0.0)),
                    ]),
                )],
            })
        },
        "commands" => Some(scenomise::contexts::commands(
            data.source.clone(),
            data.revision.clone(),
            &offers(page, data, subject)
                .into_iter()
                .map(|offer| offer.data)
                .collect::<Vec<_>>(),
            scene.commands.as_deref(),
        )),
        _ => None,
    }
}
