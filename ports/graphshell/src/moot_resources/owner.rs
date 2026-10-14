// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! An admitted Graphshell mount over the actual Moot resource owners.
//!
//! The carrier supplies Notochord's verified retained authority and the host
//! supplies its current ledger, signer, Moot and content store. No principal,
//! grant, payload or activity session is fabricated from a displayed snapshot.
//! The asynchronous owner calls are scheduled by the embedding application.

use cambium::{AnyView, GenetCtx, GenetElement, button, el, lens};
use eidetic::Store;
use gemot::moot::Moot;
use moot::resource_coop::{Outcome, ResourceRef, Snapshot};
use moot::resource_owner::{self as owner, Applied, ForkDestination};
use moot::resource_view::Reading;
use muniment::Backend;
use notochord::{RetainedAuthority, RevocationLedger};
use personae::IdentityProvider;
use serde::{Deserialize, Serialize};

pub type View = Box<dyn AnyView<Mount, (), GenetCtx, GenetElement>>;

/// Existing owners borrowed for one operation, with the current host clock and
/// session revocation ledger. The caller retains all storage and key custody.
pub struct Owners<'a, B, P: ?Sized> {
    pub moot: &'a Moot<B>,
    pub identity: &'a P,
    pub store: &'a mut dyn Store,
    pub revocations: &'a RevocationLedger,
    pub at_ms: u64,
}

pub struct Scope {
    pub collection: [u8; 32],
    pub fork_destination: Option<ForkDestination>,
}

