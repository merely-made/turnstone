// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    /// Fork (tear-out G4-R R2): snapshot the connected component containing
    /// `seed` into a freshly minted session — new `SessionId` + real `GraphId`,
    /// a `picked_from` record on the new manifest (a pick, not a fork), the
    /// component's nodes + internal edges copied with `CopiedFrom` provenance,
    /// and the donor's per-node character carried by **facets** through the
    /// copy's id remap (`arrangement.*` layout, `web.*` browser state, foreign
    /// namespaces) plus the container's `scene.*`. Persists the fork's
    /// `graph.json` + `facets.json`, then returns the switch effect — v0 opens
    /// by session-switch (the shell saves the departing donor first, as every
    /// switch does); overmap navigation replaces that when it lands. Donor
    /// untouched; the two are independent thereafter. Returns no effects if
    /// `seed` names no node.
    pub fn fork_session_from(&mut self, seed: uuid::Uuid) -> Vec<Effect> {
        let donor_placement = match self
            .graph_runtimes
            .active_persistence()
            .placement_for(&self.session_dir())
        {
            Ok(placement) => placement,
            Err(error) => {
                tracing::warn!(%error, "session fork refused");
                return Vec::new();
            },
        };
        if self.graph_runtimes.graph().get_node_by_id(seed).is_none() {
            return Vec::new();
        }
        // Admission state is session-owned. A malformed existing sidecar must
        // stop the fork before any new session is minted; treating it as an
        // empty/default state would silently turn paused or revoked residents
        // into fresh active bindings in the child.
        let admission_path = crate::resident_admission::path(&self.session_dir());
        let donor_admissions = match admission_path.try_exists() {
            Ok(false) => None,
            Ok(true) => match crate::resident_admission::load(&self.session_dir()) {
                Ok(state) => Some(state),
                Err(error) => {
                    tracing::warn!(%error, "refusing to fork with malformed resident admission state");
                    return Vec::new();
                },
            },
            Err(error) => {
                tracing::warn!(%error, "refusing to fork because resident admission state cannot be inspected");
                return Vec::new();
            },
        };
        // The carry must read the moment, not the last save.
        self.refresh_browser_states();
        self.refresh_facets();

        // The kernel half: component copy with the id remap for the carry.
        let donor_graph_label = self.container_id().map(|c| c.to_string());
        let (mut fork_graph, copy) = match session::copy_session_component(
            self.graph_runtimes.graph(),
            seed,
            donor_graph_label,
        ) {
            Ok(copy) => copy,
            Err(error) => {
                tracing::warn!(%error, "fork component copy refused");
                return Vec::new();
            },
        };
        if copy.new_keys.is_empty() {
            return Vec::new();
        }

        // The world-carry: a donor node bearing a nested graph forks with a
        // REAL copy of its world. The component copy deliberately drops
        // `nested` (two live nodes must never share one world file); here the
        // fork re-bears each carried world directly (`bear_nested`, no delta
        // spine — the fork graph has no journal yet) and the world files copy
        // below once the fork's session dir exists.
        let mut carried_worlds: Vec<String> = Vec::new();
        for (donor_id, minted_id) in &copy.id_remap {
            let Some(log) = self
                .graph_runtimes
                .graph()
                .get_node_key_by_id(*donor_id)
                .and_then(|key| self.graph_runtimes.graph().get_node(key))
                .and_then(|node| node.nested.clone())
            else {
                continue;
            };
            if let Some(key) = fork_graph.get_node_key_by_id(*minted_id) {
                let _ = fork_graph.bear_nested(key, Some(log.clone()));
                carried_worlds.push(log.as_str().to_string());
            }
        }

        // The facet-carry: whole per-node records through the remap, scene
        // settings donor-container -> fork-container.
        let fork_graph_id = GraphId::new();
        let mut fork_facets = pandect::NodeFacetStore::new();
        pandect::copy_node_facets(
            self.graph_runtimes.facets(),
            &mut fork_facets,
            &copy.id_remap,
        );
        if let Some(Ok(scene)) = self
            .container_id()
            .and_then(|donor| crate::scene_state::read_scene(self.graph_runtimes.facets(), donor))
        {
            let ids = copy.id_remap.iter().copied().collect();
            let carried = mere::canvas::saved_scene::remap_scene(&scene, &ids);
            pandect::write_saved_scene(&mut fork_facets, *fork_graph_id.as_uuid(), &carried);
        }
        let derivation_facet =
            chartulary::FacetId::new(mere::kernel::graph::node_facets::PROVENANCE_DERIVATIONS);
        let copied_derivations = copy
            .id_remap
            .iter()
            .filter_map(|(_, minted)| {
                fork_graph
                    .facets()
                    .get(minted, &derivation_facet)
                    .cloned()
                    .map(|value| (*minted, value))
            })
            .collect::<Vec<_>>();
        if let Err(error) = fork_graph.overlay_facets(fork_facets) {
            tracing::warn!(%error, "forked component facets could not be installed");
            return vec![Effect::Redraw];
        }
        let import_facet =
            chartulary::FacetId::new(mere::kernel::graph::node_facets::PROVENANCE_IMPORT);
        if let Err(error) = crate::graph_facets::edit_graph_facets(&mut fork_graph, |facets| {
            for (_, minted) in &copy.id_remap {
                facets.remove(minted, &import_facet);
                crate::source_capture::discard_copied_request_admission(facets, *minted);
            }
            for (minted, value) in copied_derivations {
                facets
                    .set(
                        minted,
                        derivation_facet.clone(),
                        value,
                        &chartulary::AcceptAll,
                    )
                    .expect("AcceptAll cannot reject copied derivation");
            }
        }) {
            tracing::warn!(%error, "fork facet edits were refused");
            return vec![Effect::Redraw];
        }

        // Carry only admission records for members that actually crossed the
        // component boundary. The member UUID and ResidentId both change;
        // subject, body revision, lifecycle, generation, and skipped-wake
        // count remain the resident's durable state.
        let fork_admissions = donor_admissions.as_ref().map(|donor| {
            let mut fork = crate::resident_admission::ResidentAdmissions::default();
            for (donor_id, minted_id) in &copy.id_remap {
                if let Some(mut record) = donor.get(*donor_id).cloned() {
                    record.binding.id = crate::resident_admission::resident_id(*minted_id);
                    fork.insert(*minted_id, record);
                }
            }
            fork
        });

        // Mint the fork's session: manifest with the parent back-reference,
        // then its on-disk state, so the switch below adopts a real session.
        let fork_placement = donor_placement;
        let fork_facets = fork_graph.facets().clone();
        let mut fork_graph = match pandect::graph_placement::materialize_snapshot(
            &fork_graph.to_snapshot(),
            fork_placement,
        ) {
            Ok(graph) => graph,
            Err(error) => {
                tracing::warn!(%error, "assembled fork graph refused");
                return Vec::new();
            },
        };
        if let Err(error) = fork_graph.overlay_facets(fork_facets) {
            tracing::warn!(%error, "fork graph facets could not be restored after placement");
            return vec![Effect::Redraw];
        }
        let fork_id = crate::panes::SessionId::new();
        let fork_dir = session::session_dir(&self.data_root, fork_id);
        let fork_writer = match crate::session_persistence::SessionWriteLease::acquire(&fork_dir) {
            Ok(writer) => writer,
            Err(error) => {
                tracing::warn!(%error, "refusing fork because its writer lease could not be acquired");
                return Vec::new();
            },
        };
        if let Some(admissions) = &fork_admissions
            && let Err(error) = crate::resident_admission::save(&fork_dir, admissions)
        {
            tracing::warn!(%error, "refusing to fork because resident admission state could not be preserved");
            return Vec::new();
        }
        let mut manifest = pandect::GraphSessionManifest::new(fork_id, fork_graph_id);
        manifest.storage_path = Some(session::session_dir(&self.data_root, fork_id));
        // A torn-out component is picked off, not forked (Mark, 2026-10-15):
        // it records where it was picked from, never a fork's parent. The
        // container is the donor's, which each copied node's CopiedFrom
        // derivation names; Turnstone keeps no pandect journal for a session,
        // so the pick's cursor is 0.
        manifest.picked_from = Some(pandect::PickedFrom {
            session: self.session_id,
            at: 0,
            container: self.container_id().unwrap_or_default(),
        });
        // A torn-out component is a picked-off part (Mark, 2026-10-15): it
        // names where it was picked from, "Evening reading (picked from
        // Evening reading)", never reading as its donor.
        let donor_name = self
            .sessions
            .get(self.session_id)
            .and_then(|m| m.display_name.clone())
            .or_else(|| self.derive_session_name());
        manifest.display_name = Some(pandect::picked_session_name(donor_name.as_deref()));
        let fork_state = crate::session_persistence::SessionPersistence::writable_with_lease(
            fork_dir.clone(),
            fork_placement,
            fork_writer,
        );
        if let Err(error) = fork_state.save_graph_with(
            &fork_dir,
            &fork_graph,
            || pandect::save_node_facets(&fork_dir, fork_graph.facets()),
            |placement| {
                crate::session_graph_file::save_profiled(
                    &fork_dir.join(pandect::session_graph_store::GRAPH_FILE),
                    &fork_graph,
                    placement,
                )
            },
        ) {
            tracing::warn!(%error, "fork persistence failed before manifest publication");
            return Vec::new();
        }
        // Each carried world becomes the fork's own file: donor and fork
        // evolve their copies independently thereafter. A missing donor file
        // is fine — the resident rebuilds on an empty world, as always.
        let donor_dir = self.session_dir();
        for log_id in &carried_worlds {
            let from = crate::denizen::nested_log_path(&donor_dir, log_id);
            let to = crate::denizen::nested_log_path(&fork_dir, log_id);
            if !from.is_file() {
                continue;
            }
            let result = (|| -> std::io::Result<()> {
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&from, &to).map(|_| ())
            })();
            if let Err(err) = result {
                tracing::warn!(%err, log_id, "failed to carry a participant world into the fork");
                return Vec::new();
            }
        }
        self.sessions.insert(manifest);
        if let Err(error) = self.sessions.flush_dirty() {
            tracing::warn!(%error, "fork manifest publication failed; refusing switch");
            self.sessions.remove(fork_id);
            return Vec::new();
        }
        self.events.push(AppEvent::SessionForked);
        vec![Effect::SwitchSession { id: fork_id }]
    }

    /// A session's display label: the manifest's name when set, else the
    /// id's first 8 hex chars.
    pub fn session_label(&self, id: crate::panes::SessionId) -> String {
        self.sessions
            .get(id)
            .and_then(|m| m.display_name.clone())
            .unwrap_or_else(|| id.as_uuid().to_string()[..8].to_string())
    }

    /// Derive a human name for the live session from its graph: the display
    /// label of the most recently visited node (the page you were last on).
    /// `None` for an empty graph, so the uuid label stands until there is
    /// content to name the session after. The host stamps this into the
    /// manifest once, when `display_name` is still unset, so the switcher
    /// reads "Example Domain" instead of eight hex chars without churning as
    /// you keep browsing.
    pub(crate) fn derive_session_name(&self) -> Option<String> {
        let graph = self.graph_runtimes.graph();
        let key = graph
            .nodes()
            .filter_map(|(key, node)| graph.node_last_visited(key).map(|at| (at, key, node)))
            .max_by_key(|(at, _, node)| (*at, node.id))
            .map(|(_, key, _)| key)?;
        let label = graph.node_display_label(key);
        (!label.trim().is_empty()).then_some(label)
    }

    /// This profile's Personae root. The same root the worker evaluates
    /// authority for; read here so a card can name it without a round trip.
    /// `None` while the vault is locked.
    pub(crate) fn personae_root(&self) -> Option<[u8; 32]> {
        use identity::IdentityProvider as _;
        self.identity
            .as_ref()
            .map(|root| root.master_public_key().to_bytes())
    }

    /// The vault unlocked: the pending identity becomes the profile root.
    /// Participants rebuild under it, so their chains verify again; the
    /// shell starts what speaks as the profile. A second root for the same
    /// run is ignored: a live persona switch is not this path.
    pub(crate) fn adopt_profile_root(
        &mut self,
        root: std::sync::Arc<crate::identity::RootIdentity>,
    ) -> Vec<Effect> {
        if self.identity.is_some() {
            return Vec::new();
        }
        let description = root.description();
        self.identity = Some(root);
        let sdir = self.session_dir();
        self.denizens = crate::denizen::rebuild(
            self.graph_runtimes.facets(),
            self.graph_runtimes.graph(),
            &sdir,
            self.personae_root(),
        );
        self.events
            .push(AppEvent::ProfileIdentityUnlocked(description));
        vec![Effect::Redraw]
    }

    /// Refuse one place gesture out loud, changing nothing.
    pub(crate) fn refuse_place(&mut self, reason: impl Into<String>) -> Vec<Effect> {
        let reason = reason.into();
        tracing::warn!(%reason, "place gesture refused");
        self.events.push(AppEvent::PlaceRefused(reason));
        vec![Effect::Redraw]
    }

    /// Put this bind's FULL rendezvous ticket(s) on the system clipboard.
    /// The status row elides the middle of a ~200-character ticket, so this is
    /// the only way the whole of it leaves the app. Multiple tickets join with
    /// newlines; the clipboard itself lives behind the shell.
    pub(crate) fn copy_local_rendezvous(&mut self) -> Vec<Effect> {
        let tickets = self.place.local_rendezvous().to_vec();
        if tickets.is_empty() {
            return self.refuse_place("no local rendezvous to copy");
        }
        self.events
            .push(AppEvent::PlaceRendezvousCopied(tickets.len()));
        vec![Effect::CopyText(tickets.join("\n")), Effect::Redraw]
    }

    /// Open one free-text place prompt, in the rename shape.
    ///
    /// The precondition is checked HERE rather than only at commit, so a
    /// prompt that could not possibly succeed never opens.
    pub(in super::super) fn begin_place_prompt(
        &mut self,
        prompt: crate::ui::PlacePrompt,
    ) -> Vec<Effect> {
        use crate::ui::PlacePrompt;
        let joined = self.place.binding().is_some();
        match prompt {
            PlacePrompt::FoundPlace | PlacePrompt::OfferPrekey | PlacePrompt::Join if joined => {
                return self.refuse_place("leave this place before joining or founding another");
            },
            PlacePrompt::ExportCard
            | PlacePrompt::RefreshContact
            | PlacePrompt::Invite
            | PlacePrompt::InviteReader
            | PlacePrompt::SendMessage
                if !joined =>
            {
                return self.refuse_place("this session is not in a place");
            },
            _ => {},
        }
        self.omnibar = OmnibarState {
            open: true,
            mode: crate::ui::OmnibarMode::Place(prompt),
            ..OmnibarState::default()
        };
        let target = self.fallback_shell_context();
        self.shell.begin_omnibar(target);
        self.focus = FocusTarget::Chrome;
        self.recompute_omnibar_suggestions();
        self.events.push(AppEvent::OmnibarOpened);
        vec![Effect::Redraw]
    }

    /// Found a place in this session. Claims no binding: `Joining` carries a
    /// generation and nothing else until the worker answers.
    pub fn found_place(&mut self, name: String) -> Vec<Effect> {
        if self.place.binding().is_some() {
            return self.refuse_place("leave this place before founding another");
        }
        let name = name.trim().to_string();
        if name.is_empty() {
            return self.refuse_place("a place needs a name");
        }
        self.next_place_generation = self.next_place_generation.wrapping_add(1);
        let generation = self.next_place_generation;
        self.place = crate::place::PlaceState::Joining { generation };
        vec![Effect::FoundPlace {
            session: self.session_id,
            generation,
            name,
        }]
    }

    /// Write this place's card. Contact metadata and public ids only.
    pub fn export_place_card(&mut self, path: String) -> Vec<Effect> {
        let Some(binding) = self.place.binding().cloned() else {
            return self.refuse_place("open a place before exporting its card");
        };
        let Some(root) = self.personae_root() else {
            return self.refuse_place(crate::denizen::PENDING_IDENTITY);
        };
        let local_rendezvous = match &self.place {
            crate::place::PlaceState::Offline { snapshot, .. } => snapshot
                .sync
                .as_ref()
                .map(|sync| sync.local_rendezvous.clone())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let card = crate::place::PlaceCardV1 {
            version: crate::place::PLACE_CARD_VERSION,
            binding,
            founder_root: crate::place::hex32(&root),
            rendezvous: local_rendezvous
                .into_iter()
                .map(|hint| crate::place::invite::RendezvousV1 {
                    carrier: crate::place::invite::P2PANDA_ENDPOINT_TICKET.into(),
                    hint,
                })
                .collect(),
        };
        match serde_json::to_vec_pretty(&card) {
            Ok(bytes) => vec![Effect::WritePlaceArtifact { path, bytes }],
            Err(error) => self.refuse_place(format!("encode place card: {error}")),
        }
    }

    pub(crate) fn refresh_place_contact(&mut self, path: String) -> Vec<Effect> {
        let Some(generation) = self
            .place
            .generation()
            .filter(|_| self.place.binding().is_some())
        else {
            return self.refuse_place("join a place before refreshing its contact");
        };
        vec![Effect::ReadPlaceArtifact {
            path,
            kind: crate::action::PlaceArtifactKind::Contact {
                session: self.session_id,
                generation,
            },
        }]
    }
}
