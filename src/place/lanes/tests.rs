// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chartulary::{Author, Container};
use commons::chat::{Channel, ChatEvent, Message};
use gemot::moot::{MootAccessLevel, MootFile, MootId, MootMembershipAction, MootOutboundOperation};
use identity::{IdentityProvider, InMemoryProvider};
use muniment::RedbBackend;
use stickleback::DataKeyring;
use transport::P2pandaTransport;

use graphshell_client::{ResolvedContent, RetainedEndpointSession};
use transport::Transport;

use crate::action::Update;
use crate::identity::RootIdentity;
use crate::panes::SessionId;
use crate::place::invite::{P2PANDA_ENDPOINT_TICKET, RendezvousV1};
use crate::place::worker::tests::{
    binding, found_place_for_authoring, founder_signing_key, place_delegation, place_rules,
    seed_exact_collection, seed_exact_collection_captures, settings,
};
use crate::place::worker::{
    PlaceCommand, PlaceWorkerCommand, author_invitation, found_place_group, load_group_session,
    open_cached_place, place_store_dir, prepare_group_identity, spawn_place_worker,
};
use crate::place::{
    CapturedCollectionSelection, CapturedCollectionSelectionStatus, PlaceBindingV1,
};
use commons::GroupKeys;
use commons::chat::ChatReplica;
use commons::encrypted::{COMMONS_ENCRYPTED_GRAPH_LANE, EncryptedReplica};
use moot_port::coop;

use crate::place::worker::GRAPH_STORE;

/// How long a test waits for an answer the worker must give. Generous,
/// since a full parallel suite can stall a worker for seconds; a
/// passing run never waits it out.
const ANSWER: Duration = Duration::from_secs(60);

/// One founder, live and serving a vault, plus the envelope a joiner was
/// admitted with. The founder half of both projection tests.
struct ServedFounder {
    root: std::path::PathBuf,
    vault: std::path::PathBuf,
    document: std::path::PathBuf,
    worker: armillary::ActorHandle<PlaceWorkerCommand>,
    updates: std::sync::mpsc::Receiver<Update>,
    session: SessionId,
    binding: PlaceBindingV1,
    ticket: String,
    invite: Box<crate::place::invite::PlaceInviteV1>,
    guest: std::path::PathBuf,
}

/// Classifies every operation revoked, so a projection names each by id.
struct EveryOperation;

/// K3 of the coop lifecycle parity plan: the place worker driving
/// `moot_port::coop`'s shared conformance harness over its own founder
/// and member workers, render-free. Both run the system clock rather
/// than the fixed clock most of this file's tests use, so a short-lived
/// grant expires for real -- matching Turnstone's declared
/// [`coop::Clock::System`], where the fixture instead drives a clock it
/// owns.
struct ConformanceDriver {
    founder: (
        armillary::ActorHandle<PlaceWorkerCommand>,
        std::sync::mpsc::Receiver<Update>,
    ),
    founder_session: SessionId,
    binding: PlaceBindingV1,
    host: std::path::PathBuf,
    guest: std::path::PathBuf,
    member_prekey: Vec<u8>,
    clock: crate::place::worker::PlaceWorkerSettings,
    member: Option<(
        armillary::ActorHandle<PlaceWorkerCommand>,
        std::sync::mpsc::Receiver<Update>,
        SessionId,
    )>,
    invite: Option<Box<crate::place::invite::PlaceInviteV1>>,
    grant_expires_at_ms: Option<u64>,
    /// This profile's last-refreshed retained view, `None` before join
    /// and after leave. Standing in for `PlaceState::Offline`'s
    /// snapshot the way the app itself holds it.
    snapshot: Option<crate::place::OfflinePlaceSnapshot>,
    /// Set by `leave`, cleared by `join`/`rejoin`. Standing in for
    /// `PlaceState::Left`'s retained summary the way the app builds it
    /// in `finish_leave_place`.
    left: Option<crate::place::PlaceLeftSummary>,
    /// The clock reading `report()` carries. Refreshed only by a step
    /// that actually re-reads standing, so a duplicate delivery that
    /// changes no retained state also reports no clock tick -- the same
    /// "as of last refresh" story every other snapshot field already
    /// tells.
    now_ms: u64,
    next_request: u64,
}

#[path = "tests/a_founder_binds_listen_only_and_reports_its_cases.rs"]
mod a_founder_binds_listen_only_and_reports_its_cases;
#[path = "tests/a_joined_member_reads_chat_authored_after_a_cases.rs"]
mod a_joined_member_reads_chat_authored_after_a_cases;
#[path = "tests/a_live_share_withdrawal_empties_the_receiver_selected_cases.rs"]
mod a_live_share_withdrawal_empties_the_receiver_selected_cases;
#[path = "tests/a_restarted_place_keeps_what_it_converged_on_cases.rs"]
mod a_restarted_place_keeps_what_it_converged_on_cases;
#[path = "tests/a_revoked_member_stops_reading_while_the_others_cases.rs"]
mod a_revoked_member_stops_reading_while_the_others_cases;
#[path = "tests/a_writer_edits_the_document_its_founder_holds_cases.rs"]
mod a_writer_edits_the_document_its_founder_holds_cases;
#[path = "tests/a_writer_visits_a_held_document_and_is_cases.rs"]
mod a_writer_visits_a_held_document_and_is_cases;
#[path = "tests/content_authored_the_instant_after_a_revoke_still_cases.rs"]
mod content_authored_the_instant_after_a_revoke_still_cases;

