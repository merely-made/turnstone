// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl TurnstoneEndpoint {
    pub fn new(app: App) -> Result<Self, String> {
        Self::with_card_extent(app, (248.0, 168.0))
    }

    /// Deterministic live graph used by the cross-process projection receipt.
    pub fn fixture() -> Result<Self, String> {
        Self::new(App::projection_fixture())
    }

    pub fn with_card_extent(app: App, card_extent: (f32, f32)) -> Result<Self, String> {
        let session = ProjectionSession(SESSION.into());
        // The endpoint's identity is DERIVED from the profile identity, not
        // hashed out of its own name. Before the capability-model audit the
        // subject was `blake3(session name)` — not a key at all, so nothing
        // could ever prove it was this endpoint — and the endpoint then
        // granted ITSELF the layout capability. A refusal there stated only
        // "the endpoint did not grant itself", which is not an authority
        // statement. Now: a per-session keypair derived from the user's
        // master key, holding a capability the USER delegated to it.
        // The endpoint speaks for the profile, so it waits for the vault to
        // unlock like everything else that does.
        let root = app
            .identity
            .clone()
            .ok_or_else(|| crate::denizen::PENDING_IDENTITY.to_string())?;
        let salt = format!("turnstone/projection-endpoint/{}", session.0);
        let endpoint_key = root
            .derive_keypair(salt.as_bytes())
            .map_err(|error| format!("failed to derive the endpoint identity: {error:?}"))?;
        let subject = Subject::new(endpoint_key.public_key().to_bytes());
        let gate = Gate::new();
        let mut audit = GraphLog::new();
        let layout = Cap::Scope(layout_scope());
        gate.project_grant(
            &mut audit,
            &Grant::new(subject, layout.clone(), Mode::Write),
        )
        .map_err(|error| format!("failed to project endpoint grant: {error:?}"))?;

        // The AUTHORITY is a signed delegation from the profile identity: the
        // user lets this endpoint present layout for this session, and
        // nothing else. The projection above stays the browsable audit record
        // of the same fact. Depth 0 — the endpoint may act, never delegate
        // onward; that bumps to 1 when a remote viewer with its own key needs
        // a narrower capability from it (the sub-delegation consumer named in
        // the capability plan's follow-ons).
        //
        // One clock read: issuing at a later instant than the table's own
        // `now` would leave `not_before` in its future and the endpoint
        // authorized for nothing.
        let now = crate::denizen::now_ms();
        let certificate = root_certificate(
            IdentityProvider::master_public_key(root.as_ref()).to_bytes(),
            subject,
            &layout,
            Mode::Write,
            format!("projection:{}", session.0).into_bytes(),
            now,
            None,
            0,
            *blake3::hash(salt.as_bytes()).as_bytes(),
        );
        // Signed inside djinn (D11): only the certificate crosses.
        let signed = root
            .issue_certificate(certificate)
            .map_err(|error| format!("failed to sign the endpoint delegation: {error:?}"))?;
        let mut authority =
            DelegationTable::new(IdentityProvider::master_public_key(root.as_ref()).to_bytes());
        authority.adopt(signed);
        authority.set_now(now);

        Ok(Self {
            app,
            session,
            card_extent,
            snapshot: None,
            snapshot_coverage: None,
            resources: BTreeMap::new(),
            gate,
            authority,
            audit,
            subject,
        })
    }

    pub fn session(&self) -> &ProjectionSession {
        &self.session
    }

    pub fn app(&self) -> &App {
        &self.app
    }

    /// Known graph limits beside the last successful local snapshot.
    ///
    /// None means no snapshot has been installed. An empty note means no known
    /// limit in that supplied graph, never world completeness. Chirograph V1
    /// does not carry this note to remote clients.
    pub fn snapshot_coverage(&self) -> Option<&CoverageNote> {
        self.snapshot_coverage.as_ref()
    }

    pub fn audit(&self) -> &GraphLog<Container, Relation> {
        &self.audit
    }

    pub(super) fn actions() -> Vec<AdvertisedAction> {
        vec![
            AdvertisedAction {
                intent: IntentReference(FIT_INTENT.into()),
                // The stack's label for the verb (SE49), so the advertised
                // card and Turnstone's own palette say the same thing.
                label: fit_label().into(),
                explanation: "Frame the disclosed Turnstone graph without changing it.".into(),
                payload_schema: r#"{"type":"null"}"#.into(),
                input_form: None,
                effect: IntentEffect::Curation,
            },
            AdvertisedAction {
                intent: IntentReference(OPEN_INTENT.into()),
                label: "Open address".into(),
                explanation: "Ask Turnstone to add or select an address in graph truth.".into(),
                payload_schema: r#"{"type":"string","format":"uri"}"#.into(),
                input_form: None,
                effect: IntentEffect::DomainTruth,
            },
        ]
    }

    pub(super) fn presentation(
        &mut self,
        scene: &SceneSnapshot,
    ) -> Result<PresentationManifest, String> {
        let mut manifest = PresentationManifest::default();
        let graph = self.app.graph_runtimes.graph();
        for (instance, item) in scene.active_items_in_order() {
            let source = scene
                .tables
                .sources
                .get(item.source.0 as usize)
                .and_then(Option::as_ref)
                .ok_or_else(|| format!("scene item {} has no source", instance.0))?;
            let id = uuid::Uuid::parse_str(&source.id)
                .map_err(|error| format!("invalid Mere node id {}: {error}", source.id))?;
            let (key, node) = graph
                .get_node_by_id(id)
                .ok_or_else(|| format!("Mere node {id} vanished during disclosure"))?;
            let label = graph.node_display_label(key);
            let semantics = PresentationSemantics {
                label: label.clone(),
                role: SemanticRole::Article,
                bounds: BoundsRelationship::FillFootprint,
                actions: Self::actions(),
            };
            let card = PortableCardV1 {
                title: label.clone(),
                values: vec![
                    CardValueV1 {
                        label: "Address".into(),
                        value: node.url().to_string(),
                    },
                    CardValueV1 {
                        label: "Source".into(),
                        value: "Turnstone graph".into(),
                    },
                ],
                badges: vec!["turnstone".into(), "granted projection".into()],
                media: Vec::new(),
            };
            let glyph = NativeGlyphV1 {
                label,
                icon: Some("◇".into()),
                color: Some("#d8a657".into()),
            };
            let card_bytes = serde_json::to_vec(&card)
                .map_err(|error| format!("could not encode card: {error}"))?;
            let glyph_bytes = serde_json::to_vec(&glyph)
                .map_err(|error| format!("could not encode glyph: {error}"))?;
            let card_hash = ContentHash::of(&card_bytes);
            let glyph_hash = ContentHash::of(&glyph_bytes);
            self.resources.insert(card_hash, card_bytes.clone());
            self.resources.insert(glyph_hash, glyph_bytes.clone());
            let key = PresentationKey(source.id.clone());
            manifest.bindings.push(PresentationBinding {
                instance,
                key: key.clone(),
            });
            manifest.offers.insert(
                key,
                vec![
                    PresentationOffer {
                        codec: PresentationCodec::PortableCardV1,
                        resource: card_hash,
                        byte_size: card_bytes.len() as u64,
                        requires: PresentationCapability::PortableCard,
                        semantics: semantics.clone(),
                    },
                    PresentationOffer {
                        codec: PresentationCodec::NativeGlyphV1,
                        resource: glyph_hash,
                        byte_size: glyph_bytes.len() as u64,
                        requires: PresentationCapability::NativeGlyph,
                        semantics,
                    },
                ],
            );
        }
        Ok(manifest)
    }

    pub(super) fn active_revision(&self) -> Option<(SceneEpoch, Revision)> {
        self.snapshot
            .as_ref()
            .map(|snapshot| (snapshot.scene.epoch, snapshot.scene.revision))
    }

    pub(super) fn intent_was_advertised(&self, intent: &IntentInvocation) -> bool {
        self.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.scene.active_item(intent.target).is_some()
                && snapshot
                    .presentation
                    .offers_for(intent.target)
                    .is_some_and(|offers| {
                        offers.iter().any(|offer| {
                            offer
                                .semantics
                                .actions
                                .iter()
                                .any(|action| action.intent.0 == intent.intent)
                        })
                    })
        })
    }

    pub(super) fn petition(
        &mut self,
        scope: &ScopePath,
        node: Container,
    ) -> Result<(), servitor::GateError> {
        let revision = self.audit.revision();
        self.gate.petition(
            &self.authority,
            &mut self.audit,
            self.subject,
            scope,
            revision,
            vec![EditSpec::InsertNode(node)],
        )?;
        Ok(())
    }
}