#[derive(Debug)]
pub enum Error {
    Admission(String),
    Owner(owner::Error),
    View(String),
    Resume(String),
    Persistence(muniment::StoreError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admission(reason) | Self::View(reason) | Self::Resume(reason) => {
                f.write_str(reason)
            },
            Self::Owner(error) => error.fmt(f),
            Self::Persistence(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for Error {}
impl From<owner::Error> for Error {
    fn from(error: owner::Error) -> Self {
        Self::Owner(error)
    }
}

/// Local reader output. Bytes remain owned by this particular mount. A plain
/// UTF-8 preview is available; this does not install a document editor or execute
/// embedded content. The embedding application may give the bytes to its reader.
#[derive(Debug)]
pub struct Opened {
    pub reference: ResourceRef,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub enum Update {
    Refreshed,
    Dispatched {
        outcome: Outcome,
        feedback_applied: bool,
        refreshed: bool,
    },
}

/// One independent mount. Drop/selection/appearance never changes membership,
/// hosting or resource custody. Closing this view cannot erase retained copies.
#[derive(Debug)]
pub struct Mount {
    id: String,
    authority: RetainedAuthority,
    moot: [u8; 32],
    collection: [u8; 32],
    destination: Option<ForkDestination>,
    reading: Reading,
    opened: Option<Opened>,
    closed: Option<String>,
    refresh_requested: bool,
    status: String,
}

/// Host-local attention, never a credential or replicated community record.
/// No owner snapshot, grant, payload, destination or queued command is saved.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedReading {
    version: u16,
    moot: [u8; 32],
    reader: [u8; 32],
    collection: [u8; 32],
    selected: Option<ResourceRef>,
    reopen_content: bool,
}

impl Mount {
    /// Save local attention to an explicitly chosen host-owned Muniment slot.
    /// Persistence is opt-in; rendering and dropping a mount never writes it.
    /// The host supplies a separate slot for each independent reading and owns
    /// its privacy, retention and deletion policy.
    pub async fn save<B: Backend + Clone>(&self, backend: B, key: &str) -> Result<(), Error> {
        if self.closed.is_some() {
            return Err(Error::Resume("A closed reading cannot be saved".into()));
        }
        let selected = self.reading.pane().selected_resource();
        let saved = SavedReading {
            version: 1,
            moot: self.moot,
            reader: self.authority.principal().subject,
            collection: self.collection,
            selected,
            reopen_content: self
                .opened
                .as_ref()
                .is_some_and(|opened| Some(opened.reference) == selected),
        };
        muniment::JsonSlots::new(backend)
            .save(key, &saved)
            .await
            .map_err(Error::Persistence)
    }

    /// Reopen saved attention using newly supplied admitted authority, current
    /// owners, ledger and clock. The caller supplies the expected collection
    /// and any freshly reviewed fork destination; saved bytes cannot redirect
    /// either. Missing, malformed, future-version and mismatched slots refuse.
    /// A withdrawn selection is cleared. Previously opened content is fetched
    /// and verified again through its owner, never restored from cached bytes.
    pub async fn reopen<B: Backend + Clone, P: IdentityProvider + ?Sized, S: Backend + Clone>(
        id: impl Into<String>,
        authority: RetainedAuthority,
        scope: Scope,
        owners: &mut Owners<'_, B, P>,
        backend: S,
        key: &str,
    ) -> Result<Self, Error> {
        check_session(&authority, owners)?;
        let saved: SavedReading = muniment::JsonSlots::new(backend)
            .load(key)
            .await
            .map_err(Error::Persistence)?
            .ok_or_else(|| Error::Resume("No saved resource reading in this slot".into()))?;
        if saved.version != 1 {
            return Err(Error::Resume(format!(
                "Unsupported saved reading version {}",
                saved.version
            )));
        }
        if saved.moot != owners.moot.moot_id().0
            || saved.reader != authority.principal().subject
            || saved.collection != scope.collection
            || saved
                .selected
                .is_some_and(|reference| reference.moot != saved.moot)
            || (saved.reopen_content && saved.selected.is_none())
        {
            return Err(Error::Resume(
                "Saved reading does not match this reader, Moot or collection".into(),
            ));
        }
        let mut mount = Self::open(id, authority, scope, owners).await?;
        if let Some(reference) = saved.selected {
            if mount
                .reading
                .pane()
                .snapshot()
                .resources
                .iter()
                .any(|r| r.reference == reference)
            {
                mount.reading.select_resource(reference);
                if saved.reopen_content {
                    // Prepare a new read from the newly minted owner offers.
                    // No saved intention or side-effecting command is replayed.
                    match mount
                        .reading
                        .request_resource(moot::resource_coop::ResourceAction::Open)
                    {
                        Ok(()) => {
                            mount.poll(owners).await?;
                        },
                        Err(error) => {
                            mount.status = format!("Saved content could not be reopened: {error}")
                        },
                    }
                }
            } else {
                mount.status = "The saved resource is no longer in the current collection".into();
            }
        }
        Ok(mount)
    }

    /// `authority` must come from the existing Graphshell admission path. For
    /// carriers use `RetainedAuthority::from_admitted`; sans-I/O owners retain
    /// their verified hello's chain. This is a local composition, not a second
    /// admission protocol or a serialized credential.
    pub async fn open<B: Backend + Clone, P: IdentityProvider + ?Sized>(
        id: impl Into<String>,
        authority: RetainedAuthority,
        scope: Scope,
        owners: &mut Owners<'_, B, P>,
    ) -> Result<Self, Error> {
        check_session(&authority, owners)?;
        let snapshot = owner::snapshot(
            owners.moot,
            authority.principal().subject,
            scope.collection,
            scope.fork_destination.as_ref(),
            vec![],
            owners.store,
            owners.at_ms,
        )
        .await?;
        let id = id.into();
        let reading = Reading::new(id.clone(), snapshot).map_err(Error::View)?;
        Ok(Self {
            id,
            authority,
            moot: owners.moot.moot_id().0,
            collection: scope.collection,
            destination: scope.fork_destination,
            reading,
            opened: None,
            closed: None,
            refresh_requested: false,
            status: "Reading the owners' shared resource view".into(),
        })
    }

    pub fn reading(&self) -> &Reading {
        &self.reading
    }
    pub fn opened(&self) -> Option<&Opened> {
        self.opened.as_ref()
    }
    pub fn closed_reason(&self) -> Option<&str> {
        self.closed.as_deref()
    }
    pub fn request_refresh(&mut self) {
        self.refresh_requested = true;
    }

    /// Pump after input, on an explicit refresh or when the host observes a
    /// session ledger change. It returns immediately when there is no work;
    /// it does not poll content stores on every rendering frame.
    pub async fn poll<B: Backend + Clone, P: IdentityProvider + ?Sized>(
        &mut self,
        owners: &mut Owners<'_, B, P>,
    ) -> Result<Option<Update>, Error> {
        if let Some(reason) = &self.closed {
            return Err(Error::Admission(reason.clone()));
        }
        if let Err(error) = check_session(&self.authority, owners) {
            self.close(error.to_string());
            return Err(error);
        }
        if owners.moot.moot_id().0 != self.moot {
            let error = Error::Admission("This mount belongs to another Moot".into());
            self.close(error.to_string());
            return Err(error);
        }
        if self.refresh_requested {
            self.refresh_requested = false;
            self.refresh(owners).await?;
            self.status = "Updated from the resource owners".into();
            return Ok(Some(Update::Refreshed));
        }
        let Some(request) = self.reading.take_request() else {
            return Ok(None);
        };
        let applied = owner::dispatch(
            owners.moot,
            owners.identity,
            owners.store,
            &request,
            self.destination.as_ref(),
            owners.at_ms,
        )
        .await;
        let outcome = match &applied {
            Ok(applied) => applied.outcome(),
            Err(error) => error.outcome(),
        };
        let feedback_applied = self.reading.complete(&request, outcome.clone());
        if feedback_applied {
            self.status = match &applied {
                Ok(Applied::Opened { .. }) => "Opened locally held content".into(),
                Ok(Applied::CollectionForked { .. }) => {
                    "The owner forked the reviewed collection references".into()
                },
                Err(error) => error.to_string(),
            };
            if let Ok(Applied::Opened { reference, bytes }) = applied {
                self.opened = Some(Opened { reference, bytes });
            }
        }
        // An accepted write remains accepted even if its subsequent read is
        // refused. The owner's receipt remains in `outcome`; closing the local
        // projection is never represented as undoing that command.
        let refreshed = self.refresh(owners).await.is_ok();
        Ok(Some(Update::Dispatched {
            outcome,
            feedback_applied,
            refreshed,
        }))
    }

    async fn refresh<B: Backend + Clone, P: IdentityProvider + ?Sized>(
        &mut self,
        owners: &mut Owners<'_, B, P>,
    ) -> Result<(), Error> {
        // Recheck retained authority after an awaited owner command as well.
        // The clock and ledger are caller-held inputs for this operation, not
        // a claim of an atomic transaction across independent owner stores.
        let result: Result<(), Error> = async {
            check_session(&self.authority, owners)?;
            let snapshot = owner::snapshot(
                owners.moot,
                self.authority.principal().subject,
                self.collection,
                self.destination.as_ref(),
                vec![],
                owners.store,
                owners.at_ms,
            )
            .await?;
            let still_open = self.opened.as_ref().is_none_or(|opened| {
                snapshot.resources.iter().any(|r| {
                    r.reference == opened.reference
                        && matches!(r.content, moot::resource_coop::ContentRead::Resolved)
                })
            });
            self.reading
                .refresh(snapshot)
                .map_err(|e| Error::View(e.to_string()))?;
            if !still_open {
                self.opened = None;
            }
            Ok(())
        }
        .await;
        if let Err(error) = &result {
            self.close(error.to_string());
        }
        result
    }

    fn close(&mut self, reason: String) {
        self.closed = Some(reason);
        self.refresh_requested = false;
        self.opened = None;
        // Invalidate local attention and queued completions and remove cached
        // disclosure. No content is deleted from an owner store.
        let _ = self.reading.refresh(Snapshot {
            moot: self.moot,
            reader: self.authority.principal().subject,
            collection: None,
            resources: vec![],
            activities: vec![],
        });
    }
}

fn check_session<B: Backend + Clone, P: IdentityProvider + ?Sized>(
    authority: &RetainedAuthority,
    owners: &Owners<'_, B, P>,
) -> Result<(), Error> {
    if !crate::admission::serves_action(authority.principal()) {
        return Err(Error::Admission(
            "This session was admitted for another service".into(),
        ));
    }
    if authority.principal().subject != owners.identity.master_public_key().to_bytes() {
        return Err(Error::Admission(
            "The current signer is not this admitted reader".into(),
        ));
    }
    if let Some(reason) = authority.lapse(owners.revocations, owners.at_ms) {
        return Err(Error::Admission(format!(
            "Projection session authority ended: {reason:?}"
        )));
    }
    Ok(())
}

pub const CSS: &str = moot::resource_view::CSS;
/// Bound the ordinary text projection independently of the owner's byte limit.
/// The exact opened bytes remain available to an application reader.
pub const TEXT_PREVIEW_CHARACTERS: usize = 8_192;

/// Ordinary Cambium composition. The embedding host supplies typography,
/// theme, layout/paint and the operation pump. Rendering performs no I/O.
pub fn view(mount: &Mount) -> View {
    let mut children: Vec<View> = vec![Box::new(
        el("p", mount.status.clone()).attr("role", "status"),
    )];
    if let Some(reason) = &mount.closed {
        children.push(Box::new(el("p", format!("Reading closed: {reason}"))));
    } else {
        children.push(Box::new(
            button("Refresh from owners", |mount: &mut Mount, _| {
                mount.request_refresh();
            })
            .attr("id", format!("moot-{}-refresh", mount.id)),
        ));
        children.push(Box::new(lens(
            |reading: &mut Reading| moot::resource_view::view(reading),
            |mount: &mut Mount| &mut mount.reading,
        )));
        if let Some(opened) = &mount.opened {
            children.push(Box::new(el("h2", "Local content")));
            let text = match std::str::from_utf8(&opened.bytes) {
                Ok(body) => {
                    if let Some((end, _)) = body.char_indices().nth(TEXT_PREVIEW_CHARACTERS) {
                        children.push(Box::new(el(
                            "p",
                            format!("Showing the first {TEXT_PREVIEW_CHARACTERS} characters"),
                        )));
                        body[..end].to_owned()
                    } else {
                        body.to_owned()
                    }
                },
                Err(_) => "These bytes need their application's reader".into(),
            };
            children.push(Box::new(
                el("pre", text).attr("id", format!("moot-{}-opened", mount.id)),
            ));
        }
    }
    Box::new(el("section", children).attr("aria-label", format!("Moot reading {}", mount.id)))
}
