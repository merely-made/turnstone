// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::io::Read;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use mere::dataset::View;
use mere::formats::parse::parse_turtle;
use mere::profile::{lens::SelectionRecord, load::Limits};
use oxrdf::{Graph, Triple};

const MAX_SHAPES_BYTES: u64 = 128 * 1024;

/// Keep the original Turtle receipt shape readable after an upgrade.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum StoredShapes {
    Pack {
        canonical_pack: String,
        selection: String,
        shape: String,
    },
    Turtle {
        text: String,
        base: String,
    },
}

pub(super) struct CheckedShapes {
    pub graph: Graph,
    pub pack_label: Option<String>,
}

impl StoredShapes {
    pub(super) fn read_turtle(path: &Path) -> Result<(Self, CheckedShapes), String> {
        let bytes = read_file(path, MAX_SHAPES_BYTES, "Turtle shapes file")?;
        let text = String::from_utf8(bytes).map_err(|_| "The Turtle shapes file is not UTF-8.")?;
        let base = url::Url::from_file_path(path)
            .map_err(|_| "The shapes file needs an absolute path.")?
            .to_string();
        let shapes = Self::Turtle { text, base };
        let checked = shapes.check()?;
        Ok((shapes, checked))
    }

    pub(super) fn read_pack(path: &Path) -> Result<(Self, CheckedShapes), String> {
        let limits = Limits::default();
        let bytes = read_file(path, limits.max_pack_bytes as u64, "shape pack")?;
        let profile = load_pack(&bytes, &limits)?;
        let mut shapes = profile.shapes();
        let selected = shapes.next().ok_or("The pack's lens selects no shapes.")?;
        if shapes.next().is_some() {
            return Err("Choose a pack whose lens selects one shapes graph.".into());
        }
        Ok((
            Self::Pack {
                canonical_pack: STANDARD.encode(&bytes),
                selection: selection_identity(profile.selection())?,
                shape: selected.iri().to_owned(),
            },
            checked_pack(&profile, selected),
        ))
    }

    pub(super) fn check(&self) -> Result<CheckedShapes, String> {
        match self {
            Self::Turtle { text, base } => {
                if text.len() as u64 > MAX_SHAPES_BYTES {
                    return Err("The shapes file exceeds the 128 KiB limit.".into());
                }
                let shapes = parse_turtle(text, Some(base))
                    .map_err(|error| format!("Could not interpret the Turtle shapes: {error}"))?;
                Ok(CheckedShapes {
                    graph: shapes
                        .quads(View::Live)
                        .map(|quad| Triple::new(quad.subject, quad.predicate, quad.object))
                        .collect(),
                    pack_label: None,
                })
            },
            Self::Pack {
                canonical_pack,
                selection,
                shape,
            } => {
                let limits = Limits::default();
                if canonical_pack.len() > 4 * limits.max_pack_bytes.div_ceil(3) {
                    return Err("The saved shape pack exceeds its size limit.".into());
                }
                let bytes = STANDARD
                    .decode(canonical_pack)
                    .map_err(|_| "The saved shape pack encoding is invalid.")?;
                let profile = load_pack(&bytes, &limits)?;
                if selection_identity(profile.selection())? != *selection {
                    return Err("The saved shape pack selection has changed.".into());
                }
                profile
                    .shape(shape)
                    .map(|shape| checked_pack(&profile, shape))
                    .ok_or_else(|| "The saved shape pack no longer selects those shapes.".into())
            },
        }
    }
}

fn selection_identity(selection: &SelectionRecord) -> Result<String, String> {
    selection
        .to_canonical()
        .map(|bytes| STANDARD.encode(bytes))
        .map_err(|error| format!("Could not encode the shape pack selection: {error}"))
}

fn checked_pack(
    profile: &mere_shacl::ShapeProfile,
    shape: &mere_shacl::PackedShape,
) -> CheckedShapes {
    let pin = &profile.selection().lens;
    CheckedShapes {
        graph: shape.graph().clone(),
        pack_label: Some(format!("Shape pack {} {}", pin.id, pin.version)),
    }
}

fn load_pack(bytes: &[u8], limits: &Limits) -> Result<mere_shacl::ShapeProfile, String> {
    mere_shacl::load_local_shape_pack(bytes, limits)
        .map_err(|error| format!("Could not admit the shape pack: {error}"))
}

fn read_file(path: &Path, limit: u64, description: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
        .map_err(|error| format!("Could not read the {description}: {error}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!("The {description} exceeds its size limit."));
    }
    Ok(bytes)
}