#[path = "tests/delegate_to.rs"]
mod delegate_to;
use delegate_to::delegate_to;
#[path = "tests/author_host_content.rs"]
mod author_host_content;
use author_host_content::author_host_content;
#[path = "tests/expect_authored.rs"]
mod expect_authored;
use expect_authored::expect_authored;
#[path = "tests/converge_until.rs"]
mod converge_until;
use converge_until::converge_until;
#[path = "tests/found_and_invite.rs"]
mod found_and_invite;
use found_and_invite::found_and_invite;
#[path = "tests/found_and_invite_with.rs"]
mod found_and_invite_with;
use found_and_invite_with::found_and_invite_with;
#[path = "tests/founder_projection.rs"]
mod founder_projection;
use founder_projection::founder_projection;
#[path = "tests/joiner_lanes.rs"]
mod joiner_lanes;
use joiner_lanes::joiner_lanes;
#[path = "tests/viewing_profile.rs"]
mod viewing_profile;
use viewing_profile::viewing_profile;
#[path = "tests/hex_head.rs"]
mod hex_head;
use hex_head::hex_head;
#[path = "tests/settle.rs"]
mod settle;
use settle::settle;
#[path = "tests/next_nudge.rs"]
mod next_nudge;
use next_nudge::next_nudge;
#[path = "tests/dial_without_a_grant.rs"]
mod dial_without_a_grant;
use dial_without_a_grant::dial_without_a_grant;
#[path = "tests/expect_invited.rs"]
mod expect_invited;
use expect_invited::expect_invited;
#[path = "tests/lane_counters.rs"]
mod lane_counters;
use lane_counters::lane_counters;
#[path = "tests/an_admission_reaches_a_connected_joiner.rs"]
mod an_admission_reaches_a_connected_joiner;
use an_admission_reaches_a_connected_joiner::an_admission_reaches_a_connected_joiner;
#[path = "tests/authored.rs"]
mod authored;
use authored::authored;
#[path = "tests/visit.rs"]
mod visit;
use visit::visit;
#[path = "tests/release.rs"]
mod release;
use release::release;
#[path = "tests/bypass_after_restart.rs"]
mod bypass_after_restart;
use bypass_after_restart::bypass_after_restart;
#[path = "tests/drop_bind.rs"]
mod drop_bind;
use drop_bind::drop_bind;
#[path = "tests/both_permissions.rs"]
mod both_permissions;
use both_permissions::both_permissions;
#[path = "tests/status_of.rs"]
mod status_of;
use status_of::status_of;
#[path = "tests/joined_snapshot.rs"]
mod joined_snapshot;
use joined_snapshot::joined_snapshot;
#[path = "tests/try_both_writes.rs"]
mod try_both_writes;
use try_both_writes::try_both_writes;
#[path = "tests/found_live.rs"]
mod found_live;
use found_live::found_live;
#[path = "tests/invite_writer.rs"]
mod invite_writer;
use invite_writer::invite_writer;
#[path = "tests/join_member.rs"]
mod join_member;
use join_member::join_member;
#[path = "tests/send_message.rs"]
mod send_message;
use send_message::send_message;
#[path = "tests/until_nudged.rs"]
mod until_nudged;
use until_nudged::until_nudged;
#[path = "tests/reopen_by_hand.rs"]
mod reopen_by_hand;
use reopen_by_hand::reopen_by_hand;
#[path = "tests/chat_bodies.rs"]
mod chat_bodies;
use chat_bodies::chat_bodies;
#[path = "tests/impl_commons_commons_authority_for_every.rs"]
mod impl_commons_commons_authority_for_every;
#[path = "tests/share_node.rs"]
mod share_node;
use share_node::share_node;
#[path = "tests/addresses.rs"]
mod addresses;
use addresses::addresses;
#[path = "tests/lane_ops.rs"]
mod lane_ops;
use lane_ops::lane_ops;
#[path = "tests/report_lanes.rs"]
mod report_lanes;
use report_lanes::report_lanes;
#[path = "tests/conformance_driver_f_o_u_n_d_e_r.rs"]
mod conformance_driver_f_o_u_n_d_e_r;
#[path = "tests/impl_coop_lifecycle_driver_for_conformance.rs"]
mod impl_coop_lifecycle_driver_for_conformance;
#[path = "tests/impl_drop_for_conformance_driver.rs"]
mod impl_drop_for_conformance_driver;
