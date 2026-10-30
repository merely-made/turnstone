// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's owner adapter for Alembic's shared evidence-brief workshop.
//!
//! The owner admits public fixtures and current sealed saved-source versions.
//! Legacy and unverified private captures remain unavailable.

use crate::saved_sources::{self, Admission, Entry, Key, Target};
use alembic::workshop::{Owner, Passage, Source};
use alembic::workshop_surface::SourceChoice;
use athanor::workshop_seal::Protection;
use mere::kernel::graph::{Graph, apply::GraphDelta};
use mere::kernel::persistence::PersistedResourceRecord;
use uuid::Uuid;

#[derive(Clone, Copy)]
struct Example {
    id: Uuid,
    family: &'static str,
    version: &'static str,
    title: &'static str,
    body: &'static str,
}

const EXAMPLES: [Example; 3] = [
    Example {
        id: Uuid::from_u128(0xa1eb_1c00_0000_0000_0000_0000_0000_0001),
        family: alembic::workshop::PUBLIC_EXAMPLE[0].0,
        version: alembic::workshop::PUBLIC_EXAMPLE[0].1,
        title: alembic::workshop::PUBLIC_EXAMPLE[0].2,
        body: alembic::workshop::PUBLIC_EXAMPLE[0].3,
    },
    Example {
        id: Uuid::from_u128(0xa1eb_1c00_0000_0000_0000_0000_0000_0002),
        family: alembic::workshop::PUBLIC_EXAMPLE[1].0,
        version: alembic::workshop::PUBLIC_EXAMPLE[1].1,
        title: alembic::workshop::PUBLIC_EXAMPLE[1].2,
        body: alembic::workshop::PUBLIC_EXAMPLE[1].3,
    },
    Example {
        id: Uuid::from_u128(0xa1eb_1c00_0000_0000_0000_0000_0000_0003),
        family: alembic::workshop::PUBLIC_EXAMPLE[2].0,
        version: alembic::workshop::PUBLIC_EXAMPLE[2].1,
        title: alembic::workshop::PUBLIC_EXAMPLE[2].2,
        body: alembic::workshop::PUBLIC_EXAMPLE[2].3,
    },
];

const MAX_PRIVATE_SOURCE_BYTES: usize = 2048;

#[cfg(test)]
#[path = "alembic_workshop/tests.rs"]
mod tests;

#[path = "alembic_workshop/example_choices.rs"]
mod example_choices;
pub(crate) use example_choices::example_choices;
#[path = "alembic_workshop/example_seed_deltas.rs"]
mod example_seed_deltas;
pub(crate) use example_seed_deltas::example_seed_deltas;
#[path = "alembic_workshop/example_seed_conflict.rs"]
mod example_seed_conflict;
pub(crate) use example_seed_conflict::example_seed_conflict;
#[path = "alembic_workshop/fixture_surface.rs"]
mod fixture_surface;
pub(crate) use fixture_surface::fixture_surface;
#[path = "alembic_workshop/fixture_resource.rs"]
mod fixture_resource;
use fixture_resource::fixture_resource;
#[path = "alembic_workshop/owner.rs"]
mod owner;
pub(crate) use owner::owner;
#[path = "alembic_workshop/capture_public_examples.rs"]
mod capture_public_examples;
pub(crate) use capture_public_examples::capture_public_examples;
#[path = "alembic_workshop/saved_target.rs"]
mod saved_target;
use saved_target::saved_target;
#[path = "alembic_workshop/saved_source_choice_key.rs"]
mod saved_source_choice_key;
use saved_source_choice_key::saved_source_choice_key;
#[path = "alembic_workshop/private_source_choices.rs"]
mod private_source_choices;
pub(crate) use private_source_choices::private_source_choices;
#[path = "alembic_workshop/capture_private_saved_source.rs"]
mod capture_private_saved_source;
use capture_private_saved_source::capture_private_saved_source;
#[path = "alembic_workshop/capture_selected_sources.rs"]
mod capture_selected_sources;
pub(crate) use capture_selected_sources::capture_selected_sources;
#[path = "alembic_workshop/private_passage_current.rs"]
mod private_passage_current;
pub(crate) use private_passage_current::private_passage_current;
#[path = "alembic_workshop/passage_current.rs"]
mod passage_current;
pub(crate) use passage_current::passage_current;
#[path = "alembic_workshop/proposal_reading.rs"]
mod proposal_reading;
pub(crate) use proposal_reading::proposal_reading;
#[path = "alembic_workshop/helper_subject.rs"]
mod helper_subject;
use helper_subject::helper_subject;
#[path = "alembic_workshop/fixture_authority.rs"]
mod fixture_authority;
use fixture_authority::fixture_authority;
#[path = "alembic_workshop/authority.rs"]
mod authority;
pub(crate) use authority::authority;
#[path = "alembic_workshop/authority_for_destination.rs"]
mod authority_for_destination;
pub(crate) use authority_for_destination::authority_for_destination;
